use crate::{
    config::ResolvedProfile,
    llm::context_manager::SmartContextManager,
    llm::tokenizer::TokenCounter,
    llm::{Message as LlmMessage, Role},
    services::ContextService,
    server::dto::ServerEvent,
    server::handlers::{ServerContext, SessionState},
};
use anyhow::Result;
use std::{future::Future, sync::Arc};
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

pub struct RunAgentOptions {
    pub auto_summarize: bool,
}

pub struct ContextPipeline<'a> {
    ctx: &'a Arc<ServerContext>,
    session: &'a SessionState,
    outbound: &'a UnboundedSender<ServerEvent>,
    conversation_id: Uuid,
}

fn count_tokens(msgs: &[LlmMessage], counter: &TokenCounter) -> usize {
    msgs.iter()
        .map(|m| counter.count_message_tokens(m))
        .sum()
}

impl<'a> ContextPipeline<'a> {
    pub fn new(
        ctx: &'a Arc<ServerContext>,
        session: &'a SessionState,
        outbound: &'a UnboundedSender<ServerEvent>,
        conversation_id: Uuid,
    ) -> Self {
        Self {
            ctx,
            session,
            outbound,
            conversation_id,
        }
    }

    pub async fn prepare_for_agent<F, Fut>(
        &self,
        mut llm_messages: Vec<LlmMessage>,
        resolved: &ResolvedProfile,
        options: RunAgentOptions,
        reload_messages: F,
    ) -> Result<Vec<LlmMessage>>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<Vec<LlmMessage>>>,
    {
        llm_messages = ContextService::inject_prompts(
            llm_messages,
            &self.ctx.prompt_manager,
            resolved.profile(),
        )?;

        if options.auto_summarize {
            llm_messages = self
                .maybe_summarize(llm_messages, resolved, &reload_messages)
                .await?;
        }

        self.inject_memory(&mut llm_messages).await?;

        Ok(self.enforce_token_budget(llm_messages, resolved))
    }

    async fn maybe_summarize<F, Fut>(
        &self,
        llm_messages: Vec<LlmMessage>,
        resolved: &ResolvedProfile,
        reload_messages: &F,
    ) -> Result<Vec<LlmMessage>>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<Vec<LlmMessage>>>,
    {
        let preset = resolved.preset();
        let token_counter = TokenCounter::new(preset);
        let total_tokens = count_tokens(&llm_messages, &token_counter);
        let summarize_threshold =
            token_counter.get_summarize_threshold_tokens(preset, resolved.profile());
        let will_summarize = total_tokens > summarize_threshold;

        if will_summarize {
            self.ctx
                .subscriptions
                .broadcast(
                    self.conversation_id,
                    ServerEvent::Info {
                        message: "Summarizing conversation…".into(),
                    },
                )
                .await;
        }

        let storage = self.ctx.storage.clone();
        let llm_client = self.session.llm_client.clone();

        if let Err(e) = ContextService::check_and_trigger_summarization(
            &llm_messages,
            self.conversation_id,
            storage,
            &llm_client,
            resolved,
            &self.ctx.config.conversation_compact,
        )
        .await
        {
            tracing::warn!(error = %e, "Failed to check/trigger summarization, continuing anyway");
        } else if will_summarize {
            self.ctx
                .subscriptions
                .broadcast(
                    self.conversation_id,
                    ServerEvent::Info {
                        message: "Conversation summarized.".into(),
                    },
                )
                .await;
        }

        let mut llm_messages = reload_messages().await?;
        llm_messages = ContextService::inject_prompts(
            llm_messages,
            &self.ctx.prompt_manager,
            resolved.profile(),
        )?;
        Ok(llm_messages)
    }

    async fn inject_memory(&self, llm_messages: &mut Vec<LlmMessage>) -> Result<()> {
        let resolved = self.session.active_resolved(&self.ctx.config)?;
        let preset = resolved.preset();
        let token_counter = TokenCounter::new(preset);
        let outcome = crate::services::memory_rag::inject_memory_block(
            self.ctx.storage.clone(),
            self.ctx.embedding_provider.as_deref(),
            &self.ctx.config.embedding,
            &token_counter,
            llm_messages,
        )
        .await;
        if let Some(outcome) = outcome {
            let storage_guard = self.ctx.storage.lock().await;
            if let Err(e) = storage_guard
                .record_memory_recalls(&self.conversation_id.to_string(), &outcome.ids)
            {
                tracing::warn!(error = %e, "Failed to record memory recalls (analytics)");
            }
            drop(storage_guard);
            let _ = self.outbound.send(ServerEvent::MemoriesRecalled {
                conversation_id: self.conversation_id.to_string(),
                memory_ids: outcome.ids,
            });
        }
        Ok(())
    }

