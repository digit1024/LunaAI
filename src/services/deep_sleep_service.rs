//! Deep Sleep Service: periodic background memory maintenance
//!
//! Three sequential steps per cycle:
//! 1. Summarize conversations with new messages into a compact session digest
//! 2. Evaluate ALL existing memories against the digest (UPDATE/DELETE/KEEP)
//! 3. Extract NEW memories from the digest that aren't already stored

use crate::config::DeepSleepConfig;
use crate::embeddings::EmbeddingProvider;
use crate::llm::{LlmClient, Message as LlmMessage, Role};
use crate::storage::Storage;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing;

const STATE_LAST_RUN_AT: &str = "last_run_at";
const STATE_LAST_PROCESSED_MSG_ID: &str = "last_processed_message_id";

/// Maximum memories to fetch when listing all (for reorganize)
const REORGANIZE_MEMORY_LIMIT: usize = 100_000;

/// Reorganize the memory_vec index: delete all vector rows, then re-embed and re-insert
/// every memory from the `memory` table. Requires embedding to be configured.
pub async fn reorganize_memory_vectors(
    storage: Arc<Mutex<Storage>>,
    embedding_provider: Arc<dyn EmbeddingProvider>,
) -> Result<()> {
    tracing::info!("Reorganize: starting memory vector rebuild");

    let memories = {
        let guard = storage.lock().await;
        guard.delete_all_memory_vec_rows().context("Failed to delete memory_vec rows")?;
        guard.list_memory(REORGANIZE_MEMORY_LIMIT).context("Failed to list memories")?
    };

    tracing::info!(count = memories.len(), "Reorganize: deleted old vectors, re-embedding memories");

    let mut inserted = 0usize;
    let mut failed = 0usize;

    for entry in &memories {
        match embedding_provider.embed(&entry.content).await {
            Ok(embedding) => {
                let guard = storage.lock().await;
                if let Err(e) = guard.insert_memory_vec_row(entry.id, &embedding) {
                    tracing::warn!(id = entry.id, error = %e, "Reorganize: failed to insert vector");
                    failed += 1;
                } else {
                    inserted += 1;
                }
            }
            Err(e) => {
                tracing::warn!(id = entry.id, error = %e, "Reorganize: embedding failed");
                failed += 1;
            }
        }
        // Small delay to avoid rate limits on embedding API
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    tracing::info!(
        inserted,
        failed,
        total = memories.len(),
        "Reorganize: memory vector rebuild complete"
    );

    Ok(())
}

// ── JSON response types for LLM outputs ──

#[derive(Debug, Deserialize)]
struct MemoryEvaluation {
    id: i64,
    action: String,
    content: Option<String>,
    importance: Option<i32>,
    #[allow(dead_code)]
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NewMemory {
    content: String,
    category: Option<String>,
    importance: Option<i32>,
}

/// Run a full deep sleep cycle: processes ALL unprocessed conversations
/// by looping step1→step2→step3 in batches until caught up.
pub async fn run_deep_sleep_cycle(
    storage: Arc<Mutex<Storage>>,
    config: &DeepSleepConfig,
    llm_client: Arc<dyn LlmClient>,
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
) -> Result<()> {
    let start = std::time::Instant::now();
    tracing::info!("Deep Sleep: starting cycle");

    let mut last_processed_id: i64 = {
        let guard = storage.lock().await;
        guard
            .get_deep_sleep_state(STATE_LAST_PROCESSED_MSG_ID)
            .unwrap_or(None)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    };

    tracing::info!(last_processed_id, "Deep Sleep: resuming from message ID");

    let mut batch_num = 0u32;
    let mut total_conversations = 0usize;

    loop {
        batch_num += 1;

        // ── STEP 1: Summarize next batch of conversations ──
        let (session_digest, max_msg_id) = step1_summarize_conversations(
            &storage,
            config,
            &llm_client,
            last_processed_id,
        )
        .await?;

        if session_digest.is_empty() {
            if batch_num == 1 {
                tracing::info!("Deep Sleep: no new conversations to process");
            } else {
                tracing::info!(
                    batches = batch_num - 1,
                    total_conversations,
                    "Deep Sleep: all conversations processed"
                );
            }
            break;
        }

        // Count conversations in this batch (from step1 internals)
        let batch_convos = session_digest.matches("--- Conversation:").count();
        total_conversations += batch_convos;

        tracing::info!(
            batch = batch_num,
            conversations = batch_convos,
            digest_len = session_digest.len(),
            watermark = max_msg_id,
            "Deep Sleep: batch digest built"
        );

        // ── STEP 2: Evaluate existing memories against this batch's digest ──
        step2_evaluate_memories(
            &storage,
            config,
            &llm_client,
            &session_digest,
            embedding_provider.as_ref(),
        )
        .await?;

        // ── STEP 3: Extract new memories from this batch's digest ──
        step3_extract_new_memories(
            &storage,
            config,
            &llm_client,
            &session_digest,
            embedding_provider.as_ref(),
        )
        .await?;

        // ── Advance watermark after successful batch ──
        last_processed_id = max_msg_id;
        {
            let guard = storage.lock().await;
            guard.set_deep_sleep_state(STATE_LAST_PROCESSED_MSG_ID, &max_msg_id.to_string())?;
        }

        tracing::info!(
            batch = batch_num,
            new_watermark = max_msg_id,
            "Deep Sleep: batch complete, watermark advanced"
        );
    }

    // Update last_run_at
    {
        let guard = storage.lock().await;
        guard.set_deep_sleep_state(STATE_LAST_RUN_AT, &chrono::Utc::now().timestamp().to_string())?;
    }

    tracing::info!(
        elapsed_secs = start.elapsed().as_secs(),
        batches = batch_num,
        total_conversations,
        "Deep Sleep: cycle completed"
    );

    Ok(())
}

// ── STEP 1: Summarize Conversations ──

async fn step1_summarize_conversations(
    storage: &Arc<Mutex<Storage>>,
    config: &DeepSleepConfig,
    llm_client: &Arc<dyn LlmClient>,
    last_processed_id: i64,
) -> Result<(String, i64)> {
    let conversations = {
        let guard = storage.lock().await;
        guard.get_conversations_with_messages_after(last_processed_id, config.max_conversations_per_run)?
    };

    if conversations.is_empty() {
        return Ok((String::new(), last_processed_id));
    }

    tracing::info!(
        count = conversations.len(),
        "Deep Sleep Step 1: summarizing conversations"
    );

    let mut digest_parts: Vec<String> = Vec::new();
    let mut max_msg_id = last_processed_id;

    for conversation in &conversations {
        // Load messages for this conversation
        let messages = {
            let guard = storage.lock().await;
            guard.load_conversation_messages(&conversation.id)?
        };

        if messages.is_empty() {
            continue;
        }

        // Track max message ID
        if let Some(last) = messages.last() {
            if last.id > max_msg_id {
                max_msg_id = last.id;
            }
        }

        // Build conversation text: use summary + post-summary messages, user/assistant only
        let conversation_text = build_conversation_text(&messages);
        if conversation_text.is_empty() {
            continue;
        }

        // LLM call to summarize
        let summary = call_llm_summarize(llm_client, &conversation_text, &conversation.title).await;
        match summary {
            Ok(s) if !s.is_empty() => {
                digest_parts.push(format!(
                    "--- Conversation: {} ---\n{}",
                    conversation.title, s
                ));
            }
            Ok(_) => {
                tracing::debug!(conv = %conversation.id, "Deep Sleep: empty summary, skipping");
            }
            Err(e) => {
                tracing::warn!(conv = %conversation.id, error = %e, "Deep Sleep: failed to summarize");
            }
        }

        // Delay between calls
        if config.inter_call_delay_secs > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(config.inter_call_delay_secs)).await;
        }
    }

    Ok((digest_parts.join("\n\n"), max_msg_id))
}

