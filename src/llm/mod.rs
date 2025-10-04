use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use futures::Stream;
use std::pin::Pin;
use anyhow::Result;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
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
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Attachment {
    pub file_path: String,
    pub file_name: String,
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
        }
    }
    
    pub fn new_prompt(role: Role, content: String) -> Self {
        Self {
            role,
            content,
            timestamp: Some(Utc::now()),
            is_prompt: true, // Mark as prompt
            tool_call_id: None,
            tool_calls: None,
            attachments: None,
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
}

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub content: String,
    pub is_error: bool,
}

#[async_trait]
pub trait LlmClient: Send + Sync {

    async fn send_message_stream(
        &self,
        messages: Vec<Message>,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LlmError>> + Send>>, LlmError>;
    
    // New method for tool-enabled chat
    async fn send_message_with_tools(
        &self,
        messages: Vec<Message>,
        available_tools: Vec<ToolDefinition>,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
    ) -> Result<ChatResponse, LlmError>;
}

pub mod openai;
pub mod anthropic;
pub mod ollama;
pub mod gemini;
pub mod file_utils;