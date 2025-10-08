use crate::llm::{ToolDefinition, ToolCall, ToolResult};
use crate::config::MCPConfig;
use crate::mcp::transport::MCPTransport;
use anyhow::Result;
use log::{error, info};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub enum MCPTransportEnum {
    Stdio(super::stdio_client::StdioMCPClient),
}

#[async_trait::async_trait]
impl super::transport::MCPTransport for MCPTransportEnum {
    async fn connect(&mut self) -> Result<()> {
        match self {
            MCPTransportEnum::Stdio(client) => client.connect().await,
        }
    }
    
    async fn disconnect(&mut self) -> Result<()> {
        match self {
            MCPTransportEnum::Stdio(client) => client.disconnect().await,
        }
    }
    
    async fn discover_tools(&mut self) -> Result<Vec<ToolDefinition>> {
        match self {
            MCPTransportEnum::Stdio(client) => client.discover_tools().await,
        }
    }
    
    async fn call_tool(&mut self, tool_call: ToolCall) -> Result<ToolResult> {
        match self {
            MCPTransportEnum::Stdio(client) => client.call_tool(tool_call).await,
        }
    }
}

pub struct MCPServerRegistry {
    pub servers: HashMap<String, Arc<RwLock<MCPTransportEnum>>>,
    pub tool_index: HashMap<String, String>, // tool_name -> server_name
    pub all_tools: Vec<ToolDefinition>,
    pub enabled_tools: HashMap<String, bool>, // tool_name -> enabled
}

impl MCPServerRegistry {
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
            tool_index: HashMap::new(),
            all_tools: Vec::new(),
            enabled_tools: HashMap::new(),
        }
    }
    
    pub fn get_available_tools(&self) -> Vec<ToolDefinition> {
        self.all_tools.clone()
    }
    
    pub fn get_enabled_tools(&self) -> Vec<ToolDefinition> {
        self.all_tools.iter()
            .filter(|tool| self.is_tool_enabled(&tool.name))
            .cloned()
            .collect()
    }
    
    pub fn is_tool_enabled(&self, tool_name: &str) -> bool {
        self.enabled_tools.get(tool_name).copied().unwrap_or(true)
    }
    
    pub fn set_tool_enabled(&mut self, tool_name: &str, enabled: bool) {
        self.enabled_tools.insert(tool_name.to_string(), enabled);
    }
    
    pub fn enable_all_tools(&mut self) {
        for tool in &self.all_tools {
            self.enabled_tools.insert(tool.name.clone(), true);
        }
    }
    
    pub fn disable_all_tools(&mut self) {
        for tool in &self.all_tools {
            self.enabled_tools.insert(tool.name.clone(), false);
        }
    }
    
    pub fn get_tool_states(&self) -> HashMap<String, bool> {
        self.enabled_tools.clone()
    }
    
    pub fn get_server_for_tool(&self, tool_name: &str) -> Result<&String> {
        self.tool_index.get(tool_name)
            .ok_or_else(|| anyhow::anyhow!("Tool {} not found", tool_name))
    }
    
    pub async fn call_tool(&mut self, tool_call: ToolCall) -> Result<ToolResult> {
        let server_name = self.get_server_for_tool(&tool_call.name)?;
        let server = self.servers.get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server {} not found", server_name))?;
        
        let mut server_guard = server.write().await;
        server_guard.call_tool(tool_call).await
    }
    
    pub async fn initialize_from_config(&mut self, mcp_config: &MCPConfig) -> Result<()> {
        // Load MCP servers from configuration (Claude Desktop format)
        for (server_name, server_config) in &mcp_config.servers {
            match self.add_stdio_server(
                server_name.clone(),
                server_config.command.clone(),
                server_config.args.clone(),
                server_config.env.clone(),
            ).await {
                Ok(_) => {
                    info!("Successfully connected to MCP server {}", server_name);
                },
                Err(e) => {
                    error!("Failed to connect to MCP server {}: {}", server_name, e);
                }
            }
        }
        Ok(())
    }
    
    pub async fn add_stdio_server(&mut self, name: String, command: String, args: Vec<String>, env: HashMap<String, String>) -> Result<()> {
        let mut client = super::stdio_client::StdioMCPClient::new(command, args, env);
        
        // Try to connect
        match client.connect().await {
            Ok(_) => {
                // Connection successful, discover tools
                info!("MCP server {} connected successfully, discovering tools...", name);
                let tools = client.discover_tools().await?;
                info!("MCP server {} discovered {} tools", name, tools.len());
                
                // Index tools
                for tool in &tools {
                    info!("MCP server {} tool: {}", name, tool.name);
                    self.tool_index.insert(tool.name.clone(), name.clone());
                    // Enable new tools by default
                    self.enabled_tools.insert(tool.name.clone(), true);
                }
                self.all_tools.extend(tools);
                
                // Store client
                self.servers.insert(name.clone(), Arc::new(RwLock::new(MCPTransportEnum::Stdio(client))));
            },
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to connect to MCP server {}: {}", name, e));
            }
        }
        Ok(())
    }
}