/// Build a text representation of a conversation suitable for LLM summarization.
/// Uses summary message as anchor if available, then appends non-summarized user/assistant messages.
fn build_conversation_text(
    messages: &[crate::storage::sqlite_storage_simple::Message],
) -> String {
    let mut parts: Vec<String> = Vec::new();

    for msg in messages {
        // Skip tool messages entirely
        if msg.role == "tool" {
            continue;
        }
        // Skip assistant messages that are just tool calls with no content
        if msg.role == "assistant" && msg.content.trim().is_empty() {
            continue;
        }

        let role_label = match msg.role.as_str() {
            "user" => "User",
            "assistant" => "Assistant",
            "system" if msg.is_summary => "Previous Summary",
            "system" => continue, // skip system prompts
            _ => continue,
        };

        let content = msg.content.trim();
        if !content.is_empty() {
            // Truncate very long messages to keep context manageable
            let truncated = if content.len() > 500 {
                // Must truncate at char boundary (UTF-8: 'ś' = 2 bytes, emoji = 4 bytes)
                let mut end = 500.min(content.len());
                while end > 0 && !content.is_char_boundary(end) {
                    end -= 1;
                }
                format!("{}...", &content[..end])
            } else {
                content.to_string()
            };
            parts.push(format!("{}: {}", role_label, truncated));
        }
    }

    parts.join("\n")
}

