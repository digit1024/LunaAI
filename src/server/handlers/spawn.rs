use super::{helpers::truncate_preview, ServerContext};
use crate::{
    agentic::{
        loop_engine::AgenticLoop,
        protocol::{AgentUpdate, PlannedTool},
        RunContext,
    },
    llm::{self, Message as LlmMessage, Role},
    llm::tokenizer::TokenCounter,
    server::dto::{PlannedToolView, ServerEvent},
    services::{ContextService, MessageConverter},
    storage::{
        sqlite_storage_simple::{Message as StorageMessage, MessageMetadata},
        ScheduledJob, Storage,
    },
};
use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::{collections::HashMap, collections::HashSet, sync::Arc, time::Duration};
use tokio::{sync::Mutex, task::JoinHandle};
use uuid::Uuid;

/// Spawn the agent task for a conversation. Used by handle_send_message and run_scheduled_task.
pub fn spawn_agent_task(
    ctx: Arc<ServerContext>,
    conversation_id: Uuid,
    agent_messages: Vec<LlmMessage>,
    profile_name: String,
    llm_client: Arc<dyn crate::llm::LlmClient>,
    allowed_tool_names: HashSet<String>,
) -> JoinHandle<()> {
    let subscriptions = ctx.subscriptions.clone();
    let mcp_registry = ctx.mcp_registry.clone();
    let schedule_service = ctx.schedule_service.clone();
    let timeout = Duration::from_secs(ctx.server_cfg.stream_timeout_secs);
    let storage = ctx.storage.clone();
    let run_context = RunContext {
        conversation_id: Some(conversation_id),
        profile_name: profile_name.clone(),
        allowed_tool_names,
        embedding_provider: ctx.embedding_provider.clone(),
        attachment_rag: ctx.config.attachment_rag.clone(),
    };

    tokio::spawn(async move {
        let (agent_tx, mut agent_rx) = tokio::sync::mpsc::unbounded_channel::<AgentUpdate>();
        let mut loop_engine = AgenticLoop::new(
            mcp_registry,
            llm_client,
            Some(schedule_service),
            ctx.server_cfg.tool_call_timeout_secs,
        ).with_storage(storage.clone());
        let subs = subscriptions.clone();
        let stream_task = tokio::spawn(async move {
            let mut persistence = PersistenceContext::new(storage, conversation_id);
            while let Some(update) = agent_rx.recv().await {
                if let Err(err) = process_agent_update(
                    &subs,
                    conversation_id,
                    &mut persistence,
                    update,
                )
                .await
                {
                    let _ = subs
                        .broadcast(
                            conversation_id,
                            ServerEvent::Error {
                                message: err.to_string(),
                            },
                        )
                        .await;
                }
            }
        });

        let result = tokio::time::timeout(
            timeout,
            loop_engine.process_message(agent_messages, Some(agent_tx), Some(run_context)),
        )
        .await;

        match result {
            Ok(Ok(_)) => {}
            Ok(Err(err)) => {
                subscriptions
                    .broadcast(
                        conversation_id,
                        ServerEvent::Error {
                            message: err.to_string(),
                        },
                    )
                    .await;
            }
            Err(elapsed) => {
                subscriptions
                    .broadcast(
                        conversation_id,
                        ServerEvent::Error {
                            message: format!("Streaming timeout after {:?}", elapsed),
                        },
                    )
                    .await;
            }
        }

        let _ = stream_task.await;

        ctx.active_agent_runs.write().await.remove(&conversation_id);
    })
}

