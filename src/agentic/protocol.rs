use serde::{Deserialize, Serialize};

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
    AssistantComplete {
        full_text: String,
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


