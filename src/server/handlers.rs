use crate::{
    agentic::{
        loop_engine::AgenticLoop,
        protocol::{AgentUpdate, PlannedTool},
        RunContext,
    },
    config::{AppConfig, ServerConfig},
    llm::{self, Message as LlmMessage, Role},
    llm::tokenizer::TokenCounter,
    llm::context_manager::SmartContextManager,
    prompts::PromptManager,
    services::{ContextService, MessageConverter, ScheduleService},
    server::conversation_subscriptions::ConnectionId,
    server::dto::{
        ClientCommand, ConversationSummary, ConversationView, MessageView, PlannedToolView,
        SearchResult, ServerEvent,
    },
    storage::{
        conversation_storage::Conversation as StoredConversation,
        sqlite_storage_simple::{Message as StorageMessage, MessageMetadata}, ScheduledJob, Storage,
    },
};
use agentic_loop::mcp_servers_registry::MCPServerRegistry;
use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::{collections::{HashMap, HashSet}, sync::Arc, time::Duration};
use tokio::{
    sync::{mpsc::UnboundedSender, Mutex, RwLock},
    task::JoinHandle,
};
use uuid::Uuid;

pub struct ServerContext {
    pub config: Arc<AppConfig>,
    pub server_cfg: Arc<ServerConfig>,
    pub prompt_manager: PromptManager,
    pub storage: Arc<Mutex<Storage>>,
    pub mcp_registry: Arc<RwLock<MCPServerRegistry>>,
    pub subscriptions: Arc<crate::server::conversation_subscriptions::ConversationSubscriptions>,
    pub schedule_service: Arc<ScheduleService>,
    /// Tracks which memory IDs have been injected per conversation (dedup for Memory RAG)
    pub memory_dedup: Mutex<HashMap<Uuid, HashSet<i64>>>,
}

pub struct SessionState {
    pub profile_name: String,
    pub llm_client: Arc<dyn crate::llm::LlmClient>,
    pub active_conversation_id: Option<Uuid>,
    inflight: Vec<JoinHandle<()>>,
}

impl SessionState {
    pub fn new(config: &AppConfig) -> Result<Self> {
        let profile = config
            .get_default_profile()
            .unwrap_or(&crate::config::LlmProfile::default())
            .clone();
        Ok(Self {
            profile_name: config.default.clone(),
            llm_client: llm::build_llm_client(&profile),
            active_conversation_id: None,
            inflight: Vec::new(),
        })
    }

    pub fn update_profile(&mut self, profile_name: &str, config: &AppConfig) -> Result<()> {
        if let Some(profile) = config.get_profile(profile_name).cloned() {
            self.profile_name = profile_name.to_string();
            self.llm_client = llm::build_llm_client(&profile);
            Ok(())
        } else {
            anyhow::bail!("Profile '{}' not found", profile_name)
        }
    }

    pub fn active_profile<'a>(
        &'a self,
        config: &'a AppConfig,
    ) -> Result<&'a crate::config::LlmProfile> {
        config
            .get_profile(&self.profile_name)
            .or_else(|| config.get_default_profile())
            .context("No active profile configured")
    }

    pub fn track_task(&mut self, handle: JoinHandle<()>) {
        self.inflight.push(handle);
        self.inflight.retain(|h| !h.is_finished());
    }
}

pub struct ServerHandler {
    pub ctx: Arc<ServerContext>,
    pub session: SessionState,
    pub connection_id: ConnectionId,
    outbound: UnboundedSender<ServerEvent>,
}

impl ServerHandler {
    pub fn new(
        ctx: Arc<ServerContext>,
        connection_id: ConnectionId,
        outbound: UnboundedSender<ServerEvent>,
    ) -> Result<Self> {
        Ok(Self {
            session: SessionState::new(&ctx.config)?,
            ctx,
            connection_id,
            outbound,
        })
    }

