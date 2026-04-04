//! Connection for single MCP server
//!
//! This module provides a connection manager for a single MCP server using stdio transport.
pub mod model;
mod no_stderr_transport;

use std::sync::Arc;
use rust_mcp_sdk::schema::{CallToolRequestParams, CallToolResult};
use tracing::{debug, info};

use crate::error::{AgenticLoopError, Result};
use crate::mcp_config::model::MCPServerConfig;

use no_stderr_transport::NoStderrTransport;
use rust_mcp_sdk::{
    mcp_client::{client_runtime, ClientHandler, McpClientOptions},
    schema::{ClientCapabilities, Implementation, InitializeRequestParams},
    McpClient, ToMcpClientHandler, TransportOptions,
};

fn default_client_details() -> InitializeRequestParams 
{
    InitializeRequestParams {
        capabilities: ClientCapabilities::default(),
        client_info: Implementation {
            name: "agentic-loop".into(),
            version: "0.1.0".into(),
            title: Some("Agentic Loop MCP Client".into()),
            description: Some("MCP Client for Agentic Loop".into()),
            icons: vec![],
            website_url: None,
        },
        protocol_version: "2024-11-05".into(),
        meta: None,
    }
}

/// MCP Connection manages a connection to a single MCP server
pub struct MCPConnection {
    server_name: String,
    config: MCPServerConfig,
    client: Option<Arc<dyn McpClient + Send + Sync>>,
    tools: Vec<rust_mcp_sdk::schema::Tool>,
}

impl MCPConnection {
    /// Create a new MCP connection for the given server configuration
    pub fn new(server_name: String, config: MCPServerConfig) -> Self {
        Self {
            server_name,
            config,
            client: None,
            tools: Vec::new(),
        }
    }

    /// Connect to the MCP server by spawning the process and establishing the connection
    pub async fn connect(&mut self) -> Result<&mut Self> {
        debug!(
            server = %self.server_name,
            command = %self.config.command,
            "Starting MCP server process"
        );

        // Step 1: Define client details and capabilities
        let client_details: InitializeRequestParams = default_client_details();

        // Step 2: Create transport with server launch
        // Use env HashMap directly (or None if empty)`
        let env: Option<std::collections::HashMap<String, String>> = if self.config.env.is_empty() {
            None
        } else {
            Some(self.config.env.clone())
        };

        let transport = NoStderrTransport::create_with_server_launch(
            &self.config.command,
            self.config.args.clone(),
            env,
            TransportOptions::default(),
        )
        .map_err(|e| AgenticLoopError::MCPConnectionError(
            format!("Failed to create stdio transport: {}", e)
        ))?;

        // Step 3: Create handler
        let handler = SimpleClientHandler;

        // Step 4: Create MCP client
        let client = client_runtime::create_client(McpClientOptions {
            client_details,
            transport,
            handler: handler.to_mcp_client_handler(),
            task_store: None,
            server_task_store: None,
            message_observer: None,
        });

        // Step 5: Start the MCP client
        client.clone().start().await
            .map_err(|e| AgenticLoopError::MCPConnectionError(
                format!("Failed to start MCP client: {}", e)
            ))?;

        self.client = Some(client);

        info!(server = %self.server_name, "Successfully connected to MCP server");
        Ok(self)
    }

    /// Disconnect from the MCP server by shutting down the client
    pub async fn disconnect(&mut self) -> Result<()> {
        if let Some(client) = self.client.take() {
            debug!(server = %self.server_name, "Disconnecting from MCP server");
            client.shut_down().await
                .map_err(|e| AgenticLoopError::MCPConnectionError(
                    format!("Failed to shut down MCP client: {}", e)
                ))?;
        }
        Ok(())
    }

    /// Get a reference to the MCP client (if connected)
    pub fn client(&self) -> Option<&Arc<dyn McpClient + Send + Sync>> {
        self.client.as_ref()
    }

    /// Get the server name
    pub fn server_name(&self) -> &str {
        &self.server_name

    }
    //get the tools list from the MCP server
    pub async fn update_tools(&mut self) -> Result<&mut Self> {
        match self.client().unwrap().request_tool_list(None).await {
            Ok(tools_list_result) => {
                self.tools = tools_list_result.tools;
                Ok(self)
            }
            Err(e) => {
                tracing::error!("Failed to get tools list for server '{}': {}", self.server_name, e);
                Err(AgenticLoopError::MCPConnectionError(
                    format!("Failed to get tools list: {}", e)
                ))
            }
        }
    }
    //get the tools list from the MCP server
    pub fn tools(&self) -> &Vec<rust_mcp_sdk::schema::Tool> {
        &self.tools
    }
//call a tool
pub async fn call_tool(&self, name: String, arguments: serde_json::Map<String, serde_json::Value>) -> Result<CallToolResult> {
    let tool_call = CallToolRequestParams {name, arguments:Some(arguments), meta: None, task: None};
    let tool_result = self.client().unwrap().request_tool_call(tool_call).await.map_err(|e| AgenticLoopError::MCPConnectionError(
        format!("Failed to call tool: {}", e)
    ))?;
    Ok(tool_result)
}

}

/// Simple client handler implementation
struct SimpleClientHandler;

#[async_trait::async_trait]
impl ClientHandler for SimpleClientHandler {
    // Use default implementations for all handler methods
    // Override specific methods as needed for custom behavior
    // I don't need to override any methods for now 
}


#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    //WARNING: This test will run the command "uvx mcp-shell-server" - It will fail if uvx is not installed or not in the path!
    #[tokio::test]
    async fn test_mcp_tool_flow() {
        //Step 1: Connect to the MCP server
        let mut mcp_connection = MCPConnection::new("test_mcp_server".into(), MCPServerConfig {
            command: "uvx".into(),
            args: vec!["mcp-shell-server".into()],
            env: HashMap::from([("ALLOW_COMMANDS".into(), "pwd".into())]),
        });
        mcp_connection.connect().await.unwrap();

    //Step 2: Get the tools list
     mcp_connection.update_tools().await.unwrap();
     let tools = mcp_connection.tools();
    assert!(tools.len() > 0 , "Tools list should not be empty for shell server");
    assert!(tools.iter().any(|tool| tool.name == "shell_execute"), "shell_execute tool should be available");
    
    //Step 3: Call the tools
    
    
    let tool_result = mcp_connection.call_tool(
        "shell_execute".into(),
        serde_json::json!({"directory":"/" , "command": ["pwd"]} ).as_object().unwrap().clone()).await.unwrap();
    assert_eq!(tool_result.is_error, Some(false),"Tool call should not return an error");

    //Step 4 pwd in / should return the current directory which is /
    //print the tool result ( you have to iterate over structured_content and print the text)
    // tool_result.content.iter().for_each(|c| {
    //     println!("Content: {:?}", c.as_text_content().unwrap().text);
    // });

    // Alternatevely  if we pass wrong params it shoudl fail

    
    
    let tool_result = mcp_connection.call_tool("shell_execute".into(),
        serde_json::json!({"directory":"/" , "command": ["ls"]} ).as_object().unwrap().clone()).await.unwrap();
    assert_eq!(tool_result.is_error, Some(true),"Tool call should return an error");
    
    // tool_result.content.iter().for_each(|c| {
    //     println!("Content: {:?}", c.as_text_content().unwrap().text);
    // });

    }
}