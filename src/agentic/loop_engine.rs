use crate::llm::{ChatStreamEvent, Message, Role, LlmClient, ToolCall, ToolResult, LlmError};
use super::protocol::{AgentUpdate, PlannedTool};
use crate::mcp::MCPServerRegistry;
use anyhow::Result;
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{timeout, Duration};


pub struct AgenticLoop {
    pub mcp_registry: Arc<RwLock<MCPServerRegistry>>,
    pub llm_client: Arc<dyn LlmClient>,
    pub tool_logger: super::tool_logger::ToolLogger,
}

impl AgenticLoop {
    pub fn new(mcp_registry: Arc<RwLock<MCPServerRegistry>>, llm_client: Arc<dyn LlmClient>) -> Self {
        Self {
            mcp_registry,
            llm_client,
            tool_logger: super::tool_logger::ToolLogger::new("agentic_tool_calls.log".to_string()),
        }
    }
    
    pub async fn process_message(
        &mut self,
        mut messages: Vec<Message>,
        agent_tx: Option<tokio::sync::mpsc::UnboundedSender<AgentUpdate>>,
        _message_id: Option<uuid::Uuid>,
    ) -> Result<String> {
        loop {
            let available_tools = {
                let registry = self.mcp_registry.read().await;
                let tools = registry.get_enabled_tools();
                log::debug!("🔧 Enabled tools count: {}", tools.len());
                tools
            };

            let mut stream = match self
                .llm_client
                .send_message_stream_with_tools(messages.clone(), available_tools, None, None)
                .await
            {
                Ok(stream) => stream,
                Err(LlmError::Config(e)) => {
                    log::warn!(
                        "Tool streaming unsupported for backend, falling back to non-streaming mode: {}",
                        e
                    );
                    return self.process_non_streaming(messages, agent_tx).await;
                }
                Err(e) => {
                    log::error!("❌ LLM streaming call failed: {}", e);
                    if let Some(tx) = agent_tx.as_ref() {
                        let _ = tx.send(AgentUpdate::ModelError {
                            error: format!("Model communication failed: {}", e),
                        });
                    }
                    return Err(anyhow::anyhow!("LLM call failed: {}", e));
                }
            };

            if let Some(tx) = agent_tx.as_ref() {
                let _ = tx.send(AgentUpdate::AssistantStreamingStarted);
            }

            let mut assistant_response = String::new();
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
                        log::error!("❌ Streaming event error: {}", e);
                        if let Some(tx) = agent_tx.as_ref() {
                            let _ = tx.send(AgentUpdate::ModelError {
                                error: format!("Streaming error: {}", e),
                            });
                        }
                        return Err(anyhow::anyhow!("Streaming error: {}", e));
                    }
                }
            }

            if let Some(tx) = agent_tx.as_ref() {
                let _ = tx.send(AgentUpdate::AssistantComplete {
                    full_text: assistant_response.clone(),
                });
            }

            if planned_tools.is_empty() {
                self.tool_logger.log_final_response(&assistant_response)?;
                if let Some(tx) = agent_tx.as_ref() {
                    let _ = tx.send(AgentUpdate::ConversationComplete {
                        final_text: assistant_response.clone(),
                    });
                }
                return Ok(assistant_response);
            }

            messages.push(Message::new_with_tool_calls(
                Role::Assistant,
                assistant_response.clone(),
                planned_tools.clone(),
            ));

            for tool_call in planned_tools {
                self.tool_logger.log_tool_call(&tool_call)?;

                if let Some(tx) = agent_tx.as_ref() {
                    let _ = tx.send(AgentUpdate::ToolStarted {
                        tool_call_id: tool_call.id.clone(),
                        name: tool_call.name.clone(),
                        params_json: serde_json::to_string(&tool_call.parameters)
                            .unwrap_or_default(),
                    });
                }

                let result =
                    self.execute_tool_with_retry(tool_call.clone(), agent_tx.as_ref())
                        .await;

                self.tool_logger
                    .log_tool_result(&tool_call, &result.content, result.is_error)?;

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
            let call_future = async {
                let mut registry = self.mcp_registry.write().await;
                registry.call_tool(tool_call.clone()).await
            };
            match timeout(per_call_timeout, call_future).await {
                Ok(Ok(result)) => {
                    return result;
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
                registry.get_enabled_tools()
            };

            let response = match self
                .llm_client
                .send_message_with_tools(messages.clone(), available_tools, None, None)
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    log::error!("❌ Non-streaming LLM call failed: {}", e);
                    if let Some(tx) = agent_tx.as_ref() {
                        let _ = tx.send(AgentUpdate::ModelError {
                            error: format!("Model communication failed: {}", e),
                        });
                    }
                    return Err(anyhow::anyhow!("LLM call failed: {}", e));
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
                });
            }

            if response.tool_calls.is_empty() {
                self.tool_logger.log_final_response(&response.content)?;
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

            messages.push(Message::new_with_tool_calls(
                Role::Assistant,
                response.content.clone(),
                response.tool_calls.clone(),
            ));

            for tool_call in response.tool_calls {
                self.tool_logger.log_tool_call(&tool_call)?;

                if let Some(tx) = agent_tx.as_ref() {
                    let _ = tx.send(AgentUpdate::ToolStarted {
                        tool_call_id: tool_call.id.clone(),
                        name: tool_call.name.clone(),
                        params_json: serde_json::to_string(&tool_call.parameters)
                            .unwrap_or_default(),
                    });
                }

                let result =
                    self.execute_tool_with_retry(tool_call.clone(), agent_tx.as_ref())
                        .await;

                self.tool_logger
                    .log_tool_result(&tool_call, &result.content, result.is_error)?;

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