async fn call_llm_summarize(
    llm_client: &Arc<dyn LlmClient>,
    conversation_text: &str,
    title: &str,
) -> Result<String> {
    let system_prompt = "Summarize the key outcomes of this conversation in 2-5 bullet points. \
Focus on: decisions made, facts revealed, user preferences expressed, \
technical details discussed, things the user asked to remember. \
Be specific and factual. Skip greetings and small talk. \
Output ONLY the bullet points, no preamble.";

    let user_message = format!(
        "Conversation title: {}\n\n{}",
        title, conversation_text
    );

    let messages = vec![
        LlmMessage::new(Role::System, system_prompt.to_string()),
        LlmMessage::new(Role::User, user_message),
    ];

    let response = llm_client
        .send_message_with_tools(messages, Vec::new(), Some(0.3), None)
        .await
        .context("Deep Sleep Step 1: LLM summarize call failed")?;

    Ok(response.content.trim().to_string())
}

// ── STEP 2: Evaluate Existing Memories ──

async fn step2_evaluate_memories(
    storage: &Arc<Mutex<Storage>>,
    config: &DeepSleepConfig,
    llm_client: &Arc<dyn LlmClient>,
    session_digest: &str,
    embedding_provider: Option<&Arc<dyn EmbeddingProvider>>,
) -> Result<()> {
    let memories = {
        let guard = storage.lock().await;
        guard.list_memory(1000)?
    };

    if memories.is_empty() {
        tracing::info!("Deep Sleep Step 2: no memories to evaluate");
        return Ok(());
    }

    tracing::info!(
        count = memories.len(),
        batch_size = config.memory_batch_size,
        "Deep Sleep Step 2: evaluating memories"
    );

    let mut updated = 0usize;
    let mut deleted = 0usize;
    let mut kept = 0usize;

    for batch in memories.chunks(config.memory_batch_size) {
        let evaluations = call_llm_evaluate(llm_client, session_digest, batch).await;

        match evaluations {
            Ok(evals) => {
                let guard = storage.lock().await;
                for eval in evals {
                    match eval.action.to_lowercase().as_str() {
                        "delete" => {
                            if let Ok(true) = guard.delete_memory(eval.id) {
                                deleted += 1;
                                tracing::info!(
                                    id = eval.id,
                                    reason = eval.reason.as_deref().unwrap_or("none"),
                                    "Deep Sleep: deleted memory"
                                );
                            }
                        }
                        "update" => {
                            if let Some(new_content) = &eval.content {
                                let importance = eval.importance.unwrap_or(5);
                                // Preserve existing category when LLM doesn't change it
                                let category = batch
                                    .iter()
                                    .find(|m| m.id == eval.id)
                                    .and_then(|m| m.category.as_deref());
                                if let Ok(true) =
                                    guard.update_memory(eval.id, new_content, category, importance)
                                {
                                    updated += 1;
                                    if let Some(provider) = embedding_provider {
                                        if let Ok(embedding) = provider.embed(new_content).await {
                                            if let Err(e) =
                                                guard.update_memory_vec_row(eval.id, &embedding)
                                            {
                                                tracing::warn!(error = %e, "Deep Sleep: failed to update memory vector");
                                            }
                                        }
                                    }
                                    tracing::info!(
                                        id = eval.id,
                                        "Deep Sleep: updated memory"
                                    );
                                }
                            }
                        }
                        "keep" | _ => {
                            kept += 1;
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "Deep Sleep Step 2: failed to evaluate batch");
            }
        }

        if config.inter_call_delay_secs > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(config.inter_call_delay_secs)).await;
        }
    }

    tracing::info!(
        updated, deleted, kept,
        "Deep Sleep Step 2: evaluation complete"
    );

    Ok(())
}

