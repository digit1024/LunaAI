pub mod conversation_subscriptions;
pub mod dto;
mod handlers;
mod http;
mod websocket;

use crate::server::handlers::{run_scheduled_task, ServerContext};
use crate::services::ScheduleService;
use crate::storage::ScheduledJob;
use crate::{
    config::AppConfig,
    prompts::PromptManager,
    storage::{sqlite_storage_simple::SqliteSettings, Storage},
};
use agentic_loop::mcp_servers_registry::MCPServerRegistry;
use anyhow::{Context, Result};
use chrono::Utc;
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
    let mcp_registry = Arc::new(RwLock::new(MCPServerRegistry::new()));
    let subscriptions = Arc::new(conversation_subscriptions::ConversationSubscriptions::new());
    let schedule_service = Arc::new(ScheduleService::new(storage.clone()));
    let mcp_config = load_mcp_config(&config);
    initialize_mcp_registry(&mcp_registry, &mcp_config, &config).await;

    let ctx = Arc::new(ServerContext {
        config: config.clone(),
        server_cfg: Arc::new(config.server.clone()),
        prompt_manager,
        storage: storage.clone(),
        mcp_registry,
        subscriptions,
        schedule_service,
        memory_dedup: tokio::sync::Mutex::new(std::collections::HashMap::new()),
    });

    // Spawn background title generation thread only if profile is configured
    if config.title_summary.title_generation_profile.is_some() {
        spawn_title_generation_thread(config.clone(), storage);
    }

    // Scheduler loop: run due scheduled jobs every 45s
    spawn_scheduler_loop(ctx.clone());

    // Deep sleep loop: periodic memory maintenance
    if config.deep_sleep.enabled {
        if let Some(ref profile_name) = config.deep_sleep.profile {
            tracing::info!(
                profile = %profile_name,
                interval_hours = config.deep_sleep.interval_hours,
                max_conversations = config.deep_sleep.max_conversations_per_run,
                "Deep Sleep: enabled, first check in {}s",
                DEEP_SLEEP_POLL_SECS
            );
            spawn_deep_sleep_loop(ctx.clone());
        } else {
            tracing::warn!("Deep sleep is enabled but no profile configured -- skipping");
        }
    } else {
        tracing::info!("Deep Sleep: disabled (set [deep_sleep] enabled=true in config)");
    }

    // Single server on host:port: HTTP (/api/*) and WebSocket (/ws)
    let bind_addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .context("failed to bind server")?;
    tracing::info!(address = %bind_addr, "Luna server listening (HTTP + WebSocket on /ws)");

    let app = http::create_http_router(ctx);
    axum::serve(listener, app)
        .await
        .context("server error")?;
    Ok(())
}

fn load_config(path: Option<&PathBuf>) -> Result<AppConfig, config::ConfigError> {
    if let Some(custom) = path {
        AppConfig::load_from_path(Some(custom))
    } else {
        AppConfig::load()
    }
}

fn load_mcp_config(config: &AppConfig) -> crate::config::MCPConfig {
    crate::config::MCPConfig::load_from_json().unwrap_or_else(|err| {
        tracing::warn!("Failed to load external MCP config: {}", err);
        config.mcp.clone()
    })
}

/// Convert main app MCPConfig to agentic-loop MCPConfig
fn convert_mcp_config(config: &crate::config::MCPConfig) -> agentic_loop::mcp_config::MCPConfig {
    // Since both configs have identical structure, convert via JSON
    let json = serde_json::to_value(config).unwrap_or_default();
    serde_json::from_value(json).unwrap_or_else(|_| agentic_loop::mcp_config::MCPConfig::new())
}

