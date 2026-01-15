use super::protocol::{AgentUpdate, PlannedTool};
use crate::llm::{ChatStreamEvent, LlmClient, LlmError, Message, Role, ToolCall, ToolResult};
use crate::mcp::conversions::{tool_call_to_params, tools_to_definitions};
use agentic_loop::mcp_servers_registry::MCPServerRegistry;
use anyhow::{Context, Result};
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{timeout, Duration};

pub struct AgenticLoop {
    pub mcp_registry: Arc<RwLock<MCPServerRegistry>>,
    pub llm_client: Arc<dyn LlmClient>,
    
}

impl AgenticLoop {
    pub fn new(
        mcp_registry: Arc<RwLock<MCPServerRegistry>>,
        llm_client: Arc<dyn LlmClient>,
    ) -> Self {
        Self {
            mcp_registry,
            llm_client,
            
        }
    }

    pub async fn process_message(
        &mut self,
        mut messages: Vec<Message>,
        agent_tx: Option<tokio::sync::mpsc::UnboundedSender<AgentUpdate>>,
        _message_id: Option<uuid::Uuid>,
    ) -> Result<String> {
        loop {
            let available_tools: Vec<crate::llm::ToolDefinition> = {
                let registry = self.mcp_registry.read().await;
                let tools = registry.get_enabled_tools().await
                    .context("Failed to get enabled tools")?;
                let defs = tools_to_definitions(&tools);
                tracing::debug!(tool_count = defs.len(), "Enabled tools");
                defs
            };

            let mut stream = match self
                .llm_client
                .send_message_stream_with_tools(messages.clone(), available_tools, None, None)
                .await
            {
                Ok(stream) => stream,
                Err(LlmError::Config(e)) => {
                    tracing::warn!(
                        "Tool streaming unsupported for backend, falling back to non-streaming mode: {}",
                        e
                    );
                    return self.process_non_streaming(messages, agent_tx).await;
                }
                Err(e) => {
                    tracing::error!(error = %e, "LLM streaming call failed");
                    if let Some(tx) = agent_tx.as_ref() {
                        let _ = tx.send(AgentUpdate::ModelError {
                            error: format!("Model communication failed: {}", e),
                        });
                    }
                    return Err(e).context("LLM call failed");
                }
            };

            if let Some(tx) = agent_tx.as_ref() {
                let _ = tx.send(AgentUpdate::AssistantStreamingStarted);
            }

            let mut assistant_response = String::new();
            let mut reasoning_content = String::new();
            let mut planned_tools: Vec<ToolCall> = Vec::new();
            let mut seq: u64 = 0;

            while let Some(event) = stream.next().await {
                match event {
                    Ok(ChatStreamEvent::ContentDelta(chunk)) => {
                        if chunk.is_empty() {
                            continue;
                        }
                        assistant_response.push_str(&chunk);
                        seq += 1;
                        if let Some(tx) = agent_tx.as_ref() {
                            let _ = tx.send(AgentUpdate::AssistantDelta {
                                text_chunk: chunk,
                                seq,
                            });
                        }
                    }
                    Ok(ChatStreamEvent::ReasoningContentDelta(chunk)) => {
                        if !chunk.is_empty() {
                            reasoning_content.push_str(&chunk);
                            // Send reasoning content delta during streaming
                            if let Some(tx) = agent_tx.as_ref() {
                                let _ = tx.send(AgentUpdate::ReasoningContentDelta {
                                    chunk: chunk.clone(),
                                });
                            }
                        }
                    }
                    Ok(ChatStreamEvent::ToolCallDelta(tool_call)) => {
                        if let Some(tx) = agent_tx.as_ref() {
                            let planned = PlannedTool {
                                id: tool_call.id.clone(),
                                name: tool_call.name.clone(),
                                params_json: serde_json::to_string(&tool_call.parameters)
                                    .unwrap_or_default(),
                            };
                            let _ = tx.send(AgentUpdate::ToolPlanned {
                                plan_items: vec![planned],
                            });
                        }
                        planned_tools.push(tool_call);
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Streaming event error");
                        if let Some(tx) = agent_tx.as_ref() {
                            let _ = tx.send(AgentUpdate::ModelError {
                                error: format!("Streaming error: {}", e),
                            });
                        }
                        return Err(e).context("Streaming error");
                    }
                }
            }

            if let Some(tx) = agent_tx.as_ref() {
                let _ = tx.send(AgentUpdate::AssistantComplete {
                    full_text: assistant_response.clone(),
                    reasoning_content: if reasoning_content.is_empty() { None } else { Some(reasoning_content.clone()) },
                });
            }

            if planned_tools.is_empty() {
                
                if let Some(tx) = agent_tx.as_ref() {
                    let _ = tx.send(AgentUpdate::ConversationComplete {
                        final_text: assistant_response.clone(),
                    });
                }
                return Ok(assistant_response);
            }

            let mut assistant_msg = Message::new_with_tool_calls(
                Role::Assistant,
                assistant_response.clone(),
                planned_tools.clone(),
            );
            assistant_msg.reasoning_content = if reasoning_content.is_empty() { None } else { Some(reasoning_content.clone()) };
            messages.push(assistant_msg);

            for tool_call in planned_tools {
                

                if let Some(tx) = agent_tx.as_ref() {
                    let _ = tx.send(AgentUpdate::ToolStarted {
                        tool_call_id: tool_call.id.clone(),
                        name: tool_call.name.clone(),
                        params_json: serde_json::to_string(&tool_call.parameters)
                            .unwrap_or_default(),
                    });
                }

                let result = self
                    .execute_tool_with_retry(tool_call.clone(), agent_tx.as_ref())
                    .await;


                if let Some(tx) = agent_tx.as_ref() {
                    let _ = tx.send(AgentUpdate::ToolResult {
                        tool_call_id: tool_call.id.clone(),
                        name: tool_call.name.clone(),
                        result_json: result.content.clone(),
                    });
                }

                messages.push(Message::new_tool_result(
                    tool_call.id.clone(),
                    result.content,
                    result.is_error,
                ));
            }
        }
    }

