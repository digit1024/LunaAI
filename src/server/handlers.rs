use crate::{
    agentic::{
        loop_engine::AgenticLoop,
        protocol::{AgentUpdate, PlannedTool},
        RunContext,
    },
    config::{AppConfig, ServerConfig},
    embeddings::EmbeddingProvider,
    llm::{self, Message as LlmMessage, Role},
    llm::tokenizer::TokenCounter,
    llm::context_manager::SmartContextManager,
    prompts::PromptManager,
    services::{
        conversation_tail, ContextService, MessageConverter, ScheduleService,
    },
    server::conversation_subscriptions::ConnectionId,
    server::dto::{
        ClientCommand, ConversationSummary, ConversationView, MemoryView, MessageView,
        PlannedToolView, SearchResult, ServerEvent,
    },
    storage::{
        conversation_storage::Conversation as StoredConversation,
        sqlite_storage_simple::{
            MemoryEntry, Message as StorageMessage, MessageMetadata,
        },
        ScheduledJob, Storage,
    },
};
use agentic_loop::mcp_servers_registry::MCPServerRegistry;
use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Arc,
    time::Duration,
};
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
    /// Allowed tool names for the default profile (from tools policy); used when creating new sessions.
    pub default_allowed_tool_names: HashSet<String>,
    /// Embedding provider for memory vector search. None when embedding is disabled.
    pub embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
    /// Per-conversation agent runs (abort handles); prevents double-spawn across connections.
    pub active_agent_runs: Arc<RwLock<HashMap<Uuid, tokio::task::AbortHandle>>>,
}

struct RunAgentOptions {
    auto_summarize: bool,
}

pub struct SessionState {
    pub profile_name: String,
    pub llm_client: Arc<dyn crate::llm::LlmClient>,
    pub active_conversation_id: Option<Uuid>,
    /// Allowed tool names for this profile (from tools policy); internal tools are only added if in this set.
    pub allowed_tool_names: HashSet<String>,
    inflight: Vec<JoinHandle<()>>,
}

impl SessionState {
    pub fn new(config: &AppConfig, default_allowed_tool_names: &HashSet<String>) -> Result<Self> {
        let default_name = &config.default;
        let resolved = config.resolve_default_profile().ok_or_else(|| {
            let hint = if config.profiles.get(default_name).is_none() {
                format!(
                    "Profile '{default_name}' is not defined in [profiles]. Add [profiles.{default_name}] with model_preset and tools_policy (see docs/sample_config.toml)."
                )
            } else {
                let preset_name = config.profiles.get(default_name).map(|p| p.model_preset.clone()).unwrap_or_default();
                format!(
                    "Profile '{default_name}' references model_preset '{preset_name}' which is not defined in [model_presets]. Add [model_presets.{preset_name}] (see docs/sample_config.toml)."
                )
            };
            anyhow::anyhow!("No default profile or preset configured. {}", hint)
        })?;
        Ok(Self {
            profile_name: config.default.clone(),
            llm_client: llm::build_llm_client(resolved.preset()),
            active_conversation_id: None,
            allowed_tool_names: default_allowed_tool_names.clone(),
            inflight: Vec::new(),
        })
    }

    pub async fn update_profile(
        &mut self,
        profile_name: &str,
        config: &AppConfig,
        mcp_registry: &Arc<RwLock<MCPServerRegistry>>,
    ) -> Result<()> {
        let resolved = config
            .resolve_profile(profile_name)
            .context("Profile or its model preset not found")?;
        self.profile_name = profile_name.to_string();
        self.llm_client = llm::build_llm_client(resolved.preset());
        self.allowed_tool_names = crate::tools_policy::compute_allowed_tool_names(
            mcp_registry,
            config,
            resolved.profile(),
        )
        .await
        .context("Compute tools policy for profile")?;
        Ok(())
    }

