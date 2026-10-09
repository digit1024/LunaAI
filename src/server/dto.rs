use crate::llm::ToolCall;
use crate::storage::conversation_storage::StoredMessage;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientCommand {
    HealthCheck,
    StartConversation {
        title: Option<String>,
        #[serde(default)]
        internal: Option<bool>,
    },
    LoadConversation {
        conversation_id: String,
    },
    ListConversations {
        query: Option<String>,
        limit: Option<u32>,
        offset: Option<u32>,
        #[serde(default)]
        include_internal: Option<bool>,
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
    /// Manually summarize (compact) a conversation's history.
    SummarizeConversation {
        conversation_id: String,
    },
    /// Resume the agentic loop from persisted history (no new user message).
    ResumeAgent {
        conversation_id: String,
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
        #[serde(default)]
        internal: Option<bool>,
    },
    RenameConversation {
        conversation_id: String,
        title: String,
    },
    SetConversationInternal {
        conversation_id: String,
        internal: bool,
    },
    ListMemories {
        query: Option<String>,
        limit: Option<u32>,
        offset: Option<u32>,
    },
    UpdateMemory {
        id: i64,
        content: Option<String>,
        category: Option<String>,
        importance: Option<i32>,
    },
    DeleteMemory {
        id: i64,
    },
}

#[derive(Debug, Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    HealthOk {
        timestamp: i64,
        profile: String,
        static_token: String,
    },
    Error {
        message: String,
    },
    /// Informational message (e.g. context truncation) – show as toast, do not treat as error.
    Info {
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
        message_id: String,
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
    ConversationRenamed {
        conversation_id: String,
        title: String,
    },
    ConversationInternalChanged {
        conversation_id: String,
        internal: bool,
    },
    MemoriesList {
        memories: Vec<MemoryView>,
    },
    MemoryUpdated {
        memory: MemoryView,
    },
    MemoryDeleted {
        id: i64,
    },
    StreamingStopped {
        conversation_id: String,
    },
    /// Sent when the memory RAG system injected a fresh memory block for this turn.
    MemoriesRecalled {
        conversation_id: String,
        message_id: String,
        memories: Vec<MemoryView>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        memory_ids: Vec<i64>,
    },
}

#[derive(Debug, Serialize, Clone)]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub last_message_preview: Option<String>,
    pub updated_at: i64,
    #[serde(default)]
    pub internal: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct SearchResult {
    pub conversation_id: String,
    #[serde(default)]
    pub conversation_title: String,
    pub snippet: String,
    pub timestamp: i64,
    pub rank: f64,
}

#[derive(Debug, Serialize, Clone)]
pub struct MemoryView {
    pub id: i64,
    pub content: String,
    pub category: Option<String>,
    pub importance: i32,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Clone)]
pub struct ConversationView {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub messages: Vec<MessageView>,
    pub profile_name: Option<String>,
    #[serde(default)]
    pub internal: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct AttachmentView {
    /// Attachment id (`{uid}.{ext}` file stem) for `GET /api/attachment/{uid}`.
    pub uid: String,
    pub file_name: String,
    pub mime_type: String,
    pub file_size: u64,
}

impl AttachmentView {
    fn from_stored(attachment: &crate::llm::Attachment) -> Self {
        let uid = std::path::Path::new(&attachment.file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        Self {
            uid,
            file_name: attachment.file_name.clone(),
            mime_type: attachment.mime_type.clone(),
            file_size: attachment.file_size,
        }
    }
}

/// Remove `@attach:<uid>` tokens from stored content before it is shown to a
/// client (the structure lives in `attachments`); the LLM payload keeps the
/// token and expands it at build time.
fn strip_attach_tokens(content: &str) -> String {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"@attach:[0-9a-zA-Z-]{1,64}").expect("attach token regex")
    });
    if !re.is_match(content) {
        return content.to_string();
    }
    re.replace_all(content, "").trim().to_string()
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
    pub attachments: Option<Vec<AttachmentView>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>, // For DeepSeek thinking/reasoning content
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_summary: bool, // True if this message is a summary of previous messages
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summarized_count: Option<usize>, // Count of messages summarized
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recalled_memories: Vec<MemoryView>,
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
        Self::from_stored(msg, Vec::new())
    }
}

impl MessageView {
    pub fn from_stored(msg: &StoredMessage, recalled_memories: Vec<MemoryView>) -> Self {
        Self {
            id: msg.id.to_string(),
            role: msg.role.clone(),
            content: strip_attach_tokens(&msg.content),
            timestamp: msg.timestamp.timestamp(),
            tool_calls: msg.tool_calls.clone(),
            tool_call_id: msg.tool_call_id.clone(),
            tool_name: msg.tool_name.clone(),
            tool_status: msg.tool_status.clone(),
            tool_params_json: msg.tool_params_json.clone(),
            tool_result_json: msg.tool_result_json.clone(),
            attachments: msg.attachments.as_ref().map(|atts| {
                atts.iter().map(AttachmentView::from_stored).collect::<Vec<_>>()
            }).filter(|atts| !atts.is_empty()),
            reasoning_content: msg.reasoning_content.clone(),
            is_summary: msg.is_summary,
            summarized_count: msg.summarized_count,
            recalled_memories,
        }
    }
}

