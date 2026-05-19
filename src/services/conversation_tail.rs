//! Repair incomplete assistant/tool tails after stop or failed runs.

use crate::storage::sqlite_storage_simple::Message as StorageMessage;
use std::collections::HashSet;

/// If the trailing assistant message has `tool_calls` but not every id has a matching
/// `tool` row, return DB ids to delete (that assistant plus any partial tool rows after it).
/// Only repairs the last incomplete assistant turn in the conversation.
pub fn ids_to_repair_incomplete_tool_tail(db_messages: &[StorageMessage]) -> Vec<i64> {
    if db_messages.is_empty() {
        return Vec::new();
    }

    let last_assistant_idx = match db_messages
        .iter()
        .rposition(|m| m.role == "assistant" && m.tool_calls.as_ref().is_some_and(|t| !t.is_empty()))
    {
        Some(i) => i,
        None => return Vec::new(),
    };

    let assistant = &db_messages[last_assistant_idx];
    let required_ids: HashSet<&str> = assistant
        .tool_calls
        .as_ref()
        .map(|tcs| tcs.iter().map(|tc| tc.id.as_str()).collect())
        .unwrap_or_default();

    if required_ids.is_empty() {
        return Vec::new();
    }

    let mut fulfilled = HashSet::new();
    for msg in db_messages.iter().skip(last_assistant_idx + 1) {
        if msg.role == "tool" {
            if let Some(ref tid) = msg.tool_call_id {
                if required_ids.contains(tid.as_str()) {
                    fulfilled.insert(tid.as_str());
                }
            }
        }
    }

    if fulfilled.len() == required_ids.len() {
        return Vec::new();
    }

    let mut ids = vec![assistant.id];
    for msg in db_messages.iter().skip(last_assistant_idx + 1) {
        if msg.role == "tool" {
            if let Some(ref tid) = msg.tool_call_id {
                if required_ids.contains(tid.as_str()) {
                    ids.push(msg.id);
                }
            }
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::ToolCall;

    fn msg(
        id: i64,
        role: &str,
        tool_calls: Option<Vec<ToolCall>>,
        tool_call_id: Option<&str>,
    ) -> StorageMessage {
        StorageMessage {
            id,
            conversation_id: "c".to_string(),
            role: role.to_string(),
            content: String::new(),
            tool_calls,
            tool_call_id: tool_call_id.map(|s| s.to_string()),
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
            attachments: None,
        }
    }

    #[test]
    fn complete_tool_chain_no_repair() {
        let db = vec![
            msg(
                1,
                "assistant",
                Some(vec![ToolCall {
                    id: "c1".into(),
                    name: "t".into(),
                    parameters: serde_json::json!({}),
                }]),
                None,
            ),
            msg(2, "tool", None, Some("c1")),
        ];
        assert!(ids_to_repair_incomplete_tool_tail(&db).is_empty());
    }

    #[test]
    fn no_assistant_tools_no_repair() {
        let db = vec![msg(1, "user", None, None), msg(2, "assistant", None, None)];
        assert!(ids_to_repair_incomplete_tool_tail(&db).is_empty());
    }

    #[test]
    fn incomplete_tail_returns_assistant_and_partial_tools() {
        let db = vec![
            msg(
                1,
                "assistant",
                Some(vec![
                    ToolCall {
                        id: "c1".into(),
                        name: "a".into(),
                        parameters: serde_json::json!({}),
                    },
                    ToolCall {
                        id: "c2".into(),
                        name: "b".into(),
                        parameters: serde_json::json!({}),
                    },
                ]),
                None,
            ),
            msg(2, "tool", None, Some("c1")),
        ];
        assert_eq!(ids_to_repair_incomplete_tool_tail(&db), vec![1, 2]);
    }
}
