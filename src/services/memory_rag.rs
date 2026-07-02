//! Memory RAG: automatic recall of relevant long-term memories.
//!
//! Behavior (per user turn, both interactive chat and scheduled tasks):
//!   1. Build a query from the current user message + recent user/assistant turns.
//!   2. Embed the query and run vector search over the `memory` table.
//!   3. Filter by configured `max_distance` and `min_importance`.
//!   4. Cap the resulting set to fit a token budget (`max_memory_tokens`).
//!   5. Strip any previously injected memory block from the message list and
//!      insert a fresh one as a system message just after pinned system prompts.
//!
//! There is intentionally NO session-wide dedup: the same memory may be
//! re-injected on every turn it remains relevant. The previous "inject once
//! per conversation" design caused silent forgetting once the original
//! injection fell out of the context window via summarization or truncation.
//!
//! `record_memory_recalls` / `get_recalled_memory_ids` are still updated for
//! analytics / observability, but are no longer used as a retrieval gate.

use crate::config::EmbeddingConfig;
use crate::embeddings::EmbeddingProvider;
use crate::llm::tokenizer::TokenCounter;
use crate::llm::{Message as LlmMessage, Role};
use crate::storage::Storage;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Marker prefix used to identify (and strip) an injected memory block.
/// Kept stable so prior versions' blocks are recognized too.
pub const MEMORY_BLOCK_PREFIX: &str = "[Relevant memories from past conversations]";
const MEMORY_BLOCK_FOOTER: &str =
    "[End of memories - use these only when relevant to the user's question]";
/// Hard cap for embedding query size so larger history windows don't bloat latency/cost.
const MAX_RECALL_QUERY_CHARS: usize = 4000;

/// Result of a memory RAG injection.
pub struct InjectionOutcome {
    /// Memory IDs included in the injected block (in retrieval order).
    pub ids: Vec<i64>,
    /// Full memory rows included in the injected block.
    pub entries: Vec<crate::storage::sqlite_storage_simple::MemoryEntry>,
}

/// Strip any previously injected memory block from `messages`.
/// A memory block is a system message whose content starts with `MEMORY_BLOCK_PREFIX`.
pub fn strip_memory_block(messages: &mut Vec<LlmMessage>) {
    messages.retain(|m| !is_memory_block(m));
}

fn is_memory_block(msg: &LlmMessage) -> bool {
    matches!(msg.role, Role::System) && msg.content.starts_with(MEMORY_BLOCK_PREFIX)
}

/// Build the recall query from recent dialogue turns in `messages`.
/// Includes up to `query_history_turns + 1` user turns plus assistant context,
/// newest-first, with a hard character cap for stable embedding size.
fn build_query(messages: &[LlmMessage], query_history_turns: usize) -> Option<String> {
    let max_user_turns = query_history_turns.saturating_add(1);
    let max_assistant_turns = query_history_turns.max(1);
    let mut user_count = 0usize;
    let mut assistant_count = 0usize;
    let mut snippets: Vec<String> = Vec::new();

    for msg in messages.iter().rev() {
        let content = msg.content.trim();
        if content.is_empty() {
            continue;
        }

        match msg.role {
            Role::User if user_count < max_user_turns => {
                snippets.push(format!("User: {content}"));
                user_count += 1;
            }
            Role::Assistant if assistant_count < max_assistant_turns => {
                snippets.push(format!("Assistant: {content}"));
                assistant_count += 1;
            }
            _ => {}
        }

        if user_count >= max_user_turns && assistant_count >= max_assistant_turns {
            break;
        }
    }

    if user_count == 0 {
        return None;
    }

    // Newest first reads better for the embedding model: the current question is the anchor.
    let mut query = snippets.join("\n");
    if query.chars().count() > MAX_RECALL_QUERY_CHARS {
        query = query.chars().take(MAX_RECALL_QUERY_CHARS).collect();
    }
    Some(query)
}

