use crate::{
    config::AppConfig,
    embeddings::EmbeddingProvider,
    llm,
    prompts::PromptManager,
    server::conversation_subscriptions::ConnectionId,
    server::dto::{ClientCommand, ServerEvent},
    services::ScheduleService,
    storage::{Storage},
};
use agentic_loop::mcp_servers_registry::MCPServerRegistry;
use anyhow::{anyhow, Context, Result};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::sync::{mpsc::UnboundedSender, Mutex, RwLock};
use uuid::Uuid;

mod agent;
mod conversation;
mod helpers;
mod memory;
mod profile;
mod spawn;

pub(crate) use helpers::record_and_build_memories_recalled_event;
pub use spawn::run_scheduled_task;

pub struct ServerContext {
    pub config: Arc<AppConfig>,
    pub server_cfg: Arc<crate::config::ServerConfig>,
    /// Ephemeral token for static file URLs (not the permanent API key).
    pub static_token: String,
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

pub(crate) struct RunAgentOptions {
    pub auto_summarize: bool,
    pub triggering_message_rowid: Option<i64>,
}

pub struct SessionState {
    pub profile_name: String,
    pub llm_client: Arc<dyn crate::llm::LlmClient>,
    pub active_conversation_id: Option<Uuid>,
    /// Allowed tool names for this profile (from tools policy); internal tools are only added if in this set.
    pub allowed_tool_names: HashSet<String>,
}

impl SessionState {
    pub fn new(config: &AppConfig, default_allowed_tool_names: &HashSet<String>) -> Result<Self> {
        let default_name = &config.default;
        let resolved = config.resolve_default_profile().ok_or_else(|| {
            let hint = if !config.profiles.contains_key(default_name) {
                format!(
                    "Profile '{default_name}' is not defined in [profiles]. Add [profiles.{default_name}] with model_preset and tools_policy (see docs/sample_config.toml)."
                )
            } else {
                let preset_name = config
                    .profiles
                    .get(default_name)
                    .map(|p| p.model_preset.clone())
                    .unwrap_or_default();
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
            ClientCommand::StartConversation { title, internal } => {
                self.handle_start_conversation(title, internal).await
            }
            ClientCommand::LoadConversation { conversation_id } => {
                self.handle_load_conversation(conversation_id).await
            }
            ClientCommand::ListConversations {
                query,
                limit,
                offset,
                include_internal,
            } => {
                self.handle_list_conversations(query, limit, offset, include_internal)
                    .await
            }
            ClientCommand::ChangeProfile { profile } => self.handle_change_profile(profile).await,
            ClientCommand::ListProfiles => self.handle_list_profiles().await,
            ClientCommand::SendMessage {
                conversation_id,
                content,
                attachment_ids,
                internal,
            } => {
                self.handle_send_message(conversation_id, content, attachment_ids, internal)
                    .await
            }
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
            ClientCommand::SetConversationInternal {
                conversation_id,
                internal,
            } => self.handle_set_conversation_internal(conversation_id, internal).await,
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
            let _ = self.send_event(ServerEvent::Error {
                message: err.to_string(),
            });
        }
    }

    fn send_event(&self, event: ServerEvent) -> Result<()> {
        self.outbound
            .send(event)
            .map_err(|_| anyhow!("WebSocket outbound channel closed"))
    }

    async fn handle_health(&self) -> Result<()> {
        let timestamp = chrono::Utc::now().timestamp();
        self.send_event(ServerEvent::HealthOk {
            timestamp,
            profile: self.session.profile_name.clone(),
            static_token: self.ctx.static_token.clone(),
        })?;
        Ok(())
    }
}
