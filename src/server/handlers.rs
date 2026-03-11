use crate::{
    config::{AppConfig, ServerConfig},
    server::engine::{ConversationEngine, RigEngine, TurnParams},
    embeddings::EmbeddingProvider,
    llm::{self, Message as LlmMessage, Role},
    llm::tokenizer::TokenCounter,
    llm::context_manager::SmartContextManager,
    prompts::PromptManager,
    services::{ContextService, MessageConverter, ScheduleService},
    server::conversation_subscriptions::ConnectionId,
    server::dto::{
        ClientCommand, ConversationSummary, ConversationView, MessageView,
        SearchResult, ServerEvent,
    },
    storage::{
        conversation_storage::Conversation as StoredConversation,
        sqlite_storage_simple::{Message as StorageMessage}, ScheduledJob, Storage,
    },
};
use crate::mcp::McpRegistry;
use anyhow::{anyhow, Context, Result};
use std::{collections::{HashMap, HashSet}, sync::Arc};
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
    pub mcp_registry: Arc<RwLock<McpRegistry>>,
    pub subscriptions: Arc<crate::server::conversation_subscriptions::ConversationSubscriptions>,
    pub schedule_service: Arc<ScheduleService>,
    /// Tracks which memory IDs have been injected per conversation (dedup for Memory RAG)
    pub memory_dedup: Mutex<HashMap<Uuid, HashSet<i64>>>,
    /// Allowed tool names for the default profile (from tools policy); used when creating new sessions.
    pub default_allowed_tool_names: HashSet<String>,
    /// Embedding provider for memory vector search. None when embedding is disabled.
    pub embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
    /// Rig-based conversation engine.
    pub engine: Arc<RigEngine>,
}

