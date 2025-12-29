pub mod dto;
mod handlers;
mod http;
mod websocket;

use self::handlers::ServerContext;
use self::http::AttachmentStorage;
use crate::{
    config::{AppConfig, MCPConfig},
    prompts::PromptManager,
    storage::{sqlite_storage_simple::SqliteSettings, Storage},
};
use anyhow::{Context, Result};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::{Mutex, RwLock};

pub struct ServerOptions {
    pub config_path: Option<PathBuf>,
}

pub fn run(options: ServerOptions) -> Result<()> {
    let runtime = tokio::runtime::Runtime::new().context("failed to create Tokio runtime")?;
    runtime.block_on(async move { launch(options).await })
}

async fn launch(options: ServerOptions) -> Result<()> {
    let raw_config = load_config(options.config_path.as_ref()).unwrap_or_else(|err| {
        tracing::warn!("Failed to load config: {}. Falling back to defaults.", err);
        AppConfig::default()
    });
    let config = Arc::new(raw_config);
    let prompt_manager = PromptManager::load_from_config(&config.prompts).unwrap_or_else(|err| {
        tracing::warn!("Failed to load prompts: {}", err);
        PromptManager::load_from_config(&crate::prompts::PromptConfig::default())
            .unwrap_or_else(|e| {
                tracing::error!(error = %e, "Failed to load default prompt config, using empty PromptManager");
                PromptManager {
                    system_prompt: None,
                }
            })
    });
    let sqlite_settings = SqliteSettings::from(&config.server);
    let storage =
        Storage::new_default_with_settings(sqlite_settings.clone()).unwrap_or_else(|err| {
            tracing::error!("SQLite init failed: {}. Using temp db.", err);
            Storage::new_with_settings(
                std::env::temp_dir().join("cosmic_llm_server.db"),
                sqlite_settings,
            )
            .unwrap_or_else(|e| {
                tracing::error!(error = %e, "Failed to create temporary database");
                std::process::exit(1);
            })
        });

    let storage = Arc::new(Mutex::new(storage));
    let mcp_registry = Arc::new(RwLock::new(crate::mcp::MCPServerRegistry::new()));
    let mcp_config = load_mcp_config(&config);
    initialize_mcp_registry(&mcp_registry, &mcp_config, &config).await;

    // Create attachment storage
    let attachment_storage = Arc::new(AttachmentStorage::new());

    let ctx = Arc::new(ServerContext {
        config: config.clone(),
        server_cfg: Arc::new(config.server.clone()),
        prompt_manager,
        storage: storage.clone(),
        mcp_registry,
        attachment_storage: attachment_storage.clone(),
    });

    // Spawn background title generation thread only if profile is configured
    if config.title_summary.title_generation_profile.is_some() {
        spawn_title_generation_thread(config.clone(), storage);
    }

    // Start HTTP server on port + 1 (e.g., if WS is on 8080, HTTP is on 8081)
    let http_port = config.server.port + 1;
    let http_addr = format!("{}:{}", config.server.host, http_port);
    let http_router = http::create_http_router(ctx.clone());
    let http_listener = tokio::net::TcpListener::bind(&http_addr)
        .await
        .context("failed to bind HTTP server")?;
    
    tracing::info!(address = %http_addr, "HTTP server for file attachments listening");
    
    // Spawn HTTP server in background
    tokio::spawn(async move {
        if let Err(err) = axum::serve(http_listener, http_router).await {
            tracing::error!("HTTP server error: {}", err);
        }
    });

    // Start WebSocket server (blocks)
    websocket::serve(ctx).await
}

fn load_config(path: Option<&PathBuf>) -> Result<AppConfig, config::ConfigError> {
    if let Some(custom) = path {
        AppConfig::load_from_path(Some(custom))
    } else {
        AppConfig::load()
    }
}

fn load_mcp_config(config: &AppConfig) -> MCPConfig {
    MCPConfig::load_from_json().unwrap_or_else(|err| {
        tracing::warn!("Failed to load external MCP config: {}", err);
        config.mcp.clone()
    })
}

