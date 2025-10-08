use crate::llm::{Message, Role, LlmClient};
use super::protocol::{AgentUpdate, PlannedTool};
use crate::mcp::MCPServerRegistry;
use anyhow::Result;
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
    
    pub async fn process_message(&mut self, mut messages: Vec<Message>, agent_tx: Option<tokio::sync::mpsc::UnboundedSender<AgentUpdate>>, _message_id: Option<uuid::Uuid>) -> Result<String> {
        
        let mut iteration = 0;
        
        loop {
            iteration += 1;
            self.tool_logger.log_iteration_start(iteration)?;
            let _ = self.tool_logger.log_begin_turn(iteration);
            let turn_id = uuid::Uuid::new_v4();
            if let Some(tx) = agent_tx.as_ref() {
                let _ = tx.send(AgentUpdate::BeginTurn {
                    conversation_id: None,
                    turn_id,
                    iteration,
                    plan_summary: None,
                });
            }
            
            // Get enabled tools from MCP registry
            let available_tools = {
                let registry = self.mcp_registry.read().await;
                let tools = registry.get_enabled_tools();
                log::debug!("🔧 Enabled tools count: {}", tools.len());
                tools
            };
            
            // Call LLM with current messages and available tools
            let response = match self.llm_client.send_message_with_tools(
                messages.clone(), 
                available_tools, 
                None, 
                None
            ).await {
                Ok(response) => response,
                Err(e) => {
                    log::error!("❌ LLM call failed: {}", e);
                    // Send model error via AgentUpdate
                    if let Some(tx) = agent_tx.as_ref() {
                        let _ = tx.send(AgentUpdate::ModelError { 
                            turn_id, 
                            error: format!("Model communication failed: {}", e)
                        });
                    }
                    return Err(anyhow::anyhow!("LLM call failed: {}", e));
                },
            };
            
            
            
            // Send assistant content and planned tools via AgentUpdate
            if !response.tool_calls.is_empty() {
                if let Some(tx) = agent_tx.as_ref() {
                    if !response.content.trim().is_empty() {
                        let _ = tx.send(AgentUpdate::AssistantComplete {
                            turn_id,
                            full_text: response.content.clone(),
                        });
                    }
                    let planned: Vec<PlannedTool> = response
                        .tool_calls
                        .iter()
                        .map(|tc| PlannedTool { name: tc.name.clone(), params_json: serde_json::to_string(&tc.parameters).unwrap_or_default() })
                        .collect();
                    let _ = tx.send(AgentUpdate::ToolPlanned { turn_id, plan_items: planned });
                }
            }
            
            // If no tool calls, this is our final response!
            if response.tool_calls.is_empty() {
                self.tool_logger.log_final_response(&response.content, iteration)?;
                
                if let Some(tx) = agent_tx.as_ref() {
                    let _ = tx.send(AgentUpdate::AssistantComplete { turn_id, full_text: response.content.clone() });
                    let _ = tx.send(AgentUpdate::EndTurn { turn_id });
                    let _ = tx.send(AgentUpdate::EndConversation { final_text: response.content.clone() });
                }
                
                let _ = self.tool_logger.log_end_turn(iteration);
                return Ok(response.content);
            }
            
            // Execute tool calls and get results (with timeout & simple retries)
            let mut tool_results = Vec::new();
            let mut started_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
            for (_tool_idx, tool_call) in response.tool_calls.iter().enumerate() {
                
                // Log tool call
                self.tool_logger.log_tool_call(tool_call, iteration)?;
                
                // Send tool call start notification via AgentUpdate
                if let Some(tx) = agent_tx.as_ref() {
                    if started_ids.insert(tool_call.id.clone()) {
                        let _ = tx.send(AgentUpdate::ToolStarted { turn_id, tool_call_id: tool_call.id.clone(), name: tool_call.name.clone(), params_json: serde_json::to_string(&tool_call.parameters).unwrap_or_default() });
                    }
                }
                
                // Execute tool with timeout and up to 2 retries
                let mut attempt: u8 = 0;
                let max_retries: u8 = 2;
                let per_call_timeout = Duration::from_secs(20);
                let result = loop {
                    attempt += 1;
                    let call_future = async {
                        let mut registry = self.mcp_registry.write().await;
                        registry.call_tool(tool_call.clone()).await
                    };
                    match timeout(per_call_timeout, call_future).await {
                        Ok(Ok(result)) => break result,
                        Ok(Err(e)) => {
                            // Report error and decide retryability
                            if let Some(tx) = agent_tx.as_ref() {
                                let _ = tx.send(AgentUpdate::ToolError { turn_id, tool_call_id: tool_call.id.clone(), name: tool_call.name.clone(), error: e.to_string(), retryable: attempt <= max_retries });
                            }
                            if attempt > max_retries { break crate::llm::ToolResult { content: format!("Error: {}", e), is_error: true }; }
                            continue;
                        }
                        Err(_) => {
                            // Timeout
                            let err_msg = format!("Timeout after {:?}", per_call_timeout);
                            if let Some(tx) = agent_tx.as_ref() {
                                let _ = tx.send(AgentUpdate::ToolError { turn_id, tool_call_id: tool_call.id.clone(), name: tool_call.name.clone(), error: err_msg, retryable: attempt <= max_retries });
                            }
                            if attempt > max_retries { break crate::llm::ToolResult { content: "Timeout".to_string(), is_error: true }; }
                            continue;
                        }
                    }
                };
                
                // Log tool result
                self.tool_logger.log_tool_result(tool_call, &result.content, result.is_error, iteration)?;
                
                // Send tool result notification via AgentUpdate
                if let Some(tx) = agent_tx.as_ref() {
                    let _ = tx.send(AgentUpdate::ToolResult { turn_id, tool_call_id: tool_call.id.clone(), name: tool_call.name.clone(), result_json: result.content.clone() });
                }
                
                // Convert result to message for LLM
                let result_message = Message::new_tool_result(
                    tool_call.id.clone(),
                    result.content,
                    result.is_error
                );
                
                tool_results.push(result_message);
            }
            
            // Add assistant message with tool calls to message history
            messages.push(Message::new_with_tool_calls(Role::Assistant, response.content, response.tool_calls.clone()));
            
            // Add tool results to message history
            messages.extend(tool_results);
            
            // End turn and log completion
            if let Some(tx) = agent_tx.as_ref() {
                let _ = tx.send(AgentUpdate::EndTurn { turn_id });
            }
            let _ = self.tool_logger.log_end_turn(iteration);
        }
    }
}

