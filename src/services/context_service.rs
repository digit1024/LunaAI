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
use crate::llm::tokenizer::TokenCounter;
use crate::prompts::PromptManager;
use crate::storage::sqlite_storage_simple::Message as StorageMessage;
use anyhow::Result;
use tracing::debug;

/// Returns the range of `db_messages` (ordered by created_at ASC) to summarize:
/// - All messages **after** the last summary message (includes user, assistant, and **tool** messages).
/// - If there is no previous summary, returns the range for **all** messages (first summarization).
/// - Returns `None` if there is nothing to summarize (e.g. only summary message(s) or empty).
///
/// No "keep recent" — we summarize everything since the last summary so tool calls are never excluded.
fn range_to_summarize(db_messages: &[StorageMessage]) -> Option<std::ops::Range<usize>> {
    let start = db_messages
        .iter()
        .rposition(|m| m.is_summary)
        .map(|i| i + 1)
        .unwrap_or(0);
    if start >= db_messages.len() {
        return None;
    }
    Some(start..db_messages.len())
}

/// Context service (stateless)
pub struct ContextService;

impl ContextService {
    /// Prepare context for LLM API call
    ///
    /// # Arguments
    /// - `messages`: Conversation messages
    /// - `resolved`: Resolved profile (profile + preset)
    /// - `prompt_manager`: Prompt manager for system prompts
    pub fn prepare_context(
        &self,
        messages: Vec<LlmMessage>,
        resolved: &ResolvedProfile,
        prompt_manager: &PromptManager,
    ) -> Result<Vec<LlmMessage>> {
        let preset = resolved.preset();
        let profile = resolved.profile();

        let final_messages = Self::inject_prompts(messages, prompt_manager, profile)?;

        let token_counter = TokenCounter::new(preset);
        let total_tokens: usize = final_messages
            .iter()
            .map(|msg| token_counter.count_message_tokens(msg))
            .sum();

        let context_limit = token_counter.get_context_limit(preset);
        let summarize_threshold_tokens = token_counter.get_summarize_threshold_tokens(preset, profile);

        debug!(
            total_tokens = total_tokens,
            context_limit = context_limit,
            summarize_threshold = summarize_threshold_tokens,
            "Context preparation"
        );

        let final_messages = if total_tokens > token_counter.get_safe_context_limit(preset) {
            debug!(
                total_tokens,
                safe_limit = token_counter.get_safe_context_limit(preset),
                "Context exceeds safe limit, applying smart truncation"
            );
            crate::llm::context_manager::SmartContextManager::select_context(
                final_messages,
                &token_counter,
                preset,
            )
        } else {
            final_messages
        };

        let final_tokens: usize = final_messages
            .iter()
            .map(|msg| token_counter.count_message_tokens(msg))
            .sum();
        
        debug!(
            final_tokens,
            message_count = final_messages.len(),
            "Context preparation complete"
        );

        Ok(final_messages)
    }

    /// Perform manual summarization on a conversation
    ///
    /// This unifies summarization logic that's currently in:
    /// - `app.rs` (desktop)
    /// - `server/handlers.rs` (server)
    ///
    /// # Arguments
    /// - `conv_id`: Conversation ID to summarize
    /// - `storage`: Storage instance
    /// - `llm_client`: LLM client for summarization
    /// - `profile`: LLM profile with context limits
    ///
    /// # Returns
    /// Summary message content, or error
    pub async fn perform_manual_summarization(
        conv_id: uuid::Uuid,
        storage: &crate::storage::Storage,
        llm_client: &std::sync::Arc<dyn crate::llm::LlmClient>,
        resolved: &ResolvedProfile,
    ) -> Result<String> {
        use anyhow::Context;

        tracing::debug!(conversation_id = %conv_id, "Manual summarization triggered");
        
        // Load messages from DB (ordered by created_at ASC)
        let db_messages = storage
            .load_conversation_messages(&conv_id.to_string())
            .context("Failed to load messages from database")?;

        // All messages after last summary (or all if first summarization). Includes tool calls.
        let range = match range_to_summarize(&db_messages) {
            Some(r) => r,
            None => {
                tracing::warn!(conversation_id = %conv_id, "No messages to summarize (empty or only summary)");
                return Err(anyhow::anyhow!("No messages to summarize").context("Summarization skipped"));
            }
        };
        let msgs_to_summarize: Vec<_> = db_messages[range].to_vec();
        tracing::debug!(
            conversation_id = %conv_id,
            messages_to_summarize = msgs_to_summarize.len(),
            "Will summarize all messages after last summary"
        );
        
        // Convert to LlmMessage for summarization using MessageConverter (single source of truth)
        use crate::services::MessageConverter;
        let llm_msgs_to_summarize = MessageConverter::db_to_llm(&msgs_to_summarize, false);
        
        if llm_msgs_to_summarize.is_empty() {
            return Err(anyhow::anyhow!("No valid messages to summarize").context("Summarization failed"));
        }

        // Generate summary
        tracing::debug!("Generating summary");
        let summary_msg = crate::llm::context_manager::SmartContextManager::summarize_messages(
            llm_msgs_to_summarize,
            resolved.preset(),
            llm_client.as_ref(),
        )
        .await
        .context("Failed to generate summary")?;
        
        tracing::info!(
            summary_length = summary_msg.content.len(),
            "Summary generated"
        );
        
        // Perform database summarization
        storage
            .perform_summarization(
                &conv_id.to_string(),
                &msgs_to_summarize,
                &summary_msg.content,
            )
            .context("Failed to save summary to DB")?;
        
        tracing::debug!("Summary saved to database");
        
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
    ) -> Result<()> {
        Self::check_and_trigger_summarization_impl(
            llm_messages,
            conv_id,
            storage,
            llm_client,
            resolved,
        ).await
    }

