use crate::{
    agentic::{
        loop_engine::AgenticLoop,
        protocol::{AgentUpdate, PlannedTool},
    },
    config::{AppConfig, ServerConfig},
    llm::{self, Attachment, Message as LlmMessage, Role},
    mcp::MCPServerRegistry,
    prompts::PromptManager,
    server::{dto::{
        ClientCommand, ConversationSummary, ConversationView, MessageView, PlannedToolView,
        SearchResult, ServerEvent,
    }, http::AttachmentStorage},
    storage::{
        conversation_storage::Conversation as StoredConversation,
        sqlite_storage_simple::MessageMetadata, Storage,
    },
};
use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::{collections::HashMap, sync::Arc, time::Duration};
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
    pub attachment_storage: Arc<AttachmentStorage>,
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
            Err(anyhow!("Profile '{}' not found", profile_name))
        }
    }

    pub fn active_profile<'a>(
        &'a self,
        config: &'a AppConfig,
    ) -> Result<&'a crate::config::LlmProfile> {
        config
            .get_profile(&self.profile_name)
            .or_else(|| config.get_default_profile())
            .ok_or_else(|| anyhow!("No active profile configured"))
    }

    pub fn track_task(&mut self, handle: JoinHandle<()>) {
        self.inflight.push(handle);
        self.inflight.retain(|h| !h.is_finished());
    }
}

pub struct ServerHandler {
    pub ctx: Arc<ServerContext>,
    pub session: SessionState,
    outbound: UnboundedSender<ServerEvent>,
}

impl ServerHandler {
    pub fn new(ctx: Arc<ServerContext>, outbound: UnboundedSender<ServerEvent>) -> Result<Self> {
        Ok(Self {
            session: SessionState::new(&ctx.config)?,
            ctx,
            outbound,
        })
    }