    pub fn enforce_token_budget(
        &self,
        llm_messages: Vec<LlmMessage>,
        resolved: &ResolvedProfile,
    ) -> Vec<LlmMessage> {
        let preset = resolved.preset();
        let token_counter = TokenCounter::new(preset);
        let context_limit = token_counter.get_context_limit(preset);
        let safe_limit = token_counter.get_safe_context_limit(preset);
        let hard_limit = token_counter.get_context_limit(preset);

        let total_tokens = count_tokens(&llm_messages, &token_counter);
        let usage_percent = (total_tokens as f32 / context_limit as f32 * 100.0) as u32;
        tracing::info!(
            "Context usage after summarization: {} tokens / {} limit ({}%)",
            total_tokens,
            context_limit,
            usage_percent
        );

        let mut agent_messages = if total_tokens > safe_limit {
            tracing::info!(
                total_tokens,
                safe_limit,
                "Context exceeds safe limit, applying smart truncation"
            );
            SmartContextManager::select_context(llm_messages, &token_counter, preset)
        } else {
            llm_messages
        };

        let total_tokens = count_tokens(&agent_messages, &token_counter);

        if total_tokens > hard_limit {
            tracing::warn!(
                total_tokens,
                hard_limit,
                "CRITICAL: Context exceeds hard limit, forcing truncation"
            );
            let original_count = agent_messages.len();
            agent_messages =
                SmartContextManager::select_context(agent_messages, &token_counter, preset);
            let selected_count = agent_messages.len();
            let selected_tokens = count_tokens(&agent_messages, &token_counter);

            tracing::warn!(
                original_count,
                selected_count,
                total_tokens,
                selected_tokens,
                "Emergency truncation: messages and tokens reduced"
            );

            let _ = self.outbound.send(ServerEvent::Info {
                message: format!(
                    "Context exceeded limit! Truncated: {} messages -> {} messages ({} tokens)",
                    original_count,
                    selected_count,
                    selected_tokens
                ),
            });
        } else if total_tokens > safe_limit {
            tracing::info!(
                "Context overflow detected: {} tokens > {} safe limit. Applying smart context selection.",
                total_tokens,
                safe_limit
            );
            let original_count = agent_messages.len();
            agent_messages =
                SmartContextManager::select_context(agent_messages, &token_counter, preset);
            let selected_count = agent_messages.len();
            let selected_tokens = count_tokens(&agent_messages, &token_counter);

            tracing::info!(
                "Context selection: {} messages -> {} messages ({} tokens -> {} tokens)",
                original_count,
                selected_count,
                total_tokens,
                selected_tokens
            );

            let _ = self.outbound.send(ServerEvent::Info {
                message: format!(
                    "Context truncated: {} messages selected from {} ({} tokens used)",
                    selected_count,
                    original_count,
                    selected_tokens
                ),
            });
        }

        let final_tokens = count_tokens(&agent_messages, &token_counter);

        if final_tokens > hard_limit {
            tracing::error!(
                total_tokens = final_tokens,
                context_limit = hard_limit,
                "FATAL: After truncation, still over limit (this should not happen)"
            );
            let system_count = agent_messages
                .iter()
                .take_while(|m| matches!(m.role, Role::System))
                .count();
            let mut emergency_messages: Vec<LlmMessage> = agent_messages[..system_count].to_vec();
            let mut emergency_tokens = count_tokens(&emergency_messages, &token_counter);

            for msg in agent_messages.iter().skip(system_count).rev() {
                let msg_tokens = token_counter.count_message_tokens(msg);
                if emergency_tokens + msg_tokens <= hard_limit {
                    emergency_messages.push(msg.clone());
                    emergency_tokens += msg_tokens;
                } else {
                    break;
                }
            }

            agent_messages = emergency_messages;
            tracing::warn!(
                message_count = agent_messages.len(),
                "Emergency fallback: Reduced messages"
            );
        }

        agent_messages
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::Message as LlmMessage;

    #[test]
    fn count_tokens_non_empty_for_user_message() {
        let preset = crate::config::ModelPreset::default();
        let counter = TokenCounter::new(&preset);
        let msgs = vec![LlmMessage::new(Role::User, "hello".into())];
        assert!(count_tokens(&msgs, &counter) > 0);
    }
}
