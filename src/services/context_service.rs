//! Context management service
//!
//! Unified context management for both desktop and server.
//! Handles:
//! - Token counting
//! - Context truncation
//! - Summarization
//! - Prompt injection

use crate::config::{LlmProfile, ResolvedProfile};
use crate::llm::{Message as LlmMessage, Role};
use crate::prompts::PromptManager;
use crate::storage::sqlite_storage_simple::Message as StorageMessage;
use anyhow::Result;

/// Returns the range of `db_messages` (ordered by created_at ASC) to summarize.
///
/// The trailing message is excluded **only when it is a `User` turn** (the
/// auto-summarize-on-send case), so the user's question stays verbatim for the LLM. For other
/// tails — assistant replies, tool results, or summary rows produced by on-demand
/// summarization — we summarize the whole history. Excluding an assistant/tool tail would either
/// orphan a tool result (its matching tool_call would be inside the summary) or strand a reply
/// with no preceding context, both of which break the next LLM call.
///
/// - No previous summary: full history (or full history minus the last user turn).
/// - With a previous summary: from that summary (inclusive) to the end of the summarizable range,
///   so the new summary re-summarizes the prior summary plus intervening messages.
/// - `None` when there is nothing new to summarize.
fn range_to_summarize(db_messages: &[StorageMessage]) -> Option<std::ops::Range<usize>> {
    if db_messages.is_empty() {
        return None;
    }

    let keep_last_user = matches!(
        db_messages.last(),
        Some(m) if m.role == "user" && !m.is_summary
    );
    let summarize_end = if keep_last_user {
        db_messages.len() - 1
    } else {
        db_messages.len()
    };
    if summarize_end == 0 {
        return None;
    }

    if let Some(last_summary_idx) = db_messages[..summarize_end]
        .iter()
        .rposition(|m| m.is_summary)
    {
        if last_summary_idx + 1 >= summarize_end {
            return None;
        }
        Some(last_summary_idx..summarize_end)
    } else {
        Some(0..summarize_end)
    }
}

/// `created_at` for a newly inserted summary row.
///
/// - With a kept tail (auto on-send): slot the summary between the last summarized message and the
///   kept user turn so chronological DB order matches LLM context order (summary, then tail).
/// - Without a kept tail (manual summarize of full history): use `now` so the summary sorts at the
///   end of the conversation in DB/UI listings.
fn summary_created_at(db_messages: &[StorageMessage], summarize_range: &std::ops::Range<usize>) -> i64 {
    use chrono::Utc;

    let now = Utc::now().timestamp();
    let Some(last_summarized) = db_messages.get(summarize_range.end.saturating_sub(1)) else {
        return now;
    };
    let Some(kept) = db_messages.get(summarize_range.end) else {
        return now.max(last_summarized.created_at);
    };
    if kept.created_at > last_summarized.created_at {
        kept.created_at.saturating_sub(1).max(last_summarized.created_at)
    } else {
        last_summarized.created_at
    }
}

/// Context service (stateless)
pub struct ContextService;

impl ContextService {
    /// Manually summarize a conversation on demand (user-triggered).
    pub async fn perform_manual_summarization(
        conv_id: uuid::Uuid,
        storage: std::sync::Arc<tokio::sync::Mutex<crate::storage::Storage>>,
        llm_client: &std::sync::Arc<dyn crate::llm::LlmClient>,
        resolved: &ResolvedProfile,
        compact_config: &crate::config::ConversationCompactConfig,
    ) -> Result<String> {
        use anyhow::Context;

        let db_messages = {
            let storage_guard = storage.lock().await;
            storage_guard
                .load_conversation_messages(&conv_id.to_string())
                .context("Failed to load messages from database")?
        };

        let range = match range_to_summarize(&db_messages) {
            Some(r) => r,
            None => {
                return Err(anyhow::anyhow!("No messages to summarize"));
            }
        };
        let msgs_to_summarize: Vec<_> = db_messages[range.clone()].to_vec();

        use crate::services::MessageConverter;
        let llm_msgs_to_summarize = MessageConverter::db_to_llm(&msgs_to_summarize, false);
        if llm_msgs_to_summarize.is_empty() {
            return Err(anyhow::anyhow!("No valid messages to summarize"));
        }

        let summary_msg = crate::llm::context_manager::SmartContextManager::summarize_messages(
            llm_msgs_to_summarize,
            resolved.preset(),
            compact_config,
            llm_client.as_ref(),
        )
        .await
        .context("Failed to generate summary")?;

        {
            let summary_created_at = summary_created_at(&db_messages, &range);
            let storage_guard = storage.lock().await;
            storage_guard
                .perform_summarization(
                    &conv_id.to_string(),
                    &msgs_to_summarize,
                    &summary_msg.content,
                    summary_created_at,
                )
                .context("Failed to save summary to DB")?;
        }

        Ok(summary_msg.content)
    }

