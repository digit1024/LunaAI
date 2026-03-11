//! Rig-based conversation engine.
//!
//! Runs LLM/tool loop via Rig, broadcasts ServerEvents to subscribers.

use crate::llm::{Message as LlmMessage, ToolCall};
use crate::server::handlers::ServerContext;
use crate::storage::sqlite_storage_simple::MessageMetadata;
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::Instrument;
use uuid::Uuid;

/// Parameters for a single conversation turn.
#[derive(Clone)]
pub struct TurnParams {
    pub conversation_id: Uuid,
    pub agent_messages: Vec<LlmMessage>,
    pub profile_name: String,
    pub allowed_tool_names: HashSet<String>,
}

/// Rig-backed engine. Runs a turn via Rig pipeline with MCP + internal tools.
#[derive(Debug, Default)]
pub struct RigEngine;

/// Trait for running a conversation turn (kept for handler ergonomics).
pub trait ConversationEngine: Send + Sync {
    fn run_turn(
        &self,
        ctx: Arc<ServerContext>,
        params: TurnParams,
    ) -> Result<JoinHandle<()>>;
}

impl ConversationEngine for RigEngine {
    fn run_turn(
        &self,
        ctx: Arc<ServerContext>,
        params: TurnParams,
    ) -> Result<JoinHandle<()>> {
        use crate::llm::Role;
        use crate::rig_core::{run_turn_streaming, RigConversationContext, StreamChunk};
        use crate::server::dto::ServerEvent;
        use futures::StreamExt;

        let resolved = ctx
            .config
            .resolve_profile(&params.profile_name)
            .or_else(|| ctx.config.resolve_default_profile())
            .ok_or_else(|| anyhow::anyhow!("Profile not found: {}", params.profile_name))?;

        let preset = resolved.preset().clone();
        let conversation_id = params.conversation_id;
        let cid_str = conversation_id.to_string();
        let storage = ctx.storage.clone();
        let timeout = std::time::Duration::from_secs(ctx.server_cfg.stream_timeout_secs);

        // Extract preamble from System messages (ContextService::inject_prompts)
        let preamble: String = params
            .agent_messages
            .iter()
            .filter(|m| matches!(m.role, Role::System))
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");

        // Build context: last message is user, rest is history (exclude System; they're in preamble)
        let non_system: Vec<_> = params
            .agent_messages
            .iter()
            .filter(|m| !matches!(m.role, Role::System))
            .cloned()
            .collect();

        let (history, user_message) = if let Some((last, rest)) = non_system.split_last() {
            if matches!(last.role, Role::User) {
                (rest.to_vec(), last.content.clone())
            } else {
                (non_system, String::new())
            }
        } else {
            (vec![], String::new())
        };

        if user_message.is_empty() {
            return Err(anyhow::anyhow!("No user message to send"));
        }

        let preamble = if preamble.trim().is_empty() {
            "You are a helpful assistant.".to_string()
        } else {
            preamble
        };

        let mcp_registry = ctx.mcp_registry.clone();
        let allowed_tool_names = params.allowed_tool_names.clone();
        let subscriptions = ctx.subscriptions.clone();
        let schedule_service = ctx.schedule_service.clone();
        let embedding_provider = ctx.embedding_provider.clone();
        let profile_name = params.profile_name.clone();

        let span = tracing::info_span!(
            "luna.run_turn",
            conversation_id = %cid_str,
            model = %preset.model
        );

        let handle = tokio::spawn(
            async move {
                let (mcp_servers, internal_tools) = if allowed_tool_names.is_empty() {
                    (vec![], vec![])
                } else {
                    match crate::server::rig_tools::build_turn_tools(
                        &mcp_registry,
                        &allowed_tool_names,
                        subscriptions.clone(),
                        conversation_id,
                        profile_name,
                        schedule_service,
                        storage.clone(),
                        embedding_provider,
                    )
                    .await
                    {
                        Ok(t) => t,
                        Err(e) => {
                            tracing::warn!(error = %e, "Failed to build tools for Rig, continuing without tools");
                            (vec![], vec![])
                        }
                    }
                };

            let rig_ctx = RigConversationContext {
                messages: history,
                user_message,
                preset,
                preamble: preamble.clone(),
                mcp_servers,
                internal_tools,
            };

            let mut stream = match run_turn_streaming(rig_ctx).await {
                Ok(s) => s,
                Err(e) => {
                    let _ = subscriptions
                        .broadcast(
                            conversation_id,
                            ServerEvent::Error {
                                message: e.to_string(),
                            },
                        )
                        .await;
                    return;
                }
            };

            let _ = subscriptions
                .broadcast(
                    conversation_id,
                    ServerEvent::StreamingStarted {
                        conversation_id: cid_str.clone(),
                    },
                )
                .await;

            let mut full_content = String::new();
            let mut seq = 0u64;

            // Accumulators for tool-call persistence
            let mut content_before_tools = String::new();
            let mut tool_calls: Vec<ToolCall> = Vec::new();
            let mut tool_params: HashMap<String, serde_json::Value> = HashMap::new();
            let mut assistant_with_tools_persisted = false;

            let stream_future = async {
                while let Some(chunk) = stream.next().await {
                    match chunk {
                        Ok(StreamChunk::Delta(text)) => {
                            full_content.push_str(&text);
                            if tool_calls.is_empty() {
                                content_before_tools.push_str(&text);
                            }
                            seq += 1;
                            let _ = subscriptions
                                .broadcast(
                                    conversation_id,
                                    ServerEvent::AssistantDelta {
                                        conversation_id: cid_str.clone(),
                                        chunk: text,
                                        seq,
                                    },
                                )
                                .await;
                        }
                        Ok(StreamChunk::Final(final_text)) => {
                            if final_text.len() > full_content.len() {
                                let suffix = &final_text[full_content.len()..];
                                full_content.push_str(suffix);
                                seq += 1;
                                let _ = subscriptions
                                    .broadcast(
                                        conversation_id,
                                        ServerEvent::AssistantDelta {
                                            conversation_id: cid_str.clone(),
                                            chunk: suffix.to_string(),
                                            seq,
                                        },
                                    )
                                    .await;
                            }
                        }
                        Ok(StreamChunk::ToolPlanned { tools }) => {
                            for t in &tools {
                                tool_calls.push(ToolCall {
                                    id: t.id.clone(),
                                    name: t.name.clone(),
                                    parameters: t.params_json.clone(),
                                });
                            }
                            let _ = subscriptions
                                .broadcast(
                                    conversation_id,
                                    ServerEvent::ToolPlanned {
                                        conversation_id: cid_str.clone(),
                                        tools,
                                    },
                                )
                                .await;
                        }
                        Ok(StreamChunk::ToolStarted {
                            tool_call_id,
                            name,
                            params_json,
                        }) => {
                            tool_params.insert(tool_call_id.clone(), params_json.clone());
                            let _ = subscriptions
                                .broadcast(
                                    conversation_id,
                                    ServerEvent::ToolStarted {
                                        conversation_id: cid_str.clone(),
                                        tool_call_id,
                                        name,
                                        params_json,
                                    },
                                )
                                .await;
                        }
                        Ok(StreamChunk::ToolResult {
                            tool_call_id,
                            name,
                            result_json,
                            is_error,
                        }) => {
                            // Persist assistant with tool_calls before first tool result
                            if !assistant_with_tools_persisted && !tool_calls.is_empty() {
                                assistant_with_tools_persisted = true;
                                let guard = storage.lock().await;
                                let meta = MessageMetadata {
                                    tool_calls: Some(&tool_calls),
                                    ..Default::default()
                                };
                                let _ = guard.add_message_with_metadata(
                                    &conversation_id,
                                    "assistant".to_string(),
                                    content_before_tools.clone(),
                                    None,
                                    meta,
                                );
                            }
                            // Persist tool result
                            let params = tool_params.get(&tool_call_id).cloned();
                            let guard = storage.lock().await;
                            let content = result_json
                                .get("content")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let meta = MessageMetadata {
                                tool_call_id: Some(&tool_call_id),
                                tool_name: Some(&name),
                                tool_status: Some(if is_error { "error" } else { "ok" }),
                                tool_params_json: params.as_ref(),
                                tool_result_json: Some(&result_json),
                                ..Default::default()
                            };
                            let _ = guard.add_message_with_metadata(
                                &conversation_id,
                                "tool".to_string(),
                                content,
                                None,
                                meta,
                            );

                            let _ = if is_error {
                                subscriptions.broadcast(
                                    conversation_id,
                                    ServerEvent::ToolError {
                                        conversation_id: cid_str.clone(),
                                        tool_call_id,
                                        name,
                                        error: result_json
                                            .get("content")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string(),
                                    },
                                )
                            } else {
                                subscriptions.broadcast(
                                    conversation_id,
                                    ServerEvent::ToolResult {
                                        conversation_id: cid_str.clone(),
                                        tool_call_id,
                                        name,
                                        result_json,
                                    },
                                )
                            }
                            .await;
                        }
                        Err(e) => {
                            let _ = subscriptions
                                .broadcast(
                                    conversation_id,
                                    ServerEvent::Error {
                                        message: e.to_string(),
                                    },
                                )
                                .await;
                            return;
                        }
                    }
                }
            };

            if tokio::time::timeout(timeout, stream_future)
                .await
                .is_err()
            {
                let _ = subscriptions
                    .broadcast(
                        conversation_id,
                        ServerEvent::Error {
                            message: "Streaming timeout".to_string(),
                        },
                    )
                    .await;
                return;
            }

            // Persist assistant message(s)
            if assistant_with_tools_persisted {
                // Final assistant content: text after tool calls (avoid duplicating content_before_tools)
                let final_content = if content_before_tools.len() < full_content.len() {
                    full_content[content_before_tools.len()..].trim().to_string()
                } else {
                    String::new()
                };
                if !final_content.is_empty() {
                    let guard = storage.lock().await;
                    let _ = guard.add_message_with_metadata(
                        &conversation_id,
                        "assistant".to_string(),
                        final_content,
                        None,
                        MessageMetadata::default(),
                    );
                }
            } else if !full_content.is_empty() {
                // No tools: single assistant message
                let guard = storage.lock().await;
                let _ = guard.add_message_with_metadata(
                    &conversation_id,
                    "assistant".to_string(),
                    full_content.clone(),
                    None,
                    MessageMetadata::default(),
                );
            }

            let _ = subscriptions
                .broadcast(
                    conversation_id,
                    ServerEvent::AssistantComplete {
                        conversation_id: cid_str.clone(),
                        content: full_content,
                        reasoning_content: None,
                    },
                )
                .await;

            let _ = subscriptions
                .broadcast(
                    conversation_id,
                    ServerEvent::ConversationComplete {
                        conversation_id: cid_str,
                    },
                )
                .await;
            }
            .instrument(span),
        );

        Ok(handle)
    }
}
