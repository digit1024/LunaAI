use crate::config::MCPConfig;
use crate::llm::{ToolCall, ToolDefinition, ToolResult};
use crate::mcp::transport::MCPTransport;
use anyhow::Result;
use log::{error, info};
use std::collections::{HashMap, HashSet};
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

#[derive(Debug, Clone, PartialEq)]
pub enum ServerStatus {
    Connected,
    Failed(String), // error message
}

pub struct MCPServerRegistry {
    pub servers: HashMap<String, Arc<RwLock<MCPTransportEnum>>>,
    pub tool_index: HashMap<String, String>, // tool_name -> server_name
    pub all_tools: Vec<ToolDefinition>,
    pub enabled_tools: HashMap<String, bool>, // tool_name -> enabled
    pub failed_servers: HashMap<String, String>, // server_name -> error_message
}

impl MCPServerRegistry {
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
            tool_index: HashMap::new(),
            all_tools: Vec::new(),
            enabled_tools: HashMap::new(),
            failed_servers: HashMap::new(),
        }
    }

    pub fn get_available_tools(&self) -> Vec<ToolDefinition> {
        self.all_tools.clone()
    }

    pub fn get_enabled_tools(&self) -> Vec<ToolDefinition> {
        self.all_tools
            .iter()
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

    pub fn apply_profile_tool_defaults(&mut self, allowed_servers: &[String]) {
        if allowed_servers.is_empty() {
            return;
        }

        let allowed: HashSet<String> = allowed_servers
            .iter()
            .map(|name| name.trim().to_lowercase())
            .filter(|name| !name.is_empty())
            .collect();

        if allowed.is_empty() {
            return;
        }

        for (tool_name, server_name) in &self.tool_index {
            let enabled = allowed.contains(&server_name.to_lowercase());
            self.enabled_tools.insert(tool_name.clone(), enabled);
        }
    }

    pub fn get_tool_states(&self) -> HashMap<String, bool> {
        self.enabled_tools.clone()
    }

    pub fn set_server_enabled(&mut self, server_name: &str, enabled: bool) {
        for (tool_name, tool_server_name) in &self.tool_index {
            if tool_server_name == server_name {
                self.enabled_tools.insert(tool_name.clone(), enabled);
            }
        }
    }

    pub fn is_server_enabled(&self, server_name: &str) -> bool {
        // A server is considered enabled if at least one of its tools is enabled
        // This is a simple heuristic - we could also check if all tools are enabled
        for (tool_name, tool_server_name) in &self.tool_index {
            if tool_server_name == server_name {
                if self.is_tool_enabled(tool_name) {
                    return true;
                }
            }
        }
        false
    }

    pub fn get_server_for_tool(&self, tool_name: &str) -> Result<&String> {
        self.tool_index
            .get(tool_name)
            .ok_or_else(|| anyhow::anyhow!("Tool {} not found", tool_name))
    }

    pub async fn call_tool(&mut self, tool_call: ToolCall) -> Result<ToolResult> {
        let server_name = self.get_server_for_tool(&tool_call.name)?;
        let server = self
            .servers
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server {} not found", server_name))?;

        let mut server_guard = server.write().await;
        server_guard.call_tool(tool_call).await
    }

    pub async fn initialize_from_config(&mut self, mcp_config: &MCPConfig) -> Result<()> {
        // Load MCP servers from configuration (Claude Desktop format)
        for (server_name, server_config) in &mcp_config.servers {
            match self
                .add_stdio_server(
                    server_name.clone(),
                    server_config.command.clone(),
                    server_config.args.clone(),
                    server_config.env.clone(),
                )
                .await
            {
                Ok(_) => {
                    info!("Successfully connected to MCP server {}", server_name);
                    // Remove from failed_servers if it was there before
                    self.failed_servers.remove(server_name);
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    error!("Failed to connect to MCP server {}: {}", server_name, error_msg);
                    self.failed_servers.insert(server_name.clone(), error_msg);
                }
            }
        }
        Ok(())
    }

    pub async fn add_stdio_server(
        &mut self,
        name: String,
        command: String,
        args: Vec<String>,
        env: HashMap<String, String>,
    ) -> Result<()> {
        let mut client = super::stdio_client::StdioMCPClient::new(command, args, env);

        // Try to connect
        match client.connect().await {
            Ok(_) => {
                // Connection successful, discover tools
                info!(
                    "MCP server {} connected successfully, discovering tools...",
                    name
                );
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
                self.servers.insert(
                    name.clone(),
                    Arc::new(RwLock::new(MCPTransportEnum::Stdio(client))),
                );
            }
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "Failed to connect to MCP server {}: {}",
                    name,
                    e
                ));
            }
        }
        Ok(())
    }

    pub fn get_server_status(&self, server_name: &str) -> ServerStatus {
        if self.servers.contains_key(server_name) {
            ServerStatus::Connected
        } else if let Some(error_msg) = self.failed_servers.get(server_name) {
            ServerStatus::Failed(error_msg.clone())
        } else {
            // Server not found in either map - shouldn't happen, but return Failed
            ServerStatus::Failed("Server not initialized".to_string())
        }
    }

    pub fn get_tools_by_server(&self) -> HashMap<String, Vec<ToolDefinition>> {
        let mut tools_by_server: HashMap<String, Vec<ToolDefinition>> = HashMap::new();

        for tool in &self.all_tools {
            if let Some(server_name) = self.tool_index.get(&tool.name) {
                tools_by_server
                    .entry(server_name.clone())
                    .or_insert_with(Vec::new)
                    .push(tool.clone());
            }
        }

        // Sort tools alphabetically by name within each server
        for tools in tools_by_server.values_mut() {
            tools.sort_by(|a, b| a.name.cmp(&b.name));
        }

        tools_by_server
    }

    pub fn get_all_server_names(&self, config: &MCPConfig) -> Vec<String> {
        let mut server_names: Vec<String> = config.servers.keys().cloned().collect();
        server_names.sort();
        server_names
    }
}
