use anyhow::Context;
use config::{Config, ConfigError, File};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ── Model preset (identity + optional request params; no defaults sent to API) ──

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct ReasoningConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct ModelPreset {
    pub backend: String,
    pub model: String,
    /// Full URL we POST to (chat completions). No path appended.
    pub endpoint: String,
    pub api_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window_size: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    /// Stop sequences: string or array of strings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningConfig>,
}

impl Default for ModelPreset {
    fn default() -> Self {
        Self {
            backend: "openai".to_string(),
            model: "gpt-3.5-turbo".to_string(),
            endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
            api_key: String::new(),
            max_tokens: None,
            context_window_size: None,
            temperature: None,
            top_p: None,
            frequency_penalty: None,
            presence_penalty: None,
            seed: None,
            stop: None,
            response_format: None,
            tool_choice: None,
            parallel_tool_calls: None,
            reasoning: None,
        }
    }
}

// ── Tools policy (glob patterns; empty = deny all) ──

#[derive(Debug, Deserialize, Clone, Serialize, Default)]
pub struct ToolsPolicy {
    #[serde(default)]
    pub enabled_mcp: Vec<String>,
    #[serde(default)]
    pub enabled_tools: Vec<String>,
    #[serde(default)]
    pub disabled_tools: Vec<String>,
}

// ── Profile: references preset + policy + prompts ──

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct LlmProfile {
    pub model_preset: String,
    #[serde(default)]
    pub prompts: Vec<String>,
    pub tools_policy: String,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub context_window_size: Option<usize>,
    #[serde(default = "default_summarize_threshold")]
    pub summarize_threshold: f32,
}

fn default_summarize_threshold() -> f32 {
    0.7
}

impl Default for LlmProfile {
    fn default() -> Self {
        Self {
            model_preset: "openai".to_string(),
            prompts: Vec::new(),
            tools_policy: "default".to_string(),
            hidden: false,
            context_window_size: None,
            summarize_threshold: default_summarize_threshold(),
        }
    }
}

/// Resolved profile: profile + its model preset. Use for building LLM client and requests.
#[derive(Debug, Clone)]
pub struct ResolvedProfile {
    pub profile: LlmProfile,
    pub preset: ModelPreset,
}