async fn call_llm_evaluate(
    llm_client: &Arc<dyn LlmClient>,
    session_digest: &str,
    memories: &[crate::storage::sqlite_storage_simple::MemoryEntry],
) -> Result<Vec<MemoryEvaluation>> {
    let mut memory_list = String::new();
    for m in memories {
        let cat = m.category.as_deref().unwrap_or("none");
        memory_list.push_str(&format!(
            "- [id:{}] (category: {}, importance: {}) {}\n",
            m.id, cat, m.importance, m.content
        ));
    }

    let system_prompt = "You are a memory maintenance assistant. You are given a digest of recent \
conversations and a batch of existing long-term memories. For EACH memory, decide:\n\
- KEEP: still accurate, no changes needed\n\
- UPDATE: conversation digest reveals corrected/updated info (provide new content + importance 1-10)\n\
- DELETE: contradicted, outdated, or now irrelevant (provide reason)\n\n\
Respond ONLY with a JSON array, no markdown fences:\n\
[{\"id\": 42, \"action\": \"keep\"}, {\"id\": 17, \"action\": \"update\", \"content\": \"...\", \"importance\": 8}, \
{\"id\": 5, \"action\": \"delete\", \"reason\": \"...\"}]";

    let user_message = format!(
        "## Recent Conversation Digest\n{}\n\n## Memories to Evaluate\n{}",
        session_digest, memory_list
    );

    let messages = vec![
        LlmMessage::new(Role::System, system_prompt.to_string()),
        LlmMessage::new(Role::User, user_message),
    ];

    let response = llm_client
        .send_message_with_tools(messages, Vec::new(), Some(0.2), None)
        .await
        .context("Deep Sleep Step 2: LLM evaluate call failed")?;

    parse_json_array::<MemoryEvaluation>(&response.content)
}

// ── STEP 3: Extract New Memories ──

async fn step3_extract_new_memories(
    storage: &Arc<Mutex<Storage>>,
    config: &DeepSleepConfig,
    llm_client: &Arc<dyn LlmClient>,
    session_digest: &str,
    embedding_provider: Option<&Arc<dyn EmbeddingProvider>>,
) -> Result<()> {
    // Load current memories (post-evaluation) for context
    let current_memories = {
        let guard = storage.lock().await;
        guard.list_memory(1000)?
    };

    let new_memories = call_llm_extract(llm_client, session_digest, &current_memories).await;

    match new_memories {
        Ok(proposals) => {
            if proposals.is_empty() {
                tracing::info!(
                    "Deep Sleep Step 3: no new memories proposed by LLM (check RUST_LOG=debug for response preview)"
                );
                return Ok(());
            }

            tracing::info!(
                proposed = proposals.len(),
                "Deep Sleep Step 3: LLM proposed new memories, applying dedup and store"
            );

            let guard = storage.lock().await;
            let mut stored = 0usize;
            let mut skipped = 0usize;

            for proposal in &proposals {
                if proposal.content.trim().is_empty() {
                    continue;
                }

                // FTS5 dedup check: search for similar existing memories
                let keywords: Vec<String> = proposal
                    .content
                    .split_whitespace()
                    .filter(|w| w.len() >= 3)
                    .take(5)
                    .map(|s| s.to_lowercase())
                    .collect();

                if !keywords.is_empty() {
                    if let Ok(existing) = guard.search_memory(&keywords, 3) {
                        let dominated = existing.iter().any(|e| {
                            // Simple similarity: if any existing memory contains most of the
                            // proposed content words, consider it a duplicate
                            let proposed_words: std::collections::HashSet<_> = proposal
                                .content
                                .to_lowercase()
                                .split_whitespace()
                                .filter(|w| w.len() >= 3)
                                .map(|s| s.to_string())
                                .collect();
                            let existing_words: std::collections::HashSet<_> = e
                                .content
                                .to_lowercase()
                                .split_whitespace()
                                .filter(|w| w.len() >= 3)
                                .map(|s| s.to_string())
                                .collect();
                            if proposed_words.is_empty() {
                                return false;
                            }
                            let overlap = proposed_words.intersection(&existing_words).count();
                            (overlap as f32 / proposed_words.len() as f32) > 0.85
                        });

                        if dominated {
                            skipped += 1;
                            tracing::debug!(
                                content = proposal.content.as_str(),
                                "Deep Sleep: skipping as near-duplicate"
                            );
                            continue;
                        }
                    }
                }

                match guard.store_memory(
                    &proposal.content,
                    proposal.category.as_deref(),
                    proposal.importance,
                ) {
                    Ok(entry) => {
                        if let Some(provider) = embedding_provider {
                            if let Ok(embedding) = provider.embed(&proposal.content).await {
                                if let Err(e) = guard.insert_memory_vec_row(entry.id, &embedding) {
                                    tracing::warn!(error = %e, "Deep Sleep: failed to insert memory vector");
                                }
                            }
                        }
                        stored += 1;
                        tracing::info!(
                            id = entry.id,
                            category = entry.category.as_deref().unwrap_or("none"),
                            "Deep Sleep: stored new memory"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Deep Sleep: failed to store new memory");
                    }
                }
            }

            tracing::info!(
                stored, skipped,
                total_proposed = proposals.len(),
                "Deep Sleep Step 3: extraction complete"
            );
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                "Deep Sleep Step 3: failed to extract new memories (parse or LLM error)"
            );
        }
    }

    // Delay after extraction
    if config.inter_call_delay_secs > 0 {
        tokio::time::sleep(std::time::Duration::from_secs(config.inter_call_delay_secs)).await;
    }

    Ok(())
}

