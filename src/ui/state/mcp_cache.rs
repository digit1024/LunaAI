//! MCP Cache State
//!
//! Cached MCP state for sync UI access. Updated by async operations,
//! read by sync view functions.

use crate::llm::ToolDefinition;
use agentic_loop::mcp_servers_registry::model::ServerStatus;
use std::collections::HashMap;

/// Server status info for cache
#[derive(Debug, Clone)]
pub struct ServerStatusInfo {
    pub server_name: String,
    pub server_status: ServerStatus,
}

/// Cached MCP state for UI access
#[derive(Debug, Clone, Default)]
pub struct MCPCache {
    /// All available tools
    pub all_tools: Vec<ToolDefinition>,
    /// Enabled tools only
    pub enabled_tools: Vec<ToolDefinition>,
    /// Tool enable/disable state (tool_name -> enabled)
    pub tool_states: HashMap<String, bool>,
    /// Server statuses (server_name -> status)
    pub server_statuses: HashMap<String, ServerStatus>,
    /// Tools grouped by server (server_name -> tools)
    pub tools_by_server: HashMap<String, Vec<ToolDefinition>>,
    /// All server names (sorted)
    pub all_server_names: Vec<String>,
}

impl MCPCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Update tool states from enabled tools list
    pub fn update_tool_states_from_enabled(&mut self) {
        let enabled_names: std::collections::HashSet<String> = self
            .enabled_tools
            .iter()
            .map(|t| t.name.clone())
            .collect();
        
        // Update states based on enabled tools
        for tool in &self.all_tools {
            self.tool_states
                .insert(tool.name.clone(), enabled_names.contains(&tool.name));
        }
    }

    /// Get enabled tools count
    pub fn enabled_tools_count(&self) -> usize {
        self.enabled_tools.len()
    }

    /// Get total tools count
    pub fn total_tools_count(&self) -> usize {
        self.all_tools.len()
    }

    /// Check if a tool is enabled
    pub fn is_tool_enabled(&self, tool_name: &str) -> bool {
        self.tool_states.get(tool_name).copied().unwrap_or(false)
    }

    /// Get tools for a server
    pub fn get_tools_for_server(&self, server_name: &str) -> Vec<ToolDefinition> {
        self.tools_by_server
            .get(server_name)
            .cloned()
            .unwrap_or_default()
    }

    /// Get server status
    pub fn get_server_status(&self, server_name: &str) -> Option<&ServerStatus> {
        self.server_statuses.get(server_name)
    }
}

