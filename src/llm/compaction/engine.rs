//! Boundary compaction orchestrator: prune → boundaries → assemble. Never
//! reorders messages; a dropped middle becomes one contiguous omission notice
//! carrying a recovery pointer. The threshold summarizer (Phase 1) normally
//! handles the middle first; this is the deterministic safety net.

use crate::config::ConversationCompactConfig;
use crate::llm::compaction::boundaries;
use crate::llm::compaction::stubs;
use crate::llm::tokenizer::TokenCounter;
use crate::llm::{Message, Role};

/// Notice inserted where a contiguous middle was dropped.
pub const OMISSION_NOTICE: &str =
    "[Earlier turns omitted to fit the context window — use search_history to recover specifics]";

pub struct BoundaryCompactor;

impl BoundaryCompactor {
    /// Prune old tool output, compute boundaries, and assemble head + notice +
    /// tail, preserving original order. Returns `messages` unchanged when it
    /// already fits.
    pub fn select(
        messages: Vec<Message>,
        counter: &TokenCounter,
        cfg: &ConversationCompactConfig,
        context_limit: usize,
    ) -> Vec<Message> {
        if messages.is_empty() {
            return messages;
        }

        let mut working = messages;

        // 1. Cheap prune of old bulky tool output.
        stubs::prune_old_tool_results(&mut working, cfg.protect_last_n);

        // 2. Contiguous boundaries.
        let boundaries = boundaries::compute(&working, counter, cfg, context_limit);
        let dropped = boundaries.dropped_len(working.len());
        if boundaries.is_noop() || dropped == 0 {
            return working;
        }

        // 3. Assemble head + notice + tail (original order preserved).
        let role = continuation_role(working.get(boundaries.head_end.wrapping_sub(1)).map(|m| &m.role));
        let notice = Message {
            role,
            content: format!("{OMISSION_NOTICE} ({dropped} earlier message(s) omitted.)"),
            timestamp: None,
            is_prompt: false,
            tool_call_id: None,
            tool_calls: None,
            attachments: None,
            reasoning_content: None,
        };

        let mut result = Vec::with_capacity(working.len() - dropped + 1);
        let mut iter = working.into_iter();
        result.extend(iter.by_ref().take(boundaries.head_end));
        let tail: Vec<Message> = iter.skip(dropped).collect();
        result.push(notice);
        result.extend(tail);
        result
    }

    /// Exposed for tests: the boundary cut for a list without assembly.
    #[cfg(test)]
    pub fn boundaries(
        messages: &[Message],
        counter: &TokenCounter,
        cfg: &ConversationCompactConfig,
        context_limit: usize,
    ) -> boundaries::Boundaries {
        boundaries::compute(messages, counter, cfg, context_limit)
    }
}

/// Pick a role for the inserted notice that differs from the preceding turn.
/// Anthropic merges consecutive same-role turns, but avoiding the immediate
/// collision keeps payloads clean on every provider.
fn continuation_role(head_last: Option<&Role>) -> Role {
    if matches!(head_last, Some(Role::Assistant)) {
        Role::User
    } else {
        Role::Assistant
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ConversationCompactConfig, ModelPreset};
    use crate::llm::ToolCall;

    fn counter() -> TokenCounter {
        TokenCounter::new(&ModelPreset::default())
    }

    fn turn(i: usize, len: usize) -> Vec<Message> {
        vec![
            Message::new(Role::User, format!("u{i} {}", "x".repeat(len))),
            Message::new(Role::Assistant, format!("a{i} {}", "y".repeat(len))),
        ]
    }

    #[test]
    fn under_budget_is_unchanged() {
        let msgs = vec![
            Message::new(Role::System, "sys".into()),
            Message::new(Role::User, "hi".into()),
            Message::new(Role::Assistant, "hello".into()),
        ];
        let out = BoundaryCompactor::select(
            msgs.clone(),
            &counter(),
            &ConversationCompactConfig::default(),
            1_000_000,
        );
        assert_eq!(out.len(), msgs.len());
    }

    #[test]
    fn over_budget_keeps_head_and_tail_with_one_notice() {
        let mut msgs = vec![Message::new(Role::System, "sys".into())];
        for i in 0..40 {
            msgs.extend(turn(i, 400));
        }
        let cfg = ConversationCompactConfig {
            protect_first_n: 2,
            protect_last_n: 4,
            min_tail_user_messages: 1,
            ..Default::default()
        };

        let out = BoundaryCompactor::select(msgs.clone(), &counter(), &cfg, 1000);

        assert!(out.len() < msgs.len());
        assert_eq!(out[0].content, "sys");
        assert_eq!(out.last().unwrap().content, msgs.last().unwrap().content);
        let notices: Vec<_> = out
            .iter()
            .filter(|m| m.content.contains("Earlier turns omitted"))
            .collect();
        assert_eq!(notices.len(), 1, "exactly one contiguous omission notice");
        assert!(!out.iter().any(|m| m.role == Role::Tool));
    }

    #[test]
    fn head_never_ends_on_orphaned_tool_call() {
        // Head would end on the assistant that requested tools; both the call and
        // its results must land on the same side of the cut.
        let msgs = vec![
            Message::new(Role::System, "sys".into()),
            Message::new_with_tool_calls(
                Role::Assistant,
                "calling".into(),
                vec![ToolCall { id: "call_1".into(), name: "t".into(), parameters: serde_json::json!({}) }],
            ),
            Message::new_tool_result("call_1".into(), "r1".into(), false),
            Message::new_tool_result("call_1".into(), "r2".into(), false),
            Message::new(Role::User, "next".into()),
        ];
        let cfg = ConversationCompactConfig {
            protect_first_n: 1,
            protect_last_n: 1,
            min_tail_user_messages: 1,
            ..Default::default()
        };

        let b = BoundaryCompactor::boundaries(&msgs, &counter(), &cfg, 0);
        // Assistant (index 1) and both results dropped together; head is system only.
        assert_eq!(b.head_end, 1);
        assert_eq!(b.tail_start, 4);
    }

    #[test]
    fn tail_never_starts_on_orphaned_tool_result() {
        let msgs = vec![
            Message::new(Role::System, "sys".into()),
            Message::new(Role::User, "ask".into()),
            Message::new_with_tool_calls(
                Role::Assistant,
                "calling".into(),
                vec![ToolCall { id: "call_1".into(), name: "t".into(), parameters: serde_json::json!({}) }],
            ),
            Message::new_tool_result("call_1".into(), "result".into(), false),
        ];
        let cfg = ConversationCompactConfig {
            protect_first_n: 1,
            protect_last_n: 1,
            min_tail_user_messages: 0,
            ..Default::default()
        };

        let b = BoundaryCompactor::boundaries(&msgs, &counter(), &cfg, 0);
        // Tail start pulled back from the Tool to its parent assistant.
        assert_eq!(b.tail_start, 2);
    }

    #[test]
    fn old_bulky_tool_results_are_stubbed() {
        let msgs = vec![
            Message::new(Role::System, "sys".into()),
            Message::new(Role::User, "ask".into()),
            Message::new_tool_result("c1".into(), "z".repeat(5000), false),
            Message::new(Role::Assistant, "done".into()),
        ];
        let cfg = ConversationCompactConfig {
            protect_last_n: 1,
            ..Default::default()
        };
        let out = BoundaryCompactor::select(msgs, &counter(), &cfg, 1_000_000);
        let tool = out.iter().find(|m| m.role == Role::Tool).unwrap();
        assert_eq!(tool.content, super::stubs::TOOL_STUB);
    }
}
