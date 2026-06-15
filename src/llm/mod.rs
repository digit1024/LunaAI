pub mod attachment_limits;
pub mod observability;

// Re-export the public observability surface so callers can `use crate::llm::*`.
// `set_llm_observer` and `LlmCallRecord`/`LlmObserver` are used by the
// server wiring; the rest are used by LLM provider modules directly.
#[allow(unused_imports)]
pub use observability::{
    llm_observer, set_llm_observer, shared_http_client, LlmCallRecord, LlmCallSpan,
    LlmObserver,
};

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::sync::Arc;

tokio::task_local! {
    /// Conversation ID propagated from the agentic loop into LLM clients via
    /// task-locals so they can tag observability events without changing
    /// every call site. Set with `CONVERSATION_ID.scope(id, fut).await`.
    pub static CONVERSATION_ID: Option<uuid::Uuid>;
}

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
    /// 429 from the provider. Transient — the agentic loop should back off
    /// (honoring `retry_after` when present) and try again. DeepSeek and
    /// most OpenAI-compatible providers use this for dynamic concurrency
    /// limits and short-window quotas.
    #[error("Rate limited by provider (retry_after={retry_after:?}): {message}")]
    RateLimited {
        retry_after: Option<std::time::Duration>,
        message: String,
    },
    /// 500 / 503 — provider internal error or overload. Transient: the
    /// agentic loop should back off and retry. DeepSeek docs explicitly
    /// recommend retrying after a brief wait for both codes.
    #[error("Provider server busy (HTTP {status}, retry_after={retry_after:?}): {message}")]
    ServerBusy {
        status: u16,
        retry_after: Option<std::time::Duration>,
        message: String,
    },
    /// Model hit `finish_reason: "length"` (max_tokens exhausted) *and* did
    /// not emit any usable content or completed tool call. Distinct from a
    /// successful-but-truncated response so the UI can show a precise
    /// "raise your max_tokens" hint instead of a generic empty turn.
    #[error("Model output truncated by length limit before any usable output (partial_tool_calls={partial_tool_calls})")]
    LengthTruncated {
        partial_tool_calls: usize,
        content_chars: usize,
    },
    /// Provider signalled intent to call tools (`finish_reason: "tool_calls"`
    /// or streamed partial tool-call deltas) yet emitted **no** fully-parsed
    /// tool call and **no** text content. The canonical DeepSeek-chat
    /// "phantom tool calls" failure mode. Marked transient so the loop can
    /// re-roll the turn — empirically one re-roll fixes it almost every
    /// time. Distinct from `LengthTruncated` (those won't recover on
    /// retry — they need a bigger max_tokens) and from a genuine
    /// stop-with-empty-output (which is the model's decision, not a bug).
    #[error("Provider produced empty completion despite tool-call intent ({partial_tool_calls} partial tool call(s) dropped)")]
    EmptyToolCallsCompletion {
        partial_tool_calls: usize,
    },
    /// Stream was severed before the provider signalled completion. Carries
    /// what we observed so the agentic loop can distinguish this from a clean
    /// finish (which is what was silently masking DeepSeek drops).
    #[error(
        "Stream truncated by provider after {bytes_read} bytes (last activity {last_event_age_ms}ms ago): {reason}"
    )]
    StreamTruncated {
        bytes_read: usize,
        last_event_age_ms: u128,
        reason: String,
    },
}

impl LlmError {
    /// `Some(retry_after)` if the error is a transient 429/5xx the agentic
    /// loop should back off and retry. `None` if the error is terminal
    /// (auth, malformed request, parse failure, truncation mid-stream, ...).
    /// The inner `Option<Duration>` is the provider's `Retry-After` hint
    /// when it sent one; otherwise the caller picks its own backoff.
    pub fn transient_retry_after(&self) -> Option<Option<std::time::Duration>> {
        match self {
            LlmError::RateLimited { retry_after, .. } => Some(*retry_after),
            LlmError::ServerBusy { retry_after, .. } => Some(*retry_after),
            // No `Retry-After` hint applies here — this fires only after the
            // stream has been consumed, so the provider didn't include one.
            LlmError::EmptyToolCallsCompletion { .. } => Some(None),
            _ => None,
        }
    }
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
    Content(String),
    ToolCall(ToolCall),
    Reasoning(String),
}

#[async_trait]
pub trait LlmClient: Send + Sync {
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
