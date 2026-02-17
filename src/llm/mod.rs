use crate::config::LlmProfile;
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

impl Role {
    /// Convert role to lowercase string (for API compatibility)
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::System => "system",
            Role::Tool => "tool",
        }
    }
}

impl From<&str> for Role {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "user" => Role::User,
            "assistant" => Role::Assistant,
            "system" => Role::System,
            "tool" => Role::Tool,
            _ => Role::User, // Default fallback
        }
    }
}

impl From<String> for Role {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl From<Role> for String {
    fn from(role: Role) -> Self {
        role.as_str().to_string()
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub role: Role,
    pub content: String,
    pub timestamp: Option<DateTime<Utc>>,
    pub is_prompt: bool, // Flag to distinguish prompts from regular messages
    pub tool_call_id: Option<String>, // For tool result messages
    pub tool_calls: Option<Vec<ToolCall>>, // For assistant messages with tool calls
    pub attachments: Option<Vec<Attachment>>, // File attachments
    pub reasoning_content: Option<String>, // For DeepSeek thinking/reasoning content
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Attachment {
    pub file_path: String,
    pub file_name: String,
    #[allow(dead_code)] // Field used for serialization
    pub mime_type: String,
    pub file_size: u64,
    pub content: Option<String>, // For text files, store content directly
}

impl Message {
    pub fn new(role: Role, content: String) -> Self {
        Self {
            role,
            content,
            timestamp: Some(Utc::now()),
            is_prompt: false, // Default to false for regular messages
            tool_call_id: None,
            tool_calls: None,
            attachments: None,
            reasoning_content: None,
        }
    }

    pub fn new_with_attachments(role: Role, content: String, attachments: Vec<Attachment>) -> Self {
        Self {
            role,
            content,
            timestamp: Some(Utc::now()),
            is_prompt: false,
            tool_call_id: None,
            tool_calls: None,
            attachments: Some(attachments),
            reasoning_content: None,
        }
    }

    pub fn new_tool_result(tool_call_id: String, content: String, is_error: bool) -> Self {
        let prefix = if is_error { "Error: " } else { "" };
        Self {
            role: Role::Tool,
            content: format!("{}{}", prefix, content),
            timestamp: Some(Utc::now()),
            is_prompt: false,
            tool_call_id: Some(tool_call_id),
            tool_calls: None,
            attachments: None,
            reasoning_content: None,
        }
    }

    pub fn new_with_tool_calls(role: Role, content: String, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role,
            content,
            timestamp: Some(Utc::now()),
            is_prompt: false,
            tool_call_id: None,
            tool_calls: Some(tool_calls),
            attachments: None,
            reasoning_content: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error: {0}")]
    Api(String),
    #[error("Configuration error: {0}")]
    Config(String),
}

// Tool-related types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub reasoning_content: Option<String>, // For DeepSeek thinking/reasoning content
}

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub content: String,
    pub is_error: bool,
}

#[derive(Debug, Clone)]
pub enum ChatStreamEvent {
    ContentDelta(String),
    ToolCallDelta(ToolCall),
    ReasoningContentDelta(String), // For DeepSeek thinking/reasoning content
}

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn send_message_stream(
        &self,
        messages: Vec<Message>,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LlmError>> + Send>>, LlmError>;

    // Legacy non-streaming tool path
    async fn send_message_with_tools(
        &self,
        messages: Vec<Message>,
        available_tools: Vec<ToolDefinition>,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
    ) -> Result<ChatResponse, LlmError>;

    async fn send_message_stream_with_tools(
        &self,
        messages: Vec<Message>,
        available_tools: Vec<ToolDefinition>,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<ChatStreamEvent, LlmError>> + Send>>, LlmError>
    {
        let _ = (messages, available_tools, temperature, max_tokens);
        Err(LlmError::Config(
            "Tool streaming not implemented for this backend".into(),
        ))
    }
}

pub mod anthropic;
pub mod context_manager;
pub mod file_utils;
pub mod gemini;
pub mod ollama;
pub mod openai;
pub mod tokenizer;

pub fn build_llm_client(preset: &crate::config::ModelPreset) -> Arc<dyn LlmClient> {
    match preset.backend.as_str() {
        "anthropic" => Arc::new(crate::llm::anthropic::AnthropicClient::new(preset.clone())),
        "openai" => Arc::new(crate::llm::openai::OpenAIClient::new(preset.clone())),
        "ollama" => Arc::new(crate::llm::ollama::OllamaClient::new(preset.clone())),
        "gemini" => Arc::new(crate::llm::gemini::GeminiClient::new(preset.clone())),
        _ => Arc::new(crate::llm::openai::OpenAIClient::new(preset.clone())),
    }
}
