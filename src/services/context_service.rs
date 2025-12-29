//! Context management service
//!
//! Unified context management for both desktop and server.
//! Handles:
//! - Token counting
//! - Context truncation
//! - Summarization
//! - Prompt injection

use crate::config::LlmProfile;
use crate::llm::{Message as LlmMessage, Role};
use crate::llm::tokenizer::TokenCounter;
use crate::prompts::PromptManager;
use anyhow::Result;
use tracing::debug;

/// Context service (stateless)
pub struct ContextService;

impl ContextService {
    /// Prepare context for LLM API call
    ///
    /// This unifies context preparation logic that's currently duplicated in:
    /// - `app.rs` (desktop)
    /// - `server/handlers.rs` (server)
    ///
    /// # Arguments
    /// - `messages`: Conversation messages
    /// - `profile`: LLM profile with context limits
    /// - `prompt_manager`: Prompt manager for system prompts
    ///
    /// # Returns
    /// Prepared messages ready for API call (with prompts injected, truncation applied)
    pub fn prepare_context(
        &self,
        messages: Vec<LlmMessage>,
        profile: &LlmProfile,
        prompt_manager: &PromptManager,
    ) -> Result<Vec<LlmMessage>> {
        // Inject system prompts first
        let mut final_messages = Self::inject_prompts(messages, prompt_manager, profile)?;

        // Apply context management (token counting and smart selection)
        let token_counter = TokenCounter::new(profile);
        let total_tokens: usize = final_messages
            .iter()
            .map(|msg| token_counter.count_message_tokens(msg))
            .sum();

        let context_limit = token_counter.get_context_limit(profile);
        let summarize_threshold_tokens = token_counter.get_summarize_threshold_tokens(profile);

        debug!(
            total_tokens = total_tokens,
            context_limit = context_limit,
            summarize_threshold = summarize_threshold_tokens,
            "Context preparation"
        );

        // Apply truncation if needed
        let final_messages = if total_tokens > token_counter.get_safe_context_limit(profile) {
            debug!(
                total_tokens,
                safe_limit = token_counter.get_safe_context_limit(profile),
                "Context exceeds safe limit, applying smart truncation"
            );
            
            // Use SmartContextManager to select important messages
            crate::llm::context_manager::SmartContextManager::select_context(
                final_messages,
                &token_counter,
                profile,
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
        profile: &crate::config::LlmProfile,
    ) -> Result<String> {
        use anyhow::Context;

        tracing::debug!(conversation_id = %conv_id, "Manual summarization triggered");
        
        // Load messages from DB for summarization
        let db_messages = storage
            .load_conversation_messages(&conv_id.to_string())
            .context("Failed to load messages from database")?;
        
        // Filter to regular messages (exclude summaries, tools, and already summarized messages)
        let regular_messages: Vec<_> = db_messages.iter()
            .filter(|msg| !msg.is_summary && !msg.is_summarized && msg.role != "tool")
            .collect();
        
        if regular_messages.is_empty() {
            tracing::warn!(conversation_id = %conv_id, "No messages available to summarize");
            return Err(anyhow::anyhow!("No messages available to summarize"));
        }
        
        let keep_recent_count = 10;
        let messages_to_summarize_count = regular_messages.len().saturating_sub(keep_recent_count);
        
        if messages_to_summarize_count == 0 {
            tracing::debug!(
                conversation_id = %conv_id,
                keep_recent_count,
                "All messages are recent, nothing to summarize"
            );
            return Err(anyhow::anyhow!("All messages are recent, nothing to summarize"));
        }
        
        tracing::debug!(
            conversation_id = %conv_id,
            messages_to_summarize = messages_to_summarize_count,
            keep_recent_count,
            "Will summarize messages"
        );
        
        // Get IDs to summarize
        let ids_to_summarize: Vec<i64> = regular_messages[..messages_to_summarize_count]
            .iter()
            .map(|msg| msg.id)
            .collect();
        
        // Get full messages to summarize
        let msgs_to_summarize: Vec<_> = db_messages.iter()
            .filter(|msg| ids_to_summarize.contains(&msg.id))
            .cloned()
            .collect();
        
        // Convert to LlmMessage for summarization
        let llm_msgs_to_summarize: Vec<crate::llm::Message> = msgs_to_summarize.iter()
            .filter_map(|msg| {
                let role = match msg.role.as_str() {
                    "user" => crate::llm::Role::User,
                    "assistant" => crate::llm::Role::Assistant,
                    "system" => crate::llm::Role::System,
                    _ => return None,
                };
                Some(crate::llm::Message::new(role, msg.content.clone()))
            })
            .collect();
        
        if llm_msgs_to_summarize.is_empty() {
            return Err(anyhow::anyhow!("No valid messages to summarize"));
        }

        // Generate summary
        tracing::debug!("Generating summary");
        let summary_msg = crate::llm::context_manager::SmartContextManager::summarize_messages(
            llm_msgs_to_summarize,
            profile,
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

        // Add profile prompt if available
        if let Some(profile_prompt) = profile
            .profile_prompt_file
            .as_ref()
            .and_then(|path| {
                let resolved = crate::config::AppConfig::resolve_config_path(path);
                let owned = resolved.to_string_lossy().to_string();
                prompt_manager.load_profile_prompt(&owned).ok()
            }) {
            final_messages.push(LlmMessage::new(Role::System, profile_prompt));
        }

        final_messages.append(&mut history);
        Ok(final_messages)
    }
}