impl ResolvedProfile {
    pub fn preset(&self) -> &ModelPreset {
        &self.preset
    }
    pub fn profile(&self) -> &LlmProfile {
        &self.profile
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
    #[serde(default = "default_tool_call_timeout_secs")]
    pub tool_call_timeout_secs: u64,
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
    600 // 10 minutes - increased to support long-running streams
}

fn default_tool_call_timeout_secs() -> u64 {
    240 // 4 minutes per MCP tool call (was 20s)
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
            tool_call_timeout_secs: default_tool_call_timeout_secs(),
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

// ── Embedding config (for long-term memory vector search) ──

fn default_embedding_enabled() -> bool {
    false
}

fn default_embedding_endpoint() -> String {
    "https://api.openai.com/v1/embeddings".to_string()
}

fn default_embedding_model() -> String {
    "text-embedding-3-small".to_string()
}

fn default_embedding_dimensions() -> usize {
    1536
}

fn default_max_memories() -> usize {
    3
}

fn default_min_importance() -> Option<i32> {
    None
}

fn default_max_memory_tokens() -> Option<usize> {
    Some(800)
}

fn default_query_history_turns() -> usize {
    2
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct EmbeddingConfig {
    #[serde(default = "default_embedding_enabled")]
    pub enabled: bool,
    /// OpenAI-compatible embeddings API endpoint
    #[serde(default = "default_embedding_endpoint")]
    pub endpoint: String,
    #[serde(default = "default_embedding_model")]
    pub model: String,
    #[serde(default = "default_embedding_dimensions")]
    pub dimensions: usize,
    #[serde(default)]
    pub api_key: Option<String>,
    /// Maximum number of memories to retrieve per query
    #[serde(default = "default_max_memories")]
    pub max_memories: usize,
    /// Maximum cosine distance threshold (0.0-2.0, lower = more similar). None = no filter.
    #[serde(default)]
    pub max_distance: Option<f32>,
    /// Minimum importance score (1-5). None = no filter.
    #[serde(default = "default_min_importance")]
    pub min_importance: Option<i32>,
    /// Maximum tokens spent on the injected memory block. None = no token cap (only count cap applies).
    #[serde(default = "default_max_memory_tokens")]
    pub max_memory_tokens: Option<usize>,
    /// Number of prior user turns (in addition to the current one) included when building the recall query.
    #[serde(default = "default_query_history_turns")]
    pub query_history_turns: usize,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            enabled: default_embedding_enabled(),
            endpoint: default_embedding_endpoint(),
            model: default_embedding_model(),
            dimensions: default_embedding_dimensions(),
            api_key: None,
            max_memories: default_max_memories(),
            max_distance: None,
            min_importance: default_min_importance(),
            max_memory_tokens: default_max_memory_tokens(),
            query_history_turns: default_query_history_turns(),
        }
    }
}

impl EmbeddingConfig {
    /// Returns true if embedding is configured and enabled for memory recall.
    pub fn is_active(&self) -> bool {
        self.enabled && !self.endpoint.is_empty() && self.dimensions > 0
    }
}

// ── Attachment document RAG (chunk + vector index; requires embedding + sqlite-vec) ──

fn default_attachment_inline_max_chars() -> usize {
    100_000
}

fn default_attachment_chunk_chars() -> usize {
    1500
}

fn default_attachment_chunk_overlap() -> usize {
    200
}

fn default_attachment_search_limit() -> usize {
    8
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct AttachmentRagConfig {
    /// Extracted text longer than this is not fully inlined; chunks are indexed for `search_attachment_chunks`.
    #[serde(default = "default_attachment_inline_max_chars")]
    pub inline_max_chars: usize,
    #[serde(default = "default_attachment_chunk_chars")]
    pub chunk_chars: usize,
    #[serde(default = "default_attachment_chunk_overlap")]
    pub chunk_overlap: usize,
    #[serde(default = "default_attachment_search_limit")]
    pub search_limit: usize,
    #[serde(default)]
    pub max_distance: Option<f32>,
}

impl Default for AttachmentRagConfig {
    fn default() -> Self {
        Self {
            inline_max_chars: default_attachment_inline_max_chars(),
            chunk_chars: default_attachment_chunk_chars(),
            chunk_overlap: default_attachment_chunk_overlap(),
            search_limit: default_attachment_search_limit(),
            max_distance: None,
        }
    }
}

// ── Deep Sleep config ──

fn default_deep_sleep_enabled() -> bool {
    false
}
fn default_deep_sleep_interval_hours() -> u64 {
    24
}
fn default_deep_sleep_memory_batch_size() -> usize {
    20
}
fn default_deep_sleep_max_conversations() -> usize {
    50
}
fn default_deep_sleep_inter_call_delay_secs() -> u64 {
    2
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct DeepSleepConfig {
    #[serde(default = "default_deep_sleep_enabled")]
    pub enabled: bool,
    /// LLM profile name to use for deep sleep analysis
    #[serde(default)]
    pub profile: Option<String>,
    /// How often to run (hours)
    #[serde(default = "default_deep_sleep_interval_hours")]
    pub interval_hours: u64,
    /// Memories per LLM evaluation call
    #[serde(default = "default_deep_sleep_memory_batch_size")]
    pub memory_batch_size: usize,
    /// Max conversations to process per cycle
    #[serde(default = "default_deep_sleep_max_conversations")]
    pub max_conversations_per_run: usize,
    /// Delay between LLM calls (seconds, for RPi4 thermal management)
    #[serde(default = "default_deep_sleep_inter_call_delay_secs")]
    pub inter_call_delay_secs: u64,
}

impl Default for DeepSleepConfig {
    fn default() -> Self {
        Self {
            enabled: default_deep_sleep_enabled(),
            profile: None,
            interval_hours: default_deep_sleep_interval_hours(),
            memory_batch_size: default_deep_sleep_memory_batch_size(),
            max_conversations_per_run: default_deep_sleep_max_conversations(),
            inter_call_delay_secs: default_deep_sleep_inter_call_delay_secs(),
        }
    }
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct AppConfig {
    pub default: String,
    pub profiles: HashMap<String, LlmProfile>,
    #[serde(default)]
    pub model_presets: HashMap<String, ModelPreset>,
    #[serde(default)]
    pub tools_policies: HashMap<String, ToolsPolicy>,
    #[serde(default)]
    pub prompts: crate::prompts::PromptConfig,
    #[serde(default)]
    pub mcp: MCPConfig,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub title_summary: TitleSummaryConfig,
    #[serde(default)]
    pub deep_sleep: DeepSleepConfig,
    #[serde(default)]
    pub embedding: EmbeddingConfig,
    #[serde(default)]
    pub attachment_rag: AttachmentRagConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        let mut profiles = HashMap::new();
        profiles.insert("openai".to_string(), LlmProfile::default());
        let mut model_presets = HashMap::new();
        model_presets.insert(
            "openai".to_string(),
            ModelPreset {
                backend: "openai".to_string(),
                model: "gpt-3.5-turbo".to_string(),
                endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
                api_key: String::new(),
                max_tokens: None,
                context_window_size: None,
                temperature: None,
                top_p: None,
                frequency_penalty: None,
                presence_penalty: None,
                seed: None,
                stop: None,
                response_format: None,
                tool_choice: None,
                parallel_tool_calls: None,
                reasoning: None,
            },
        );
        let mut tools_policies = HashMap::new();
        tools_policies.insert("default".to_string(), ToolsPolicy::default());
        Self {
            default: "openai".to_string(),
            profiles,
            model_presets,
            tools_policies,
            prompts: crate::prompts::PromptConfig::default(),
            mcp: MCPConfig::default(),
            server: ServerConfig::default(),
            title_summary: TitleSummaryConfig::default(),
            deep_sleep: DeepSleepConfig::default(),
            embedding: EmbeddingConfig::default(),
            attachment_rag: AttachmentRagConfig::default(),
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

    /// Directory for uploaded files: `config_dir/uploads`
    pub fn uploads_dir(&self) -> PathBuf {
        Self::config_dir().join("uploads")
    }

    /// Directory for static assets served at /api/static: `config_dir/static`
    pub fn static_dir(&self) -> PathBuf {
        Self::config_dir().join("static")
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

    /// Resolve profile by name to profile + model preset. Use for building LLM client.
    pub fn resolve_profile(&self, name: &str) -> Option<ResolvedProfile> {
        let profile = self.profiles.get(name)?.clone();
        let preset = self.model_presets.get(&profile.model_preset)?.clone();
        Some(ResolvedProfile { profile, preset })
    }

    /// Resolve default profile to profile + model preset.
    pub fn resolve_default_profile(&self) -> Option<ResolvedProfile> {
        self.resolve_profile(&self.default)
    }

    #[allow(dead_code)] // Public API method
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        use chrono::Local;
        use std::fs;
        use toml;

        let config_path = Self::config_toml_path();

        // Create config directory if it doesn't exist
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Create backup if config file exists
        if config_path.exists() {
            let now = Local::now();
            let backup_filename = format!("config_bcp_{}.toml", now.format("%Y_%m_%d_%H_%M_%S"));
            let backup_path = config_path
                .parent()
                .ok_or_else(|| anyhow::anyhow!("Config path has no parent directory"))
                .with_context(|| {
                    format!("Failed to create backup path for {}", config_path.display())
                })?
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
