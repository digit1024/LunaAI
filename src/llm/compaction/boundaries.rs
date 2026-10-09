//! Contiguous head/tail boundary selection.
//!
//! Replaces importance-scored message slicing: the model always sees a
//! contiguous head and a contiguous tail, never an arbitrarily re-ordered
//! subset. A contiguous middle may be dropped by the deterministic safety net;
//! the threshold summarizer normally handles that region first.

use crate::config::ConversationCompactConfig;
use crate::llm::tokenizer::TokenCounter;
use crate::llm::{Message, Role};

/// Head/tail cut points over a message list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Boundaries {
    /// Number of leading messages preserved (leading system messages plus
    /// `protect_first_n` regular messages).
    pub head_end: usize,
    /// Index where the preserved tail begins.
    pub tail_start: usize,
}

impl Boundaries {
    /// True when nothing would be dropped (`head` and `tail` already meet).
    pub fn is_noop(&self) -> bool {
        self.tail_start <= self.head_end
    }

    /// Number of messages that would be dropped from the middle.
    pub fn dropped_len(&self, len: usize) -> usize {
        self.tail_start.min(len).saturating_sub(self.head_end)
    }
}

/// Compute contiguous head/tail boundaries for `messages`.
pub fn compute(
    messages: &[Message],
    counter: &TokenCounter,
    cfg: &ConversationCompactConfig,
    context_limit: usize,
) -> Boundaries {
    let n = messages.len();
    if n == 0 {
        return Boundaries::default();
    }

    // Head: leading system messages plus `protect_first_n` regular messages.
    let mut head_end = 0;
    while head_end < n && messages[head_end].role == Role::System {
        head_end += 1;
    }
    let mut kept = 0;
    while head_end < n && kept < cfg.protect_first_n {
        head_end += 1;
        kept += 1;
    }
    // Never claim the entire list as head: leave at least the last message.
    if head_end >= n {
        head_end = n - 1;
    }

    // Tail: walk backward under a token budget, honoring the minimum message
    // count and the minimum number of real user messages.
    let tail_budget = ((context_limit as f32) * cfg.tail_token_budget_ratio) as usize;
    let mut tail_start = n;
    let mut used = 0usize;
    let mut user_count = 0usize;
    let mut count = 0usize;
    while tail_start > head_end {
        let candidate = tail_start - 1;
        let msg = &messages[candidate];
        let tokens = counter.count_message_tokens(msg);
        let below_minimums =
            count < cfg.protect_last_n || user_count < cfg.min_tail_user_messages;
        if !below_minimums && used + tokens > tail_budget {
            break;
        }
        used += tokens;
        if msg.role == Role::User {
            user_count += 1;
        }
        count += 1;
        tail_start = candidate;
    }

    // Keep a tool result's parent assistant call: a tail must never begin on an
    // orphaned Tool message.
    while tail_start > head_end && messages[tail_start].role == Role::Tool {
        tail_start -= 1;
    }

    // Keep tool groups whole at the head edge too: if the head would end on an
    // assistant that requested tools (its results are in the dropped middle),
    // pull that assistant into the dropped region instead of orphaning the call.
    while head_end > 0
        && messages[head_end - 1]
            .tool_calls
            .as_ref()
            .is_some_and(|calls| !calls.is_empty())
    {
        head_end -= 1;
    }

    // Alignment can only shrink the dropped region.
    if tail_start < head_end {
        tail_start = head_end;
    }

    Boundaries { head_end, tail_start }
}