/// Find a stable insertion position for the memory block: right after any
/// leading system messages (pinned prompts), before the first non-system message.
fn memory_insert_position(messages: &[LlmMessage]) -> usize {
    messages
        .iter()
        .position(|m| !matches!(m.role, Role::System))
        .unwrap_or(messages.len())
}

/// Validate `min_importance` (1-5). Logs a warning and returns `None` if invalid.
fn validated_min_importance(cfg: &EmbeddingConfig) -> Option<i32> {
    match cfg.min_importance {
        Some(imp) if (1..=5).contains(&imp) => Some(imp),
        Some(imp) => {
            tracing::warn!(
                min_importance = imp,
                "Invalid min_importance, ignoring filter (must be 1-5)"
            );
            None
        }
        None => None,
    }
}

/// Format a memory entry into a single bullet line.
fn format_entry(entry: &crate::storage::sqlite_storage_simple::MemoryEntry) -> String {
    let category_tag = entry
        .category
        .as_deref()
        .map(|c| format!(" [{}]", c))
        .unwrap_or_default();
    format!(
        "- (id:{}) {}{} (importance: {})",
        entry.id,
        truncate_content(&entry.content, 300),
        category_tag,
        entry.importance,
    )
}

/// Build the system message body from a list of selected entries.
fn render_block(entries: &[crate::storage::sqlite_storage_simple::MemoryEntry]) -> String {
    let mut lines = Vec::with_capacity(entries.len() + 2);
    lines.push(MEMORY_BLOCK_PREFIX.to_string());
    for entry in entries {
        lines.push(format_entry(entry));
    }
    lines.push(MEMORY_BLOCK_FOOTER.to_string());
    lines.join("\n")
}

/// Trim entries (from the end of the slice, i.e. lowest similarity first) until the rendered block
/// fits within `max_tokens`. Returns the kept entries (still in retrieval order).
fn fit_to_token_budget(
    entries: Vec<crate::storage::sqlite_storage_simple::MemoryEntry>,
    max_tokens: usize,
    token_counter: &TokenCounter,
) -> Vec<crate::storage::sqlite_storage_simple::MemoryEntry> {
    if entries.is_empty() {
        return entries;
    }
    let mut kept = entries;
    loop {
        if kept.is_empty() {
            return kept;
        }
        let body = render_block(&kept);
        let cost = token_counter.count_tokens(&body);
        if cost <= max_tokens {
            return kept;
        }
        // Drop the least-relevant tail entry.
        kept.pop();
    }
}

/// Search relevant memories and (if any) inject a fresh memory system message into `messages`.
///
/// Always strips any prior memory block from `messages` first (regardless of whether new memories
/// are found), so a stale block from an earlier turn is never duplicated and never lingers.
///
/// Returns `Some(InjectionOutcome)` when a new block was injected; `None` otherwise.
pub async fn inject_memory_block(
    storage: Arc<Mutex<Storage>>,
    embedding_provider: Option<&dyn EmbeddingProvider>,
    embedding_config: &EmbeddingConfig,
    token_counter: &TokenCounter,
    messages: &mut Vec<LlmMessage>,
) -> Option<InjectionOutcome> {
    strip_memory_block(messages);

    let provider = embedding_provider?;

    let query = match build_query(messages, embedding_config.query_history_turns) {
        Some(q) => q,
        None => {
            tracing::debug!("Memory RAG: no user message to query with, skipping");
            return None;
        }
    };

    let query_embedding = match provider.embed(&query).await {
        Ok(emb) => emb,
        Err(e) => {
            tracing::warn!(error = %e, "Memory RAG: embedding failed");
            return None;
        }
    };

    let entries = {
        let guard = storage.lock().await;
        match guard.search_memory_by_vector(
            &query_embedding,
            embedding_config.max_memories,
            embedding_config.max_distance,
        ) {
            Ok(entries) => entries,
            Err(e) => {
                tracing::warn!(error = %e, "Memory RAG: vector search failed");
                return None;
            }
        }
    };

    if entries.is_empty() {
        tracing::info!("Memory RAG: no matching memories found");
        return None;
    }

    let candidate_count = entries.len();

    let filtered: Vec<_> = if let Some(min_imp) = validated_min_importance(embedding_config) {
        entries.into_iter().filter(|e| e.importance >= min_imp).collect()
    } else {
        entries
    };

    if filtered.is_empty() {
        tracing::info!(
            candidates = candidate_count,
            "Memory RAG: all candidates filtered out by min_importance"
        );
        return None;
    }

    let after_importance = filtered.len();

    let kept = if let Some(max_tokens) = embedding_config.max_memory_tokens {
        fit_to_token_budget(filtered, max_tokens, token_counter)
    } else {
        filtered
    };

    if kept.is_empty() {
        tracing::warn!(
            candidates = candidate_count,
            after_importance,
            "Memory RAG: token budget too small for any memory entry"
        );
        return None;
    }

    let body = render_block(&kept);
    let token_cost = token_counter.count_tokens(&body);
    let ids: Vec<i64> = kept.iter().map(|e| e.id).collect();

    let pos = memory_insert_position(messages);
    messages.insert(pos, LlmMessage::new(Role::System, body));

    tracing::info!(
        candidates = candidate_count,
        after_importance,
        injected = ids.len(),
        token_cost,
        "Memory RAG: injected memory block"
    );

    Some(InjectionOutcome {
        ids,
        entries: kept,
    })
}

