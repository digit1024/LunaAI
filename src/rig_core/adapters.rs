//! Adapters between Luna message/storage types and Rig message types.
//!
//! Pure conversion layer (no I/O). Reusable by HTTP/WebSocket server and
//! scheduled flows.

use crate::llm::{Message as LunaMessage, Role};
use rig::message::Message as RigMessage;

/// Convert Luna LLM messages to Rig chat history.
///
/// - User, Assistant: map directly
/// - System: prepend to preamble (caller must handle); here we skip to avoid duplication
/// - Tool: convert to RigMessage::tool_result
pub fn luna_messages_to_rig_history(messages: &[LunaMessage]) -> Vec<RigMessage> {
    messages
        .iter()
        .filter_map(luna_to_rig_message)
        .collect()
}

/// Convert a single Luna message to Rig format.
pub fn luna_to_rig_message(msg: &LunaMessage) -> Option<RigMessage> {
    match &msg.role {
        Role::User => Some(RigMessage::user(msg.content.clone())),
        Role::Assistant => {
            // Rig assistant can have tool calls; for now we only pass text
            Some(RigMessage::assistant(msg.content.clone()))
        }
        Role::System => None, // Rig uses preamble; caller injects system separately
        Role::Tool => {
            let id = msg.tool_call_id.as_ref()?;
            Some(RigMessage::tool_result(id.clone(), msg.content.clone()))
        }
    }
}
