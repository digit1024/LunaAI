use super::{
    helpers::memory_entry_to_view, ServerHandler,
};
use crate::server::dto::{
        MemoryView, ServerEvent,
    };
use anyhow::{anyhow, Context, Result};

impl ServerHandler {
    pub(super) async fn handle_list_memories(
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
    pub(super) async fn handle_update_memory(
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
        self.send_event(ServerEvent::MemoryUpdated {
            memory: memory_entry_to_view(&entry),
        })?;
        Ok(())
    }
    pub(super) async fn handle_delete_memory(&self, id: i64) -> Result<()> {
        let storage = self.ctx.storage.lock().await;
        let deleted = storage.delete_memory(id).context("failed to delete memory")?;
        if deleted {
            self.send_event(ServerEvent::MemoryDeleted { id })?;
        } else {
            return Err(anyhow!("Memory not found"));
        }
        Ok(())
    }
}
