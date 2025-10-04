use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedTool {
    pub name: String,
    pub params_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentUpdate {
    BeginTurn {
        conversation_id: Option<Uuid>,
        turn_id: Uuid,
        iteration: u32,
        plan_summary: Option<String>,
    },
    AssistantDelta {
        turn_id: Uuid,
        text_chunk: String,
        seq: u64,
    },
    AssistantComplete {
        turn_id: Uuid,
        full_text: String,
    },
    ToolPlanned {
        turn_id: Uuid,
        plan_items: Vec<PlannedTool>,
    },
    ToolStarted {
        turn_id: Uuid,
        tool_call_id: String,
        name: String,
        params_json: String,
    },
    ToolResult {
        turn_id: Uuid,
        tool_call_id: String,
        name: String,
        result_json: String,
    },
    ToolError {
        turn_id: Uuid,
        tool_call_id: String,
        name: String,
        error: String,
        retryable: bool,
    },
    EndTurn {
        turn_id: Uuid,
    },
    EndConversation {
        final_text: String,
    },
    Heartbeat {
        turn_id: Uuid,
        ts_ms: i64,
    },
}