    async fn execute_tool_with_retry(
        &self,
        tool_call: ToolCall,
        agent_tx: Option<&tokio::sync::mpsc::UnboundedSender<AgentUpdate>>,
    ) -> ToolResult {
        let mut attempt: u8 = 0;
        let max_retries: u8 = 2;
        let per_call_timeout = Duration::from_secs(20);

        loop {
            attempt += 1;
            let tool_name = tool_call.name.clone();
            let call_future = async {
                let mut registry = self.mcp_registry.write().await;
                let params = tool_call_to_params(&tool_call);
                registry.call_tool(tool_name, params.arguments.unwrap_or_default()).await
            };
            match timeout(per_call_timeout, call_future).await {
                Ok(Ok(sdk_result)) => {
                    return ToolResult::from(&sdk_result);
                }
                Ok(Err(e)) => {
                    if let Some(tx) = agent_tx {
                        let _ = tx.send(AgentUpdate::ToolError {
                            tool_call_id: tool_call.id.clone(),
                            name: tool_call.name.clone(),
                            error: e.to_string(),
                            retryable: attempt <= max_retries,
                        });
                    }
                    if attempt > max_retries {
                        return ToolResult {
                            content: format!("Error: {}", e),
                            is_error: true,
                        };
                    }
                }
                Err(_) => {
                    let err_msg = format!("Timeout after {:?}", per_call_timeout);
                    if let Some(tx) = agent_tx {
                        let _ = tx.send(AgentUpdate::ToolError {
                            tool_call_id: tool_call.id.clone(),
                            name: tool_call.name.clone(),
                            error: err_msg.clone(),
                            retryable: attempt <= max_retries,
                        });
                    }
                    if attempt > max_retries {
                        return ToolResult {
                            content: "Timeout".to_string(),
                            is_error: true,
                        };
                    }
                }
            }
        }
    }

    async fn process_non_streaming(
        &mut self,
        mut messages: Vec<Message>,
        agent_tx: Option<tokio::sync::mpsc::UnboundedSender<AgentUpdate>>,
    ) -> Result<String> {
        loop {
            let available_tools = {
                let registry = self.mcp_registry.read().await;
                let tools = registry.get_enabled_tools().await
                    .context("Failed to get enabled tools")?;
                tools_to_definitions(&tools)
            };

            let response = match self
                .llm_client
                .send_message_with_tools(messages.clone(), available_tools, None, None)
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    tracing::error!(error = %e, "Non-streaming LLM call failed");
                    if let Some(tx) = agent_tx.as_ref() {
                        let _ = tx.send(AgentUpdate::ModelError {
                            error: format!("Model communication failed: {}", e),
                        });
                    }
                    return Err(e).context("LLM call failed");
                }
            };

            if let Some(tx) = agent_tx.as_ref() {
                let _ = tx.send(AgentUpdate::AssistantStreamingStarted);
                if !response.content.is_empty() {
                    let _ = tx.send(AgentUpdate::AssistantDelta {
                        text_chunk: response.content.clone(),
                        seq: 1,
                    });
                }
                let _ = tx.send(AgentUpdate::AssistantComplete {
                    full_text: response.content.clone(),
                    reasoning_content: response.reasoning_content.clone(),
                });
            }

            if response.tool_calls.is_empty() {
                
                if let Some(tx) = agent_tx.as_ref() {
                    let _ = tx.send(AgentUpdate::ConversationComplete {
                        final_text: response.content.clone(),
                    });
                }
                return Ok(response.content);
            }

            if let Some(tx) = agent_tx.as_ref() {
                let plan_items: Vec<PlannedTool> = response
                    .tool_calls
                    .iter()
                    .map(|tc| PlannedTool {
                        id: tc.id.clone(),
                        name: tc.name.clone(),
                        params_json: serde_json::to_string(&tc.parameters).unwrap_or_default(),
                    })
                    .collect();
                if !plan_items.is_empty() {
                    let _ = tx.send(AgentUpdate::ToolPlanned { plan_items });
                }
            }

            let mut assistant_msg = Message::new_with_tool_calls(
                Role::Assistant,
                response.content.clone(),
                response.tool_calls.clone(),
            );
            assistant_msg.reasoning_content = response.reasoning_content.clone();
            messages.push(assistant_msg);

            for tool_call in response.tool_calls {
                
                if let Some(tx) = agent_tx.as_ref() {
                    let _ = tx.send(AgentUpdate::ToolStarted {
                        tool_call_id: tool_call.id.clone(),
                        name: tool_call.name.clone(),
                        params_json: serde_json::to_string(&tool_call.parameters)
                            .unwrap_or_default(),
                    });
                }

                let result = self
                    .execute_tool_with_retry(tool_call.clone(), agent_tx.as_ref())
                    .await;


                if let Some(tx) = agent_tx.as_ref() {
                    let _ = tx.send(AgentUpdate::ToolResult {
                        tool_call_id: tool_call.id.clone(),
                        name: tool_call.name.clone(),
                        result_json: result.content.clone(),
                    });
                }

                messages.push(Message::new_tool_result(
                    tool_call.id.clone(),
                    result.content,
                    result.is_error,
                ));
            }
        }
    }
}
