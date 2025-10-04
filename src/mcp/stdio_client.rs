use super::MCPTransport;
use crate::llm::ToolResult;
use crate::llm::{ToolDefinition, ToolCall};
use anyhow::Result;
use async_trait::async_trait;
use log::debug;
use serde_json;
use std::collections::HashMap;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

pub struct StdioMCPClient {
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub process: Option<Child>,
    pub stdin: Option<ChildStdin>,
    pub stdout: Option<BufReader<ChildStdout>>,
    pub tools: Vec<ToolDefinition>,
    pub request_id: u64,
}

impl StdioMCPClient {
    pub fn new(command: String, args: Vec<String>, env: HashMap<String, String>) -> Self {
        Self {
            command,
            args,
            env,
            process: None,
            stdin: None,
            stdout: None,
            tools: Vec::new(),
            request_id: 1,
        }
    }
    
    async fn send_request(&mut self, request: super::protocol::MCPRequest) -> Result<super::protocol::MCPResponse> {
        if self.process.is_none() {
            self.connect().await?;
        }
        
        let request_json = serde_json::to_string(&request)?;
        
        if let Some(ref mut stdin) = self.stdin {
            stdin.write_all(request_json.as_bytes()).await?;
            stdin.write_all(b"\n").await?;
            stdin.flush().await?;
        }
        
        // Read response
        if let Some(ref mut stdout) = self.stdout {
            let mut line = String::new();
            match stdout.read_line(&mut line).await {
                Ok(_) => {
                    debug!("MCP Response: {}", line);
                    let response: super::protocol::MCPResponse = serde_json::from_str(&line)?;
                    Ok(response)
                }
                Err(e) => Err(anyhow::anyhow!("Failed to read response: {}", e))
            }
        } else {
            Err(anyhow::anyhow!("No stdout available"))
        }
    }
    
    async fn send_initialized_notification(&mut self) -> Result<()> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        
        let notification_json = serde_json::to_string(&notification)?;
        
        if let Some(ref mut stdin) = self.stdin {
            stdin.write_all(notification_json.as_bytes()).await?;
            stdin.write_all(b"\n").await?;
            stdin.flush().await?;
        }
        
        Ok(())
    }
}

#[async_trait]
impl MCPTransport for StdioMCPClient {
    async fn connect(&mut self) -> Result<()> {
        debug!("Starting MCP server: {} {:?}", self.command, self.args);
        
        let mut cmd = Command::new(&self.command);
        cmd.args(&self.args);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        
        // Apply environment variables from config
        for (key, value) in &self.env {
            cmd.env(key, value);
        }
        
        let mut child = cmd.spawn()?;
        
        let stdin = child.stdin.take().ok_or_else(|| anyhow::anyhow!("Failed to get stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow::anyhow!("Failed to get stdout"))?;
        
        self.stdin = Some(stdin);
        self.stdout = Some(BufReader::new(stdout));
        self.process = Some(child);
        
        // Send initialize request
        let init_request = super::protocol::MCPRequest::initialize(self.request_id);
        self.request_id += 1;
        
        let response = self.send_request(init_request).await?;
        debug!("Initialize response: {:?}", response);
        
        // Send initialized notification to server (no need to wait for server response)
        self.send_initialized_notification().await?;
        
        Ok(())
    }
    
    async fn disconnect(&mut self) -> Result<()> {
        if let Some(mut process) = self.process.take() {
            let _ = process.kill().await;
        }
        self.stdin = None;
        self.stdout = None;
        Ok(())
    }
    
    async fn discover_tools(&mut self) -> Result<Vec<ToolDefinition>> {
        let request = super::protocol::MCPRequest::tools_list(self.request_id);
        self.request_id += 1;
        
        let response = self.send_request(request).await?;
        
        if let Some(result) = response.result {
            if let Ok(tools_response) = serde_json::from_value::<serde_json::Value>(result) {
                if let Some(tools) = tools_response.get("tools").and_then(|t| t.as_array()) {
                    let mut tool_definitions = Vec::new();
                    for tool in tools {
                        if let Ok(tool_def) = serde_json::from_value::<ToolDefinition>(tool.clone()) {
                            tool_definitions.push(tool_def);
                        }
                    }
                    self.tools = tool_definitions.clone();
                    return Ok(tool_definitions);
                }
            }
        }
        
        Ok(Vec::new())
    }
    
    async fn call_tool(&mut self, tool_call: ToolCall) -> Result<ToolResult> {
        let arguments = tool_call.parameters.clone();
        let request = super::protocol::MCPRequest::tools_call(self.request_id, tool_call.name, arguments);
        self.request_id += 1;
        
        let response = self.send_request(request).await?;
        
        if let Some(error) = response.error {
            return Ok(ToolResult {
                content: format!("Error: {}", error.message),
                is_error: true,
            });
        }
        
        if let Some(result) = response.result {
            // Try to parse as MCP tool result format first
            if let Ok(mcp_result) = serde_json::from_value::<serde_json::Value>(result.clone()) {
                if let Some(content_array) = mcp_result.get("content").and_then(|c| c.as_array()) {
                    if let Some(first_content) = content_array.first() {
                        if let Some(text_content) = first_content.get("text").and_then(|t| t.as_str()) {
                            return Ok(ToolResult {
                                content: text_content.to_string(),
                                is_error: false,
                            });
                        }
                    }
                }
            }
            
            // Fallback to simple string parsing
            match serde_json::from_value::<String>(result.clone()) {
                Ok(content) => Ok(ToolResult {
                    content,
                    is_error: false,
                }),
                Err(_) => Ok(ToolResult {
                    content: format!("Unexpected result format: {:?}", result),
                    is_error: true,
                })
            }
        } else {
            Ok(ToolResult {
                content: "No result received".to_string(),
                is_error: true,
            })
        }
    }
}