    /// Check if automatic summarization is needed and trigger it
    ///
    /// This unifies the automatic summarization trigger logic that's duplicated in:
    /// - `src/ui/handlers/chat.rs` (handle_context_management)
    /// - `src/server/handlers.rs` (handle_send_message)
    ///
    /// # Arguments
    /// - `llm_messages`: Current LLM messages to check token count
    /// - `conv_id`: Conversation ID
    /// - `storage`: Storage instance
    /// - `llm_client`: LLM client for summarization
    /// - `resolved`: Resolved profile (profile + preset) for context limits and LLM
    ///
    /// # Returns
    /// Ok(()) if summarization was triggered or not needed, Err if it failed
    /// Check and trigger summarization (server version - takes Arc<Mutex<Storage>>)
    pub async fn check_and_trigger_summarization(
        llm_messages: &[LlmMessage],
        conv_id: uuid::Uuid,
        storage: std::sync::Arc<tokio::sync::Mutex<crate::storage::Storage>>,
        llm_client: &std::sync::Arc<dyn crate::llm::LlmClient>,
        resolved: &ResolvedProfile,
        compact_config: &crate::config::ConversationCompactConfig,
    ) -> Result<()> {
        Self::check_and_trigger_summarization_impl(
            llm_messages,
            conv_id,
            storage,
            llm_client,
            resolved,
            compact_config,
        ).await
    }

    /// Internal implementation for server (Arc<Mutex<Storage>>)
    async fn check_and_trigger_summarization_impl(
        llm_messages: &[LlmMessage],
        conv_id: uuid::Uuid,
        storage: std::sync::Arc<tokio::sync::Mutex<crate::storage::Storage>>,
        llm_client: &std::sync::Arc<dyn crate::llm::LlmClient>,
        resolved: &ResolvedProfile,
        compact_config: &crate::config::ConversationCompactConfig,
    ) -> Result<()> {
        use anyhow::Context;
        use crate::llm::tokenizer::TokenCounter;

        let preset = resolved.preset();
        let profile = resolved.profile();
        let token_counter = TokenCounter::new(preset);
        let total_tokens: usize = llm_messages
            .iter()
            .map(|msg| token_counter.count_message_tokens(msg))
            .sum();

        let context_limit = token_counter.get_context_limit(preset);
        let summarize_threshold_tokens = token_counter.get_summarize_threshold_tokens(preset, profile);

        tracing::debug!(
            total_tokens,
            usage_percent = (total_tokens as f32 / context_limit as f32 * 100.0),
            context_limit,
            summarize_threshold_tokens,
            "Context usage check"
        );

        // Check if summarization is needed
        if total_tokens <= summarize_threshold_tokens {
            return Ok(());
        }

        tracing::info!(
            total_tokens,
            summarize_threshold_tokens,
            "Summarization threshold exceeded, triggering automatic summarization"
        );

        // Load messages from DB (ordered by created_at ASC)
        let db_messages = {
            let storage_guard = storage.lock().await;
            storage_guard
                .load_conversation_messages(&conv_id.to_string())
                .context("failed to load conversation for summarization")?
        };

        // Summarize history except the last message (kept verbatim for the LLM).
        let range = match range_to_summarize(&db_messages) {
            Some(r) => r,
            None => {
                tracing::debug!(
                    conversation_id = %conv_id,
                    "No messages to summarize (empty or only summary)"
                );
                return Ok(());
            }
        };
        let msgs_to_summarize: Vec<_> = db_messages[range.clone()].to_vec();
        tracing::debug!(
            conversation_id = %conv_id,
            messages_to_summarize = msgs_to_summarize.len(),
            "Will summarize messages (last message kept verbatim)"
        );

        // Convert to LlmMessage for summarization using MessageConverter
        use crate::services::MessageConverter;
        let llm_msgs_to_summarize = MessageConverter::db_to_llm(&msgs_to_summarize, false);

        if llm_msgs_to_summarize.is_empty() {
            tracing::warn!(conversation_id = %conv_id, "No valid messages to summarize");
            return Ok(()); // Not an error, just nothing to do
        }

        // Generate summary
        tracing::debug!(conversation_id = %conv_id, "Generating summary");
        let summary_msg = crate::llm::context_manager::SmartContextManager::summarize_messages(
            llm_msgs_to_summarize,
            preset,
            compact_config,
            llm_client.as_ref(),
        )
        .await
        .context("failed to generate summary")?;

        tracing::info!(
            conversation_id = %conv_id,
            summary_length = summary_msg.content.len(),
            "Summary generated"
        );

        // Perform database summarization (marks all msgs_to_summarize as summarized, including tools)
        {
            let summary_created_at = summary_created_at(&db_messages, &range);
            let storage_guard = storage.lock().await;
            storage_guard
                .perform_summarization(
                    &conv_id.to_string(),
                    &msgs_to_summarize,
                    &summary_msg.content,
                    summary_created_at,
                )
                .context("failed to save summary to database")?;
        }

        tracing::debug!(conversation_id = %conv_id, "Summary saved to database");

        Ok(())
    }

