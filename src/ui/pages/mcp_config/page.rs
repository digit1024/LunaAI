//! MCP Config page module
//!
//! Full libcosmic module for the MCP configuration interface.

use std::collections::HashSet;

/// MCP Config page state
pub struct Page {
    /// Expanded MCP servers (server names)
    pub expanded_servers: HashSet<String>,
}

impl Page {
    /// Create a new MCP config page
    pub fn new() -> Self {
        Self {
            expanded_servers: HashSet::new(),
        }
    }

    /// Toggle server expansion
    pub fn toggle_server(&mut self, server_name: String) {
        if self.expanded_servers.contains(&server_name) {
            self.expanded_servers.remove(&server_name);
        } else {
            self.expanded_servers.insert(server_name);
        }
    }
}

impl Default for Page {
    fn default() -> Self {
        Self::new()
    }
}

/// MCP Config page messages
#[derive(Debug, Clone)]
pub enum Message {
    /// Toggle server expansion
    ToggleServer(String),
}

