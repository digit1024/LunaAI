//! Agent message handlers
//!
//! Handles agent-related messages: AgentUpdate

use cosmic::app;
use serde_json::Value;

use crate::agentic::protocol::AgentUpdate;
use crate::storage::sqlite_storage_simple::MessageMetadata;
use crate::ui::app::{AnchoredToolCall, ChatMessage, CosmicLlmApp, Message, ToolCallInfo, ToolCallStatus, ToolRuntimeContext};
use crate::ui::helpers::utils;

/// Handle agent-related messages
pub fn handle_agent_messages(
    app: &mut CosmicLlmApp,
    message: Message,
) -> Option<app::Task<Message>> {
    match message {
        Message::AgentUpdate(u) => {
            handle_agent_update(app, u);
            None
        }
        _ => None, // Not an agent message
    }
}

fn handle_agent_update(app: &mut CosmicLlmApp, update: AgentUpdate) {
    match update {
        AgentUpdate::AssistantStreamingStarted => {
            app.tool_call_state.pending_tool_calls_for_history.clear();
            app.tool_call_state.tool_runtime_context.clear();
            app.tool_call_state.active_tool_calls.clear();
            app.conversation_state.messages.push(ChatMessage {
                content: String::new(),
                is_user: false,
                is_error: false,
                reasoning_content: None,
                is_summary: false,
                is_summarized: false,
                summarized_count: None,
            });
            app.tool_call_state.set_current_ai_message_index(Some(app.conversation_state.messages.len() - 1));
        }
        AgentUpdate::AssistantDelta { text_chunk, .. } => {
            if let Some(idx) = app.tool_call_state.current_ai_message_index {
                if let Some(msg) = app.conversation_state.messages.get_mut(idx) {
                    msg.content.push_str(&text_chunk);
                }
            }
        }
        AgentUpdate::ReasoningContentDelta { chunk } => {
            if let Some(idx) = app.tool_call_state.current_ai_message_index {
                if let Some(msg) = app.conversation_state.messages.get_mut(idx) {
                    // Accumulate reasoning content during streaming
                    match &mut msg.reasoning_content {
                        Some(existing) => {
                            existing.push_str(&chunk);
                        }
                        None => {
                            msg.reasoning_content = Some(chunk.clone());
                        }
                    }
                }
            }
        }
        AgentUpdate::AssistantComplete { full_text, reasoning_content } => {
            if let Some(idx) = app.tool_call_state.current_ai_message_index {
                if let Some(msg) = app.conversation_state.messages.get_mut(idx) {
                    msg.content = full_text.clone();
                    msg.reasoning_content = reasoning_content.clone();
                }
            } else {
                app.conversation_state.messages.push(ChatMessage {
                    content: full_text.clone(),
                    is_user: false,
                    is_error: false,
                    reasoning_content: reasoning_content.clone(),
                    is_summary: false,
                    is_summarized: false,
                    summarized_count: None,
                });
                app.tool_call_state.set_current_ai_message_index(Some(app.conversation_state.messages.len() - 1));
            }
            if let Some(conv_id) = app.conversation_state.current_conversation_id {
                let tool_calls_slice = if app.tool_call_state.pending_tool_calls_for_history.is_empty() {
                    None
                } else {
                    Some(app.tool_call_state.pending_tool_calls_for_history.as_slice())
                };
                let metadata = MessageMetadata {
                    tool_calls: tool_calls_slice,
                    tool_call_id: None,
                    tool_name: None,
                    tool_status: None,
                    tool_params_json: None,
                    tool_result_json: None,
                    reasoning_content: reasoning_content.as_deref(),
                };
                if let Err(e) = app.storage.add_message_with_metadata(
                    &conv_id,
                    "assistant".to_string(),
                    full_text,
                    None,
                    metadata,
                ) {
                    tracing::error!(error = %e, "Failed to add assistant message");
                } else {
                    // Update context usage cache after adding message
                    app.update_context_usage_cache(conv_id);
                }
            }
            app.tool_call_state.pending_tool_calls_for_history.clear();
        }
        AgentUpdate::ToolPlanned { plan_items } => {
            let anchor = app
                .tool_call_state.current_ai_message_index
                .unwrap_or_else(|| app.conversation_state.messages.len().saturating_sub(1));
            for plan in plan_items {
                let params_value: Value = serde_json::from_str(&plan.params_json)
                    .unwrap_or(Value::String(plan.params_json.clone()));
                let params_pretty = serde_json::to_string_pretty(&params_value)
                    .unwrap_or(plan.params_json.clone());
                let tool_call = crate::llm::ToolCall {
                    id: plan.id.clone(),
                    name: plan.name.clone(),
                    parameters: params_value.clone(),
                };
                app.tool_call_state.pending_tool_calls_for_history.push(tool_call.clone());
                app.tool_call_state.tool_runtime_context.insert(
                    plan.id.clone(),
                    ToolRuntimeContext {
                        anchor_index: anchor,
                        params: Some(params_value.clone()),
                    },
                );
                app.tool_call_state.active_tool_calls.push(ToolCallInfo {
                    id: Some(plan.id),
                    tool_name: plan.name,
                    parameters: params_pretty,
                    status: ToolCallStatus::Started,
                    result: None,
                    error: None,
                });
            }
        }
        AgentUpdate::ToolStarted {
            tool_call_id,
            name,
            params_json,
        } => {
            let params_value: Value = serde_json::from_str(&params_json)
                .unwrap_or(Value::String(params_json.clone()));
            let params_pretty = serde_json::to_string_pretty(&params_value)
                .unwrap_or(params_json.clone());
            let anchor = app
                .tool_call_state.current_ai_message_index
                .unwrap_or_else(|| app.conversation_state.messages.len().saturating_sub(1));

            app.tool_call_state.tool_runtime_context
                .entry(tool_call_id.clone())
                .and_modify(|ctx| {
                    if ctx.params.is_none() {
                        ctx.params = Some(params_value.clone());
                    }
                })
                .or_insert(ToolRuntimeContext {
                    anchor_index: anchor,
                    params: Some(params_value.clone()),
                });

            if let Some(existing) = app
                .tool_call_state.active_tool_calls
                .iter_mut()
                .find(|tc| tc.id.as_ref().map(|s| s == &tool_call_id).unwrap_or(false))
            {
                existing.tool_name = name.clone();
                existing.parameters = params_pretty;
                existing.status = ToolCallStatus::Started;
                existing.result = None;
                existing.error = None;
            } else {
                app.tool_call_state.active_tool_calls.push(ToolCallInfo {
                    id: Some(tool_call_id),
                    tool_name: name,
                    parameters: params_pretty,
                    status: ToolCallStatus::Started,
                    result: None,
                    error: None,
                });
            }
        }
        AgentUpdate::ToolResult {
            tool_call_id,
            name,
            result_json,
        } => {
            let context = app.tool_call_state.tool_runtime_context.get(&tool_call_id).cloned();
            let result_display = utils::format_json_string(&result_json);
            let anchor = context
                .as_ref()
                .map(|ctx| ctx.anchor_index)
                .or(app.tool_call_state.current_ai_message_index)
                .unwrap_or_else(|| app.conversation_state.messages.len().saturating_sub(1));

            let mut archived_entry = None;
            if let Some(pos) = app.tool_call_state.active_tool_calls.iter().position(|tc| {
                tc.id.as_ref().map(|s| s == &tool_call_id).unwrap_or(false)
            }) {
                let mut info = app.tool_call_state.active_tool_calls.remove(pos);
                info.status = ToolCallStatus::Completed;
                info.result = Some(result_display.clone());
                archived_entry = Some(info);
            }
            if archived_entry.is_none() {
                let params_pretty = context
                    .as_ref()
                    .and_then(|ctx| ctx.params.as_ref())
                    .map(|value| {
                        serde_json::to_string_pretty(value)
                            .unwrap_or_else(|_| value.to_string())
                    })
                    .unwrap_or_else(|| "{}".to_string());
                archived_entry = Some(ToolCallInfo {
                    id: Some(tool_call_id.clone()),
                    tool_name: name.clone(),
                    parameters: params_pretty,
                    status: ToolCallStatus::Completed,
                    result: Some(result_display.clone()),
                    error: None,
                });
            }

            if let Some(entry) = archived_entry {
                app.tool_call_state.archived_tool_calls.push(AnchoredToolCall {
                    anchor_index: anchor,
                    tool_call: entry,
                });
                
                // Play tool completion sound
                crate::ui::audio::AudioService::play_sound("tool.mp3");
            }

            if let Some(conv_id) = app.conversation_state.current_conversation_id {
                let params_owned = context.as_ref().and_then(|ctx| ctx.params.clone());
                let params_ref = params_owned.as_ref();
                let result_value = utils::coerce_value(&result_json);
                let metadata = MessageMetadata {
                    tool_calls: None,
                    tool_call_id: Some(tool_call_id.as_str()),
                    tool_name: Some(name.as_str()),
                    tool_status: Some("success"),
                    tool_params_json: params_ref,
                    tool_result_json: Some(&result_value),
                    reasoning_content: None,
                };
                // Use empty content - tool_result_json holds the actual data
                if let Err(e) = app.storage.add_message_with_metadata(
                    &conv_id,
                    "tool".to_string(),
                    String::new(),
                    None,
                    metadata,
                ) {
                    tracing::error!(error = %e, "Failed to add tool result");
                } else {
                    // Update context usage cache after adding tool message
                    app.update_context_usage_cache(conv_id);
                }
            }

            app.tool_call_state.tool_runtime_context.remove(&tool_call_id);
        }
        AgentUpdate::ToolError {
            tool_call_id,
            name,
            error,
            retryable: _,
        } => {
            let context = app.tool_call_state.tool_runtime_context.get(&tool_call_id).cloned();
            let anchor = context
                .as_ref()
                .map(|ctx| ctx.anchor_index)
                .or(app.tool_call_state.current_ai_message_index)
                .unwrap_or_else(|| app.conversation_state.messages.len().saturating_sub(1));

            let mut archived_entry = None;
            if let Some(pos) = app.tool_call_state.active_tool_calls.iter().position(|tc| {
                tc.id.as_ref().map(|s| s == &tool_call_id).unwrap_or(false)
            }) {
                let mut info = app.tool_call_state.active_tool_calls.remove(pos);
                info.status = ToolCallStatus::Error;
                info.error = Some(error.clone());
                archived_entry = Some(info);
            }
            if archived_entry.is_none() {
                let params_pretty = context
                    .as_ref()
                    .and_then(|ctx| ctx.params.as_ref())
                    .map(|value| {
                        serde_json::to_string_pretty(value)
                            .unwrap_or_else(|_| value.to_string())
                    })
                    .unwrap_or_else(|| "{}".to_string());
                archived_entry = Some(ToolCallInfo {
                    id: Some(tool_call_id.clone()),
                    tool_name: name.clone(),
                    parameters: params_pretty,
                    status: ToolCallStatus::Error,
                    result: None,
                    error: Some(error.clone()),
                });
            }

            if let Some(entry) = archived_entry {
                app.tool_call_state.archived_tool_calls.push(AnchoredToolCall {
                    anchor_index: anchor,
                    tool_call: entry,
                });
            }

            if let Some(conv_id) = app.conversation_state.current_conversation_id {
                let params_owned = context.as_ref().and_then(|ctx| ctx.params.clone());
                let params_ref = params_owned.as_ref();
                let error_value = Value::String(error.clone());
                let metadata = MessageMetadata {
                    tool_calls: None,
                    tool_call_id: Some(tool_call_id.as_str()),
                    tool_name: Some(name.as_str()),
                    tool_status: Some("error"),
                    tool_params_json: params_ref,
                    tool_result_json: Some(&error_value),
                    reasoning_content: None,
                };
                // Use empty content - tool_result_json holds the error
                if let Err(e) = app.storage.add_message_with_metadata(
                    &conv_id,
                    "tool".to_string(),
                    String::new(),
                    None,
                    metadata,
                ) {
                    tracing::error!(error = %e, "Failed to add tool error");
                } else {
                    // Update context usage cache after adding tool error message
                    app.update_context_usage_cache(conv_id);
                }
            }

            app.tool_call_state.tool_runtime_context.remove(&tool_call_id);
        }
        AgentUpdate::ConversationComplete { final_text: _ } => {
            if let Some(idx) = app.tool_call_state.current_ai_message_index {
                let should_remove = app
                    .conversation_state.messages
                    .get(idx)
                    .map(|m| !m.is_user && m.content.trim().is_empty())
                    .unwrap_or(false);
                if should_remove {
                    app.conversation_state.messages.remove(idx);
                    for anchored in &mut app.tool_call_state.archived_tool_calls {
                        if anchored.anchor_index > idx {
                            anchored.anchor_index -= 1;
                        } else if anchored.anchor_index == idx {
                            anchored.anchor_index = idx.saturating_sub(1);
                        }
                    }
                }
            }
            app.is_streaming = false;
            app.current_streaming_id = None;
            app.tool_call_state.set_current_ai_message_index(None);
            app.attachment_state.pending_llm_messages = None;
            app.chat_page.typing_indicator_start_time = None;
            app.chat_page.typing_indicator_progress = 0.0;
            app.tool_call_state.active_tool_calls.clear();
            app.tool_call_state.pending_tool_calls_for_history.clear();
            app.tool_call_state.tool_runtime_context.clear();
            
            // Play completion sound
            crate::ui::audio::AudioService::play_sound("done.mp3");
        }
        AgentUpdate::ModelError { error } => {
            // Stop streaming and show error message
            app.is_streaming = false;
            app.current_streaming_id = None;
            app.tool_call_state.set_current_ai_message_index(None);
            app.attachment_state.pending_llm_messages = None;
            app.chat_page.typing_indicator_start_time = None;
            app.chat_page.typing_indicator_progress = 0.0;
            app.tool_call_state.active_tool_calls.clear();
            app.tool_call_state.pending_tool_calls_for_history.clear();
            app.tool_call_state.tool_runtime_context.clear();

            // Add error message as a separate chat bubble
            app.conversation_state.messages.push(ChatMessage {
                content: format!("❌ **Model Communication Error**\n\n{}", error),
                is_user: false,
                is_error: true,
                reasoning_content: None,
                is_summary: false,
                is_summarized: false,
                summarized_count: None,
            });
        }
    }
}

// Helper functions use the ones from CosmicLlmApp

