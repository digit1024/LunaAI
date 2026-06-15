use super::{
    helpers::{to_conversation_view, truncate_preview}, ServerHandler,
};
use crate::server::dto::{
        ConversationSummary, SearchResult, ServerEvent,
    };
use anyhow::{anyhow, Context, Result};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

impl ServerHandler {
    pub(super) async fn handle_start_conversation(&mut self, title: Option<String>) -> Result<()> {
        let title_text = title.unwrap_or_else(|| "Generating title...".to_string());
        let profile_name = Some(self.session.profile_name.clone());
        let storage = self.ctx.storage.lock().await;
        let conversation_id = storage
            .create_conversation_with_profile(title_text, profile_name.as_deref())
            .context("failed to create conversation")?;
        // Clear active conversation - new conversation will be set when loaded
        self.session.active_conversation_id = None;
        self.send_event(ServerEvent::ConversationCreated {
            conversation_id: conversation_id.to_string(),
        })?;
        Ok(())
    }
    pub(super) async fn handle_load_conversation(&mut self, conversation_id: String) -> Result<()> {
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
                    self.send_event(ServerEvent::ProfileChanged {
                        profile: conv_profile.clone(),
                    })?;
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
    pub(super) async fn handle_list_conversations(
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
            self.send_event(ServerEvent::ConversationsList {
                conversations: summaries,
            })?;
        }
        Ok(())
    }
    pub(super) async fn handle_rename_conversation(
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
            self.send_event(ServerEvent::ConversationRenamed {
                conversation_id,
                title,
            })?;
        } else {
            return Err(anyhow!("Conversation not found"));
        }
        Ok(())
    }
    pub(super) async fn handle_delete_conversation(&mut self, conversation_id: String) -> Result<()> {
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
            self.send_event(ServerEvent::ConversationDeleted {
                conversation_id,
            })?;
        } else {
            return Err(anyhow!("Conversation not found"));
        }
        Ok(())
    }
    pub(super) async fn handle_truncate_conversation(
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
            self.send_event(ServerEvent::ConversationLoaded {
                conversation: view,
            })?;
        } else {
            return Err(anyhow!("Conversation not found after truncation"));
        }

        Ok(())
    }
}