async fn call_llm_extract(
    llm_client: &Arc<dyn LlmClient>,
    session_digest: &str,
    current_memories: &[crate::storage::sqlite_storage_simple::MemoryEntry],
) -> Result<Vec<NewMemory>> {
    let mut memory_list = String::new();
    for m in current_memories {
        let cat = m.category.as_deref().unwrap_or("none");
        memory_list.push_str(&format!("- [{}] {} (importance: {})\n", cat, m.content, m.importance));
    }

    let system_prompt = "You are a memory extraction assistant. You are given a digest of recent \
conversations and the current set of long-term memories.\n\n\
Identify NEW facts, preferences, or knowledge from the digest that are NOT already covered by \
existing memories. Focus on: user preferences, technical setup, important people/projects, events or places where the user has been, \
decisions made or things the user asked to remember.\n\n\
If this is a generic conversation then maybe it's not relevant to create new memories ( like generic knowledge question), but if the converastion is about event or place then its' relevant to store memory. 
When the digest contains notable information, prefer suggesting 1-5 new memory candidates. \
Return empty array [] only when there is truly nothing new or the digest is just small talk.\n\n\
Do NOT duplicate existing memories. Respond ONLY with a JSON array, no markdown fences or preamble:\n\
[{\"content\": \"...\", \"category\": \"optional\", \"importance\": 1-10}]\n";

    let user_message = format!(
        "## Recent Conversation Digest\n{}\n\n## Current Memories\n{}",
        session_digest,
        if memory_list.is_empty() {
            "(no memories stored yet)".to_string()
        } else {
            memory_list
        }
    );

    let messages = vec![
        LlmMessage::new(Role::System, system_prompt.to_string()),
        LlmMessage::new(Role::User, user_message),
    ];

    let response = llm_client
        .send_message_with_tools(messages, Vec::new(), Some(0.3), None)
        .await
        .context("Deep Sleep Step 3: LLM extract call failed")?;

    let parsed = parse_json_array::<NewMemory>(&response.content);
    if let Ok(ref arr) = parsed {
        if arr.is_empty() {
            let preview = response.content.trim();
            let preview_len = preview.len().min(300);
            tracing::debug!(
                response_len = response.content.len(),
                response_preview = %preview.chars().take(preview_len).collect::<String>(),
                "Deep Sleep Step 3: LLM returned empty or no parseable array"
            );
        }
    }
    parsed
}

// ── Helpers ──

/// Parse a JSON array from LLM response text.
/// Handles common issues: markdown fences, trailing text, etc.
fn parse_json_array<T: serde::de::DeserializeOwned>(text: &str) -> Result<Vec<T>> {
    let trimmed = text.trim();

    // Strip markdown code fences if present
    let json_str = if trimmed.starts_with("```") {
        let inner = trimmed
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        inner
    } else {
        trimmed
    };

    // Find the JSON array boundaries
    let start = json_str.find('[');
    let end = json_str.rfind(']');

    match (start, end) {
        (Some(s), Some(e)) if s < e => {
            let array_str = &json_str[s..=e];
            serde_json::from_str(array_str)
                .context("Failed to parse LLM JSON response")
        }
        _ => {
            tracing::warn!(
                response_preview = &json_str[..json_str.len().min(200)],
                "Deep Sleep: LLM response does not contain a JSON array"
            );
            Ok(Vec::new())
        }
    }
}

/// Check if deep sleep is due based on last_run_at and interval.
pub fn is_due(storage: &Storage, interval_hours: u64) -> bool {
    let last_run: Option<i64> = storage
        .get_deep_sleep_state(STATE_LAST_RUN_AT)
        .ok()
        .flatten()
        .and_then(|s| s.parse().ok());

    match last_run {
        None => true, // never run before
        Some(ts) => {
            let now = chrono::Utc::now().timestamp();
            let interval_secs = (interval_hours * 3600) as i64;
            now - ts >= interval_secs
        }
    }
}
