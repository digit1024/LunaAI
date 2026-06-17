//! Internal `spawn_worker` tool: run an isolated sub-agent and return its final text.

use super::loop_engine::AgenticLoop;
use super::protocol::{AgentUpdate, RunContext};
use crate::config::{AppConfig, AttachmentRagConfig};
use crate::llm::{LlmClient, Message, Role, ToolCall, ToolResult};
use crate::storage::sqlite_storage_simple::MessageMetadata;
use agentic_loop::mcp_servers_registry::MCPServerRegistry;
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

/// Run a worker sub-agent. Takes only owned `Send` data so nested workers are safe to spawn.
pub async fn execute_spawn_worker(
    app_config: Option<Arc<AppConfig>>,
    storage: Option<Arc<Mutex<crate::storage::Storage>>>,
    mcp_registry: Arc<RwLock<MCPServerRegistry>>,
    tool_call_timeout_secs: u64,
    task: String,
    profile_name: String,
) -> ToolResult {
    let Some(app_config) = app_config else {
        return ToolResult {
            content: "spawn_worker requires app configuration (not available in this context)"
                .to_string(),
            is_error: true,
        };
    };
    let Some(storage) = storage else {
        return ToolResult {
            content: "spawn_worker requires storage".to_string(),
            is_error: true,
        };
    };

    if task.trim().is_empty() {
        return ToolResult {
            content: "spawn_worker requires non-empty 'task'".to_string(),
            is_error: true,
        };
    }

    let (llm_client, allowed_tools) =
        match resolve_worker_profile(&app_config, &mcp_registry, &profile_name).await {
            Ok(v) => v,
            Err(e) => {
                return ToolResult {
                    content: format!("spawn_worker: {}", e),
                    is_error: true,
                };
            }
        };

    let title = {
        let max_chars = 60.min(task.len());
        let mut end = max_chars;
        while end > 0 && !task.is_char_boundary(end) {
            end -= 1;
        }
        format!("worker: {}", &task[..end])
    };

    let conv_id = match storage
        .lock()
        .await
        .create_internal_conversation(title, Some(&profile_name))
    {
        Ok(id) => id,
        Err(e) => {
            return ToolResult {
                content: format!(
                    "spawn_worker: failed to create internal conversation: {}",
                    e
                ),
                is_error: true,
            };
        }
    };

    if let Err(e) = storage.lock().await.add_message_to_conversation(
        &conv_id,
        "user".to_string(),
        task.clone(),
    ) {
        return ToolResult {
            content: format!("spawn_worker: failed to persist task message: {}", e),
            is_error: true,
        };
    }

    let (agent_tx, mut agent_rx) = tokio::sync::mpsc::unbounded_channel::<AgentUpdate>();
    let storage_clone = storage.clone();
    let persist_conv_id = conv_id;
    let persist_task = tokio::spawn(async move {
        let mut persistence = WorkerPersistence::new(storage_clone, persist_conv_id);
        while let Some(update) = agent_rx.recv().await {
            if let Err(e) = persistence.handle_update(update).await {
                tracing::warn!(error = %e, "Worker persistence failed");
            }
        }
    });

    let worker_run_context = RunContext {
        conversation_id: Some(conv_id),
        profile_name,
        allowed_tool_names: allowed_tools,
        embedding_provider: None,
        attachment_rag: AttachmentRagConfig::default(),
    };

    let result = Box::pin(run_worker_agent(
        mcp_registry,
        llm_client,
        tool_call_timeout_secs,
        storage,
        app_config,
        task,
        agent_tx,
        worker_run_context,
    ))
    .await;

    let _ = persist_task.await;

    match result {
        Ok(text) => ToolResult {
            content: text,
            is_error: false,
        },
        Err(e) => ToolResult {
            content: format!("Worker failed: {}", e),
            is_error: true,
        },
    }
}