async fn initialize_mcp_registry(
    registry: &Arc<RwLock<MCPServerRegistry>>,
    config: &crate::config::MCPConfig,
    app_config: &AppConfig,
) {
    let default_tools = app_config
        .get_default_profile()
        .map(|profile| profile.enabled_mcp.clone())
        .unwrap_or_default();

    // Convert to agentic-loop config type
    let agentic_config = convert_mcp_config(config);

    let mut guard = registry.write().await;
    if let Err(err) = guard.initialize_from_config(&agentic_config).await {
        tracing::warn!("MCP registry init failed: {}", err);
    } else {
        // Enable tools based on profile configuration
        // If default_tools is empty, enable_all_tools() is called internally
        // If default_tools has invalid server names, warnings are logged but we continue
        guard.enable_tools_for_multiple_servers(default_tools.clone()).await;
        
        // If no tools were enabled (e.g., due to invalid server names), enable all tools as fallback
        if guard.tools_white_list.is_empty() && !guard.servers.is_empty() {
            tracing::warn!("No tools enabled from profile configuration, enabling all tools from connected servers as fallback");
            guard.enable_all_tools().await;
        }
    }

    // Log all connected servers and their tools
    tracing::info!("=== MCP Servers Initialization Summary ===");
    let connected_count = guard.servers.len();
    let failed_count = guard.failed_servers.len();
    
    if connected_count > 0 {
        tracing::info!(count = connected_count, "Connected MCP servers:");
        for (server_name, server_connection) in guard.servers.iter() {
            let connection_guard = server_connection.read().await;
            let tools = connection_guard.tools();
            let tool_names: Vec<String> = tools.iter().map(|tool| tool.name.clone()).collect();
            let enabled_count = tool_names.iter()
                .filter(|name| guard.tools_white_list.contains(name))
                .count();
            drop(connection_guard); // Explicitly drop to release the read lock
            tracing::info!(
                server = %server_name,
                tool_count = tool_names.len(),
                enabled_count = enabled_count,
                tools = %tool_names.join(", ")
            );
        }
    } else {
        tracing::warn!("No MCP servers connected");
    }

    // Log failed/disconnected servers as errors
    if failed_count > 0 {
        tracing::error!(count = failed_count, "Failed to connect to MCP servers:");
        for (server_name, error_msg) in guard.failed_servers.iter() {
            tracing::error!(
                server = %server_name,
                error = %error_msg,
                "Failed to connect to MCP server"
            );
        }
    }
    
    tracing::info!(
        connected = connected_count,
        failed = failed_count,
        total = connected_count + failed_count,
        "MCP server initialization complete"
    );
}

const SCHEDULER_INTERVAL_SECS: u64 = 45;
const SCHEDULER_BATCH_LIMIT: u32 = 10;

fn spawn_scheduler_loop(ctx: Arc<ServerContext>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(SCHEDULER_INTERVAL_SECS));
        interval.tick().await;
        loop {
            interval.tick().await;
            let now = Utc::now().timestamp();
            let jobs: Vec<ScheduledJob> = {
                let storage = ctx.storage.lock().await;
                match storage.get_due_scheduled_jobs(now, SCHEDULER_BATCH_LIMIT) {
                    Ok(j) => j,
                    Err(e) => {
                        tracing::warn!("Scheduler: failed to get due jobs: {}", e);
                        continue;
                    }
                }
            };
            for job in jobs {
                let taken = {
                    let storage = ctx.storage.lock().await;
                    match storage.set_scheduled_job_running(&job.id, now) {
                        Ok(t) => t,
                        Err(e) => {
                            tracing::warn!(job_id = %job.id, "Scheduler: failed to mark running: {}", e);
                            continue;
                        }
                    }
                };
                if !taken {
                    continue;
                }
                let job_id = job.id.clone();
                if let Err(e) = run_scheduled_task(ctx.clone(), job).await {
                    tracing::warn!("Scheduler: run_scheduled_task failed: {}", e);
                    let storage = ctx.storage.lock().await;
                    let _ = storage.set_scheduled_job_completed(
                        &job_id,
                        Utc::now().timestamp(),
                        true,
                        Some(&e.to_string()),
                    );
                }
            }
        }
    });
}

/// Deep sleep background loop: checks every 5 minutes if a cycle is due.
const DEEP_SLEEP_POLL_SECS: u64 = 300; // 5 minutes

fn spawn_deep_sleep_loop(ctx: Arc<ServerContext>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(DEEP_SLEEP_POLL_SECS));
        interval.tick().await; // skip immediate first tick
        loop {
            interval.tick().await;

            let deep_sleep_cfg = &ctx.config.deep_sleep;

            // Check if due
            let is_due = {
                let guard = ctx.storage.lock().await;
                crate::services::deep_sleep_service::is_due(&guard, deep_sleep_cfg.interval_hours)
            };

            if !is_due {
                continue;
            }

            tracing::info!("Deep Sleep: cycle is due, starting...");

            // Build LLM client from the configured profile
            let profile_name = match &deep_sleep_cfg.profile {
                Some(name) => name.clone(),
                None => {
                    tracing::warn!("Deep Sleep: no profile configured, skipping");
                    continue;
                }
            };

            let profile = match ctx.config.get_profile(&profile_name) {
                Some(p) => p.clone(),
                None => {
                    tracing::warn!(
                        profile = %profile_name,
                        "Deep Sleep: profile not found, skipping"
                    );
                    continue;
                }
            };

            let llm_client = crate::llm::build_llm_client(&profile);

            if let Err(e) = crate::services::deep_sleep_service::run_deep_sleep_cycle(
                ctx.storage.clone(),
                deep_sleep_cfg,
                llm_client,
            )
            .await
            {
                tracing::error!(error = %e, "Deep Sleep: cycle failed");
            }
        }
    });
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