    pub async fn handle_command(&mut self, command: ClientCommand) {
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
            
            let view = to_conversation_view(&conv);
            let _ = self
                .outbound
                .send(ServerEvent::ConversationLoaded { conversation: view });
        } else {
            return Err(anyhow!("Conversation {} not found", conversation_id));
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
                .search_history(&q, limit.unwrap_or(20) as usize)
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
                eprintln!("Failed to update active conversation profile: {}", e);
            }
        }
        
        Ok(())
    }

    async fn handle_list_profiles(&self) -> Result<()> {
        let profiles: Vec<String> = self.ctx.config.profiles.keys().cloned().collect();
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
        if content.trim().is_empty() && attachment_ids.as_ref().map(|v| v.is_empty()).unwrap_or(true) {
            return Err(anyhow!("Cannot send an empty message"));
        }

        // Retrieve attachments if any
        let attachments: Vec<Attachment> = if let Some(ids) = attachment_ids.as_ref() {
            if !ids.is_empty() {
                self.ctx.attachment_storage.get_multiple(ids).await
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

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

        // Clean up attachments after message is sent
        if let Some(ids) = attachment_ids {
            self.ctx.attachment_storage.remove_multiple(&ids).await;
        }

        let _ = self.outbound.send(ServerEvent::MessageAccepted {
            conversation_id: conversation_uuid.to_string(),
        });

        let profile = self.session.active_profile(&self.ctx.config)?.clone();
        let mut llm_messages = self.build_llm_messages(conversation_uuid).await?;
        let prompt_manager = self.ctx.prompt_manager.clone();
        
        // Add current message with attachments to LLM messages (before injecting prompts)
        let current_user_message = if attachments.is_empty() {
            LlmMessage::new(Role::User, content.clone())
        } else {
            LlmMessage::new_with_attachments(Role::User, content.clone(), attachments)
        };
        llm_messages.push(current_user_message);
        
        let agent_messages = inject_prompts(llm_messages, &prompt_manager, &profile)?;

        let outbound = self.outbound.clone();
        let llm_client = self.session.llm_client.clone();
        let mcp_registry = self.ctx.mcp_registry.clone();
        let timeout = Duration::from_secs(self.ctx.server_cfg.stream_timeout_secs);
        let conversation_key = conversation_uuid.to_string();
        let storage = self.ctx.storage.clone();
        let convo_for_persistence = conversation_uuid;

        let handle = tokio::spawn(async move {
            let (agent_tx, mut agent_rx) = tokio::sync::mpsc::unbounded_channel::<AgentUpdate>();
            let mut loop_engine = AgenticLoop::new(mcp_registry, llm_client);
            let outbound_clone = outbound.clone();
            let stream_task = tokio::spawn(async move {
                let mut persistence = PersistenceContext::new(storage, convo_for_persistence);
                while let Some(update) = agent_rx.recv().await {
                    if let Err(err) = process_agent_update(
                        &outbound_clone,
                        &conversation_key,
                        &mut persistence,
                        update,
                    )
                    .await
                    {
                        let _ = outbound_clone.send(ServerEvent::Error {
                            message: err.to_string(),
                        });
                    }
                }
            });

            let result = tokio::time::timeout(
                timeout,
                loop_engine.process_message(agent_messages, Some(agent_tx), None),
            )
            .await;

            match result {
                Ok(Ok(_)) => {}
                Ok(Err(err)) => {
                    let _ = outbound.send(ServerEvent::Error {
                        message: err.to_string(),
                    });
                }
                Err(elapsed) => {
                    let _ = outbound.send(ServerEvent::Error {
                        message: format!("Streaming timeout after {:?}", elapsed),
                    });
                }
            }

            let _ = stream_task.await;
        });

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

    async fn handle_stop_streaming(&mut self, conversation_id: Option<String>) -> Result<()> {
        // Abort all inflight tasks
        let mut handles = Vec::new();
        std::mem::swap(&mut handles, &mut self.session.inflight);
        
        for handle in handles {
            handle.abort();
        }
        
        let conv_id = conversation_id.unwrap_or_else(|| "unknown".to_string());
        let _ = self.outbound.send(ServerEvent::StreamingStopped {
            conversation_id: conv_id,
        });
        
        Ok(())
    }

    async fn build_llm_messages(&self, conversation_id: Uuid) -> Result<Vec<LlmMessage>> {
        let storage = self.ctx.storage.lock().await;
        let conversation = storage
            .get_conversation(&conversation_id)
            .context("failed to load conversation history")?
            .ok_or_else(|| anyhow!("Conversation {} not found", conversation_id))?;
        Ok(conversation_to_llm(conversation))
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
    ConversationView {
        id: conv.id.to_string(),
        title: conv.title.clone(),
        created_at: conv.created_at.timestamp(),
        updated_at: conv.updated_at.timestamp(),
        messages: conv.messages.iter().map(MessageView::from).collect(),
        profile_name: conv.profile_name.clone(),
    }
}

fn conversation_to_llm(conversation: StoredConversation) -> Vec<LlmMessage> {
    conversation
        .messages
        .into_iter()
        .filter_map(|msg| {
            let role = match msg.role.as_str() {
                "user" => Role::User,
                "assistant" => Role::Assistant,
                "system" => Role::System,
                "tool" => Role::Tool,
                _ => return None,
            };
            Some(match role {
                Role::Tool => {
                    let tool_call_id = msg
                        .tool_call_id
                        .unwrap_or_else(|| "tool_result".to_string());
                    LlmMessage::new_tool_result(
                        tool_call_id,
                        msg.content,
                        msg.tool_status.as_deref() == Some("error"),
                    )
                }
                Role::Assistant => {
                    // Include tool_calls if present on assistant messages
                    let mut assistant_msg = if let Some(tool_calls) = msg.tool_calls {
                        if !tool_calls.is_empty() {
                            LlmMessage::new_with_tool_calls(role, msg.content, tool_calls)
                        } else {
                            LlmMessage::new(role, msg.content)
                        }
                    } else {
                        LlmMessage::new(role, msg.content)
                    };
                    // Preserve reasoning_content from stored message
                    assistant_msg.reasoning_content = msg.reasoning_content.clone();
                    assistant_msg
                }
                _ => LlmMessage::new(role, msg.content),
            })
        })
        .collect()
}

fn inject_prompts(
    mut history: Vec<LlmMessage>,
    prompt_manager: &PromptManager,
    profile: &crate::config::LlmProfile,
) -> Result<Vec<LlmMessage>> {
    let mut final_messages = Vec::new();
    if let Some(system) = prompt_manager.get_system_prompt() {
        final_messages.push(LlmMessage::new(Role::System, system.to_string()));
    }

    if let Some(profile_prompt) = profile.profile_prompt_file.as_ref().and_then(|path| {
        let resolved = AppConfig::resolve_config_path(path);
        let owned = resolved.to_string_lossy().to_string();
        prompt_manager.load_profile_prompt(&owned).ok()
    }) {
        final_messages.push(LlmMessage::new(Role::System, profile_prompt));
    }

    final_messages.append(&mut history);
    Ok(final_messages)
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
    outbound: &UnboundedSender<ServerEvent>,
    conversation_id: &str,
    persistence: &mut PersistenceContext,
    update: AgentUpdate,
) -> Result<()> {
    match update {
        AgentUpdate::AssistantStreamingStarted => {
            let _ = outbound.send(ServerEvent::StreamingStarted {
                conversation_id: conversation_id.to_string(),
            });
        }
        AgentUpdate::AssistantDelta { text_chunk, seq } => {
            let _ = outbound.send(ServerEvent::AssistantDelta {
                conversation_id: conversation_id.to_string(),
                chunk: text_chunk,
                seq,
            });
        }
        AgentUpdate::ReasoningContentDelta { chunk } => {
            let _ = outbound.send(ServerEvent::ReasoningContentDelta {
                conversation_id: conversation_id.to_string(),
                chunk,
            });
        }
        AgentUpdate::AssistantComplete { full_text, reasoning_content } => {
            persistence.persist_assistant(&full_text, reasoning_content.as_deref()).await?;
            let _ = outbound.send(ServerEvent::AssistantComplete {
                conversation_id: conversation_id.to_string(),
                content: full_text,
                reasoning_content: reasoning_content.clone(),
            });
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
            let _ = outbound.send(ServerEvent::ToolPlanned {
                conversation_id: conversation_id.to_string(),
                tools: converted,
            });
        }
        AgentUpdate::ToolStarted {
            tool_call_id,
            name,
            params_json,
        } => {
            persistence.mark_tool_started(&tool_call_id, &params_json);
            let params_value =
                serde_json::from_str::<Value>(&params_json).unwrap_or(Value::String(params_json));
            let _ = outbound.send(ServerEvent::ToolStarted {
                conversation_id: conversation_id.to_string(),
                tool_call_id,
                name,
                params_json: params_value,
            });
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
            let _ = outbound.send(ServerEvent::ToolResult {
                conversation_id: conversation_id.to_string(),
                tool_call_id,
                name,
                result_json: result_value,
            });
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
            let _ = outbound.send(ServerEvent::ToolError {
                conversation_id: conversation_id.to_string(),
                tool_call_id,
                name,
                error,
            });
        }
        AgentUpdate::ConversationComplete { .. } => {
            let _ = outbound.send(ServerEvent::ConversationComplete {
                conversation_id: conversation_id.to_string(),
            });
        }
        AgentUpdate::ModelError { error } => {
            let _ = outbound.send(ServerEvent::Error { message: error });
        }
    }
    Ok(())
}
