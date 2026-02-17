//! Message conversion service
//!
//! Single source of truth for converting between database storage format
//! and LLM message format. Handles:
//! - Orphaned tool call filtering
//! - Summarized message filtering
//! - Tool result JSON combination
//! - Tool call metadata preservation

use crate::llm::{Message as LlmMessage, Role};
use crate::storage::sqlite_storage_simple::Message as StorageMessage;
use tracing::{debug, warn};

/// Message converter service (stateless)
pub struct MessageConverter;

impl MessageConverter {
    /// Convert storage messages to LLM messages
    ///
    /// This is the single implementation that replaces duplicated logic in:
    /// - `app.rs:1344-1454` (SendMessage handler)
    /// - `app.rs:1574-1644` (After summarization)
    /// - `app.rs:641-727` (rebuild_conversation_view)
    /// - `server/handlers.rs:824-953` (build_llm_messages)
    ///
    /// # Arguments
    /// - `db_messages`: Messages from database
    /// - `skip_summarized`: If true, skip messages that have been summarized (but keep summary messages)
    ///
    /// # Returns
    /// Vector of LLM messages ready for API calls
    pub fn db_to_llm(
        db_messages: &[StorageMessage],
        skip_summarized: bool,
    ) -> Vec<LlmMessage> {
        // First pass: collect valid tool_call_ids only from assistant messages we'll KEEP
        // (If we skip summarized assistants, don't count their tool_calls — otherwise we'd
        // emit tool results without the preceding assistant message, causing API errors)
        let mut valid_tool_call_ids = std::collections::HashSet::new();
        for msg in db_messages {
            if msg.role == "assistant"
                && !(skip_summarized && msg.is_summarized && !msg.is_summary)
            {
                if let Some(ref tool_calls) = msg.tool_calls {
                    for tc in tool_calls {
                        valid_tool_call_ids.insert(tc.id.clone());
                    }
                }
            }
        }

        // Second pass: build messages, skipping orphaned tool results and summarized messages
        let mut skipped_orphans = 0;
        let mut skipped_summarized = 0;
        let mut llm_messages = Vec::new();

        for msg in db_messages {
            // Skip messages that have been summarized (but keep summary messages themselves)
            if skip_summarized && msg.is_summarized && !msg.is_summary {
                skipped_summarized += 1;
                continue;
            }

            let role = match msg.role.as_str() {
                "user" => Role::User,
                "assistant" => Role::Assistant,
                "system" => Role::System,
                "tool" => {
                    // Check if this tool result has a matching tool_call
                    if let Some(ref tool_call_id) = msg.tool_call_id {
                        if !valid_tool_call_ids.contains(tool_call_id) {
                            skipped_orphans += 1;
                            continue; // Skip orphaned tool result
                        }
                    } else {
                        skipped_orphans += 1;
                        continue; // No tool_call_id, skip
                    }
                    Role::Tool
                }
                _ => continue,
            };

            // For tool messages, combine content with tool_result_json
            let content = if role == Role::Tool {
                let mut combined = msg.content.clone();
                if let Some(ref result_json) = msg.tool_result_json {
                    if !combined.is_empty() {
                        combined.push_str("\n");
                    }
                    combined.push_str(&result_json.to_string());
                }
                combined
            } else {
                msg.content.clone()
            };

            let mut llm_msg = LlmMessage::new(role.clone(), content);

            // Preserve tool call metadata
            if role == Role::Tool {
                llm_msg.tool_call_id = msg.tool_call_id.clone();
            }
            if let Some(ref tool_calls) = msg.tool_calls {
                llm_msg.tool_calls = Some(
                    tool_calls
                        .iter()
                        .map(|tc| crate::llm::ToolCall {
                            id: tc.id.clone(),
                            name: tc.name.clone(),
                            parameters: tc.parameters.clone(),
                        })
                        .collect(),
                );
            }
            llm_msg.reasoning_content = msg.reasoning_content.clone();

            llm_messages.push(llm_msg);
        }

        if skipped_orphans > 0 {
            warn!(
                skipped_count = skipped_orphans,
                "Skipped orphaned tool results (no matching tool_call)"
            );
        }
        if skipped_summarized > 0 {
            debug!(
                skipped_count = skipped_summarized,
                "Skipped summarized messages (using summaries instead)"
            );
        }

        llm_messages
    }

