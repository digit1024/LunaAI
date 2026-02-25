//! Memory RAG: Automatic retrieval of relevant long-term memories
//!
//! Searches the `memory` table via vector similarity (sqlite-vec) using embeddings
//! of the user's message. Requires an embedding provider to be configured.
//! Tracks which memory IDs have already been injected per conversation to avoid duplication.

use crate::embeddings::EmbeddingProvider;
use crate::storage::Storage;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing;

/// Maximum number of memories to retrieve per query
const MAX_MEMORIES: usize = 10;

/// Search memories relevant to the user message via vector similarity, filter out already-used IDs,
/// and return a formatted system message string plus the list of newly used memory IDs.
/// Caller should persist the returned IDs via storage.record_memory_recalls() and use
/// storage.get_recalled_memory_ids() to seed used_ids after restart.
///
/// Returns `None` if embedding provider is not configured, no new relevant memories are found,
/// or embedding/search fails.
///
/// Note: Takes Arc<Mutex<Storage>> so we can await embedding without holding the lock (Send requirement).
pub async fn retrieve_memory_context(
    storage: Arc<Mutex<Storage>>,
    user_message: &str,
    used_ids: &mut HashSet<i64>,
    embedding_provider: Option<&dyn EmbeddingProvider>,
) -> Option<(String, Vec<i64>)> {
    let provider = match embedding_provider {
        Some(p) => p,
        None => {
            tracing::debug!("Memory RAG: no embedding provider configured, skipping recall");
            return None;
        }
    };

    let query_embedding = match provider.embed(user_message).await {
        Ok(emb) => emb,
        Err(e) => {
            tracing::warn!(error = %e, "Memory RAG: embedding failed");
            return None;
        }
    };

    let entries = {
        let guard = storage.lock().await;
        match guard.search_memory_by_vector(&query_embedding, MAX_MEMORIES) {
            Ok(entries) => entries,
            Err(e) => {
                tracing::warn!(error = %e, "Memory RAG: vector search failed");
                return None;
            }
        }
    };

    if entries.is_empty() {
        tracing::debug!("Memory RAG: no matching memories found");
        return None;
    }

    // Filter out memories already injected in this conversation
    let new_entries: Vec<_> = entries
        .into_iter()
        .filter(|e| !used_ids.contains(&e.id))
        .collect();

    if new_entries.is_empty() {
        tracing::debug!("Memory RAG: all matching memories already injected (dedup)");
        return None;
    }

    // Build formatted system message
    let mut lines = Vec::with_capacity(new_entries.len() + 2);
    lines.push("[Relevant memories from past conversations]".to_string());

    for entry in &new_entries {
        let category_tag = entry
            .category
            .as_deref()
            .map(|c| format!(" [{}]", c))
            .unwrap_or_default();
        lines.push(format!(
            "- (id:{}) {}{} (importance: {})",
            entry.id,
            truncate_content(&entry.content, 300),
            category_tag,
            entry.importance,
        ));
    }

    lines.push("[End of memories - use these only when relevant to the user's question]".to_string());

    let new_ids: Vec<i64> = new_entries.iter().map(|e| e.id).collect();
    for &id in &new_ids {
        used_ids.insert(id);
    }

    tracing::info!(
        new_memories = new_entries.len(),
        total_used = used_ids.len(),
        "Memory RAG: injecting memories into context"
    );

    Some((lines.join("\n"), new_ids))
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

    #[test]
    fn test_truncate_content() {
        assert_eq!(truncate_content("short", 10), "short");
        assert_eq!(truncate_content("longer text here", 6), "longer...");
    }
}