    /// Inject system prompts into message history
    pub fn inject_prompts(
        mut history: Vec<LlmMessage>,
        prompt_manager: &PromptManager,
        profile: &LlmProfile,
    ) -> Result<Vec<LlmMessage>> {
        let mut final_messages = Vec::new();

        // Add system prompt if available
        if let Some(system) = prompt_manager.get_system_prompt() {
            final_messages.push(LlmMessage::new(Role::System, system.to_string()));
        }

        // Add profile prompts in order (after system prompt)
        for path in &profile.prompts {
            let resolved = crate::config::AppConfig::resolve_config_path(path);
            let owned = resolved.to_string_lossy().to_string();
            if let Ok(profile_prompt) = prompt_manager.load_profile_prompt(&owned) {
                final_messages.push(LlmMessage::new(Role::System, profile_prompt));
            }
        }

        final_messages.append(&mut history);
        Ok(final_messages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::sqlite_storage_simple::Message as StorageMessage;

    fn msg(id: i64, role: &str, is_summary: bool, is_summarized: bool) -> StorageMessage {
        StorageMessage {
            id,
            conversation_id: "c".to_string(),
            role: role.to_string(),
            content: format!("m{id}"),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            tool_status: None,
            tool_params_json: None,
            tool_result_json: None,
            embedding: None,
            created_at: id,
            reasoning_content: None,
            is_summary,
            is_summarized,
            summarized_message_ids: None,
            summarized_count: None,
            attachments: None,
        }
    }

    #[test]
    fn range_to_summarize_excludes_trailing_user_message() {
        let db = vec![
            msg(1, "user", false, false),
            msg(2, "assistant", false, false),
            msg(3, "user", false, false),
        ];
        assert_eq!(range_to_summarize(&db), Some(0..2));
    }

    #[test]
    fn range_to_summarize_single_user_message_is_none() {
        let db = vec![msg(1, "user", false, false)];
        assert_eq!(range_to_summarize(&db), None);
    }

    #[test]
    fn range_to_summarize_assistant_tail_includes_everything() {
        // On-demand summarization after an assistant reply: summarize everything,
        // do not strand the assistant turn.
        let db = vec![
            msg(1, "user", false, false),
            msg(2, "assistant", false, false),
        ];
        assert_eq!(range_to_summarize(&db), Some(0..2));
    }

    #[test]
    fn range_to_summarize_tool_tail_includes_everything() {
        // Tool result without its assistant in the kept window would be orphaned;
        // summarize the whole chain instead.
        let db = vec![
            msg(1, "user", false, false),
            msg(2, "assistant", false, false),
            msg(3, "tool", false, false),
        ];
        assert_eq!(range_to_summarize(&db), Some(0..3));
    }

    #[test]
    fn range_to_summarize_reroll_includes_prior_summary() {
        let db = vec![
            msg(1, "system", true, false),
            msg(2, "assistant", false, false),
            msg(3, "user", false, false),
        ];
        assert_eq!(range_to_summarize(&db), Some(0..2));
    }

    #[test]
    fn range_to_summarize_only_summary_and_user_tail_is_none() {
        let db = vec![
            msg(1, "system", true, false),
            msg(2, "user", false, false),
        ];
        assert_eq!(range_to_summarize(&db), None);
    }

    #[test]
    fn summary_created_at_sorts_before_kept_tail() {
        let db = vec![
            msg(1, "user", false, true),
            msg(2, "user", false, false),
        ];
        let range = 0..1;
        assert_eq!(summary_created_at(&db, &range), 1);
    }
}

