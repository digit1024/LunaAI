pub mod model;

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{AgenticLoopError, Result};
use crate::mcp_config::model::MCPServerConfig;
use std::collections::HashMap;

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct MCPConfig {
    #[serde(rename = "mcpServers")]
    pub servers: HashMap<String, MCPServerConfig>,
}

impl Default for MCPConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl MCPConfig {
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
        }
    }

    pub fn load_from_json(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Err(AgenticLoopError::MCPConfigNotFound(
                path.display().to_string(),
            ));
        }

        let content = std::fs::read_to_string(path).map_err(AgenticLoopError::MCPConfigReadError)?;
        let config: MCPConfig = serde_json::from_str(&content).map_err(AgenticLoopError::MCPConfigParseError)?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AgenticLoopError;
    use std::path::PathBuf;

    fn test_data_path(filename: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("mcp_config")
            .join("test_data")
            .join(filename)
    }

    #[test]
    fn test_load_from_json_when_path_does_not_exist() {
        let path = Path::new("nonExistingPath.json");
        let config_result = MCPConfig::load_from_json(path);
        assert!(config_result.is_err());
        
        match config_result.unwrap_err() {
            AgenticLoopError::MCPConfigNotFound(_) => {
                // Expected error variant
            }
            other => panic!("Expected MCPConfigNotFound, got: {:?}", other),
        }
    }

    #[test]
    fn test_load_from_json_success() {
        let path = test_data_path("sample_config.json");
        let config_result = MCPConfig::load_from_json(&path);
        
        assert!(config_result.is_ok(), "Failed to load config: {:?}", config_result.err());
        
        let config = config_result.unwrap();
        assert_eq!(config.servers.len(), 2, "Expected 2 servers");
        
        // Verify filesystem server
        let filesystem = config.servers.get("filesystem").expect("filesystem server not found");
        assert_eq!(filesystem.command, "npx");
        assert_eq!(filesystem.args.len(), 2);
        assert!(filesystem.env.is_empty());
        
        // Verify weather server
        let weather = config.servers.get("weather").expect("weather server not found");
        assert_eq!(weather.command, "npx");
        assert_eq!(weather.args.len(), 1);
        assert_eq!(weather.env.get("OPENWEATHER_API_KEY"), Some(&"test-key".to_string()));
    }
}