async fn resolve_worker_profile(
    app_config: &Arc<AppConfig>,
    mcp_registry: &Arc<RwLock<MCPServerRegistry>>,
    profile_name: &str,
) -> Result<(Arc<dyn LlmClient>, std::collections::HashSet<String>)> {
    let resolved = app_config
        .resolve_profile(profile_name)
        .with_context(|| format!("Profile '{}' not found or has no model preset", profile_name))?;
    let llm_client = crate::llm::build_llm_client(resolved.preset());
    let allowed_tools = crate::tools_policy::compute_allowed_tool_names(
        mcp_registry,
        app_config.as_ref(),
        resolved.profile(),
    )
    .await
    .context("Failed to compute tools policy for worker profile")?;
    Ok((llm_client, allowed_tools))
}

async fn run_worker_agent(
    mcp_registry: Arc<RwLock<MCPServerRegistry>>,
    llm_client: Arc<dyn LlmClient>,
    tool_call_timeout_secs: u64,
    storage: Arc<Mutex<crate::storage::Storage>>,
    app_config: Arc<AppConfig>,
    task: String,
    agent_tx: tokio::sync::mpsc::UnboundedSender<AgentUpdate>,
    worker_run_context: RunContext,
) -> Result<String> {
    let mut child = AgenticLoop::new(mcp_registry, llm_client, None, tool_call_timeout_secs)
        .with_storage(storage)
        .with_app_config(app_config);

    child
        .process_message(
            vec![Message::new(Role::User, task)],
            Some(agent_tx),
            Some(worker_run_context),
        )
        .await
}

/// Persists worker agent updates to an internal conversation without WebSocket broadcast.
struct WorkerPersistence {
    storage: Arc<Mutex<crate::storage::Storage>>,
    conversation_id: Uuid,
    pending_tool_calls: Vec<ToolCall>,
    tool_params: HashMap<String, Value>,
}

impl WorkerPersistence {
    fn new(storage: Arc<Mutex<crate::storage::Storage>>, conversation_id: Uuid) -> Self {
        Self {
            storage,
            conversation_id,
            pending_tool_calls: Vec::new(),
            tool_params: HashMap::new(),
        }
    }

    async fn handle_update(&mut self, update: AgentUpdate) -> Result<()> {
        match update {
            AgentUpdate::ToolPlanned { plan_items } => {
                for plan in &plan_items {
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
            AgentUpdate::ToolStarted {
                tool_call_id,
                params_json,
                ..
            } => {
                if let Ok(value) = serde_json::from_str::<Value>(&params_json) {
                    self.tool_params.entry(tool_call_id).or_insert(value);
                }
            }
            AgentUpdate::AssistantComplete {
                full_text,
                reasoning_content,
            } => {
                let tool_calls_slice = if self.pending_tool_calls.is_empty() {
                    None
                } else {
                    Some(self.pending_tool_calls.as_slice())
                };
                let metadata = MessageMetadata {
                    tool_calls: tool_calls_slice,
                    reasoning_content: reasoning_content.as_deref(),
                    attachments: None,
                    ..MessageMetadata::default()
                };
                self.storage
                    .lock()
                    .await
                    .add_message_with_metadata(
                        &self.conversation_id,
                        "assistant".to_string(),
                        full_text,
                        None,
                        metadata,
                    )
                    .context("failed to persist worker assistant response")?;
                self.pending_tool_calls.clear();
            }
            AgentUpdate::ToolResult {
                tool_call_id,
                name,
                result_json,
            } => {
                self.persist_tool_result(&tool_call_id, &name, &result_json, "success")
                    .await?;
            }
            AgentUpdate::ToolError {
                tool_call_id,
                name,
                error,
                ..
            } => {
                self.persist_tool_result(&tool_call_id, &name, &error, "error")
                    .await?;
            }
            _ => {}
        }
        Ok(())
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
        self.storage
            .lock()
            .await
            .add_message_with_metadata(
                &self.conversation_id,
                "tool".to_string(),
                String::new(),
                None,
                metadata,
            )
            .context("failed to persist worker tool result")?;
        Ok(())
    }
}
