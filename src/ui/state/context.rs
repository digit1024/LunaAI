//! Context UI state management
//!
//! Manages UI context-related state:
//! - Expanded reasoning sections
//! - Expanded summary sections
//! - Tools context panel visibility
//! - Expanded MCP servers

use std::collections::HashSet;
use tracing::debug;

/// Context state for UI display
pub struct ContextState {
    /// Message indices with expanded reasoning
    pub expanded_reasoning: HashSet<usize>,
    
    /// Message indices with expanded summaries
    pub expanded_summaries: HashSet<usize>,
    
    /// Show tools context panel
    pub show_tools_context: bool,
    
    /// Expanded MCP servers (for settings/configuration)
    pub expanded_mcp_servers: HashSet<String>,
}

impl ContextState {
    /// Create a new context state
    pub fn new() -> Self {
        Self {
            expanded_reasoning: HashSet::new(),
            expanded_summaries: HashSet::new(),
            show_tools_context: false,
            expanded_mcp_servers: HashSet::new(),
        }
    }

    /// Toggle reasoning expansion for a message
    pub fn toggle_reasoning(&mut self, message_index: usize) {
        if self.expanded_reasoning.contains(&message_index) {
            self.expanded_reasoning.remove(&message_index);
            debug!(message_index, "Collapsed reasoning");
        } else {
            self.expanded_reasoning.insert(message_index);
            debug!(message_index, "Expanded reasoning");
        }
    }

    /// Check if reasoning is expanded for a message
    pub fn is_reasoning_expanded(&self, message_index: usize) -> bool {
        self.expanded_reasoning.contains(&message_index)
    }

    /// Toggle summary expansion for a message
    pub fn toggle_summary(&mut self, message_index: usize) {
        if self.expanded_summaries.contains(&message_index) {
            self.expanded_summaries.remove(&message_index);
            debug!(message_index, "Collapsed summary");
        } else {
            self.expanded_summaries.insert(message_index);
            debug!(message_index, "Expanded summary");
        }
    }

    /// Check if summary is expanded for a message
    pub fn is_summary_expanded(&self, message_index: usize) -> bool {
        self.expanded_summaries.contains(&message_index)
    }

    /// Show tools context panel
    pub fn show_tools_context(&mut self) {
        self.show_tools_context = true;
        debug!("Showing tools context panel");
    }

    /// Hide tools context panel
    pub fn hide_tools_context(&mut self) {
        self.show_tools_context = false;
        debug!("Hiding tools context panel");
    }

    /// Toggle tools context panel
    pub fn toggle_tools_context(&mut self) {
        self.show_tools_context = !self.show_tools_context;
        debug!(
            visible = self.show_tools_context,
            "Toggled tools context panel"
        );
    }

    /// Toggle MCP server expansion
    pub fn toggle_mcp_server(&mut self, server_name: String) {
        if self.expanded_mcp_servers.contains(&server_name) {
            self.expanded_mcp_servers.remove(&server_name);
            debug!(server_name = %server_name, "Collapsed MCP server");
        } else {
            self.expanded_mcp_servers.insert(server_name.clone());
            debug!(server_name = %server_name, "Expanded MCP server");
        }
    }

    /// Check if MCP server is expanded
    pub fn is_mcp_server_expanded(&self, server_name: &str) -> bool {
        self.expanded_mcp_servers.contains(server_name)
    }

    /// Clear all context state
    pub fn clear(&mut self) {
        self.expanded_reasoning.clear();
        self.expanded_summaries.clear();
        self.show_tools_context = false;
        self.expanded_mcp_servers.clear();
    }
}

impl Default for ContextState {
    fn default() -> Self {
        Self::new()
    }
}












