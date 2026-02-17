use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Context passed into the agent loop for internal tools (e.g. schedule_task).
#[derive(Debug, Clone)]
pub struct RunContext {
    pub conversation_id: Option<Uuid>,
    pub profile_name: String,
    /// Allowed tool names from tools policy; internal tools are only added if their name is in this set.
    pub allowed_tool_names: std::collections::HashSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedTool {
    pub id: String,
    pub name: String,
    pub params_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentUpdate {
    AssistantStreamingStarted,
    AssistantDelta {
        text_chunk: String,
        seq: u64,
    },
    ReasoningContentDelta {
        chunk: String,
    },
    AssistantComplete {
        full_text: String,
        reasoning_content: Option<String>, // For DeepSeek thinking/reasoning content
    },
    ToolPlanned {
        plan_items: Vec<PlannedTool>,
    },
    ToolStarted {
        tool_call_id: String,
        name: String,
        params_json: String,
    },
    ToolResult {
        tool_call_id: String,
        name: String,
        result_json: String,
    },
    ToolError {
        tool_call_id: String,
        name: String,
        error: String,
        retryable: bool,
    },
    ConversationComplete {
        final_text: String,
    },
    ModelError {
        error: String,
    },
}
