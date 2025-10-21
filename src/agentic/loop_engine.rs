use crate::llm::{Message, Role, LlmClient};
use crate::llm::{token_counter, context_manager::ContextManager};
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
    pub context_manager: ContextManager,
    pub context_window_size: u32,
    pub summarize_threshold: f32,
}

impl AgenticLoop {
    pub fn new(mcp_registry: Arc<RwLock<MCPServerRegistry>>, llm_client: Arc<dyn LlmClient>) -> Self {
        Self {
            mcp_registry,
            llm_client,
            tool_logger: super::tool_logger::ToolLogger::new("agentic_tool_calls.log".to_string()),
            context_manager: ContextManager::default(),
            context_window_size: 128000, // Default, will be updated from profile
            summarize_threshold: 0.7,
        }
    }
    
    pub fn with_context_config(mut self, window_size: u32, threshold: f32) -> Self {
        self.context_window_size = window_size;
        self.summarize_threshold = threshold;
        self
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
            
            // Check context size and summarize if needed
            let current_tokens = token_counter::estimate_tokens_for_messages(&messages);
            if self.context_manager.should_summarize(current_tokens, self.context_window_size, self.summarize_threshold) {
                log::info!("📝 Context size {} tokens exceeds threshold, summarizing...", current_tokens);
                
                // Get messages to summarize
                let messages_to_summarize = self.context_manager.build_summarization_messages(&messages);
                if !messages_to_summarize.is_empty() {
                    // Summarize old messages
                    match self.context_manager.summarize_messages(self.llm_client.as_ref(), &messages_to_summarize).await {
                        Ok(summary) => {
                            // Get messages to keep
                            let mut messages_to_keep = self.context_manager.get_messages_to_keep(&messages);
                            
                            // Create a summary message
                            let summary_message = Message::new(Role::Assistant, format!("[Previous conversation summarized: {}]", summary));
                            
                            // Insert summary after system prompt (if present) or at the beginning
                            let insert_pos = if messages_to_keep.first().map(|m| matches!(m.role, Role::System)).unwrap_or(false) {
                                1
                            } else {
                                0
                            };
                            messages_to_keep.insert(insert_pos, summary_message);
                            
                            // Update messages
                            let old_count = messages.len();
                            let new_count = messages_to_keep.len();
                            let tokens_saved = current_tokens.saturating_sub(token_counter::estimate_tokens_for_messages(&messages_to_keep));
                            
                            messages = messages_to_keep;
                            
                            // Send context summarized notification
                            if let Some(tx) = agent_tx.as_ref() {
                                let _ = tx.send(AgentUpdate::ContextSummarized {
                                    turn_id,
                                    old_count,
                                    new_count,
                                    tokens_saved,
                                });
                            }
                            
                            log::info!("✅ Context summarized: {} -> {} messages, {} tokens saved", old_count, new_count, tokens_saved);
                        }
                        Err(e) => {
                            log::warn!("⚠️ Failed to summarize context: {}", e);
                            // Continue with original messages if summarization fails
                        }
                    }
                }
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

