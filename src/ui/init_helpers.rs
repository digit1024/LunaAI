//! Initialization helpers
//!
//! Helper functions for app initialization that can be safely extracted
//! without lifetime issues.

use crate::ui::app::CosmicLlmApp;
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
    mcp_registry: std::sync::Arc<tokio::sync::RwLock<crate::mcp::MCPServerRegistry>>,
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