    /// Check and trigger summarization (desktop version - takes &Storage)
    pub async fn check_and_trigger_summarization_desktop(
        llm_messages: &[LlmMessage],
        conv_id: uuid::Uuid,
        storage: &crate::storage::Storage,
        llm_client: &std::sync::Arc<dyn crate::llm::LlmClient>,
        resolved: &ResolvedProfile,
    ) -> Result<()> {
        // For desktop, we can use the storage directly since we're not in a tokio::spawn context
        Self::check_and_trigger_summarization_impl_desktop(
            llm_messages,
            conv_id,
            storage,
            llm_client,
            resolved,
        ).await
    }

    /// Internal implementation for server (Arc<Mutex<Storage>>)
    async fn check_and_trigger_summarization_impl(
        llm_messages: &[LlmMessage],
        conv_id: uuid::Uuid,
        storage: std::sync::Arc<tokio::sync::Mutex<crate::storage::Storage>>,
        llm_client: &std::sync::Arc<dyn crate::llm::LlmClient>,
        resolved: &ResolvedProfile,
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

        // All messages after last summary (or all if first time). Includes tool calls. No "keep recent".
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
        let msgs_to_summarize: Vec<_> = db_messages[range].to_vec();
        tracing::debug!(
            conversation_id = %conv_id,
            messages_to_summarize = msgs_to_summarize.len(),
            "Will summarize all messages after last summary"
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
            let storage_guard = storage.lock().await;
            storage_guard
                .perform_summarization(
                    &conv_id.to_string(),
                    &msgs_to_summarize,
                    &summary_msg.content,
                )
                .context("failed to save summary to database")?;
        }

        tracing::debug!(conversation_id = %conv_id, "Summary saved to database");

        Ok(())
    }

    /// Internal implementation for desktop (&Storage)
    async fn check_and_trigger_summarization_impl_desktop(
        llm_messages: &[LlmMessage],
        conv_id: uuid::Uuid,
        storage: &crate::storage::Storage,
        llm_client: &std::sync::Arc<dyn crate::llm::LlmClient>,
        resolved: &ResolvedProfile,
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
        let db_messages = storage
            .load_conversation_messages(&conv_id.to_string())
            .context("failed to load conversation for summarization")?;

        // All messages after last summary (or all if first time). Includes tool calls. No "keep recent".
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
        let msgs_to_summarize: Vec<_> = db_messages[range].to_vec();
        tracing::debug!(
            conversation_id = %conv_id,
            messages_to_summarize = msgs_to_summarize.len(),
            "Will summarize all messages after last summary"
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
            llm_client.as_ref(),
        )
        .await
        .context("failed to generate summary")?;

        tracing::info!(
            conversation_id = %conv_id,
            summary_length = summary_msg.content.len(),
            "Summary generated"
        );

        // Perform database summarization
        storage
            .perform_summarization(
                &conv_id.to_string(),
                &msgs_to_summarize,
                &summary_msg.content,
            )
            .context("failed to save summary to database")?;

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