    /// Resolve current profile name to profile + preset. Use for building messages and token counts.
    pub fn active_resolved(&self, config: &AppConfig) -> Result<crate::config::ResolvedProfile> {
        config
            .resolve_profile(&self.profile_name)
            .or_else(|| config.resolve_default_profile())
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
            session: SessionState::new(&ctx.config, &ctx.default_allowed_tool_names)?,
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
                attachment_ids,
            } => self.handle_send_message(conversation_id, content, attachment_ids).await,
            ClientCommand::DeleteConversation { conversation_id } => {
                self.handle_delete_conversation(conversation_id).await
            }
            ClientCommand::TruncateConversation { conversation_id, message_id } => {
                self.handle_truncate_conversation(conversation_id, message_id).await
            }
            ClientCommand::StopStreaming { conversation_id } => {
                self.handle_stop_streaming(conversation_id).await
            }
            ClientCommand::SummarizeConversation { conversation_id } => {
                self.handle_summarize_conversation(conversation_id).await
            }
            ClientCommand::ResumeAgent { conversation_id } => {
                self.handle_resume_agent(conversation_id).await
            }
            ClientCommand::RenameConversation {
                conversation_id,
                title,
            } => self.handle_rename_conversation(conversation_id, title).await,
            ClientCommand::ListMemories {
                query,
                limit,
                offset,
            } => self.handle_list_memories(query, limit, offset).await,
            ClientCommand::UpdateMemory {
                id,
                content,
                category,
                importance,
            } => self.handle_update_memory(id, content, category, importance).await,
            ClientCommand::DeleteMemory { id } => self.handle_delete_memory(id).await,
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

    /// Manually summarize (compact) a conversation on demand.
    async fn handle_summarize_conversation(&mut self, conversation_id: String) -> Result<()> {
        let uuid = Uuid::parse_str(&conversation_id).context("invalid conversation id format")?;

        // Resolve profile & LLM client for this session
        let resolved = self.session.active_resolved(&self.ctx.config)?;
        let llm_client = self.session.llm_client.clone();
        let storage = self.ctx.storage.clone();

        let summary = ContextService::perform_manual_summarization(
            uuid,
            storage,
            &llm_client,
            &resolved,
        )
        .await?;

        tracing::info!(
            conversation_id = %uuid,
            summary_length = summary.len(),
            "Manual summarization completed"
        );

        // Notify this client that summarization finished
        let _ = self.outbound.send(ServerEvent::Info {
            message: "Conversation summarized.".into(),
        });

        // Reload updated conversation view for this connection
        self.handle_load_conversation(conversation_id).await
    }

    /// Resume the agentic loop from persisted history without a new user message.
    async fn handle_resume_agent(&mut self, conversation_id: String) -> Result<()> {
        let uuid = Uuid::parse_str(&conversation_id).context("invalid conversation id format")?;

        {
            let storage = self.ctx.storage.lock().await;
            if storage
                .get_conversation(&uuid)
                .context("failed to get conversation")?
                .is_none()
            {
                return Err(anyhow!("Conversation not found"));
            }
        }

        self.session.active_conversation_id = Some(uuid);
        self.ctx
            .subscriptions
            .set_viewing(
                self.connection_id,
                Some(uuid),
                self.outbound.clone(),
            )
            .await;

        if self.repair_incomplete_tool_tail(uuid).await? {
            self.ctx
                .subscriptions
                .broadcast(
                    uuid,
                    ServerEvent::Info {
                        message: "Removed incomplete tool turn; resuming agent.".into(),
                    },
                )
                .await;
            self.broadcast_conversation_loaded(uuid).await?;
        }

        {
            let storage = self.ctx.storage.lock().await;
            let db_messages = storage
                .load_conversation_messages(&uuid.to_string())
                .context("failed to load conversation messages")?;
            let context = MessageConverter::llm_context_messages(&db_messages);
            if context.is_empty() {
                return Err(anyhow!("Nothing to resume: conversation has no messages"));
            }
        }

        self.run_agent_for_conversation(uuid, RunAgentOptions { auto_summarize: true })
            .await
    }

    async fn repair_incomplete_tool_tail(&self, conv_uuid: Uuid) -> Result<bool> {
        let ids = {
            let storage = self.ctx.storage.lock().await;
            let db_messages = storage
                .load_conversation_messages(&conv_uuid.to_string())
                .context("failed to load conversation messages")?;
            conversation_tail::ids_to_repair_incomplete_tool_tail(&db_messages)
        };
        if ids.is_empty() {
            return Ok(false);
        }
        let storage = self.ctx.storage.lock().await;
        storage
            .delete_messages(&ids)
            .context("failed to delete incomplete tool tail")?;
        tracing::info!(
            conversation_id = %conv_uuid,
            deleted = ids.len(),
            "Repaired incomplete tool tail before resume"
        );
        Ok(true)
    }

