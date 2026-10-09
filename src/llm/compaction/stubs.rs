//! Cheap tool-result pruning (no LLM call).
//!
//! Verbose file/terminal/search output dominates old-token usage. Replacing the
//! bulky results with a one-line stub before the boundary walk lets more of the
//! conversation survive; the stub keeps a recovery pointer.

use crate::llm::{Message, Role};

/// Replacement text for a pruned tool result.
pub const TOOL_STUB: &str =
    "[Old tool output cleared to save context — use search_history to recover specifics]";

/// Only stub results large enough to matter.
const MIN_STUB_CHARS: usize = 200;

/// Replace bulky tool results older than the protected tail with a stub.
///
/// `protect_last_n` counts from the end; results within it are left verbatim so
/// a tool round the model has not answered keeps its output.
pub fn prune_old_tool_results(messages: &mut [Message], protect_last_n: usize) {
    let keep_from = messages.len().saturating_sub(protect_last_n);
    for msg in messages.iter_mut().take(keep_from) {
        if msg.role == Role::Tool && msg.content.chars().count() > MIN_STUB_CHARS {
            msg.content = TOOL_STUB.to_string();
        }
    }
}
