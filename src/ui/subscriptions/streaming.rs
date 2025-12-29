//! Streaming subscription helper
//!
//! Extracted from app.rs for better modularity.

use crate::ui::app::Message;
use crate::ui::state::AttachmentState;
use crate::services::ContextService;
use crate::config::AppConfig;
use crate::prompts::PromptManager;
use crate::llm::LlmClient;
use crate::mcp::MCPServerRegistry;
use cosmic::iced_futures::Subscription;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Create a streaming subscription for LLM responses
///
/// This handles the full streaming workflow:
/// - Message preparation (with attachments if available)
/// - Context management (prompt injection, truncation)
/// - Agentic loop processing
/// - Agent update forwarding
pub fn create_streaming_subscription(
    streaming_id: Option<Uuid>,
    llm_client: Arc<dyn LlmClient>,
    prompt_manager: PromptManager,
    messages: Vec<crate::ui::app::ChatMessage>,
    mcp_registry: Arc<RwLock<MCPServerRegistry>>,
    attachment_state: &AttachmentState,
    config: &AppConfig,
) -> Subscription<Message> {
    use cosmic::iced_futures::futures::SinkExt;
    use cosmic::iced_futures::stream;
    use tokio::sync::mpsc;

    // Create a streaming subscription using the channel pattern
    let id = streaming_id.unwrap_or_else(|| uuid::Uuid::new_v4());
    let llm_client_clone = llm_client.clone();
    let prompt_manager_clone = prompt_manager.clone();
    let messages_clone = messages.clone();
    let mcp_registry_clone = mcp_registry.clone();
    let pending_messages = attachment_state.pending_llm_messages.clone();
    let profile = config.get_default_profile().cloned();

    Subscription::run_with_id(
        id,
        stream::channel(100, move |mut output| async move {
            // Use prepared messages if available (which includes attachments), otherwise rebuild
            let llm_messages = if let Some(prepared_messages) = pending_messages {
                tracing::debug!("Using prepared messages with attachments");
                prepared_messages
            } else {
                tracing::debug!("Rebuilding messages from history");
                // Build LLM messages from conversation history (without prompts - ContextService will add them)
                let mut llm_messages = Vec::new();

                // Add conversation history, filtering out placeholder assistant messages
                for msg in &messages_clone {
                    let content_trimmed = msg.content.trim();
                    if !msg.is_user {
                        // Skip placeholder or empty assistant messages
                        if content_trimmed.is_empty() || content_trimmed == "🤔 Thinking..." {
                            continue;
                        }
                    }

                    let role = if msg.is_user {
                        crate::llm::Role::User
                    } else {
                        crate::llm::Role::Assistant
                    };
                    llm_messages.push(crate::llm::Message::new(role, msg.content.clone()));
                }

                llm_messages
            };

            // === CONTEXT MANAGEMENT ===
            // Use ContextService to prepare context (inject prompts, apply truncation)
            let final_messages = if let Some(ref prof) = profile {
                let context_service = ContextService;
                // Clone llm_messages for fallback in error case
                let llm_messages_fallback = llm_messages.clone();
                match context_service.prepare_context(llm_messages, prof, &prompt_manager_clone) {
                    Ok(prepared) => {
                        // Check if truncation occurred and notify user if needed
                        use crate::llm::tokenizer::TokenCounter;
                        let token_counter = TokenCounter::new(prof);
                        let safe_limit = token_counter.get_safe_context_limit(prof);
                        let final_tokens: usize = prepared.iter()
                            .map(|msg| token_counter.count_message_tokens(msg))
                            .sum();
                        
                        if final_tokens > safe_limit * 9 / 10 {
                            // Close to limit, warn user
                            let _ = output.send(Message::InlineError(format!(
                                "Context size ({} tokens) is close to limit ({}). Some messages may have been truncated.",
                                final_tokens, safe_limit
                            ))).await;
                        }
                        
                        prepared
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to prepare context");
                        // Try to handle profile prompt errors gracefully
                        if let Some(err_msg) = e.to_string().as_str().strip_prefix("Profile prompt error: ") {
                            let _ = output.send(Message::InlineError(format!(
                                "Profile prompt error: {}",
                                err_msg
                            ))).await;
                        } else {
                            let _ = output.send(Message::InlineError(format!(
                                "Failed to prepare context: {}",
                                e
                            ))).await;
                        }
                        llm_messages_fallback // Fallback to original messages
                    }
                }
            } else {
                llm_messages
            };

            // Create channel for agent updates
            let (tx_agent, mut rx_agent) = mpsc::unbounded_channel::<crate::agentic::protocol::AgentUpdate>();

            // Start agentic processing in background
            let llm_client_spawn = llm_client_clone.clone();
            let mcp_registry_spawn = mcp_registry_clone.clone();
            let llm_messages_spawn = final_messages.clone();

            tokio::spawn(async move {
                let mut agentic_loop = crate::agentic::loop_engine::AgenticLoop::new(
                    mcp_registry_spawn,
                    llm_client_spawn,
                );

                match agentic_loop
                    .process_message(llm_messages_spawn, Some(tx_agent.clone()), Some(id))
                    .await
                {
                    Ok(_final_response) => {
                        // Final response is sent via AgentUpdate::EndConversation
                    }
                    Err(e) => {
                        // Send error via AgentUpdate - this handles cases where the loop fails completely
                        let _ = tx_agent.send(crate::agentic::protocol::AgentUpdate::ModelError {
                            error: format!("Agent processing failed: {}", e),
                        });
                    }
                }
            });

            // Process AgentUpdate stream
            while let Some(update) = rx_agent.recv().await {
                let _ = output.send(Message::AgentUpdate(update)).await;
            }
        }),
    )
}