/// Run a scheduled job: build messages, spawn agent, update job status (and next run if recurring).
pub async fn run_scheduled_task(ctx: Arc<ServerContext>, job: ScheduledJob) -> Result<()> {
    use crate::services::next_run_from_cron;

    let profile_name = job
        .profile_name
        .as_deref()
        .unwrap_or_else(|| ctx.config.default.as_str());
    let resolved = ctx
        .config
        .resolve_profile(profile_name)
        .or_else(|| ctx.config.resolve_default_profile())
        .ok_or_else(|| anyhow!("No profile or preset found for scheduled job"))?;
    let llm_client = llm::build_llm_client(resolved.preset());

    let (conversation_id, mut agent_messages) = if let Some(conv_id_str) = &job.conversation_id {
        let conv_uuid = Uuid::parse_str(conv_id_str).context("invalid conversation_id in job")?;
        let exists = {
            let storage = ctx.storage.lock().await;
            storage.get_conversation(&conv_uuid).context("failed to get conversation")?.is_some()
        };
        if !exists {
            let storage = ctx.storage.lock().await;
            storage
                .set_scheduled_job_completed(
                    &job.id,
                    chrono::Utc::now().timestamp(),
                    true,
                    Some("Conversation no longer exists"),
                )
                .context("failed to mark job failed")?;
            return Ok(());
        }
        let db_messages = {
            let storage = ctx.storage.lock().await;
            storage
                .load_conversation_messages(conv_id_str)
                .context("failed to load conversation messages")?
        };
        let context: Vec<StorageMessage> = MessageConverter::llm_context_messages(&db_messages)
            .into_iter()
            .cloned()
            .collect();
        let mut llm_messages = MessageConverter::db_to_llm(&context, true);
        llm_messages.push(LlmMessage::new(
            Role::User,
            format!(
                "Scheduled task (due now): {}. Please carry out this task.",
                job.message
            ),
        ));
        let agent_messages =
            ContextService::inject_prompts(llm_messages, &ctx.prompt_manager, resolved.profile())?;
        (conv_uuid, agent_messages)
    } else {
        let title = job
            .title
            .clone()
            .unwrap_or_else(|| truncate_preview(&job.message));
        let conv_id = {
            let storage = ctx.storage.lock().await;
            storage
                .create_conversation_with_profile(title.clone(), Some(profile_name))
                .context("failed to create conversation")?
        };
        {
            let storage = ctx.storage.lock().await;
            storage
                .add_message_to_conversation(&conv_id, "user".to_string(), job.message.clone())
                .context("failed to add message")?;
        }
        let db_messages = {
            let storage = ctx.storage.lock().await;
            storage
                .load_conversation_messages(&conv_id.to_string())
                .context("failed to load messages")?
        };
        let context: Vec<StorageMessage> = MessageConverter::llm_context_messages(&db_messages)
            .into_iter()
            .cloned()
            .collect();
        let llm_messages = MessageConverter::db_to_llm(&context, true);
        let agent_messages =
            ContextService::inject_prompts(llm_messages, &ctx.prompt_manager, resolved.profile())?;
        (conv_id, agent_messages)
    };

    // Memory RAG: same path as interactive chat.
    {
        let token_counter = TokenCounter::new(resolved.preset());
        let outcome = crate::services::memory_rag::inject_memory_block(
            ctx.storage.clone(),
            ctx.embedding_provider.as_deref(),
            &ctx.config.embedding,
            &token_counter,
            &mut agent_messages,
        )
        .await;
        if let Some(outcome) = outcome {
            let storage_guard = ctx.storage.lock().await;
            if let Err(e) =
                storage_guard.record_memory_recalls(&conversation_id.to_string(), &outcome.ids)
            {
                tracing::warn!(error = %e, "Failed to record memory recalls (analytics)");
            }
            drop(storage_guard);
            ctx.subscriptions
                .broadcast(
                    conversation_id,
                    ServerEvent::MemoriesRecalled {
                        conversation_id: conversation_id.to_string(),
                        memory_ids: outcome.ids,
                    },
                )
                .await;
        }
    }

    let allowed_tool_names = match crate::tools_policy::compute_allowed_tool_names(
        &ctx.mcp_registry,
        &ctx.config,
        resolved.profile(),
    )
    .await
    {
        Ok(set) => set,
        Err(e) => {
            tracing::error!(
                job_id = %job.id,
                profile = %profile_name,
                error = %e,
                "Tools policy computation failed for scheduled job; marking job failed"
            );
            let now = chrono::Utc::now().timestamp();
            let storage = ctx.storage.lock().await;
            storage
                .set_scheduled_job_completed(&job.id, now, true, Some(&e.to_string()))
                .context("failed to mark job failed")?;
            return Ok(());
        }
    };

    {
        let map = ctx.active_agent_runs.read().await;
        if map.contains_key(&conversation_id) {
            tracing::warn!(
                job_id = %job.id,
                conversation_id = %conversation_id,
                "Skipping scheduled job: agent already running for conversation"
            );
            return Ok(());
        }
    }

    let handle = spawn_agent_task(
        ctx.clone(),
        conversation_id,
        agent_messages,
        profile_name.to_string(),
        llm_client,
        allowed_tool_names,
    );
    {
        let mut map = ctx.active_agent_runs.write().await;
        map.insert(conversation_id, handle.abort_handle());
    }
    let _ = handle.await;

    let now = chrono::Utc::now().timestamp();
    let storage = ctx.storage.lock().await;
    if let Some(ref schedule) = job.schedule {
        let s = schedule.trim();
        if !s.is_empty() && !s.eq_ignore_ascii_case("once") {
            match next_run_from_cron(s, now) {
                Ok(next_run) => {
                    storage
                        .set_scheduled_job_next_run(&job.id, next_run, now)
                        .context("failed to set next run")?;
                }
                Err(e) => {
                    tracing::warn!(job_id = %job.id, error = %e, "Failed to compute next run, marking job completed");
                    storage
                        .set_scheduled_job_completed(&job.id, now, true, Some(&e.to_string()))
                        .context("failed to mark job failed")?;
                }
            }
        } else {
            storage
                .set_scheduled_job_completed(&job.id, now, false, None)
                .context("failed to mark job completed")?;
        }
    } else {
        storage
            .set_scheduled_job_completed(&job.id, now, false, None)
            .context("failed to mark job completed")?;
    }
    Ok(())
}