/// Truncate content to `max_chars`, appending "..." if truncated.
fn truncate_content(content: &str, max_chars: usize) -> String {
    if content.len() <= max_chars {
        content.to_string()
    } else {
        let truncated: String = content.chars().take(max_chars).collect();
        format!("{}...", truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{Message, Role};

    #[test]
    fn test_truncate_content() {
        assert_eq!(truncate_content("short", 10), "short");
        assert_eq!(truncate_content("longer text here", 6), "longer...");
    }

    #[test]
    fn strip_memory_block_removes_only_marker_system_messages() {
        let mut msgs = vec![
            Message::new(Role::System, "you are helpful".into()),
            Message::new(
                Role::System,
                format!("{}\n- (id:1) foo\n{}", MEMORY_BLOCK_PREFIX, MEMORY_BLOCK_FOOTER),
            ),
            Message::new(Role::User, "hi".into()),
        ];
        strip_memory_block(&mut msgs);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].content, "you are helpful");
        assert!(matches!(msgs[1].role, Role::User));
    }

    #[test]
    fn build_query_uses_recent_dialogue_turns_newest_first() {
        let msgs = vec![
            Message::new(Role::User, "first question".into()),
            Message::new(Role::Assistant, "first answer".into()),
            Message::new(Role::User, "second question".into()),
            Message::new(Role::Assistant, "second answer".into()),
            Message::new(Role::User, "third question".into()),
        ];
        let q = build_query(&msgs, 1).expect("query");
        assert!(q.starts_with("User: third question"));
        assert!(q.contains("Assistant: second answer"));
        assert!(q.contains("User: second question"));
        assert!(!q.contains("first question"));
    }

    #[test]
    fn build_query_skips_empty_messages() {
        let msgs = vec![
            Message::new(Role::User, "".into()),
            Message::new(Role::Assistant, "   ".into()),
            Message::new(Role::User, "real question".into()),
        ];
        let q = build_query(&msgs, 2).expect("query");
        assert!(q.contains("User: real question"));
    }

    #[test]
    fn build_query_returns_none_when_no_user_messages() {
        let msgs = vec![Message::new(Role::Assistant, "hi".into())];
        assert!(build_query(&msgs, 2).is_none());
    }

    #[test]
    fn memory_insert_position_after_system_prefix() {
        let msgs = vec![
            Message::new(Role::System, "a".into()),
            Message::new(Role::System, "b".into()),
            Message::new(Role::User, "u".into()),
        ];
        assert_eq!(memory_insert_position(&msgs), 2);

        let only_system = vec![Message::new(Role::System, "a".into())];
        assert_eq!(memory_insert_position(&only_system), 1);

        let no_system = vec![Message::new(Role::User, "u".into())];
        assert_eq!(memory_insert_position(&no_system), 0);
    }
}
