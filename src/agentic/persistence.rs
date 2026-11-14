//! Shared agent-update persistence for interactive runs and workers.
//!
//! `server/handlers/spawn.rs` (PersistenceContext) and `spawn_worker.rs`
//! (WorkerPersistence) carried duplicate structs with identical
//! pending-tool-calls bookkeeping; this is the single implementation.
//! Persistence is kept separate from broadcast side effects — callers decide
//! what to fan out (interactive runs broadcast to subscribers, workers don't).

use super::protocol::{AgentUpdate, PlannedTool};
use crate::llm::ToolCall;
use crate::storage::sqlite_storage_simple::MessageMetadata;
use crate::storage::Storage;
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// Persists the durable side effects of agent updates (assistant messages,
/// tool-call metadata, tool results) into a conversation.
pub struct AgentPersistence {
    storage: Arc<Storage>,
    conversation_id: Uuid,
    pending_tool_calls: Vec<ToolCall>,
    tool_params: HashMap<String, Value>,
}

impl AgentPersistence {
    pub fn new(storage: Arc<Storage>, conversation_id: Uuid) -> Self {
        Self {
            storage,
            conversation_id,
            pending_tool_calls: Vec::new(),
            tool_params: HashMap::new(),
        }
    }

    /// Apply the persistence side effects of one update. Streaming deltas and
    /// lifecycle markers have no durable side effect and are no-ops.
    pub async fn persist(&mut self, update: &AgentUpdate) -> Result<()> {
        match update {
            AgentUpdate::AssistantComplete {
                full_text,
                reasoning_content,
            } => {
                self.persist_assistant(full_text, reasoning_content.as_deref())
                    .await
            }
            AgentUpdate::ToolPlanned { plan_items } => {
                self.register_tools(plan_items);
                Ok(())
            }
            AgentUpdate::ToolStarted {
                tool_call_id,
                params_json,
                ..
            } => {
                self.mark_tool_started(tool_call_id, params_json);
                Ok(())
            }
            AgentUpdate::ToolResult {
                tool_call_id,
                name,
                result_json,
            } => {
                self.persist_tool_result(tool_call_id, name, result_json, "success")
                    .await
            }
            AgentUpdate::ToolError {
                tool_call_id,
                name,
                error,
                ..
            } => {
                self.persist_tool_result(tool_call_id, name, error, "error")
                    .await
            }
            _ => Ok(()),
        }
    }

    async fn persist_assistant(
        &mut self,
        content: &str,
        reasoning_content: Option<&str>,
    ) -> Result<()> {
        let tool_calls_slice = if self.pending_tool_calls.is_empty() {
            None
        } else {
            Some(self.pending_tool_calls.as_slice())
        };
        let metadata = MessageMetadata {
            tool_calls: tool_calls_slice,
            reasoning_content,
            attachments: None,
            ..MessageMetadata::default()
        };
        self.storage
            .add_message_with_metadata(
                &self.conversation_id,
                "assistant".to_string(),
                content.to_string(),
                None,
                metadata,
            )
            .context("failed to persist assistant response")?;
        self.pending_tool_calls.clear();
        Ok(())
    }

    fn register_tools(&mut self, plans: &[PlannedTool]) {
        for plan in plans {
            if let Ok(params) = serde_json::from_str::<Value>(&plan.params_json) {
                self.tool_params.insert(plan.id.clone(), params.clone());
                self.pending_tool_calls.push(ToolCall {
                    id: plan.id.clone(),
                    name: plan.name.clone(),
                    parameters: params,
                });
            }
        }
    }

    fn mark_tool_started(&mut self, tool_call_id: &str, params_json: &str) {
        if let Ok(value) = serde_json::from_str::<Value>(params_json) {
            self.tool_params
                .entry(tool_call_id.to_string())
                .or_insert(value);
        }
    }

    async fn persist_tool_result(
        &mut self,
        tool_call_id: &str,
        name: &str,
        payload: &str,
        status: &str,
    ) -> Result<()> {
        let params = self.tool_params.get(tool_call_id);
        let result_value =
            serde_json::from_str::<Value>(payload).unwrap_or(Value::String(payload.to_string()));
        let metadata = MessageMetadata {
            tool_calls: None,
            tool_call_id: Some(tool_call_id),
            tool_name: Some(name),
            tool_status: Some(status),
            tool_params_json: params,
            tool_result_json: Some(&result_value),
            reasoning_content: None,
            attachments: None,
        };
        // Content stays empty — tool_result_json holds the actual data.
        self.storage
            .add_message_with_metadata(
                &self.conversation_id,
                "tool".to_string(),
                String::new(),
                None,
                metadata,
            )
            .context("failed to persist tool result")?;
        Ok(())
    }
}
