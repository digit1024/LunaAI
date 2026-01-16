use crate::llm::{Attachment, ToolCall};
use crate::storage::conversation_storage::StoredMessage;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
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
    TruncateConversation {
        conversation_id: String,
        message_id: String, // Delete all messages up to and including this message
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

#[derive(Debug, Serialize)]
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
        reasoning_content: Option<String>, // For DeepSeek thinking/reasoning content
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

#[derive(Debug, Serialize, Clone)]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub last_message_preview: Option<String>,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Clone)]
pub struct SearchResult {
    pub conversation_id: String,
    pub snippet: String,
    pub timestamp: i64,
    pub rank: f64,
}

#[derive(Debug, Serialize, Clone)]
pub struct ConversationView {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub messages: Vec<MessageView>,
    pub profile_name: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct MessageView {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: i64,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub tool_status: Option<String>,
    pub tool_params_json: Option<Value>,
    pub tool_result_json: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<Attachment>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>, // For DeepSeek thinking/reasoning content
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_summary: bool, // True if this message is a summary of previous messages
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summarized_count: Option<usize>, // Count of messages summarized
}

fn is_false(b: &bool) -> bool {
    !b
}

#[derive(Debug, Serialize, Clone)]
pub struct PlannedToolView {
    pub id: String,
    pub name: String,
    pub params_json: Value,
}

impl From<&StoredMessage> for MessageView {
    fn from(msg: &StoredMessage) -> Self {
        Self {
            id: msg.id.to_string(),
            role: msg.role.clone(),
            content: msg.content.clone(),
            timestamp: msg.timestamp.timestamp(),
            tool_calls: msg.tool_calls.clone(),
            tool_call_id: msg.tool_call_id.clone(),
            tool_name: msg.tool_name.clone(),
            tool_status: msg.tool_status.clone(),
            tool_params_json: msg.tool_params_json.clone(),
            tool_result_json: msg.tool_result_json.clone(),
            attachments: None, // StoredMessage doesn't have attachments yet
            reasoning_content: msg.reasoning_content.clone(),
            is_summary: msg.is_summary,
            summarized_count: msg.summarized_count,
        }
    }
}

