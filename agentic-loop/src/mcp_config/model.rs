use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct MCPServerConfig {
    pub command: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>, // Per-server environment variables
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct MCPServers {
    #[serde(rename = "mcpServers")]
    pub servers: HashMap<String, MCPServerConfig>,
}

impl Default for MCPServers {
    fn default() -> Self {
        Self::new()
    }
}

impl MCPServers {
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
        }
    }
}