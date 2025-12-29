//! Initialization helpers
//!
//! Helper functions for app initialization that can be safely extracted
//! without lifetime issues.

use crate::storage::Storage;
use crate::ui::app::CosmicLlmApp;
use crate::config::AppConfig;
use crate::prompts::PromptManager;
use crate::mcp::MCPServerRegistry;
use crate::llm::LlmClient;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

/// Retry title generation for conversations with "Generating title..."
///
/// This is called during app initialization to fix any conversations
/// that were left in a "Generating title..." state.
pub fn retry_title_generation(app: &mut CosmicLlmApp) {
    debug!("Checking for conversations with 'Generating title...'");
    let conversations = app.storage.list_conversations().unwrap_or_else(|e| {
        tracing::warn!(error = %e, "Failed to list conversations");
        Vec::new()
    });
    let conversation_ids: Vec<_> = conversations
        .into_iter()
        .filter(|conv| conv.title == "Generating title...")
        .map(|conv| conv.id)
        .collect();

    for conv_id in conversation_ids {
        debug!(
            conversation_id = %conv_id,
            "Found conversation with 'Generating title...', retrying"
        );

        // Get the first user message to generate title from
        if let Ok(Some(conversation)) = app.storage.get_conversation(&conv_id) {
            if let Some(first_user_msg) =
                conversation.messages.iter().find(|msg| msg.role == "user")
            {
                let message_text = &first_user_msg.content;
                debug!(
                    conversation_id = %conv_id,
                    message_preview = &message_text[..message_text.len().min(50)],
                    "Retrying title generation"
                );

                // Create a simple title based on first few words
                let fallback_title = if message_text.len() > 50 {
                    format!("{}...", &message_text[..47])
                } else {
                    message_text.clone()
                };

                if let Err(e) = app
                    .storage
                    .update_conversation_title(&conv_id, fallback_title.clone())
                {
                    tracing::error!(
                        conversation_id = %conv_id,
                        error = %e,
                        "Failed to update conversation title"
                    );
                } else {
                    debug!(
                        conversation_id = %conv_id,
                        title = %fallback_title,
                        "Updated title"
                    );
                }
            }
        }
    }
    debug!("Finished checking for conversations with 'Generating title...'");
}

/// Initialize MCP registry asynchronously
///
/// This spawns a background task to initialize the MCP registry
/// from configuration.
pub fn initialize_mcp_registry(
    mcp_registry: Arc<RwLock<MCPServerRegistry>>,
    mcp_config: crate::config::MCPConfig,
    initial_profile_mcp_servers: Vec<String>,
) {
    tokio::spawn(async move {
        let mut registry = mcp_registry.write().await;
        if let Err(e) = registry.initialize_from_config(&mcp_config).await {
            tracing::error!(error = %e, "Failed to initialize MCP registry");
        } else {
            // Always apply profile defaults, even if empty (empty = enable all)
            registry.apply_profile_tool_defaults(&initial_profile_mcp_servers);
        }
    });
}

/// Initialize storage with fallback handling
///
/// Attempts to create storage with default settings, falling back to
/// a temporary database if that fails.
pub fn initialize_storage(
    config: &AppConfig,
) -> Result<Storage, Box<dyn std::error::Error>> {
    use crate::storage::sqlite_storage_simple::SqliteSettings;
    
    let sqlite_settings = SqliteSettings::from(&config.server);
    Storage::new_default_with_settings(sqlite_settings.clone())
        .or_else(|e| {
            tracing::error!(error = %e, "Failed to initialize SQLite storage");
            // Fallback to a temporary database
            Storage::new_with_settings(
                std::env::temp_dir().join("cosmic_llm_temp.db"),
                sqlite_settings,
            )
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to create temporary database");
                e.into()
            })
        })
}

/// Initialize prompt manager with fallback handling
///
/// Attempts to load prompts from config, falling back to defaults
/// if that fails.
pub fn initialize_prompt_manager(
    config: &AppConfig,
) -> PromptManager {
    crate::prompts::PromptManager::load_from_config(&config.prompts)
        .unwrap_or_else(|e| {
            tracing::error!(error = %e, "Failed to load prompts");
            crate::prompts::PromptManager::load_from_config(
                &crate::prompts::PromptConfig::default(),
            )
            .unwrap_or_else(|e| {
                tracing::error!(error = %e, "Failed to load default prompt config, using empty PromptManager");
                crate::prompts::PromptManager {
                    system_prompt: None,
                }
            })
        })
}

/// Initialize LLM client from default profile
pub fn initialize_llm_client(config: &AppConfig) -> Arc<dyn LlmClient> {
    let profile = config
        .get_default_profile()
        .unwrap_or(&crate::config::LlmProfile::default())
        .clone();
    crate::llm::build_llm_client(&profile)
}

/// Create startup tasks for app initialization
pub fn create_startup_tasks(
    app: &CosmicLlmApp,
) -> Vec<cosmic::app::Task<crate::ui::app::Message>> {
    use cosmic::app::Task;
    use crate::ui::app::Message;
    
    let mut tasks = Vec::new();
    
    // Load MCP tools on startup (same as refresh button)
    let load_tools_task = Task::perform(
        async move {
            // Wait for MCP servers to initialize (give them more time)
            tokio::time::sleep(tokio::time::Duration::from_millis(5000)).await;
            tracing::debug!("Startup: Attempting to refresh MCP tools");
            cosmic::Action::App(Message::RefreshMCPTools)
        },
        |msg| msg,
    );
    tasks.push(load_tools_task);
    
    // Check D-Bus TTS/STT service availability at startup (only if feature enabled)
    #[cfg(feature = "ttsandstt")]
    {
        let dbus_client = app.dbus_ttsstt_client.clone();
        let dbus_check_task = Task::perform(
            async move {
                tracing::debug!("Checking D-Bus TTS/STT service availability");
                let available = dbus_client.check_availability().await;
                tracing::debug!(available, "D-Bus service check result");
                cosmic::Action::App(Message::DbusServiceAvailable(available))
            },
            |msg| msg,
        );
        tasks.push(dbus_check_task);
    }
    
    // Apply profile tool defaults
    if let Some(task) = app.profile_tool_defaults_task() {
        tasks.push(task);
    }
    
    tasks
}
