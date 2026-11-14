use super::{helpers::{record_and_build_memories_recalled_event, truncate_preview}, ServerContext};
use crate::{
    agentic::{
        loop_engine::AgenticLoop,
        persistence::AgentPersistence,
        protocol::AgentUpdate,
        RunContext,
    },
    llm::{self, Message as LlmMessage, Role},
    llm::tokenizer::TokenCounter,
    server::dto::{PlannedToolView, ServerEvent},
    services::{ContextService, MessageConverter},
    storage::{sqlite_storage_simple::Message as StorageMessage, ScheduledJob},
};
use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::{collections::HashSet, sync::Arc, time::Duration};
use tokio::task::JoinHandle;
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
        depth: 0,
    };

    tokio::spawn(async move {
        // Bounded: if persistence/broadcast stalls (e.g. storage contention),
        // the agent loop awaits instead of queueing updates without limit.
        let (agent_tx, mut agent_rx) = tokio::sync::mpsc::channel::<AgentUpdate>(
            crate::agentic::protocol::AGENT_UPDATE_CHANNEL_CAP,
        );
        let mut loop_engine = AgenticLoop::new(
            mcp_registry,
            llm_client,
            Some(schedule_service),
            ctx.server_cfg.tool_call_timeout_secs,
        )
        .with_storage(storage.clone())
        .with_app_config(ctx.config.clone());
        let subs = subscriptions.clone();
        let stream_task = tokio::spawn(async move {
            let mut persistence = AgentPersistence::new(storage, conversation_id);
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

    let (conversation_id, mut agent_messages, time_last_at) = if let Some(conv_id_str) = &job.conversation_id {
        let conv_uuid = Uuid::parse_str(conv_id_str).context("invalid conversation_id in job")?;
        let exists = {
            let storage = ctx.storage.clone();
            storage.get_conversation(&conv_uuid).context("failed to get conversation")?.is_some()
        };
        if !exists {
            let storage = ctx.storage.clone();
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
            let storage = ctx.storage.clone();
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
        // Synthetic "due now" turn is not persisted; last stored row is the prior activity.
        let time_last_at = db_messages.last().map(|m| m.created_at);
        (conv_uuid, agent_messages, time_last_at)
    } else {
        let title = job
            .title
            .clone()
            .unwrap_or_else(|| truncate_preview(&job.message));
        let conv_id = {
            let storage = ctx.storage.clone();
            storage
                .create_conversation_with_profile(title.clone(), Some(profile_name), false)
                .context("failed to create conversation")?
        };
        {
            let storage = ctx.storage.clone();
            storage
                .add_message_to_conversation(&conv_id, "user".to_string(), job.message.clone())
                .context("failed to add message")?;
        }
        let db_messages = {
            let storage = ctx.storage.clone();
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
        (conv_id, agent_messages, None)
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
            let storage_guard = ctx.storage.clone();
            if let Ok(Some(message_rowid)) = storage_guard
                .get_latest_user_message_id(&conversation_id.to_string())
            {
                match record_and_build_memories_recalled_event(
                    &storage_guard,
                    &conversation_id.to_string(),
                    message_rowid,
                    &outcome,
                ) {
                    Ok(event) => {
                        ctx.subscriptions
                            .broadcast(conversation_id, event)
                            .await;
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to record memory recalls for scheduled task");
                    }
                }
            } else {
                tracing::warn!("Scheduled task: no user message to attribute memory recalls");
            }
        }
    }

    // Time awareness: same ephemeral system block as interactive chat.
    if ctx.config.time_awareness.enabled {
        let last_at = time_last_at.and_then(|ts| chrono::DateTime::from_timestamp(ts, 0));
        if let Some(body) = crate::services::time_awareness::build_time_context(
            last_at,
            chrono::Utc::now(),
            &ctx.config.time_awareness,
        ) {
            crate::services::time_awareness::insert_time_context(&mut agent_messages, body, false);
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
            let storage = ctx.storage.clone();
            storage
                .set_scheduled_job_completed(&job.id, now, true, Some(&e.to_string()))
                .context("failed to mark job failed")?;
            return Ok(());
        }
    };

    // Atomically check + spawn + register under ONE write-lock scope so racing
    // callers can never start two agents for the same conversation.
    let mut collision = false;
    let spawned = {
        let mut map = ctx.active_agent_runs.write().await;
        match map.entry(conversation_id) {
            std::collections::hash_map::Entry::Vacant(vacant) => {
                let h = spawn_agent_task(
                    ctx.clone(),
                    conversation_id,
                    agent_messages,
                    profile_name.to_string(),
                    llm_client,
                    allowed_tool_names,
                );
                vacant.insert(h.abort_handle());
                Some(h)
            }
            std::collections::hash_map::Entry::Occupied(_) => {
                collision = true;
                None
            }
        }
    };
    if collision {
        tracing::warn!(
            job_id = %job.id,
            conversation_id = %conversation_id,
            "Skipping scheduled job: agent already running for conversation"
        );
        // Finalize the row so it doesn't stay stuck in 'running' forever:
        // recurring jobs skip to their next run, one-off jobs are failed.
        let now = chrono::Utc::now().timestamp();
        let storage = ctx.storage.clone();
        let recurring = job
            .schedule
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("once"));
        match recurring.map(|s| crate::services::next_run_from_cron(s, now)) {
            Some(Ok(next_run)) => {
                let _ = storage.set_scheduled_job_next_run(&job.id, next_run, now);
            }
            Some(Err(e)) => {
                let _ = storage.set_scheduled_job_completed(
                    &job.id,
                    now,
                    true,
                    Some(&format!(
                        "Skipped (agent already running) and failed to compute next run: {e}"
                    )),
                );
            }
            None => {
                let _ = storage.set_scheduled_job_completed(
                    &job.id,
                    now,
                    true,
                    Some("Skipped: agent already running for conversation"),
                );
            }
        }
        return Ok(());
    }
    let handle = spawned.expect("handle is Some when there was no collision");
    let agent_failed = match handle.await {
        Ok(()) => None,
        Err(join_err) => {
            let msg = if join_err.is_panic() {
                "Agent task panicked".to_string()
            } else {
                "Agent task was cancelled".to_string()
            };
            tracing::error!(job_id = %job.id, error = %join_err, "Scheduler: {}", msg);
            Some(msg)
        }
    };

    let now = chrono::Utc::now().timestamp();
    let storage = ctx.storage.clone();
    if let Some(reason) = agent_failed {
        storage
            .set_scheduled_job_completed(&job.id, now, true, Some(&reason))
            .context("failed to mark job failed")?;
        return Ok(());
    }
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

async fn process_agent_update(
    subscriptions: &std::sync::Arc<
        crate::server::conversation_subscriptions::ConversationSubscriptions,
    >,
    conversation_id: Uuid,
    persistence: &mut AgentPersistence,
    update: AgentUpdate,
) -> Result<()> {
    // Durable side effects first; on failure the stream task broadcasts an
    // Error and the event below is skipped (same behavior as before the
    // persistence unification).
    persistence.persist(&update).await?;

    let cid = conversation_id.to_string();
    match update {
        AgentUpdate::AssistantStreamingStarted => {
            subscriptions
                .broadcast(
                    conversation_id,
                    ServerEvent::StreamingStarted {
                        conversation_id: cid,
                    },
                )
                .await;
        }
        AgentUpdate::AssistantDelta { text_chunk, seq } => {
            subscriptions
                .broadcast(
                    conversation_id,
                    ServerEvent::AssistantDelta {
                        conversation_id: cid,
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
                        conversation_id: cid,
                        chunk,
                    },
                )
                .await;
        }
        AgentUpdate::AssistantComplete {
            full_text,
            reasoning_content,
        } => {
            subscriptions
                .broadcast(
                    conversation_id,
                    ServerEvent::AssistantComplete {
                        conversation_id: cid,
                        content: full_text,
                        reasoning_content,
                    },
                )
                .await;
        }
        AgentUpdate::ToolPlanned { plan_items } => {
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
                        conversation_id: cid,
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
            let params_value =
                serde_json::from_str::<Value>(&params_json).unwrap_or(Value::String(params_json));
            subscriptions
                .broadcast(
                    conversation_id,
                    ServerEvent::ToolStarted {
                        conversation_id: cid,
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
            let result_value = serde_json::from_str::<Value>(&result_json)
                .unwrap_or(Value::String(result_json.clone()));
            subscriptions
                .broadcast(
                    conversation_id,
                    ServerEvent::ToolResult {
                        conversation_id: cid,
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
            subscriptions
                .broadcast(
                    conversation_id,
                    ServerEvent::ToolError {
                        conversation_id: cid,
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
                        conversation_id: cid,
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
