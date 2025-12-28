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

        // TODO: Apply truncation/summarization if needed
        // This will be implemented in a future iteration

        Ok(final_messages)
    }

    /// Inject system prompts into message history
    fn inject_prompts(
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