    async fn broadcast_conversation_loaded(&self, uuid: Uuid) -> Result<()> {
        let storage = self.ctx.storage.lock().await;
        let conv = storage
            .get_conversation(&uuid)
            .context("failed to load conversation")?
            .ok_or_else(|| anyhow!("Conversation not found"))?;
        let view = to_conversation_view(&conv);
        drop(storage);
        self.ctx
            .subscriptions
            .broadcast(
                uuid,
                ServerEvent::ConversationLoaded {
                    conversation: view,
                },
            )
            .await;
        Ok(())
    }

    async fn try_spawn_tracked_agent(
        &mut self,
        conversation_uuid: Uuid,
        agent_messages: Vec<LlmMessage>,
    ) -> Result<()> {
        {
            let map = self.ctx.active_agent_runs.read().await;
            if map.contains_key(&conversation_uuid) {
                return Err(anyhow!("Agent already running for this conversation"));
            }
        }

        let handle = spawn_agent_task(
            self.ctx.clone(),
            conversation_uuid,
            agent_messages,
            self.session.profile_name.clone(),
            self.session.llm_client.clone(),
            self.session.allowed_tool_names.clone(),
        );
        let abort = handle.abort_handle();
        {
            let mut map = self.ctx.active_agent_runs.write().await;
            map.insert(conversation_uuid, abort);
        }
        self.session.track_task(handle);
        Ok(())
    }