    /// Convert UI chat messages to LLM messages (fallback when DB is unavailable)
    ///
    /// This is a simple conversion for UI-only messages (no tool calls, no metadata).
    /// Note: This requires access to ChatMessage type, which may create a circular dependency.
    /// Consider moving ChatMessage to a shared types module if this is needed.
    #[allow(dead_code)] // Will be used when we refactor app.rs
    pub fn ui_messages_to_llm_simple(
        messages: &[(bool, String, Option<String>)], // (is_user, content, reasoning_content)
    ) -> Vec<LlmMessage> {
        messages
            .iter()
            .map(|(is_user, content, reasoning_content)| {
                let role = if *is_user {
                    Role::User
                } else {
                    Role::Assistant
                };
                let mut llm_msg = LlmMessage::new(role, content.clone());
                llm_msg.reasoning_content = reasoning_content.clone();
                llm_msg
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::ToolCall;

    #[test]
    fn test_orphaned_tool_result_filtering() {
        let db_messages = vec![
            StorageMessage {
                id: 1,
                conversation_id: "test".to_string(),
                role: "assistant".to_string(),
                content: "I'll call a tool".to_string(),
                tool_calls: Some(vec![ToolCall {
                    id: "call_1".to_string(),
                    name: "test_tool".to_string(),
                    parameters: serde_json::json!({}),
                }]),
                tool_call_id: None,
                tool_name: None,
                tool_status: None,
                tool_params_json: None,
                tool_result_json: None,
                embedding: None,
                created_at: 0,
                reasoning_content: None,
                is_summary: false,
                is_summarized: false,
                summarized_message_ids: None,
                summarized_count: None,
            },
            StorageMessage {
                id: 2,
                conversation_id: "test".to_string(),
                role: "tool".to_string(),
                content: "result".to_string(),
                tool_calls: None,
                tool_call_id: Some("call_1".to_string()), // Valid - matches call_1
                tool_name: Some("test_tool".to_string()),
                tool_status: Some("ok".to_string()),
                tool_params_json: None,
                tool_result_json: None,
                embedding: None,
                created_at: 0,
                reasoning_content: None,
                is_summary: false,
                is_summarized: false,
                summarized_message_ids: None,
                summarized_count: None,
            },
            StorageMessage {
                id: 3,
                conversation_id: "test".to_string(),
                role: "tool".to_string(),
                content: "orphaned".to_string(),
                tool_calls: None,
                tool_call_id: Some("orphaned_call".to_string()), // Invalid - no matching call
                tool_name: Some("test_tool".to_string()),
                tool_status: Some("ok".to_string()),
                tool_params_json: None,
                tool_result_json: None,
                embedding: None,
                created_at: 0,
                reasoning_content: None,
                is_summary: false,
                is_summarized: false,
                summarized_message_ids: None,
                summarized_count: None,
            },
        ];

        let llm_messages = MessageConverter::db_to_llm(&db_messages, false);

        // Should have 2 messages: assistant + valid tool result (orphaned one filtered)
        assert_eq!(llm_messages.len(), 2);
        assert_eq!(llm_messages[0].role, Role::Assistant);
        assert_eq!(llm_messages[1].role, Role::Tool);
        assert_eq!(llm_messages[1].content, "result");
    }

    #[test]
    fn test_summarized_message_filtering() {
        let db_messages = vec![
            StorageMessage {
                id: 1,
                conversation_id: "test".to_string(),
                role: "user".to_string(),
                content: "Message 1".to_string(),
                tool_calls: None,
                tool_call_id: None,
                tool_name: None,
                tool_status: None,
                tool_params_json: None,
                tool_result_json: None,
                embedding: None,
                created_at: 0,
                reasoning_content: None,
                is_summary: false,
                is_summarized: true, // Summarized
                summarized_message_ids: None,
                summarized_count: None,
            },
            StorageMessage {
                id: 2,
                conversation_id: "test".to_string(),
                role: "assistant".to_string(),
                content: "Summary of previous messages".to_string(),
                tool_calls: None,
                tool_call_id: None,
                tool_name: None,
                tool_status: None,
                tool_params_json: None,
                tool_result_json: None,
                embedding: None,
                created_at: 0,
                reasoning_content: None,
                is_summary: true, // This is the summary itself
                is_summarized: false,
                summarized_message_ids: None,
                summarized_count: Some(1),
            },
        ];

        let llm_messages = MessageConverter::db_to_llm(&db_messages, true);

        // Should have 1 message: the summary (summarized message filtered)
        assert_eq!(llm_messages.len(), 1);
        assert_eq!(llm_messages[0].role, Role::Assistant);
        assert!(llm_messages[0].content.contains("Summary"));
    }
}