    pub async fn handle_command(&mut self, command: ClientCommand) {
        tracing::debug!("Received command: {:?}", command);
        let result = match command {
            ClientCommand::HealthCheck => self.handle_health().await,
            ClientCommand::StartConversation { title } => {
                self.handle_start_conversation(title).await
            }
            ClientCommand::LoadConversation { conversation_id } => {
                self.handle_load_conversation(conversation_id).await
            }
            ClientCommand::ListConversations { query, limit, offset } => {
                self.handle_list_conversations(query, limit, offset).await
            }
            ClientCommand::ChangeProfile { profile } => self.handle_change_profile(profile).await,
            ClientCommand::ListProfiles => self.handle_list_profiles().await,
            ClientCommand::SendMessage {
                conversation_id,
                content,
            } => self.handle_send_message(conversation_id, content).await,
            ClientCommand::DeleteConversation { conversation_id } => {
                self.handle_delete_conversation(conversation_id).await
            }
            ClientCommand::TruncateConversation { conversation_id, message_id } => {
                self.handle_truncate_conversation(conversation_id, message_id).await
            }
            ClientCommand::StopStreaming { conversation_id } => {
                self.handle_stop_streaming(conversation_id).await
            }
        };

        if let Err(err) = result {
            let _ = self.outbound.send(ServerEvent::Error {
                message: err.to_string(),
            });
        }
    }

    async fn handle_health(&self) -> Result<()> {
        let timestamp = chrono::Utc::now().timestamp();
        let _ = self.outbound.send(ServerEvent::HealthOk {
            timestamp,
            profile: self.session.profile_name.clone(),
        });
        Ok(())
    }

    async fn handle_start_conversation(&mut self, title: Option<String>) -> Result<()> {
        let title_text = title.unwrap_or_else(|| "Generating title...".to_string());
        let profile_name = Some(self.session.profile_name.clone());
        let storage = self.ctx.storage.lock().await;
        let conversation_id = storage
            .create_conversation_with_profile(title_text, profile_name.as_deref())
            .context("failed to create conversation")?;
        // Clear active conversation - new conversation will be set when loaded
        self.session.active_conversation_id = None;
        let _ = self.outbound.send(ServerEvent::ConversationCreated {
            conversation_id: conversation_id.to_string(),
        });
        Ok(())
    }

    async fn handle_load_conversation(&mut self, conversation_id: String) -> Result<()> {
        let uuid = Uuid::parse_str(&conversation_id).context("invalid conversation id format")?;
        let storage = self.ctx.storage.lock().await;
        if let Some(conv) = storage
            .get_conversation(&uuid)
            .context("failed to load conversation")?
        {
            // Restore profile if conversation has one and it differs from current session
            if let Some(conv_profile) = &conv.profile_name {
                if conv_profile != &self.session.profile_name {
                    // Restore the conversation's profile
                    self.session.update_profile(conv_profile, &self.ctx.config)?;
                    let _ = self.outbound.send(ServerEvent::ProfileChanged {
                        profile: conv_profile.clone(),
                    });
                }
            } else {
                // Conversation has no profile stored - set it to current session profile
                // This handles old conversations created before profile support
                // Do this silently (no ProfileChanged event) since session already has this profile
                let session_profile = Some(self.session.profile_name.clone());
                let _ = storage.update_conversation_profile(&uuid, session_profile.as_deref())
                    .context("failed to update conversation profile");
            }
            
            // Track this as the active conversation
            self.session.active_conversation_id = Some(uuid);

            // Subscribe this connection to conversation-scoped events (broadcast)
            self.ctx
                .subscriptions
                .set_viewing(self.connection_id, Some(uuid), self.outbound.clone())
                .await;

            let view = to_conversation_view(&conv);
            let _ = self
                .outbound
                .send(ServerEvent::ConversationLoaded { conversation: view });
        } else {
            return Err(anyhow::anyhow!("Conversation {} not found", conversation_id)
                .context("Failed to load conversation"));
        }
        Ok(())
    }

