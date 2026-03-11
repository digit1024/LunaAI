//! Storage types used by storage_wrapper and dto.
//! File-based Storage impl removed; sqlite + storage_wrapper is the canonical path.

use crate::llm::ToolCall;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: Uuid,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub messages: Vec<StoredMessage>,
    pub profile_name: Option<String>,
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
}
