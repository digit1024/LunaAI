use super::{
    helpers::{resolve_attachment_upload_path, to_conversation_view, truncate_preview},
    spawn::spawn_agent_task,
    RunAgentOptions, ServerHandler,
};
use crate::{
    llm::Message as LlmMessage,
    services::{conversation_tail, ContextService, MessageConverter},
    server::dto::ServerEvent,
    storage::sqlite_storage_simple::{Message as StorageMessage, MessageMetadata},
};
use anyhow::{anyhow, Context, Result};
use uuid::Uuid;

impl ServerHandler {
    pub(super) async fn handle_summarize_conversation(&mut self, conversation_id: String) -> Result<()> {
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
            &self.ctx.config.conversation_compact,
        )
        .await?;

        tracing::info!(
            conversation_id = %uuid,
            summary_length = summary.len(),
            "Manual summarization completed"
        );

        // Notify this client that summarization finished
        self.send_event(ServerEvent::Info {
            message: "Conversation summarized.".into(),
        })?;

        // Reload updated conversation view for this connection
        self.handle_load_conversation(conversation_id).await
    }

    /// Resume the agentic loop from persisted history without a new user message.
    pub(super) async fn handle_resume_agent(&mut self, conversation_id: String) -> Result<()> {
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
    pub(super) async fn repair_incomplete_tool_tail(&self, conv_uuid: Uuid) -> Result<bool> {
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
    pub(super) async fn broadcast_conversation_loaded(&self, uuid: Uuid) -> Result<()> {
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
    pub(super) async fn try_spawn_tracked_agent(
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
        Ok(())
    }
    pub(super) async fn run_agent_for_conversation(
        &mut self,
        conversation_uuid: Uuid,
        options: RunAgentOptions,
    ) -> Result<()> {
        use crate::server::context_pipeline::{ContextPipeline, RunAgentOptions as PipelineOptions};

        let resolved = self.session.active_resolved(&self.ctx.config)?.clone();
        let llm_messages = self.build_llm_messages(conversation_uuid).await?;
        let pipeline = ContextPipeline::new(
            &self.ctx,
            &self.session,
            &self.outbound,
            conversation_uuid,
        );
        let agent_messages = pipeline
            .prepare_for_agent(
                llm_messages,
                &resolved,
                PipelineOptions {
                    auto_summarize: options.auto_summarize,
                },
                || self.build_llm_messages(conversation_uuid),
            )
            .await?;

        self.try_spawn_tracked_agent(conversation_uuid, agent_messages)
            .await
    }
    pub(super) async fn handle_send_message(
        &mut self,
        conversation_id: Option<String>,
        content: String,
        attachment_ids: Option<Vec<String>>,
        internal: Option<bool>,
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
            let create_internal = internal.unwrap_or(false);
            let conv_id = storage
                .create_conversation_with_profile(
                    "Generating title...".to_string(),
                    profile_name.as_deref(),
                    create_internal,
                )
                .context("failed to create conversation")?;
            let preview = truncate_preview(&content);
            let _ = storage.update_conversation_title(&conv_id, preview);
            self.send_event(ServerEvent::ConversationCreated {
                conversation_id: conv_id.to_string(),
            })?;
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
                    .await
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

        self.send_event(ServerEvent::MessageAccepted {
            conversation_id: conversation_uuid.to_string(),
            message_id: message_id.to_string(),
        })?;

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
    pub(super) async fn handle_stop_streaming(&mut self, conversation_id: Option<String>) -> Result<()> {
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
        self.send_event(event.clone())?;
        if let Ok(uuid) = Uuid::parse_str(&conv_id_str) {
            self.ctx.subscriptions.broadcast(uuid, event).await;
        }

        Ok(())
    }
    pub(super) async fn build_llm_messages(&self, conversation_id: Uuid) -> Result<Vec<LlmMessage>> {
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

