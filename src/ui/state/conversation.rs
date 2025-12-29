//! Conversation state management
//!
//! Manages conversation-related state:
//! - Current conversation
//! - Message history
//! - Recent conversations list
//! - Context usage cache

use crate::storage::Storage;
use crate::ui::app::ChatMessage;
use anyhow::{Context, Result};
use std::collections::HashMap;
use tracing::{debug, error};
use uuid::Uuid;

/// Conversation state
pub struct ConversationState {
    /// Current active conversation ID
    pub current_conversation_id: Option<Uuid>,
    
    /// Messages in the current conversation (UI representation)
    pub messages: Vec<ChatMessage>,
    
    /// Recent conversations for nav bar (last 10)
    pub recent_conversations: Vec<(Uuid, String)>, // (id, title)
    
    /// Cache for context usage percentage per conversation (to avoid blocking UI)
    pub context_usage_cache: HashMap<Uuid, Option<u32>>,
}

impl ConversationState {
    /// Create a new conversation state
    pub fn new() -> Self {
        Self {
            current_conversation_id: None,
            messages: Vec::new(),
            recent_conversations: Vec::new(),
            context_usage_cache: HashMap::new(),
        }
    }

    /// Load a conversation from storage
    pub fn load_conversation(
        &mut self,
        conversation_id: Uuid,
        storage: &Storage,
    ) -> Result<()> {
        debug!(conversation_id = %conversation_id, "Loading conversation");

        // Verify conversation exists
        storage
            .get_conversation(&conversation_id)
            .context("Failed to get conversation from storage")?
            .ok_or_else(|| anyhow::anyhow!("Conversation {} not found", conversation_id))?;

        // Load messages
        let db_messages = storage
            .load_conversation_messages(&conversation_id.to_string())
            .context("Failed to load conversation messages")?;

        // Convert to UI messages
        self.messages = db_messages
            .into_iter()
            .map(|msg| ChatMessage {
                content: msg.content,
                is_user: msg.role == "user",
                is_error: false,
                reasoning_content: msg.reasoning_content,
                is_summary: msg.is_summary,
                is_summarized: msg.is_summarized,
                summarized_count: msg.summarized_count,
            })
            .collect();

        self.current_conversation_id = Some(conversation_id);

        // Note: update_context_usage_cache requires config and prompt_manager
        // This will be called separately by the caller if needed

        Ok(())
    }

    /// Create a new conversation
    pub fn create_conversation(
        &mut self,
        storage: &Storage,
        profile_name: Option<&str>,
    ) -> Result<Uuid> {
        let conv_id = storage
            .create_conversation_with_profile(
                "Generating title...".to_string(),
                profile_name,
            )
            .context("Failed to create conversation")?;

        self.current_conversation_id = Some(conv_id);
        self.messages.clear();

        debug!(conversation_id = %conv_id, "Created new conversation");

        Ok(conv_id)
    }

    /// Delete a conversation
    pub fn delete_conversation(
        &mut self,
        conversation_id: Uuid,
        storage: &Storage,
    ) -> Result<()> {
        storage
            .delete_conversation(&conversation_id)
            .context("Failed to delete conversation")?;

        // Clear if it's the current conversation
        if self.current_conversation_id == Some(conversation_id) {
            self.current_conversation_id = None;
            self.messages.clear();
        }

        // Remove from recent conversations
        self.recent_conversations.retain(|(id, _)| *id != conversation_id);

        // Remove from cache
        self.context_usage_cache.remove(&conversation_id);

        debug!(conversation_id = %conversation_id, "Deleted conversation");

        Ok(())
    }

    /// Load recent conversations for nav bar
    pub fn load_recent_conversations(&mut self, storage: &Storage) {
        match storage.list_conversations_paginated(None, Some(10)) {
            Ok(conversations) => {
                self.recent_conversations = conversations
                    .into_iter()
                    .map(|conv| (conv.id, conv.title))
                    .collect();
                debug!(
                    count = self.recent_conversations.len(),
                    "Loaded recent conversations"
                );
            }
            Err(e) => {
                error!(error = %e, "Failed to load recent conversations");
            }
        }
    }

    /// Update context usage cache for a conversation
    pub fn update_context_usage_cache(
        &mut self,
        conversation_id: Uuid,
        storage: &Storage,
        config: &crate::config::AppConfig,
        prompt_manager: &crate::prompts::PromptManager,
    ) {
        if let Ok(Some(conv)) = storage.get_conversation(&conversation_id) {
            let usage_pct = crate::ui::pages::chat::top_panel::calculate_context_usage(
                &conv,
                config,
                prompt_manager,
            );
            self.context_usage_cache.insert(conversation_id, usage_pct);
        }
    }

