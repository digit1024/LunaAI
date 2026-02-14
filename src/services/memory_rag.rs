//! Memory RAG: Automatic retrieval of relevant long-term memories
//!
//! Searches the `memory` table via FTS5 using keywords extracted from
//! the user's message, then formats matching entries as a system message
//! for injection into the LLM context. Tracks which memory IDs have
//! already been injected per conversation to avoid duplication.

use crate::storage::Storage;
use std::collections::HashSet;
use tracing;

/// Minimum word length to include in the FTS5 query (skip "a", "is", "the", etc.)
const MIN_KEYWORD_LEN: usize = 3;

/// Maximum number of memories to retrieve per query
const MAX_MEMORIES: usize = 10;

/// Extract search keywords from user message text.
///
/// Splits on whitespace/punctuation, lowercases, and filters out short words.
/// Returns a deduplicated list suitable for FTS5 OR queries.
fn extract_keywords(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    text.split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
        .map(|w| w.trim().to_lowercase())
        .filter(|w| w.len() >= MIN_KEYWORD_LEN)
        .filter(|w| seen.insert(w.clone()))
        .collect()
}

/// Search memories relevant to the user message, filter out already-used IDs,
/// and return a formatted system message string plus the set of newly used IDs.
///
/// Returns `None` if no new relevant memories are found.
pub fn retrieve_memory_context(
    storage: &Storage,
    user_message: &str,
    used_ids: &mut HashSet<i64>,
) -> Option<String> {
    let keywords = extract_keywords(user_message);
    if keywords.is_empty() {
        tracing::debug!("Memory RAG: no keywords extracted from user message");
        return None;
    }

    tracing::debug!(keyword_count = keywords.len(), "Memory RAG: searching memories");

    let entries = match storage.search_memory(&keywords, MAX_MEMORIES) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!(error = %e, "Memory RAG: FTS5 search failed");
            return None;
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

    // Record newly used IDs
    for entry in &new_entries {
        used_ids.insert(entry.id);
    }

    tracing::info!(
        new_memories = new_entries.len(),
        total_used = used_ids.len(),
        "Memory RAG: injecting memories into context"
    );

    Some(lines.join("\n"))
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
    fn test_extract_keywords_basic() {
        let keywords = extract_keywords("How is the weather in Warsaw today?");
        assert!(keywords.contains(&"how".to_string()));
        assert!(keywords.contains(&"weather".to_string()));
        assert!(keywords.contains(&"warsaw".to_string()));
        assert!(keywords.contains(&"today".to_string()));
        // "is", "the", "in" should be filtered (< 3 chars)
        assert!(!keywords.contains(&"is".to_string()));
        assert!(!keywords.contains(&"the".to_string()));
        assert!(!keywords.contains(&"in".to_string()));
    }

    #[test]
    fn test_extract_keywords_dedup() {
        let keywords = extract_keywords("test test test different");
        assert_eq!(keywords.iter().filter(|k| *k == "test").count(), 1);
    }

    #[test]
    fn test_truncate_content() {
        assert_eq!(truncate_content("short", 10), "short");
        assert_eq!(truncate_content("longer text here", 6), "longer...");
    }
}
