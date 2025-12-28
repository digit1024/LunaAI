//! Unified type system for LunaAI
//!
//! This module provides type conversions and unified types across the codebase.
//! It serves as a bridge between storage layer (which uses strings for roles)
//! and LLM layer (which uses Role enum).

use crate::llm::{Message as LlmMessage, Role};
use crate::storage::sqlite_storage_simple::Message as StorageMessage;
use chrono::{DateTime, Utc};

/// Convert storage message to LLM message
impl From<&StorageMessage> for LlmMessage {
    fn from(storage_msg: &StorageMessage) -> Self {
        let role = Role::from(storage_msg.role.as_str());
        
        let mut llm_msg = match role {
            Role::Tool => {
                let tool_call_id = storage_msg
                    .tool_call_id
                    .clone()
                    .unwrap_or_else(|| "tool_result".to_string());
                let mut content = storage_msg.content.clone();
                // Combine content AND tool_result_json (both may contain data)
                if let Some(ref result_json) = storage_msg.tool_result_json {
                    if !content.is_empty() {
                        content.push_str("\n");
                    }
                    content.push_str(&result_json.to_string());
                }
                LlmMessage::new_tool_result(
                    tool_call_id,
                    content,
                    storage_msg.tool_status.as_deref() == Some("error"),
                )
            }
            Role::Assistant => {
                // Include tool_calls if present on assistant messages
                if let Some(tool_calls) = &storage_msg.tool_calls {
                    if !tool_calls.is_empty() {
                        LlmMessage::new_with_tool_calls(role, storage_msg.content.clone(), tool_calls.clone())
                    } else {
                        LlmMessage::new(role, storage_msg.content.clone())
                    }
                } else {
                    LlmMessage::new(role, storage_msg.content.clone())
                }
            }
            _ => LlmMessage::new(role, storage_msg.content.clone()),
        };
        
        // Preserve reasoning_content from stored message
        llm_msg.reasoning_content = storage_msg.reasoning_content.clone();
        llm_msg.timestamp = Some(DateTime::from_timestamp(storage_msg.created_at, 0).unwrap_or_else(Utc::now));
        
        llm_msg
    }
}

/// Convert LLM message to storage message (for saving)
/// Note: This is a helper function since StorageMessage has additional fields
pub fn llm_message_to_storage(
    llm_msg: &LlmMessage,
    conversation_id: &str,
    message_id: i64,
) -> StorageMessage {
    StorageMessage {
        id: message_id,
        conversation_id: conversation_id.to_string(),
        role: llm_msg.role.as_str().to_string(),
        content: llm_msg.content.clone(),
        embedding: None,
        created_at: llm_msg.timestamp
            .map(|dt| dt.timestamp())
            .unwrap_or_else(|| Utc::now().timestamp()),
        tool_calls: llm_msg.tool_calls.clone(),
        tool_call_id: llm_msg.tool_call_id.clone(),
        tool_name: None,
        tool_status: None,
        tool_params_json: None,
        tool_result_json: None,
        reasoning_content: llm_msg.reasoning_content.clone(),
        is_summary: false,
        is_summarized: false,
        summarized_message_ids: None,
        summarized_count: None,
    }
}

/// Re-export commonly used types for convenience
pub use crate::llm::{Message, ToolCall, Attachment};
pub use crate::storage::sqlite_storage_simple::Conversation as StorageConversation;