struct PersistenceContext {
    storage: Arc<Mutex<Storage>>,
    conversation_id: Uuid,
    pending_tool_calls: Vec<crate::llm::ToolCall>,
    tool_params: HashMap<String, Value>,
}

impl PersistenceContext {
    fn new(storage: Arc<Mutex<Storage>>, conversation_id: Uuid) -> Self {
        Self {
            storage,
            conversation_id,
            pending_tool_calls: Vec::new(),
            tool_params: HashMap::new(),
        }
    }

    async fn persist_assistant(&mut self, content: &str, reasoning_content: Option<&str>) -> Result<()> {
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
            .lock()
            .await
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
                self.pending_tool_calls.push(crate::llm::ToolCall {
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
        // Use empty content - tool_result_json holds the actual data
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
            .context("failed to persist tool result")?;
        Ok(())
    }
}

async fn process_agent_update(
    subscriptions: &std::sync::Arc<
        crate::server::conversation_subscriptions::ConversationSubscriptions,
    >,
    conversation_id: Uuid,
    persistence: &mut PersistenceContext,
    update: AgentUpdate,
) -> Result<()> {
    let cid = conversation_id.to_string();
    match update {
        AgentUpdate::AssistantStreamingStarted => {
            subscriptions
                .broadcast(
                    conversation_id,
                    ServerEvent::StreamingStarted {
                        conversation_id: cid.clone(),
                    },
                )
                .await;
        }
        AgentUpdate::AssistantDelta { text_chunk, seq } => {
            subscriptions
                .broadcast(
                    conversation_id,
                    ServerEvent::AssistantDelta {
                        conversation_id: cid.clone(),
                        chunk: text_chunk,
                        seq,
                    },
                )
                .await;
        }
        AgentUpdate::ReasoningContentDelta { chunk } => {
            subscriptions
                .broadcast(
                    conversation_id,
                    ServerEvent::ReasoningContentDelta {
                        conversation_id: cid.clone(),
                        chunk,
                    },
                )
                .await;
        }
        AgentUpdate::AssistantComplete { full_text, reasoning_content } => {
            persistence.persist_assistant(&full_text, reasoning_content.as_deref()).await?;
            subscriptions
                .broadcast(
                    conversation_id,
                    ServerEvent::AssistantComplete {
                        conversation_id: cid.clone(),
                        content: full_text,
                        reasoning_content: reasoning_content.clone(),
                    },
                )
                .await;
        }
        AgentUpdate::ToolPlanned { plan_items } => {
            persistence.register_tools(&plan_items);
            let converted = plan_items
                .into_iter()
                .filter_map(|plan| {
                    serde_json::from_str::<Value>(&plan.params_json)
                        .ok()
                        .map(|params| PlannedToolView {
                            id: plan.id,
                            name: plan.name,
                            params_json: params,
                        })
                })
                .collect();
            subscriptions
                .broadcast(
                    conversation_id,
                    ServerEvent::ToolPlanned {
                        conversation_id: cid.clone(),
                        tools: converted,
                    },
                )
                .await;
        }
        AgentUpdate::ToolStarted {
            tool_call_id,
            name,
            params_json,
        } => {
            persistence.mark_tool_started(&tool_call_id, &params_json);
            let params_value =
                serde_json::from_str::<Value>(&params_json).unwrap_or(Value::String(params_json));
            subscriptions
                .broadcast(
                    conversation_id,
                    ServerEvent::ToolStarted {
                        conversation_id: cid.clone(),
                        tool_call_id,
                        name,
                        params_json: params_value,
                    },
                )
                .await;
        }
        AgentUpdate::ToolResult {
            tool_call_id,
            name,
            result_json,
        } => {
            persistence
                .persist_tool_result(&tool_call_id, &name, &result_json, "success")
                .await?;
            let result_value = serde_json::from_str::<Value>(&result_json)
                .unwrap_or(Value::String(result_json.clone()));
            subscriptions
                .broadcast(
                    conversation_id,
                    ServerEvent::ToolResult {
                        conversation_id: cid.clone(),
                        tool_call_id,
                        name,
                        result_json: result_value,
                    },
                )
                .await;
        }
        AgentUpdate::ToolError {
            tool_call_id,
            name,
            error,
            ..
        } => {
            persistence
                .persist_tool_result(&tool_call_id, &name, &error, "error")
                .await?;
            subscriptions
                .broadcast(
                    conversation_id,
                    ServerEvent::ToolError {
                        conversation_id: cid.clone(),
                        tool_call_id,
                        name,
                        error,
                    },
                )
                .await;
        }
        AgentUpdate::ConversationComplete { .. } => {
            subscriptions
                .broadcast(
                    conversation_id,
                    ServerEvent::ConversationComplete {
                        conversation_id: cid.clone(),
                    },
                )
                .await;
        }
        AgentUpdate::ModelError { error } => {
            subscriptions
                .broadcast(conversation_id, ServerEvent::Error { message: error })
                .await;
        }
    }
    Ok(())
}
