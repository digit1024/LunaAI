//! Internal `spawn_worker` tool: run an isolated sub-agent and return its final text.

use super::loop_engine::AgenticLoop;
use super::persistence::AgentPersistence;
use super::protocol::{AgentUpdate, RunContext};
use crate::config::{AppConfig, AttachmentRagConfig};
use crate::llm::{LlmClient, Message, Role, ToolResult};
use agentic_loop::mcp_servers_registry::MCPServerRegistry;
use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Run a worker sub-agent. Takes only owned `Send` data so nested workers are safe to spawn.
/// `depth` is this worker's nesting depth (parent depth + 1); the caller has
/// already validated it against `[agentic].max_worker_depth`.
pub async fn execute_spawn_worker(
    app_config: Option<Arc<AppConfig>>,
    storage: Option<Arc<crate::storage::Storage>>,
    mcp_registry: Arc<RwLock<MCPServerRegistry>>,
    tool_call_timeout_secs: u64,
    task: String,
    profile_name: String,
    depth: u32,
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

    if let Err(e) = storage.add_message_to_conversation(
        &conv_id,
        "user".to_string(),
        task.clone(),
    ) {
        return ToolResult {
            content: format!("spawn_worker: failed to persist task message: {}", e),
            is_error: true,
        };
    }

    // Bounded: backpressure onto the worker loop if persistence stalls.
    let (agent_tx, mut agent_rx) = tokio::sync::mpsc::channel::<AgentUpdate>(
        crate::agentic::protocol::AGENT_UPDATE_CHANNEL_CAP,
    );
    let storage_clone = storage.clone();
    let persist_conv_id = conv_id;
    let persist_task = tokio::spawn(async move {
        let mut persistence = AgentPersistence::new(storage_clone, persist_conv_id);
        while let Some(update) = agent_rx.recv().await {
            if let Err(e) = persistence.persist(&update).await {
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
        depth,
    };

    let child = AgenticLoop::new(mcp_registry, llm_client, None, tool_call_timeout_secs)
        .with_storage(storage)
        .with_app_config(app_config);

    let result = Box::pin(run_worker_agent(child, task, agent_tx, worker_run_context)).await;

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
    mut child: AgenticLoop,
    task: String,
    agent_tx: tokio::sync::mpsc::Sender<AgentUpdate>,
    worker_run_context: RunContext,
) -> Result<String> {
    child
        .process_message(
            vec![Message::new(Role::User, task)],
            Some(agent_tx),
            Some(worker_run_context),
        )
        .await
}