async fn initialize_mcp_registry(
    registry: &Arc<RwLock<crate::mcp::MCPServerRegistry>>,
    config: &MCPConfig,
    app_config: &AppConfig,
) {
    let default_tools = app_config
        .get_default_profile()
        .map(|profile| profile.enabled_mcp.clone())
        .unwrap_or_default();

    let mut guard = registry.write().await;
    if let Err(err) = guard.initialize_from_config(config).await {
        tracing::warn!("MCP registry init failed: {}", err);
    } else if !default_tools.is_empty() {
        guard.apply_profile_tool_defaults(&default_tools);
    }
}

fn spawn_title_generation_thread(
    config: Arc<AppConfig>,
    storage: Arc<Mutex<Storage>>,
) {
    tokio::spawn(async move {
        let title_config = &config.title_summary;
        let sleep_duration = std::time::Duration::from_secs(title_config.summary_loop_sleep_seconds);

        loop {
            tokio::time::sleep(sleep_duration).await;

            // Get conversations without titles
            let conversation_ids = {
                let storage_guard = storage.lock().await;
                match storage_guard.get_conversations_without_title() {
                    Ok(ids) => ids,
                    Err(e) => {
                        tracing::warn!("Failed to get conversations without titles: {}", e);
                        continue;
                    }
                }
            };

            if conversation_ids.is_empty() {
                continue;
            }

            // Get the profile to use for title generation
            // This should always be Some since we only start the thread if profile is configured
            let profile_name = match &title_config.title_generation_profile {
                Some(name) => name,
                None => {
                    tracing::warn!("Title generation profile not configured, stopping thread");
                    break;
                }
            };

            let profile = match config.get_profile(profile_name) {
                Some(p) => p.clone(),
                None => {
                    tracing::warn!(
                        profile_name = %profile_name,
                        "Title generation profile not found, stopping thread"
                    );
                    break;
                }
            };

                // Generate title for each conversation
            for conversation_id in conversation_ids {
                let conversation_id_str = conversation_id.to_string();
                let profile_clone = profile.clone();
                let summary_chars = title_config.summary_chars;
                let system_prompt = title_config.title_generation_system_prompt.clone();
                
                // Generate title using Storage wrapper method
                // Load messages first, then release lock before async LLM call
                let title_result = {
                    let messages = {
                        let storage_guard = storage.lock().await;
                        storage_guard.load_conversation_messages(&conversation_id_str)
                    };
                    
                    let messages = match messages {
                        Ok(msgs) => msgs,
                        Err(e) => {
                            tracing::warn!("Failed to load messages for conversation {}: {}", conversation_id, e);
                            continue;
                        }
                    };
                    
                    // Skip if there are no messages
                    if messages.is_empty() {
                        tracing::debug!("Skipping title generation for conversation {}: no messages", conversation_id);
                        continue;
                    }
                    
                    // Check if last message is older than 1 minute
                    if let Some(last_message) = messages.last() {
                        let last_message_time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(last_message.created_at as u64);
                        let now = std::time::SystemTime::now();
                        
                        if let Ok(duration_since_last_message) = now.duration_since(last_message_time) {
                            let one_minute = std::time::Duration::from_secs(60);
                            if duration_since_last_message < one_minute {
                                tracing::debug!(
                                    "Skipping title generation for conversation {}: last message is only {} seconds old",
                                    conversation_id,
                                    duration_since_last_message.as_secs()
                                );
                                continue;
                            }
                        }
                    }
                    
                    // Now call async function without holding the lock
                    use crate::storage::title_generation::generate_title_from_messages;
                    generate_title_from_messages(
                        messages,
                        &profile_clone,
                        summary_chars,
                        &system_prompt,
                    ).await
                };

                match title_result {
                    Ok(title) => {
                        let storage_guard = storage.lock().await;
                        if let Err(e) = storage_guard.update_conversation_title_and_flag(&conversation_id, &title) {
                            tracing::warn!("Failed to update title for conversation {}: {}", conversation_id, e);
                        } else {
                            tracing::info!("Generated title for conversation {}: {}", conversation_id, title);
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to generate title for conversation {}: {}", conversation_id, e);
                    }
                }
            }
        }
    });
}

