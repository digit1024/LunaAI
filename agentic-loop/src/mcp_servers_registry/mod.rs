use std::{collections::HashMap, sync::Arc};

use rust_mcp_sdk::schema::CallToolResult;
use tokio::sync::RwLock;
use tracing::error;

use crate::{error::{AgenticLoopError, Result}, mcp_servers_registry::model::{ServerStatus, ServerWithStatus}};
pub mod model;
//Registry of MCP servers with connections to the servers
pub struct MCPServerRegistry {
    pub servers: HashMap<String, Arc<RwLock<crate::mcp_connection::MCPConnection>>>,
    
    
    pub tools_white_list: Vec<String>, // tool_name -> enabled
    pub failed_servers: HashMap<String, String>, // server_name -> error_message
}

impl MCPServerRegistry {
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
    
            tools_white_list: Vec::new(),
            failed_servers: HashMap::new(),
        }
    }

    /// Initialize the MCP server registry from a configuration file
    pub async fn initialize_from_config(&mut self, config: &crate::mcp_config::MCPConfig) -> Result<&mut Self> {
        for (server_name , server_config )in config.servers.clone() {
            
            
            let mut server_connection = crate::mcp_connection::MCPConnection::new(server_name.clone(), server_config);
            // When Connect returns error, insert the server_name into failed_servers and continue with the next server
            match server_connection.connect().await {
                Ok(_) => {
                    
                    // Discover tools
                    match server_connection.update_tools().await {
                        Ok(_) => {
                            // WE WERE ABLE TO CONNECT AND DISCOVER TOOLS
                            
                            self.servers.insert(server_name.clone(), Arc::new(RwLock::new(server_connection)));
                        },
                        Err(e) => {
                            error!("Failed to update tools for MCP server '{}': {}", server_name, e);
                            self.failed_servers.insert(server_name.clone(), e.to_string());
                            continue;
                        }
                    }
                    
                }, 
                Err(e) => {
                    self.failed_servers.insert(server_name, e.to_string());
                }
            }
            

            
        }
        Ok(self)
    }

    /// Get the tools list from the MCP servers
    pub async fn get_enabled_tools(&self) -> Result<Vec<rust_mcp_sdk::schema::Tool>> {
        
        //otherwise return only the tools in the tools_white_list
        let mut tools = Vec::new();
        for (_, server_connection) in self.servers.iter() {
            
                let server_tools = server_connection.read().await.tools().iter().filter(|tool| self.tools_white_list.contains(&tool.name.clone())).cloned().collect::<Vec<rust_mcp_sdk::schema::Tool>>();
                tools.extend(server_tools.clone());
            }
        Ok(tools)
    }

    pub async fn get_all_tools(&self) -> Result<Vec<rust_mcp_sdk::schema::Tool>> {
        let mut tools = Vec::new();
        for (_, server_connection) in self.servers.iter() {
            let server_tools = server_connection.read().await.tools().iter().cloned().collect::<Vec<rust_mcp_sdk::schema::Tool>>();
            tools.extend(server_tools.clone());
        }
        Ok(tools)
    }
    pub async fn get_all_tools_by_server_name(&self, server_name: &str) -> Result<Vec<rust_mcp_sdk::schema::Tool>> {
        if let Some(server_connection) = self.servers.get(server_name) {
            let server_tools = server_connection.read().await.tools().iter().cloned().collect::<Vec<rust_mcp_sdk::schema::Tool>>();
            Ok(server_tools)
        } else {
            Ok(Vec::new()) // Return empty vec if server not found
        }
    }
    pub async fn get_enabled_tools_by_server_name(&self, server_name: &str) -> Result<Vec<rust_mcp_sdk::schema::Tool>> {
        if let Some(server_connection) = self.servers.get(server_name) {
            let server_tools = server_connection.read().await.tools().iter().filter(|tool| self.tools_white_list.contains(&tool.name.clone())).cloned().collect::<Vec<rust_mcp_sdk::schema::Tool>>();
            Ok(server_tools)
        } else {
            Ok(Vec::new()) // Return empty vec if server not found
        }
    }
    
    pub async fn get_all_server_names_and_statuses(&self) -> Result<Vec<ServerWithStatus>> {
        let mut  all_servers_with_statuses = self.servers.keys().cloned().collect::<Vec<String>>().iter().map(|name| ServerWithStatus { server_name: name.clone(), server_status: ServerStatus::Connected }).collect::<Vec<ServerWithStatus>>() ;
        let failed_server_names = self.failed_servers.keys().cloned().collect::<Vec<String>>().iter().map(|name| {
            let error_msg = self.failed_servers.get(name).cloned().unwrap_or_else(|| "Unknown error".to_string());
            ServerWithStatus { server_name: name.clone(), server_status: ServerStatus::Failed(error_msg) }
        }).collect::<Vec<ServerWithStatus>>();
        all_servers_with_statuses.extend(failed_server_names);
        Ok(all_servers_with_statuses)
    }

    //disables tool by name - removes it from the tools_white_list
    pub async fn  disable_tool(&mut self, tool_name: &str) {
        self.tools_white_list.retain(|tool| tool != tool_name);
    }
    //enables tool by name - adds it to the tools_white_list
    pub async fn  enable_tool(&mut self, tool_name: &str) {
        self.tools_white_list.push(tool_name.to_string());
    }
    //enables all tools
    pub async fn  enable_all_tools(&mut self) {
        let mut tools = Vec::new();
        for (_, server_connection) in self.servers.iter() {
            let server_tools = server_connection.read().await.tools().iter().cloned().collect::<Vec<rust_mcp_sdk::schema::Tool>>();
            tools.extend(server_tools.clone());
        }
        self.tools_white_list.extend(tools.iter().map(|tool| tool.name.clone()));
    }
    //disables all tools
    pub async fn disable_all_tools(&mut self) {
        self.tools_white_list.clear();
    }
    //disable  tool by server name 
    pub async fn disable_tool_by_server_name(&mut self, server_name: &str) {
        if let Some(server_connection) = self.servers.get(server_name) {
            let tool_names: Vec<String> = server_connection.read().await.tools().iter().map(|tool| tool.name.clone()).collect();
            for tool_name in tool_names {
                self.disable_tool(&tool_name).await;
            }
        } else {
            tracing::warn!(server_name = %server_name, "Server not found in registry, skipping disable_tool_by_server_name");
        }
    }
    //enable tool by server name
    pub async fn enable_tools_by_server_name(&mut self, server_name: &str) {
        if let Some(server_connection) = self.servers.get(server_name) {
            let tool_names: Vec<String> = server_connection.read().await.tools().iter().map(|tool| tool.name.clone()).collect();
            for tool_name in tool_names {
                self.enable_tool(&tool_name).await;
            }
        } else {
            tracing::warn!(server_name = %server_name, "Server not found in registry, skipping enable_tools_by_server_name");
        }
    }
    pub async fn enable_tools_for_multiple_servers(&mut self, server_names: Vec<String>) {
        // If server_names is empty, enable all tools from all connected servers
        if server_names.is_empty() {
            self.enable_all_tools().await;
            return;
        }
        
        // Otherwise, enable tools only for the specified servers
        for server_name in server_names {
            self.enable_tools_by_server_name(&server_name).await;
        }
    }

    //call tool 
    pub async fn call_tool(&mut self, tool_name: String, arguments: serde_json::Map<String, serde_json::Value>) -> Result<CallToolResult> {
        //find the server connection for the tool
        for (_, server_connection) in self.servers.iter() {
            let connection_guard = server_connection.read().await;
            if connection_guard.tools().iter().any(|tool| tool.name == tool_name) {
                // Found the server with this tool, drop the guard and call the tool
                drop(connection_guard);
                return server_connection.read().await.call_tool(tool_name, arguments).await;
            }
        }
        Err(AgenticLoopError::MCPConnectionError(
            format!("Tool '{}' not found in any MCP server", tool_name)
        ))
    }

    /// Find the name of the MCP server that owns a tool, if any.
    pub async fn find_server_for_tool(&self, tool_name: &str) -> Option<String> {
        for (server_name, server_connection) in self.servers.iter() {
            if server_connection
                .read()
                .await
                .tools()
                .iter()
                .any(|tool| tool.name == tool_name)
            {
                return Some(server_name.clone());
            }
        }
        None
    }

    /// Restart a registered MCP server by name.
    ///
    /// Best-effort shuts down the existing child process, re-spawns it, and
    /// re-discovers tools. Returns an error if the server is not registered
    /// or if the reconnect/tool-discovery fails.
    pub async fn restart_server(&self, server_name: &str) -> Result<()> {
        let server_connection = self.servers.get(server_name).ok_or_else(|| {
            AgenticLoopError::MCPConnectionError(format!(
                "Server '{}' not found in registry",
                server_name
            ))
        })?;
        server_connection.write().await.restart().await?;
        Ok(())
    }


}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_config::MCPConfig;
    
    use std::path::PathBuf;

    fn test_data_path(filename: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("mcp_config")
            .join("test_data")
            .join(filename)
    }

    #[tokio::test]
    async fn test_initialize_from_config() {
        let mut mcp_server_registry = MCPServerRegistry::new();
        let path = test_data_path("sample_config.json");
        let config = MCPConfig::load_from_json(&path.as_path()).unwrap();
        mcp_server_registry.initialize_from_config(&config).await.unwrap();

        // 1 server shoudl be connected - one shoudl be failed
        assert_eq!(mcp_server_registry.servers.len(), 1); //FILESYSTEM MCP server should be connected
        assert_eq!(mcp_server_registry.failed_servers.len(), 1); //WEATHER_API MCP server should be failed
        assert!(mcp_server_registry.servers.contains_key("filesystem"));
        assert!(mcp_server_registry.failed_servers.contains_key("weather"));

        // Enable all tools
        mcp_server_registry.enable_all_tools().await;
        
        assert_eq!(mcp_server_registry.get_enabled_tools().await.unwrap().len(), 14, "Expected 14 tools");
        mcp_server_registry.disable_tool("move_file").await;
        assert_eq!(mcp_server_registry.get_enabled_tools().await.unwrap().len(), 13, "Expected 13 tools");
        mcp_server_registry.enable_tool("move_file").await;
        assert_eq!(mcp_server_registry.get_enabled_tools().await.unwrap().len(), 14, "Expected 14 tools");
        mcp_server_registry.disable_tool_by_server_name("filesystem").await;
        assert_eq!(mcp_server_registry.get_enabled_tools().await.unwrap().len(), 0, "Expected all tools disabled");
        mcp_server_registry.enable_all_tools().await;
        assert_eq!(mcp_server_registry.get_enabled_tools().await.unwrap().len(), 14, "Expected all tools enabled");
        mcp_server_registry.disable_all_tools().await;
        assert_eq!(mcp_server_registry.get_enabled_tools().await.unwrap().len(), 0, "Expected all tools disabled");
        assert_eq!(mcp_server_registry.get_all_tools().await.unwrap().len(), 14, "Expected all tools available");
        mcp_server_registry.enable_all_tools().await;
        mcp_server_registry.disable_tool("move_file").await;
        assert_eq!(mcp_server_registry.get_enabled_tools_by_server_name("filesystem").await.unwrap().len(), 13, "Expected 13 tools enabled");
        assert_eq!(mcp_server_registry.get_all_tools_by_server_name("filesystem").await.unwrap().len(), 14, "Expected all tools available");
        

        //call tool list_directory
        let result = mcp_server_registry.call_tool("list_directory".into(), serde_json::json!({"path":"/tmp"}).as_object().unwrap().clone()).await.unwrap();
        // shoudl be successfull 
        assert!(result.is_error== Some(false) ||result.is_error== None, "Expected tool call to be successful");
        


    }
}