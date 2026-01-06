use anyhow::Context;
use config::{Config, ConfigError, File};
use serde::{
    de::{self, Deserializer, SeqAccess, Visitor},
    Deserialize, Serialize,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct LlmProfile {
    #[serde(default = "default_backend")]
    pub backend: String, // "openai", "anthropic", "deepseek", "ollama", "gemini"
    pub api_key: String,
    pub model: String,
    pub endpoint: String,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub profile_prompt_file: Option<String>,
    #[serde(default, deserialize_with = "deserialize_enabled_mcp")]
    pub enabled_mcp: Vec<String>,
    #[serde(default)]
    pub hidden: bool,
    /// Maximum context window size in tokens
    /// If not set, will be auto-detected based on model
    #[serde(default)]
    pub context_window_size: Option<usize>,
    /// Summarization threshold (0.0 - 1.0)
    /// When context usage reaches this percentage of context_window_size,
    /// summarization of older messages will be triggered
    /// Default: 0.7 (70% of context window)
    #[serde(default = "default_summarize_threshold")]
    pub summarize_threshold: f32,
}

fn default_backend() -> String {
    "openai".to_string()
}

fn default_summarize_threshold() -> f32 {
    0.7
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
            profile_prompt_file: None,
            enabled_mcp: Vec::new(),
            hidden: false,
            context_window_size: None,
            summarize_threshold: default_summarize_threshold(),
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
pub struct ServerConfig {
    #[serde(default = "default_server_enabled")]
    pub enabled: bool,
    #[serde(default = "default_server_host")]
    pub host: String,
    #[serde(default = "default_server_port")]
    pub port: u16,
    #[serde(default = "default_server_api_key")]
    pub api_key: String,
    #[serde(default = "default_stream_timeout_secs")]
    pub stream_timeout_secs: u64,
    #[serde(default = "default_healthcheck_interval_secs")]
    pub healthcheck_interval_secs: u64,
    #[serde(default = "default_wal_enabled")]
    pub wal_enabled: bool,
    #[serde(default = "default_wal_autocheckpoint")]
    pub wal_autocheckpoint: u32,
    #[serde(default = "default_sqlite_busy_timeout_ms")]
    pub sqlite_busy_timeout_ms: u64,
}

fn default_server_enabled() -> bool {
    true
}

fn default_server_host() -> String {
    "0.0.0.0".to_string()
}

fn default_server_port() -> u16 {
    8080
}

fn default_server_api_key() -> String {
    "LUna".to_string()
}

fn default_stream_timeout_secs() -> u64 {
    600  // 10 minutes - increased to support long-running streams
}

fn default_healthcheck_interval_secs() -> u64 {
    30
}

fn default_wal_enabled() -> bool {
    true
}

fn default_wal_autocheckpoint() -> u32 {
    200
}

fn default_sqlite_busy_timeout_ms() -> u64 {
    5000
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            enabled: default_server_enabled(),
            host: default_server_host(),
            port: default_server_port(),
            api_key: default_server_api_key(),
            stream_timeout_secs: default_stream_timeout_secs(),
            healthcheck_interval_secs: default_healthcheck_interval_secs(),
            wal_enabled: default_wal_enabled(),
            wal_autocheckpoint: default_wal_autocheckpoint(),
            sqlite_busy_timeout_ms: default_sqlite_busy_timeout_ms(),
        }
    }
}

fn default_summary_chars() -> u32 {
    1000
}

fn default_summary_loop_sleep_seconds() -> u64 {
    15
}

fn default_title_generation_system_prompt() -> String {
    "Your task is to generate a conversation title that will describe the topic easily. Keep original conversation language and tone. YOU SHOULD ALWAYS ANSWER ONLY WITH TITLE. MAXIMUM 100CHARS. You will receive a part of the conversation transcript in next message".to_string()
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct TitleSummaryConfig {
    #[serde(default)]
    pub title_generation_profile: Option<String>,
    #[serde(default = "default_summary_chars")]
    pub summary_chars: u32,
    #[serde(default = "default_summary_loop_sleep_seconds")]
    pub summary_loop_sleep_seconds: u64,
    #[serde(default = "default_title_generation_system_prompt")]
    pub title_generation_system_prompt: String,
}

impl Default for TitleSummaryConfig {
    fn default() -> Self {
        Self {
            title_generation_profile: None,
            summary_chars: default_summary_chars(),
            summary_loop_sleep_seconds: default_summary_loop_sleep_seconds(),
            title_generation_system_prompt: default_title_generation_system_prompt(),
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
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub title_summary: TitleSummaryConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        let mut profiles = HashMap::new();
        // Add default OpenAI profile
        profiles.insert("openai".to_string(), LlmProfile::default());
        Self {
            default: "openai".to_string(),
            profiles,
            prompts: crate::prompts::PromptConfig::default(),
            mcp: MCPConfig::default(),
            server: ServerConfig::default(),
            title_summary: TitleSummaryConfig::default(),
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

    pub fn config_toml_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn load() -> Result<Self, ConfigError> {
        Self::load_from_path(None::<&Path>)
    }

    pub fn load_from_path<P: AsRef<Path>>(path: Option<P>) -> Result<Self, ConfigError> {
        let config_path = path
            .as_ref()
            .map(|p| p.as_ref().to_path_buf())
            .unwrap_or_else(Self::config_file_path);

        if path.is_none() {
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
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

    #[allow(dead_code)] // Public API method
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        use std::fs;
        use toml;
        use chrono::Local;

        let config_path = Self::config_toml_path();

        // Create config directory if it doesn't exist
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Create backup if config file exists
        if config_path.exists() {
            let now = Local::now();
            let backup_filename = format!(
                "config_bcp_{}.toml",
                now.format("%Y_%m_%d_%H_%M_%S")
            );
            let backup_path = config_path.parent()
                .ok_or_else(|| anyhow::anyhow!("Config path has no parent directory"))
                .with_context(|| format!("Failed to create backup path for {}", config_path.display()))?
                .join(backup_filename);
            fs::copy(&config_path, &backup_path)?;
        }

        let toml_string = toml::to_string_pretty(self)?;
        fs::write(config_path, toml_string)?;
        Ok(())
    }

    pub fn resolve_config_path(path: &str) -> PathBuf {
        let candidate = Path::new(path);
        if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            Self::config_dir().join(candidate)
        }
    }
}

fn deserialize_enabled_mcp<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct StringOrVecVisitor;

    impl<'de> Visitor<'de> for StringOrVecVisitor {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a comma-separated string or a list of MCP server names")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(parse_mcp_csv(value))
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(parse_mcp_csv(&value))
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut values = Vec::new();
            while let Some(value) = seq.next_element::<String>()? {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    values.push(trimmed.to_string());
                }
            }
            Ok(values)
        }
    }

    deserializer.deserialize_any(StringOrVecVisitor)
}

fn parse_mcp_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|entry| entry.trim())
        .filter(|entry| !entry.is_empty())
        .map(|entry| entry.to_string())
        .collect()
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
    pub fn mcp_config_path() -> PathBuf {
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
            server_config.args = server_config
                .args
                .iter()
                .map(|arg| Self::expand_env_var_string(arg))
                .collect();

            // Expand env values
            server_config.env = server_config
                .env
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