    /// Rebuild conversation view from stored conversation
    /// This loads messages and tool calls into the UI state
    pub fn rebuild_conversation_view(
        &mut self,
        conversation: crate::storage::conversation_storage::Conversation,
        tool_call_state: &mut crate::ui::state::tool_calls::ToolCallState,
        context_state: &mut crate::ui::state::context::ContextState,
        storage: &Storage,
        config: &crate::config::AppConfig,
        prompt_manager: &crate::prompts::PromptManager,
    ) {
        use crate::ui::app::{AnchoredToolCall, ToolCallInfo, ToolCallStatus};

        self.messages.clear();
        tool_call_state.archived_tool_calls.clear();
        tool_call_state.active_tool_calls.clear();
        tool_call_state.set_current_ai_message_index(None);
        tool_call_state.pending_tool_calls_for_history.clear();
        tool_call_state.tool_runtime_context.clear();
        tool_call_state.expanded_tool_summaries.clear();
        context_state.expanded_reasoning.clear();
        context_state.expanded_summaries.clear();

        let mut archived_indices: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        for stored in conversation.messages {
            // Tool role messages should NOT be added as regular chat messages -
            // they only update the archived tool calls with results
            if stored.role == "tool" {
                if let Some(tool_call_id) = stored.tool_call_id.as_ref() {
                    if let Some(idx) = archived_indices.get(tool_call_id) {
                        if let Some(entry) = tool_call_state.archived_tool_calls.get_mut(*idx) {
                            entry.tool_call.status =
                                if stored.tool_status.as_deref() == Some("error") {
                                    ToolCallStatus::Error
                                } else {
                                    ToolCallStatus::Completed
                                };

                            // Prefer tool_result_json over content (content is legacy/redundant)
                            let result_text = stored
                                .tool_result_json
                                .as_ref()
                                .map(|value| {
                                    serde_json::to_string_pretty(value)
                                        .unwrap_or_else(|_| value.to_string())
                                })
                                .unwrap_or_else(|| stored.content.clone());
                            if entry.tool_call.status == ToolCallStatus::Error {
                                entry.tool_call.error = Some(result_text);
                            } else {
                                entry.tool_call.result = Some(result_text);
                            }
                        }
                    }
                }
                continue; // Skip adding tool messages to self.messages
            }

            let is_user = stored.role == "user";
            self.messages.push(ChatMessage {
                content: stored.content.clone(),
                is_user,
                is_error: false,
                reasoning_content: stored.reasoning_content.clone(),
                is_summary: stored.is_summary,
                is_summarized: stored.is_summarized,
                summarized_count: stored.summarized_count,
            });
            let anchor_index = self.messages.len().saturating_sub(1);

            if let Some(tool_calls) = stored.tool_calls {
                for call in tool_calls {
                    let params_pretty = serde_json::to_string_pretty(&call.parameters)
                        .unwrap_or_else(|_| call.parameters.to_string());
                    let info = ToolCallInfo {
                        id: Some(call.id.clone()),
                        tool_name: call.name.clone(),
                        parameters: params_pretty,
                        status: ToolCallStatus::Started,
                        result: None,
                        error: None,
                    };
                    tool_call_state.archived_tool_calls.push(AnchoredToolCall {
                        anchor_index,
                        tool_call: info,
                    });
                    archived_indices.insert(call.id.clone(), tool_call_state.archived_tool_calls.len() - 1);
                }
            }
        }
        
        // Update context usage cache for this conversation
        self.update_context_usage_cache(conversation.id, storage, config, prompt_manager);
    }

    /// Get context usage for a conversation (from cache or calculate)
    pub fn get_context_usage(&self, conversation_id: Uuid) -> Option<u32> {
        self.context_usage_cache.get(&conversation_id).copied().flatten()
    }

    /// Clear current conversation (but keep in recent list)
    pub fn clear_current(&mut self) {
        self.current_conversation_id = None;
        self.messages.clear();
    }

    /// Add a message to the current conversation
    pub fn add_message(&mut self, message: ChatMessage) {
        self.messages.push(message);
    }

    /// Check if a conversation is currently loaded
    pub fn is_conversation_loaded(&self, conversation_id: Uuid) -> bool {
        self.current_conversation_id == Some(conversation_id)
    }
}

impl Default for ConversationState {
    fn default() -> Self {
        Self::new()
    }
}