    async fn run_agent_for_conversation(
        &mut self,
        conversation_uuid: Uuid,
        options: RunAgentOptions,
    ) -> Result<()> {
        let resolved = self.session.active_resolved(&self.ctx.config)?.clone();
        let mut llm_messages = self.build_llm_messages(conversation_uuid).await?;
        llm_messages =
            ContextService::inject_prompts(llm_messages, &self.ctx.prompt_manager, resolved.profile())?;

        if options.auto_summarize {
            let preset = resolved.preset();
            let token_counter = TokenCounter::new(preset);
            let total_tokens: usize = llm_messages
                .iter()
                .map(|m| token_counter.count_message_tokens(m))
                .sum();
            let summarize_threshold =
                token_counter.get_summarize_threshold_tokens(preset, resolved.profile());
            let will_summarize = total_tokens > summarize_threshold;

            if will_summarize {
                self.ctx
                    .subscriptions
                    .broadcast(
                        conversation_uuid,
                        ServerEvent::Info {
                            message: "Summarizing conversation…".into(),
                        },
                    )
                    .await;
            }

            {
                let storage = self.ctx.storage.clone();
                let llm_client = self.session.llm_client.clone();

                if let Err(e) = ContextService::check_and_trigger_summarization(
                    &llm_messages,
                    conversation_uuid,
                    storage,
                    &llm_client,
                    &resolved,
                )
                .await
                {
                    tracing::warn!(error = %e, "Failed to check/trigger summarization, continuing anyway");
                } else if will_summarize {
                    self.ctx
                        .subscriptions
                        .broadcast(
                            conversation_uuid,
                            ServerEvent::Info {
                                message: "Conversation summarized.".into(),
                            },
                        )
                        .await;
                }
            }

            llm_messages = self.build_llm_messages(conversation_uuid).await?;
            llm_messages = ContextService::inject_prompts(
                llm_messages,
                &self.ctx.prompt_manager,
                resolved.profile(),
            )?;
        }

        {
            let token_counter = TokenCounter::new(resolved.preset());
            let outcome = crate::services::memory_rag::inject_memory_block(
                self.ctx.storage.clone(),
                self.ctx.embedding_provider.as_deref(),
                &self.ctx.config.embedding,
                &token_counter,
                &mut llm_messages,
            )
            .await;
            if let Some(outcome) = outcome {
                let storage_guard = self.ctx.storage.lock().await;
                if let Err(e) = storage_guard
                    .record_memory_recalls(&conversation_uuid.to_string(), &outcome.ids)
                {
                    tracing::warn!(error = %e, "Failed to record memory recalls (analytics)");
                }
                drop(storage_guard);
                let _ = self.outbound.send(ServerEvent::MemoriesRecalled {
                    conversation_id: conversation_uuid.to_string(),
                    memory_ids: outcome.ids,
                });
            }
        }

        let preset = resolved.preset();
        let token_counter = TokenCounter::new(preset);
        let total_tokens: usize = llm_messages
            .iter()
            .map(|msg| token_counter.count_message_tokens(msg))
            .sum();

        let context_limit = token_counter.get_context_limit(preset);

        let usage_percent = (total_tokens as f32 / context_limit as f32 * 100.0) as u32;
        tracing::info!(
            "Context usage after summarization: {} tokens / {} limit ({}%)",
            total_tokens,
            context_limit,
            usage_percent
        );

        let mut agent_messages = if total_tokens > token_counter.get_safe_context_limit(preset) {
            tracing::info!(
                total_tokens,
                safe_limit = token_counter.get_safe_context_limit(preset),
                "Context still exceeds safe limit after summarization, applying smart truncation"
            );

            SmartContextManager::select_context(llm_messages, &token_counter, preset)
        } else {
            llm_messages
        };

        let safe_limit = token_counter.get_safe_context_limit(preset);
        let hard_limit = token_counter.get_context_limit(preset);

        if total_tokens > hard_limit {
            tracing::warn!(
                total_tokens,
                hard_limit,
                "CRITICAL: Context exceeds hard limit, forcing truncation"
            );
            let original_count = agent_messages.len();
            agent_messages =
                SmartContextManager::select_context(agent_messages, &token_counter, preset);
            let selected_count = agent_messages.len();
            let selected_tokens: usize = agent_messages
                .iter()
                .map(|msg| token_counter.count_message_tokens(msg))
                .sum();

            tracing::warn!(
                original_count,
                selected_count,
                total_tokens,
                selected_tokens,
                "Emergency truncation: messages and tokens reduced"
            );

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
            agent_messages =
                SmartContextManager::select_context(agent_messages, &token_counter, preset);
            let selected_count = agent_messages.len();
            let selected_tokens: usize = agent_messages
                .iter()
                .map(|msg| token_counter.count_message_tokens(msg))
                .sum();

            tracing::info!(
                "Context selection: {} messages -> {} messages ({} tokens -> {} tokens)",
                original_count,
                selected_count,
                total_tokens,
                selected_tokens
            );

            let _ = self.outbound.send(ServerEvent::Info {
                message: format!(
                    "Context truncated: {} messages selected from {} ({} tokens used)",
                    selected_count,
                    original_count,
                    selected_tokens
                ),
            });
        }

        let final_tokens: usize = agent_messages
            .iter()
            .map(|msg| token_counter.count_message_tokens(msg))
            .sum();

        if final_tokens > hard_limit {
            tracing::error!(
                total_tokens = final_tokens,
                context_limit = hard_limit,
                "FATAL: After truncation, still over limit (this should not happen)"
            );
            let system_count = agent_messages
                .iter()
                .take_while(|m| matches!(m.role, Role::System))
                .count();
            let mut emergency_messages: Vec<LlmMessage> =
                agent_messages[..system_count].to_vec();
            let mut emergency_tokens: usize = emergency_messages
                .iter()
                .map(|msg| token_counter.count_message_tokens(msg))
                .sum();

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

        self.try_spawn_tracked_agent(conversation_uuid, agent_messages)
            .await
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
                    self.session.update_profile(conv_profile, &self.ctx.config, &self.ctx.mcp_registry).await?;
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
            let conv_ids: Vec<String> = results
                .iter()
                .map(|s| s.conversation_id.clone())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            let titles: HashMap<String, String> = storage
                .get_conversation_titles(&conv_ids)
                .context("failed to load conversation titles")?
                .into_iter()
                .collect();
            let mapped: Vec<SearchResult> = results
                .into_iter()
                .map(|snippet| SearchResult {
                    conversation_id: snippet.conversation_id.clone(),
                    conversation_title: titles
                        .get(&snippet.conversation_id)
                        .cloned()
                        .unwrap_or_else(|| "Untitled".to_string()),
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
        self.session.update_profile(&profile, &self.ctx.config, &self.ctx.mcp_registry).await?;
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
        attachment_ids: Option<Vec<String>>,
    ) -> Result<()> {
        let has_attachments = attachment_ids
            .as_ref()
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        if content.trim().is_empty() && !has_attachments {
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

        let uploads_base = self.ctx.config.uploads_dir();
        let rag_cfg = self.ctx.config.attachment_rag.clone();
        let emb = self.ctx.embedding_provider.as_deref();

        let mut resolved_attachments: Vec<crate::llm::Attachment> = Vec::new();
        let mut resolved_uids: Vec<String> = Vec::new();
        if let Some(ids) = attachment_ids.as_ref() {
            for uid in ids {
                let uid = uid.trim();
                if uid.is_empty() {
                    return Err(anyhow!("Invalid empty attachment id"));
                }
                let path = resolve_attachment_upload_path(&uploads_base, &conversation_uuid, uid)
                    .with_context(|| format!("resolve attachment {}", uid))?;
                let path_str = path
                    .to_str()
                    .ok_or_else(|| anyhow!("attachment path is not valid UTF-8"))?
                    .to_string();
                let stem_uid = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(uid)
                    .to_string();
                let mut att =
                    crate::llm::file_utils::create_attachment(&path_str).context("create_attachment")?;
                crate::llm::file_utils::validate_file_for_llm(&att)?;
                if att.mime_type.starts_with("text/") || att.mime_type == "text/markdown" {
                    att.content = crate::services::attachment_rag::trim_extracted_text_for_inline(
                        att.content.take(),
                        rag_cfg.inline_max_chars,
                    );
                }
                resolved_uids.push(stem_uid);
                resolved_attachments.push(att);
            }
        }

        let storage_arc = self.ctx.storage.clone();
        for (att, uid) in resolved_attachments.iter().zip(resolved_uids.iter()) {
            if let Err(e) = crate::services::attachment_rag::index_large_attachment_if_needed(
                storage_arc.clone(),
                emb,
                &rag_cfg,
                &conversation_uuid.to_string(),
                uid,
                &att.file_name,
                &att.file_path,
            )
            .await
            {
                tracing::warn!(error = %e, attachment_uid = %uid, "Attachment RAG indexing failed");
            }
        }

        let to_store = crate::llm::file_utils::strip_attachments_for_storage(resolved_attachments);
        let metadata = MessageMetadata {
            attachments: if to_store.is_empty() {
                None
            } else {
                Some(to_store.as_slice())
            },
            ..MessageMetadata::default()
        };

        let message_id = storage
            .add_message_with_metadata(
                &conversation_uuid,
                "user".to_string(),
                content.clone(),
                None,
                metadata,
            )
            .context("failed to persist user message")?;
        drop(storage);

        let _ = self.outbound.send(ServerEvent::MessageAccepted {
            conversation_id: conversation_uuid.to_string(),
            message_id: message_id.to_string(),
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

        self.run_agent_for_conversation(
            conversation_uuid,
            RunAgentOptions {
                auto_summarize: true,
            },
        )
        .await
    }

    async fn handle_rename_conversation(
        &self,
        conversation_id: String,
        title: String,
    ) -> Result<()> {
        const MAX_TITLE_LEN: usize = 200;
        let title = title.trim().to_string();
        if title.is_empty() {
            return Err(anyhow!("Title cannot be empty"));
        }
        if title.len() > MAX_TITLE_LEN {
            return Err(anyhow!(
                "Title too long (max {} characters)",
                MAX_TITLE_LEN
            ));
        }
        let uuid = Uuid::parse_str(&conversation_id).context("invalid conversation id format")?;
        let storage = self.ctx.storage.lock().await;
        let updated = storage
            .update_conversation_title_and_flag(&uuid, &title)
            .context("failed to rename conversation")?;
        if updated {
            let _ = self.outbound.send(ServerEvent::ConversationRenamed {
                conversation_id,
                title,
            });
        } else {
            return Err(anyhow!("Conversation not found"));
        }
        Ok(())
    }

    async fn handle_list_memories(
        &self,
        query: Option<String>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<()> {
        const DEFAULT_LIMIT: usize = 100;
        let limit = limit.unwrap_or(DEFAULT_LIMIT as u32) as usize;
        let offset = offset.unwrap_or(0) as usize;
        let storage = self.ctx.storage.lock().await;
        let entries = if let Some(q) = query.filter(|s| !s.trim().is_empty()) {
            let keywords: Vec<String> = q
                .split_whitespace()
                .map(|s| s.to_string())
                .collect();
            storage
                .search_memory_paginated(&keywords, limit, offset)
                .context("memory search failed")?
        } else {
            storage
                .list_memory_paginated(limit, offset)
                .context("failed to list memories")?
        };
        let memories: Vec<MemoryView> = entries.iter().map(memory_entry_to_view).collect();
        let _ = self
            .outbound
            .send(ServerEvent::MemoriesList { memories });
        Ok(())
    }

    async fn handle_update_memory(
        &self,
        id: i64,
        content: Option<String>,
        category: Option<String>,
        importance: Option<i32>,
    ) -> Result<()> {
        let content_changed;
        let final_content;
        {
            let storage = self.ctx.storage.lock().await;
            let current = storage
                .get_memory_by_id(id)
                .context("failed to load memory")?
                .ok_or_else(|| anyhow!("Memory not found"))?;
            let new_content = content
                .as_ref()
                .map(|c| c.trim().to_string())
                .filter(|c| !c.is_empty())
                .unwrap_or_else(|| current.content.clone());
            if new_content.is_empty() {
                return Err(anyhow!("Memory content cannot be empty"));
            }
            let new_category = match category {
                Some(c) if c.trim().is_empty() => None,
                Some(c) => Some(c.trim().to_string()),
                None => current.category.clone(),
            };
            let new_importance = importance.unwrap_or(current.importance);
            content_changed = new_content != current.content;
            final_content = new_content.clone();
            let updated = storage
                .update_memory(id, &new_content, new_category.as_deref(), new_importance)
                .context("failed to update memory")?;
            if !updated {
                return Err(anyhow!("Memory not found"));
            }
        }
        if content_changed {
            if let Some(provider) = &self.ctx.embedding_provider {
                match provider.embed(&final_content).await {
                    Ok(embedding) => {
                        let storage = self.ctx.storage.lock().await;
                        if let Err(e) = storage.update_memory_vec_row(id, &embedding) {
                            tracing::warn!(error = %e, memory_id = id, "Failed to update memory vector");
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, memory_id = id, "Memory re-embedding failed");
                    }
                }
            }
        }
        let storage = self.ctx.storage.lock().await;
        let entry = storage
            .get_memory_by_id(id)
            .context("failed to reload memory")?
            .ok_or_else(|| anyhow!("Memory not found after update"))?;
        let _ = self.outbound.send(ServerEvent::MemoryUpdated {
            memory: memory_entry_to_view(&entry),
        });
        Ok(())
    }

    async fn handle_delete_memory(&self, id: i64) -> Result<()> {
        let storage = self.ctx.storage.lock().await;
        let deleted = storage.delete_memory(id).context("failed to delete memory")?;
        if deleted {
            let _ = self.outbound.send(ServerEvent::MemoryDeleted { id });
        } else {
            return Err(anyhow!("Memory not found"));
        }
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

        if let Some(ref conv_id_str) = conversation_id {
            if let Ok(uuid) = Uuid::parse_str(conv_id_str) {
                let mut map = self.ctx.active_agent_runs.write().await;
                if let Some(abort) = map.remove(&uuid) {
                    abort.abort();
                }
            }
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

        let context: Vec<StorageMessage> = MessageConverter::llm_context_messages(&db_messages)
            .into_iter()
            .cloned()
            .collect();
        Ok(MessageConverter::db_to_llm(&context, true))
    }
}

/// Locate an uploaded file by attachment UUID prefix under `uploads/{conversation_id}/`,
/// or `uploads/_no_conversation/` when the client uploaded before a conversation id existed.
fn resolve_attachment_upload_path(
    uploads_base: &Path,
    conversation_id: &Uuid,
    attachment_uid: &str,
) -> Result<std::path::PathBuf> {
    let dirs = [
        uploads_base.join(conversation_id.to_string()),
        uploads_base.join("_no_conversation"),
    ];
    for dir in dirs {
        if !dir.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(&dir).context("read upload directory")? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(attachment_uid) {
                let p = entry.path();
                let meta = std::fs::symlink_metadata(&p)?;
                if meta.file_type().is_symlink() {
                    return Err(anyhow!("attachment path must not be a symlink"));
                }
                return Ok(p);
            }
        }
    }
    Err(anyhow!(
        "no file found for attachment id {}",
        attachment_uid
    ))
}

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

fn memory_entry_to_view(entry: &MemoryEntry) -> MemoryView {
    MemoryView {
        id: entry.id,
        content: entry.content.clone(),
        category: entry.category.clone(),
        importance: entry.importance,
        created_at: entry.created_at,
        updated_at: entry.updated_at.unwrap_or(entry.created_at),
    }
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
    // Drop messages that have been rolled up into a summary. The summary message
    // (`is_summary == true`) is kept as the anchor; clients render it as a
    // collapsible "Summary (N messages)" bubble. This mirrors what the LLM sees
    // via `MessageConverter::llm_context_messages` and avoids shipping (and
    // re-rendering) the redundant pre-summary tail on every load.
    ConversationView {
        id: conv.id.to_string(),
        title: conv.title.clone(),
        created_at: conv.created_at.timestamp(),
        updated_at: conv.updated_at.timestamp(),
        messages: conv
            .messages
            .iter()
            .filter(|m| !(m.is_summarized && !m.is_summary))
            .map(MessageView::from)
            .collect(),
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
