//! Navigation message handlers
//!
//! Handles navigation-related messages: SelectConversation, DeleteConversation, NewConversation, NavigateTo

use cosmic::app;
use uuid::Uuid;

use crate::ui::app::{CosmicLlmApp, Message, NavigationPage};

/// Handle navigation-related messages
pub fn handle_navigation_messages(
    app: &mut CosmicLlmApp,
    message: Message,
) -> Option<app::Task<Message>> {
    match message {
        Message::NavigateTo(page) => {
            app.current_page = page;

            // Refresh MCP tools when navigating to MCP config page or Chat page
            if page == NavigationPage::MCPConfig || page == NavigationPage::Chat {
                // Immediately try to get cached tools
                if let Ok(registry) = app.mcp_registry.try_read() {
                    app.available_mcp_tools = registry.get_available_tools();
                    app.tool_states = registry.get_tool_states();
                }
            }
            None
        }
        Message::SelectConversation(id) => {
            handle_select_conversation(app, id);
            None
        }
        Message::DeleteConversation(id) => {
            handle_delete_conversation(app, id);
            None
        }
        Message::NewConversation => {
            handle_new_conversation(app);
            None
        }
        _ => None, // Not a navigation message
    }
}

fn handle_select_conversation(app: &mut CosmicLlmApp, id: Uuid) {
    app.conversation_state.current_conversation_id = Some(id);
    app.current_page = NavigationPage::Chat;
    if let Ok(Some(conv)) = app.storage.get_conversation(&id) {
        // Switch to the conversation's profile, or default if not set/present
        let profile_name_to_use = conv.profile_name.as_deref()
            .and_then(|name| {
                if app.config.profiles.contains_key(name) {
                    Some(name)
                } else {
                    None
                }
            })
            .unwrap_or(&app.config.default);
        
        // Only switch if different from current default
        let profile_changed = if profile_name_to_use != &app.config.default {
            if let Some(profile) = app.config.get_profile(profile_name_to_use).cloned() {
                let masked = if profile.api_key.len() > 6 {
                    format!(
                        "{}...{}",
                        &profile.api_key[..3],
                        &profile.api_key[profile.api_key.len().saturating_sub(3)..]
                    )
                } else {
                    "***".to_string()
                };
                tracing::debug!(
                    conversation_id = %id,
                    profile_name = %profile_name_to_use,
                    model = %profile.model,
                    endpoint = %profile.endpoint,
                    api_key_masked = %masked,
                    "Switching to conversation's profile"
                );
                app.config.default = profile_name_to_use.to_string();
                app.llm_client = crate::llm::build_llm_client(&profile);
                true
            } else {
                false
            }
        } else {
            // Ensure LLM client is using the current default profile
            if let Some(profile) = app.config.get_default_profile().cloned() {
                app.llm_client = crate::llm::build_llm_client(&profile);
            }
            false
        };
        
        app.rebuild_conversation_view(conv);
        
        // Return profile tool defaults task if profile changed
        if profile_changed {
            if let Some(_task) = app.profile_tool_defaults_task() {
                // Update nav model to reflect current conversation
                app.load_recent_conversations();
                app.update_nav_model();
                // Note: We can't return the task from here, so we'll need to handle this differently
            }
        }
    }
    // Update nav model to reflect current conversation
    app.load_recent_conversations();
    app.update_nav_model();
}

fn handle_delete_conversation(app: &mut CosmicLlmApp, id: Uuid) {
    // If deleting the active conversation, clear the chat
    if app.conversation_state.current_conversation_id == Some(id) {
        app.conversation_state.current_conversation_id = None;
        app.conversation_state.messages.clear();
        app.chat_page.input.clear();
    }
    let _ = app.storage.delete_conversation(&id);
    // Stay on History page to reflect changes
    app.current_page = NavigationPage::History;
    // Update nav model to reflect deleted conversation
    app.load_recent_conversations();
    app.update_nav_model();
}

fn handle_new_conversation(app: &mut CosmicLlmApp) {
    app.conversation_state.current_conversation_id = None;
    app.conversation_state.messages.clear();
    app.tool_call_state.active_tool_calls.clear();
    app.tool_call_state.archived_tool_calls.clear();
    app.tool_call_state.expanded_tool_calls.clear();
    app.tool_call_state.expanded_tool_summaries.clear();
    app.tool_call_state.pending_tool_calls_for_history.clear();
    app.tool_call_state.tool_runtime_context.clear();
    app.attachment_state.clear();
    app.context_state.expanded_reasoning.clear();
    app.context_state.expanded_summaries.clear();
    app.chat_page.input.clear();
    app.chat_page.input_content = cosmic::widget::text_editor::Content::new();
    app.current_page = NavigationPage::Chat;
    app.load_recent_conversations();
    app.update_nav_model();
}