    async fn handle_list_conversations(
        &self,
        query: Option<String>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<()> {
        let storage = self.ctx.storage.lock().await;
        if let Some(q) = query.filter(|s| !s.trim().is_empty()) {
            let results = storage
                .search_history(&q, limit.unwrap_or(240) as usize)
                .context("history search failed")?;
            let mapped: Vec<SearchResult> = results
                .into_iter()
                .map(|snippet| SearchResult {
                    conversation_id: snippet.conversation_id,
                    snippet: snippet.content,
                    timestamp: snippet.timestamp,
                    rank: snippet.rank,
                })
                .collect();
            let _ = self
                .outbound
                .send(ServerEvent::SearchResults { results: mapped });
        } else {
            let conversations = storage
                .list_conversations_paginated(
                    offset.map(|o| o as usize),
                    limit.map(|l| l as usize),
                )
                .context("failed to list conversations")?;
            let summaries: Vec<ConversationSummary> = conversations
                .into_iter()
                .map(|conv| ConversationSummary {
                    id: conv.id.to_string(),
                    title: conv.title.clone(),
                    last_message_preview: conv
                        .messages
                        .last()
                        .map(|msg| truncate_preview(&msg.content)),
                    updated_at: conv.updated_at.timestamp(),
                })
                .collect();
            let _ = self.outbound.send(ServerEvent::ConversationsList {
                conversations: summaries,
            });
        }
        Ok(())
    }

    async fn handle_change_profile(&mut self, profile: String) -> Result<()> {
        self.session.update_profile(&profile, &self.ctx.config)?;
        let _ = self.outbound.send(ServerEvent::ProfileChanged { profile: profile.clone() });
        
        // Update active conversation's profile in database if there is one
        if let Some(conv_id) = self.session.active_conversation_id {
            let storage = self.ctx.storage.lock().await;
            if let Err(e) = storage.update_conversation_profile(&conv_id, Some(&profile)) {
                tracing::error!(
                    conversation_id = %conv_id,
                    error = %e,
                    "Failed to update active conversation profile"
                );
            }
        }
        
        Ok(())
    }

    async fn handle_list_profiles(&self) -> Result<()> {
        let mut profiles: Vec<String> = self.ctx.config.profiles
            .iter()
            .filter(|(_, p)| !p.hidden)
            .map(|(name, _)| name.clone())
            .collect();
        profiles.sort();
        let _ = self.outbound.send(ServerEvent::ProfilesList {
            profiles,
            default_profile: self.ctx.config.default.clone(),
        });
        Ok(())
    }

    async fn handle_send_message(
        &mut self,
        conversation_id: Option<String>,
        content: String,
    ) -> Result<()> {
        if content.trim().is_empty() {
            return Err(anyhow!("Cannot send an empty message"));
        }

        let storage = self.ctx.storage.lock().await;
        let conversation_uuid = if let Some(existing) = conversation_id {
            let uuid = Uuid::parse_str(&existing).context("invalid conversation id")?;
            
            // Check if conversation's stored profile differs from current session profile
            // If so, update the conversation's stored profile to match session profile
            if let Some(conv) = storage.get_conversation(&uuid).context("failed to get conversation")? {
                let conv_profile = conv.profile_name.as_deref();
                let session_profile = Some(self.session.profile_name.as_str());
                
                // Update conversation's profile if it differs
                if conv_profile != session_profile {
                    let _ = storage.update_conversation_profile(&uuid, session_profile)
                        .context("failed to update conversation profile");
                }
            }
            
            uuid
        } else {
            // Store current profile when creating new conversation
            let profile_name = Some(self.session.profile_name.clone());
            let conv_id = storage
                .create_conversation_with_profile("Generating title...".to_string(), profile_name.as_deref())
                .context("failed to create conversation")?;
            let preview = truncate_preview(&content);
            let _ = storage.update_conversation_title(&conv_id, preview);
            let _ = self.outbound.send(ServerEvent::ConversationCreated {
                conversation_id: conv_id.to_string(),
            });
            conv_id
        };

        storage
            .add_message_to_conversation(&conversation_uuid, "user".to_string(), content.clone())
            .context("failed to persist user message")?;
        drop(storage);

        let _ = self.outbound.send(ServerEvent::MessageAccepted {
            conversation_id: conversation_uuid.to_string(),
        });

        // Subscribe this connection so it (and any other viewer) receives broadcast events
        self.ctx
            .subscriptions
            .set_viewing(
                self.connection_id,
                Some(conversation_uuid),
                self.outbound.clone(),
            )
            .await;

        let profile = self.session.active_profile(&self.ctx.config)?.clone();
        let mut llm_messages = self.build_llm_messages(conversation_uuid).await?;
        let prompt_manager = self.ctx.prompt_manager.clone();
        llm_messages.push(LlmMessage::new(Role::User, content.clone()));

        let agent_messages = ContextService::inject_prompts(llm_messages, &prompt_manager, &profile)?;
        
        // Check if summarization is needed and trigger it using ContextService
        {
            let storage = self.ctx.storage.clone();
            let llm_client = self.session.llm_client.clone();
            
            if let Err(e) = ContextService::check_and_trigger_summarization(
                &agent_messages,
                conversation_uuid,
                storage,
                &llm_client,
                &profile,
            ).await {
                tracing::warn!(error = %e, "Failed to check/trigger summarization, continuing anyway");
            }
        }
        
        // Reload messages after summarization (if it happened) and rebuild
        let mut llm_messages = self.build_llm_messages(conversation_uuid).await?;
        
        // Re-inject prompts after reload
        llm_messages = ContextService::inject_prompts(llm_messages, &self.ctx.prompt_manager, &profile)?;

        // Memory RAG: search and inject relevant memories
        {
            let storage_guard = self.ctx.storage.lock().await;
            let mut dedup_guard = self.ctx.memory_dedup.lock().await;
            let used_ids = dedup_guard.entry(conversation_uuid).or_default();
            if let Some(memory_msg) = crate::services::memory_rag::retrieve_memory_context(
                &storage_guard,
                &content,
                used_ids,
            ) {
                // Insert after system prompts but before conversation history
                let insert_pos = llm_messages
                    .iter()
                    .position(|m| !matches!(m.role, Role::System))
                    .unwrap_or(llm_messages.len());
                llm_messages.insert(insert_pos, LlmMessage::new(Role::System, memory_msg));
            }
        }

        // Recalculate tokens after summarization (if it happened)
        let token_counter = TokenCounter::new(&profile);
        let total_tokens: usize = llm_messages.iter()
            .map(|msg| token_counter.count_message_tokens(msg))
            .sum();
        
        let context_limit = token_counter.get_context_limit(&profile);
        
        // Log final token usage
        let usage_percent = (total_tokens as f32 / context_limit as f32 * 100.0) as u32;
        tracing::info!(
            "Context usage after summarization: {} tokens / {} limit ({}%)",
            total_tokens,
            context_limit,
            usage_percent
        );
        
        // Apply smart context selection if still over safe limit
        let mut agent_messages = if total_tokens > token_counter.get_safe_context_limit(&profile) {
            tracing::info!(
                total_tokens,
                safe_limit = token_counter.get_safe_context_limit(&profile),
                "Context still exceeds safe limit after summarization, applying smart truncation"
            );
            
            crate::llm::context_manager::SmartContextManager::select_context(
                llm_messages,
                &token_counter,
                &profile,
            )
        } else {
            llm_messages
        };
        
        // Continue with agent loop using final messages
        // (Summarization now handled by ContextService::check_and_trigger_summarization above)
        
        // Apply additional safety checks if needed
        let safe_limit = token_counter.get_safe_context_limit(&profile);
        let hard_limit = token_counter.get_context_limit(&profile);
        
        // Always ensure we don't exceed the hard limit (API will reject if we do)
        if total_tokens > hard_limit {
            tracing::warn!(
                total_tokens,
                hard_limit,
                "CRITICAL: Context exceeds hard limit, forcing truncation"
            );
            let original_count = agent_messages.len();
            agent_messages = SmartContextManager::select_context(agent_messages, &token_counter, &profile);
            let selected_count = agent_messages.len();
            let selected_tokens: usize = agent_messages.iter()
                .map(|msg| token_counter.count_message_tokens(msg))
                .sum();
            
            tracing::warn!(
                original_count,
                selected_count,
                total_tokens,
                selected_tokens,
                "Emergency truncation: messages and tokens reduced"
            );
            
            // Notify user about truncation (info, not error – conversation continues)
            let _ = self.outbound.send(ServerEvent::Info {
                message: format!(
                    "Context exceeded limit! Truncated: {} messages -> {} messages ({} tokens)",
                    original_count,
                    selected_count,
                    selected_tokens
                ),
            });
        } else if total_tokens > safe_limit {
            tracing::info!(
                "Context overflow detected: {} tokens > {} safe limit. Applying smart context selection.",
                total_tokens,
                safe_limit
            );
            let original_count = agent_messages.len();
            agent_messages = SmartContextManager::select_context(agent_messages, &token_counter, &profile);
            let selected_count = agent_messages.len();
            let selected_tokens: usize = agent_messages.iter()
                .map(|msg| token_counter.count_message_tokens(msg))
                .sum();
            
            tracing::info!(
                "Context selection: {} messages -> {} messages ({} tokens -> {} tokens)",
                original_count,
                selected_count,
                total_tokens,
                selected_tokens
            );
            
            // Notify user about truncation (info, not error – conversation continues)
            let _ = self.outbound.send(ServerEvent::Info {
                message: format!(
                    "Context truncated: {} messages selected from {} ({} tokens used)",
                    selected_count,
                    original_count,
                    selected_tokens
                ),
            });
        }
        
        // Final safety check: verify we're under the hard limit before sending
        let final_tokens: usize = agent_messages.iter()
            .map(|msg| token_counter.count_message_tokens(msg))
            .sum();
        
        if final_tokens > hard_limit {
            tracing::error!(
                total_tokens = final_tokens,
                context_limit = hard_limit,
                "FATAL: After truncation, still over limit (this should not happen)"
            );
            // Emergency fallback: keep only system messages and most recent messages
            let system_count = agent_messages.iter()
                .take_while(|m| matches!(m.role, Role::System))
                .count();
            let mut emergency_messages: Vec<LlmMessage> = agent_messages[..system_count].to_vec();
            let mut emergency_tokens: usize = emergency_messages.iter()
                .map(|msg| token_counter.count_message_tokens(msg))
                .sum();
            
            // Add recent messages until we hit the limit
            for msg in agent_messages.iter().skip(system_count).rev() {
                let msg_tokens = token_counter.count_message_tokens(msg);
                if emergency_tokens + msg_tokens <= hard_limit {
                    emergency_messages.push(msg.clone());
                    emergency_tokens += msg_tokens;
                } else {
                    break;
                }
            }
            
            agent_messages = emergency_messages;
            tracing::warn!(
                message_count = agent_messages.len(),
                "Emergency fallback: Reduced messages"
            );
        }

        let handle = spawn_agent_task(
            self.ctx.clone(),
            conversation_uuid,
            agent_messages,
            self.session.profile_name.clone(),
            self.session.llm_client.clone(),
        );
        self.session.track_task(handle);
        Ok(())
    }

    async fn handle_delete_conversation(&mut self, conversation_id: String) -> Result<()> {
        let uuid = Uuid::parse_str(&conversation_id).context("invalid conversation id format")?;
        let storage = self.ctx.storage.lock().await;
        let deleted = storage
            .delete_conversation(&uuid)
            .context("failed to delete conversation")?;
        if deleted {
            // Clear active conversation if it was deleted
            if self.session.active_conversation_id == Some(uuid) {
                self.session.active_conversation_id = None;
            }
            let _ = self.outbound.send(ServerEvent::ConversationDeleted {
                conversation_id,
            });
        } else {
            return Err(anyhow!("Conversation not found"));
        }
        Ok(())
    }

    async fn handle_truncate_conversation(
        &mut self,
        conversation_id: String,
        message_id: String,
    ) -> Result<()> {
        tracing::info!(
            "Truncating conversation {} at message {}",
            conversation_id,
            message_id
        );

        let conv_uuid = Uuid::parse_str(&conversation_id)
            .context("invalid conversation id format")?;
        let msg_uuid = Uuid::parse_str(&message_id)
            .context("invalid message id format")?;

        // Delete messages up to and including the specified message
        let storage = self.ctx.storage.lock().await;
        let deleted_count = storage
            .truncate_conversation(&conv_uuid, &msg_uuid)
            .context("failed to truncate conversation")?;

        tracing::info!(
            "Truncated {} messages from conversation {}",
            deleted_count,
            conversation_id
        );

        // Reload the conversation and send ConversationLoaded event
        if let Some(conv) = storage
            .get_conversation(&conv_uuid)
            .context("failed to reload conversation after truncation")?
        {
            tracing::info!(
                "Reloaded conversation {} with {} messages",
                conversation_id,
                conv.messages.len()
            );
            let view = to_conversation_view(&conv);
            let _ = self.outbound.send(ServerEvent::ConversationLoaded {
                conversation: view,
            });
        } else {
            return Err(anyhow!("Conversation not found after truncation"));
        }

        Ok(())
    }

    async fn handle_stop_streaming(&mut self, conversation_id: Option<String>) -> Result<()> {
        // Abort all inflight tasks started by this connection
        let mut handles = Vec::new();
        std::mem::swap(&mut handles, &mut self.session.inflight);

        for handle in handles {
            handle.abort();
        }

        let conv_id_str = conversation_id.unwrap_or_else(|| "unknown".to_string());
        let event = ServerEvent::StreamingStopped {
            conversation_id: conv_id_str.clone(),
        };
        // Notify this connection
        let _ = self.outbound.send(event.clone());
        // Broadcast so all viewers of this conversation see the stop
        if let Ok(uuid) = Uuid::parse_str(&conv_id_str) {
            self.ctx.subscriptions.broadcast(uuid, event).await;
        }

        Ok(())
    }

    async fn build_llm_messages(&self, conversation_id: Uuid) -> Result<Vec<LlmMessage>> {
        let storage = self.ctx.storage.lock().await;
        // Load messages directly from storage (more efficient than loading full conversation)
        let db_messages = storage
            .load_conversation_messages(&conversation_id.to_string())
            .context("failed to load conversation messages")?;
        
        // Use MessageConverter service (single source of truth)
        Ok(MessageConverter::db_to_llm(&db_messages, true))
    }
}

/// Spawn the agent task for a conversation. Used by handle_send_message and run_scheduled_task.
pub fn spawn_agent_task(
    ctx: Arc<ServerContext>,
    conversation_id: Uuid,
    agent_messages: Vec<LlmMessage>,
    profile_name: String,
    llm_client: Arc<dyn crate::llm::LlmClient>,
) -> JoinHandle<()> {
    let subscriptions = ctx.subscriptions.clone();
    let mcp_registry = ctx.mcp_registry.clone();
    let schedule_service = ctx.schedule_service.clone();
    let timeout = Duration::from_secs(ctx.server_cfg.stream_timeout_secs);
    let storage = ctx.storage.clone();
    let run_context = RunContext {
        conversation_id: Some(conversation_id),
        profile_name: profile_name.clone(),
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
    })
}

