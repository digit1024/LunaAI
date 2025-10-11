use config::{Config, ConfigError, File};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct LlmProfile {
    #[serde(default = "default_backend")]
    pub backend: String,  // "openai", "anthropic", "deepseek", "ollama", "gemini"
    pub api_key: String,
    pub model: String,
    pub endpoint: String,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

fn default_backend() -> String {
    "openai".to_string()
}

impl Default for LlmProfile {
    fn default() -> Self {
        Self {
            backend: "openai".to_string(),
            api_key: "".to_string(),
            model: "gpt-3.5-turbo".to_string(),
            endpoint: "https://api.openai.com/v1".to_string(),
            temperature: Some(0.7),
            max_tokens: Some(1000),
        }
    }
}

// New Claude Desktop-style configuration
#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct MCPServerConfig {
    pub command: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>, // Per-server environment variables
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct MCPConfig {
    #[serde(rename = "mcpServers")]
    pub servers: HashMap<String, MCPServerConfig>,
}

impl Default for MCPConfig {
    fn default() -> Self {
        Self {
            servers: HashMap::new(),
        }
    }
}



#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct AppConfig {
    pub default: String,
    pub profiles: HashMap<String, LlmProfile>,
    #[serde(default)]
    pub prompts: crate::prompts::PromptConfig,
    #[serde(default)]
    pub mcp: MCPConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        let mut profiles = HashMap::new();
        // Add default OpenAI profile
        profiles.insert(
            "openai".to_string(),
            LlmProfile::default()
        );
        Self {
            default: "openai".to_string(),
            profiles,
            prompts: crate::prompts::PromptConfig::default(),
            mcp: MCPConfig::default(),
        }
    }
}

impl AppConfig {
    fn config_dir() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("cosmic_llm")
    }

    fn config_file_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    fn config_toml_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn load() -> Result<Self, ConfigError> {
        let config_path = Self::config_file_path();
        
        // Create config directory if it doesn't exist
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let config = Config::builder()
            .add_source(File::from(config_path))
            .build()?;

        config.try_deserialize()
    }

    pub fn get_default_profile(&self) -> Option<&LlmProfile> {
        self.profiles.get(&self.default)
    }

    pub fn get_profile(&self, name: &str) -> Option<&LlmProfile> {
        self.profiles.get(name)
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        use std::fs;
        use toml;
        
        let config_path = Self::config_toml_path();
        
        // Create config directory if it doesn't exist
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        let toml_string = toml::to_string_pretty(self)?;
        fs::write(config_path, toml_string)?;
        Ok(())
    }
}

impl MCPConfig {
    /// Load MCP configuration from separate mcp_config.json file (Claude Desktop format)
    pub fn load_from_json() -> Result<Self, Box<dyn std::error::Error>> {
        let mcp_config_path = Self::mcp_config_path();
        
        if !mcp_config_path.exists() {
            return Ok(Self::default());
        }
        
        let content = std::fs::read_to_string(mcp_config_path)?;
        let mut config: MCPConfig = serde_json::from_str(&content)?;
        
        // Expand environment variables in all fields
        config.expand_env_vars();
        
        Ok(config)
    }
    
    /// Get the path to mcp_config.json
    fn mcp_config_path() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("cosmic_llm")
            .join("mcp_config.json")
    }
    
    /// Save MCP configuration to mcp_config.json
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mcp_config_path = Self::mcp_config_path();
        
        if let Some(parent) = mcp_config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        let json_string = serde_json::to_string_pretty(self)?;
        std::fs::write(mcp_config_path, json_string)?;
        Ok(())
    }
    
    /// Expand environment variables in format ${env:VAR_NAME}
    fn expand_env_vars(&mut self) {
        for server_config in self.servers.values_mut() {
            // Expand command
            server_config.command = Self::expand_env_var_string(&server_config.command);
            
            // Expand args
            server_config.args = server_config.args
                .iter()
                .map(|arg| Self::expand_env_var_string(arg))
                .collect();
            
            // Expand env values
            server_config.env = server_config.env
                .iter()
                .map(|(k, v)| (k.clone(), Self::expand_env_var_string(v)))
                .collect();
        }
    }
    
    /// Expand environment variables in a single string
    fn expand_env_var_string(value: &str) -> String {
        // Simple regex-free implementation
        let mut result = value.to_string();
        
        while let Some(start) = result.find("${env:") {
            if let Some(end) = result[start..].find('}') {
                let var_name = &result[start + 6..start + end];
                let env_value = std::env::var(var_name).unwrap_or_default();
                result.replace_range(start..start + end + 1, &env_value);
            } else {
                break;
            }
        }
        
        result
    }
}