impl ServerContext {
    /// Return the engine (Rig).
    pub fn engine_for_profile(&self, _resolved: &crate::config::ResolvedProfile) -> Arc<dyn ConversationEngine> {
        self.engine.clone()
    }
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
        mcp_registry: &Arc<RwLock<McpRegistry>>,
    ) -> Result<()> {
        let resolved = config
            .resolve_profile(profile_name)
            .context("Profile or its model preset not found")?;
        self.profile_name = profile_name.to_string();
        self.llm_client = llm::build_llm_client(resolved.preset());
        let applied = crate::tools_policy::apply_tools_policy(
            mcp_registry,
            config,
            resolved.profile(),
        )
        .await
        .context("Apply tools policy for profile")?;
        self.allowed_tool_names = applied.allowed_tool_names;
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
            ClientCommand::SummarizeConversation { conversation_id } => {
                self.handle_summarize_conversation(conversation_id).await
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

        let message_id = storage
            .add_message_to_conversation(&conversation_uuid, "user".to_string(), content.clone())
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

        let resolved = self.session.active_resolved(&self.ctx.config)?.clone();
        let mut llm_messages = self.build_llm_messages(conversation_uuid).await?;
        let prompt_manager = self.ctx.prompt_manager.clone();
        llm_messages.push(LlmMessage::new(Role::User, content.clone()));

        let agent_messages = ContextService::inject_prompts(llm_messages, &prompt_manager, resolved.profile())?;

        // Check if summarization is needed and notify UI (started / finished)
        let preset = resolved.preset();
        let token_counter = TokenCounter::new(preset);
        let total_tokens: usize = agent_messages
            .iter()
            .map(|m| token_counter.count_message_tokens(m))
            .sum();
        let summarize_threshold = token_counter.get_summarize_threshold_tokens(preset, resolved.profile());
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
                &agent_messages,
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
        
        // Reload messages after summarization (if it happened) and rebuild
        let mut llm_messages = self.build_llm_messages(conversation_uuid).await?;
        
        // Re-inject prompts after reload
        llm_messages = ContextService::inject_prompts(llm_messages, &self.ctx.prompt_manager, resolved.profile())?;

        // Memory RAG: search and inject relevant memories (vector-only when embedding enabled)
        {
            let mut dedup_guard = self.ctx.memory_dedup.lock().await;
            let used_ids = dedup_guard.entry(conversation_uuid).or_default();
            // Seed from DB on first use (e.g. after restart) so we don't re-inject same memories
            if used_ids.is_empty() {
                let storage_guard = self.ctx.storage.lock().await;
                if let Ok(recalled) = storage_guard.get_recalled_memory_ids(&conversation_uuid.to_string()) {
                    used_ids.extend(recalled);
                }
            }
            let result = crate::services::memory_rag::retrieve_memory_context(
                self.ctx.storage.clone(),
                &content,
                used_ids,
                self.ctx.embedding_provider.as_deref(),
                &self.ctx.config.embedding,
            )
            .await;
            if let Some((memory_msg, new_ids)) = result {
                // Insert after system prompts but before conversation history
                let insert_pos = llm_messages
                    .iter()
                    .position(|m| !matches!(m.role, Role::System))
                    .unwrap_or(llm_messages.len());
                llm_messages.insert(insert_pos, LlmMessage::new(Role::System, memory_msg));
                // Persist recalled memories for this conversation
                let storage_guard = self.ctx.storage.lock().await;
                if let Err(e) = storage_guard.record_memory_recalls(&conversation_uuid.to_string(), &new_ids) {
                    tracing::warn!(error = %e, "Failed to record memory recalls");
                }
            }
        }

        // Recalculate tokens after summarization (if it happened)
        let preset = resolved.preset();
        let token_counter = TokenCounter::new(preset);
        let total_tokens: usize = llm_messages.iter()
            .map(|msg| token_counter.count_message_tokens(msg))
            .sum();
        
        let context_limit = token_counter.get_context_limit(preset);
        
        // Log final token usage
        let usage_percent = (total_tokens as f32 / context_limit as f32 * 100.0) as u32;
        tracing::info!(
            "Context usage after summarization: {} tokens / {} limit ({}%)",
            total_tokens,
            context_limit,
            usage_percent
        );
        
        // Apply smart context selection if still over safe limit
        let mut agent_messages = if total_tokens > token_counter.get_safe_context_limit(preset) {
            tracing::info!(
                total_tokens,
                safe_limit = token_counter.get_safe_context_limit(preset),
                "Context still exceeds safe limit after summarization, applying smart truncation"
            );
            
            crate::llm::context_manager::SmartContextManager::select_context(
                llm_messages,
                &token_counter,
                preset,
            )
        } else {
            llm_messages
        };
        
        // Continue with agent loop using final messages
        // (Summarization now handled by ContextService::check_and_trigger_summarization above)
        
        // Apply additional safety checks if needed
        let safe_limit = token_counter.get_safe_context_limit(preset);
        let hard_limit = token_counter.get_context_limit(preset);
        
        // Always ensure we don't exceed the hard limit (API will reject if we do)
        if total_tokens > hard_limit {
            tracing::warn!(
                total_tokens,
                hard_limit,
                "CRITICAL: Context exceeds hard limit, forcing truncation"
            );
            let original_count = agent_messages.len();
            agent_messages = SmartContextManager::select_context(agent_messages, &token_counter, preset);
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
            agent_messages = SmartContextManager::select_context(agent_messages, &token_counter, preset);
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

        let engine = self.ctx.engine_for_profile(&resolved);
        let params = TurnParams {
            conversation_id: conversation_uuid,
            agent_messages,
            profile_name: self.session.profile_name.clone(),
            allowed_tool_names: self.session.allowed_tool_names.clone(),
        };
        let handle = engine.run_turn(self.ctx.clone(), params)?;
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

        // Only keep the latest summary (if any) and all messages after it.
        // Older summaries and their raw messages are represented by that latest summary.
        let start_idx = db_messages
            .iter()
            .rposition(|m: &StorageMessage| m.is_summary)
            .unwrap_or(0);
        let window = &db_messages[start_idx..];

        // Use MessageConverter service (single source of truth)
        Ok(MessageConverter::db_to_llm(window, true))
    }
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
        // For existing conversations, keep only the latest summary (if any) and messages after it.
        let start_idx = db_messages
            .iter()
            .rposition(|m: &StorageMessage| m.is_summary)
            .unwrap_or(0);
        let window = &db_messages[start_idx..];
        let mut llm_messages = MessageConverter::db_to_llm(window, true);
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
        // New conversation: same rule still applies; if a summary exists, only keep the latest
        // plus subsequent messages (typically none yet), otherwise keep all messages.
        let start_idx = db_messages
            .iter()
            .rposition(|m: &StorageMessage| m.is_summary)
            .unwrap_or(0);
        let window = &db_messages[start_idx..];
        let llm_messages = MessageConverter::db_to_llm(window, true);
        let agent_messages =
            ContextService::inject_prompts(llm_messages, &ctx.prompt_manager, resolved.profile())?;
        (conv_id, agent_messages)
    };

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

    let params = TurnParams {
        conversation_id,
        agent_messages,
        profile_name: profile_name.to_string(),
        allowed_tool_names,
    };
    let engine = ctx.engine_for_profile(&resolved);
    let handle = match engine.run_turn(ctx.clone(), params) {
        Ok(h) => h,
        Err(e) => {
            tracing::error!(job_id = %job.id, error = %e, "Engine run_turn failed for scheduled job");
            let now = chrono::Utc::now().timestamp();
            let storage = ctx.storage.lock().await;
            storage
                .set_scheduled_job_completed(&job.id, now, true, Some(&e.to_string()))
                .context("failed to mark job failed")?;
            return Ok(());
        }
    };
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
// Removed: inject_prompts() - Now using ContextService::inject_prompts() service
// Removed: spawn_agent_task, PersistenceContext, process_agent_update - Rig engine handles all
