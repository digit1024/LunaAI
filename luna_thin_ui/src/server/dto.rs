//! Server DTOs for client-server communication
//!
//! These are the data transfer objects used between the thin client and server.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Commands sent from client to server
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientCommand {
    HealthCheck,
    StartConversation {
        title: Option<String>,
    },
    LoadConversation {
        conversation_id: String,
    },
    ListConversations {
        query: Option<String>,
        limit: Option<u32>,
        offset: Option<u32>,
    },
    DeleteConversation {
        conversation_id: String,
    },
    StopStreaming {
        conversation_id: Option<String>,
    },
    ChangeProfile {
        profile: String,
    },
    ListProfiles,
    SendMessage {
        conversation_id: Option<String>,
        content: String,
        #[serde(default)]
        attachment_ids: Option<Vec<String>>,
    },
}

/// Events sent from server to client
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    HealthOk {
        timestamp: i64,
        profile: String,
    },
    Error {
        message: String,
    },
    ConversationCreated {
        conversation_id: String,
    },
    ConversationLoaded {
        conversation: ConversationView,
    },
    ConversationsList {
        conversations: Vec<ConversationSummary>,
    },
    SearchResults {
        results: Vec<SearchResult>,
    },
    ProfileChanged {
        profile: String,
    },
    ProfilesList {
        profiles: Vec<String>,
        default_profile: String,
    },
    MessageAccepted {
        conversation_id: String,
    },
    StreamingStarted {
        conversation_id: String,
    },
    AssistantDelta {
        conversation_id: String,
        chunk: String,
        seq: u64,
    },
    ReasoningContentDelta {
        conversation_id: String,
        chunk: String,
    },
    AssistantComplete {
        conversation_id: String,
        content: String,
        reasoning_content: Option<String>,
    },
    ToolPlanned {
        conversation_id: String,
        tools: Vec<PlannedToolView>,
    },
    ToolStarted {
        conversation_id: String,
        tool_call_id: String,
        name: String,
        params_json: Value,
    },
    ToolResult {
        conversation_id: String,
        tool_call_id: String,
        name: String,
        result_json: Value,
    },
    ToolError {
        conversation_id: String,
        tool_call_id: String,
        name: String,
        error: String,
    },
    ConversationComplete {
        conversation_id: String,
    },
    ConversationDeleted {
        conversation_id: String,
    },
    StreamingStopped {
        conversation_id: String,
    },
}

/// Summary of a conversation for listing
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub last_message_preview: Option<String>,
    pub updated_at: i64,
}

/// Search result
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SearchResult {
    pub conversation_id: String,
    pub snippet: String,
    pub timestamp: i64,
    pub rank: f64,
}

/// Full conversation view with messages
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConversationView {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub messages: Vec<MessageView>,
    pub profile_name: Option<String>,
}

/// Message within a conversation
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MessageView {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallView>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_params_json: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_result_json: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<AttachmentView>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(default)]
    pub is_summary: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summarized_count: Option<usize>,
}

/// Tool call information
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolCallView {
    pub id: String,
    pub name: String,
    /// Parameters can be sent as either "parameters" or "arguments" from server
    #[serde(alias = "arguments")]
    pub parameters: Value,
}

/// Planned tool for execution
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlannedToolView {
    pub id: String,
    pub name: String,
    pub params_json: Value,
}

/// File attachment
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AttachmentView {
    pub file_id: String,
    pub file_name: String,
    pub mime_type: String,
    pub file_size: u64,
}

/// MCP Server status
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum MCPServerStatus {
    Connected,
    Failed { error: String },
}

/// MCP Server information
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MCPServerInfo {
    pub name: String,
    #[serde(flatten)]
    pub status: MCPServerStatus,
}

/// MCP Servers list response
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MCPServersResponse {
    pub servers: Vec<MCPServerInfo>,
}