/// Run a scheduled job: build messages, spawn agent, update job status (and next run if recurring).
pub async fn run_scheduled_task(ctx: Arc<ServerContext>, job: ScheduledJob) -> Result<()> {
    use crate::services::next_run_from_cron;

    let profile_name = job
        .profile_name
        .as_deref()
        .unwrap_or_else(|| ctx.config.default.as_str());
    let profile = ctx
        .config
        .get_profile(profile_name)
        .or_else(|| ctx.config.get_default_profile())
        .cloned()
        .ok_or_else(|| anyhow!("No profile found for scheduled job"))?;
    let llm_client = llm::build_llm_client(&profile);

    let (conversation_id, agent_messages) = if let Some(conv_id_str) = &job.conversation_id {
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
        let mut llm_messages = MessageConverter::db_to_llm(&db_messages, true);
        llm_messages.push(LlmMessage::new(
            Role::User,
            format!(
                "Scheduled task (due now): {}. Please carry out this task.",
                job.message
            ),
        ));
        let agent_messages =
            ContextService::inject_prompts(llm_messages, &ctx.prompt_manager, &profile)?;
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
            let mut storage = ctx.storage.lock().await;
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
        let llm_messages = MessageConverter::db_to_llm(&db_messages, true);
        let agent_messages =
            ContextService::inject_prompts(llm_messages, &ctx.prompt_manager, &profile)?;
        (conv_id, agent_messages)
    };

    let handle = spawn_agent_task(
        ctx.clone(),
        conversation_id,
        agent_messages,
        profile_name.to_string(),
        llm_client,
    );
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

fn truncate_preview(text: &str) -> String {
    const MAX_PREVIEW_CHARS: usize = 60;
    const TRUNCATED_CHARS: usize = MAX_PREVIEW_CHARS - 3; // keep room for "..."

    if text.chars().count() > MAX_PREVIEW_CHARS {
        let truncated: String = text.chars().take(TRUNCATED_CHARS).collect();
        format!("{truncated}...")
    } else {
        text.to_string()
    }
}

fn to_conversation_view(conv: &StoredConversation) -> ConversationView {
    ConversationView {
        id: conv.id.to_string(),
        title: conv.title.clone(),
        created_at: conv.created_at.timestamp(),
        updated_at: conv.updated_at.timestamp(),
        messages: conv.messages.iter().map(MessageView::from).collect(),
        profile_name: conv.profile_name.clone(),
    }
}

// Removed: conversation_to_llm() - Now using MessageConverter::db_to_llm() service
// This function was replaced to eliminate duplication and use the single source of truth

// Removed: inject_prompts() - Now using ContextService::inject_prompts() service
// This function was replaced to eliminate duplication and use the single source of truth

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
