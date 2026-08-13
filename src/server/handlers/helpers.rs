use crate::{
    server::dto::{ConversationView, MemoryView, MessageView, ServerEvent},
    services::memory_rag::InjectionOutcome,
    storage::{
        conversation_storage::Conversation as StoredConversation,
        sqlite_storage_simple::MemoryEntry,
        Storage,
    },
};
use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

/// Locate an uploaded file by exact `{attachment_uid}.{ext}` under
/// `uploads/{conversation_id}/`, or `uploads/_no_conversation/` when the client
/// uploaded before a conversation id existed.
pub(super) async fn resolve_attachment_upload_path(
    uploads_base: &Path,
    conversation_id: &Uuid,
    attachment_uid: &str,
) -> Result<std::path::PathBuf> {
    let dirs = [
        uploads_base.join(conversation_id.to_string()),
        uploads_base.join("_no_conversation"),
    ];
    for dir in dirs {
        if tokio::fs::metadata(&dir).await.map(|m| !m.is_dir()).unwrap_or(true) {
            continue;
        }
        let mut entries = tokio::fs::read_dir(&dir).await.context("read upload directory")?;
        while let Some(entry) = entries.next_entry().await.context("read upload directory")? {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Some((stem, ext)) = name.split_once('.') else {
                continue;
            };
            if ext.is_empty() || ext.contains('.') || stem != attachment_uid {
                continue;
            }
            let p = entry.path();
            let meta = tokio::fs::symlink_metadata(&p).await?;
            if meta.file_type().is_symlink() {
                return Err(anyhow!("attachment path must not be a symlink"));
            }
            return Ok(p);
        }
    }
    Err(anyhow!(
        "no file found for attachment id {}",
        attachment_uid
    ))
}

pub(super) fn memory_entry_to_view(entry: &MemoryEntry) -> MemoryView {
    MemoryView {
        id: entry.id,
        content: entry.content.clone(),
        category: entry.category.clone(),
        importance: entry.importance,
        created_at: entry.created_at,
        updated_at: entry.updated_at.unwrap_or(entry.created_at),
    }
}

pub(super) fn truncate_preview(text: &str) -> String {
    const MAX_PREVIEW_CHARS: usize = 60;
    const TRUNCATED_CHARS: usize = MAX_PREVIEW_CHARS - 3;

    if text.chars().count() > MAX_PREVIEW_CHARS {
        let truncated: String = text.chars().take(TRUNCATED_CHARS).collect();
        format!("{truncated}...")
    } else {
        text.to_string()
    }
}

pub(super) fn message_rowid_to_uuid_string(rowid: i64) -> String {
    let hex_id = format!("{:012x}", rowid.min(0xffffffffffff));
    format!("00000000-0000-0000-0000-{}", hex_id)
}

pub(super) fn message_uuid_to_rowid(id: &Uuid) -> Option<i64> {
    let s = id.to_string();
    let suffix = s.rsplit('-').next()?;
    if suffix.len() == 12 {
        i64::from_str_radix(suffix, 16).ok()
    } else {
        None
    }
}

pub(super) fn memory_entries_to_views(entries: &[MemoryEntry]) -> Vec<MemoryView> {
    entries.iter().map(memory_entry_to_view).collect()
}

/// Persist recall analytics + per-message linkage and build the websocket event.
pub(crate) fn record_and_build_memories_recalled_event(
    storage: &Storage,
    conversation_id: &str,
    message_id: i64,
    outcome: &InjectionOutcome,
) -> Result<ServerEvent> {
    storage
        .record_memory_recalls(conversation_id, &outcome.ids)
        .context("failed to record conversation memory recalls")?;
    storage
        .record_message_memory_recalls(message_id, &outcome.ids)
        .context("failed to record message memory recalls")?;
    Ok(ServerEvent::MemoriesRecalled {
        conversation_id: conversation_id.to_string(),
        message_id: message_rowid_to_uuid_string(message_id),
        memories: memory_entries_to_views(&outcome.entries),
        memory_ids: outcome.ids.clone(),
    })
}

pub(super) fn recalls_map_for_conversation(
    storage: &Storage,
    conv: &StoredConversation,
) -> Result<HashMap<i64, Vec<MemoryView>>> {
    let user_ids: Vec<i64> = conv
        .messages
        .iter()
        .filter(|m| m.role == "user")
        .filter_map(|m| message_uuid_to_rowid(&m.id))
        .collect();
    let raw = storage
        .get_recalled_memories_for_messages(&user_ids)
        .context("failed to load recalled memories")?;
    Ok(raw
        .into_iter()
        .map(|(id, entries)| (id, memory_entries_to_views(&entries)))
        .collect())
}

pub(super) fn to_conversation_view(
    conv: &StoredConversation,
    recalls_by_message: &HashMap<i64, Vec<MemoryView>>,
) -> ConversationView {
    ConversationView {
        id: conv.id.to_string(),
        title: conv.title.clone(),
        created_at: conv.created_at.timestamp(),
        updated_at: conv.updated_at.timestamp(),
        messages: conv
            .messages
            .iter()
            .filter(|m| !m.is_summarized || m.is_summary)
            .map(|m| {
                let recalled = message_uuid_to_rowid(&m.id)
                    .and_then(|rowid| recalls_by_message.get(&rowid).cloned())
                    .unwrap_or_default();
                MessageView::from_stored(m, recalled)
            })
            .collect(),
        profile_name: conv.profile_name.clone(),
        internal: conv.internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_preview_adds_ellipsis_for_long_text() {
        assert_eq!(truncate_preview("hello world"), "hello world");
        let long = "a".repeat(80);
        let out = truncate_preview(&long);
        assert!(out.ends_with("..."));
        assert!(out.chars().count() <= 60);
    }
}
