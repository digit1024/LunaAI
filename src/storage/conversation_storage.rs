use crate::llm::{Attachment, ToolCall};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    pub id: Uuid,
    pub iteration: u32,
    pub text: String,
    pub complete: bool,
    pub tools: Vec<ToolCallInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallInfo {
    pub id: Option<String>,
    pub tool_name: String,
    pub parameters: String,
    pub status: ToolCallStatus,
    pub result: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ToolCallStatus {
    Started,
    Completed,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: Uuid,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub messages: Vec<StoredMessage>,
    pub turns: Vec<Turn>,
    pub profile_name: Option<String>,
    /// Hidden from default history lists; excluded from deep sleep and title generation.
    #[serde(default)]
    pub internal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: Uuid,
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub tool_status: Option<String>,
    pub tool_params_json: Option<Value>,
    pub tool_result_json: Option<Value>,
    pub reasoning_content: Option<String>,
    #[serde(default)]
    pub is_summary: bool,
    #[serde(default)]
    pub is_summarized: bool,
    pub summarized_count: Option<usize>,
    #[serde(default)]
    pub attachments: Option<Vec<Attachment>>,
}
