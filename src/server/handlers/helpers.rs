use crate::{
    server::dto::{ConversationView, MemoryView, MessageView},
    storage::{
        conversation_storage::Conversation as StoredConversation,
        sqlite_storage_simple::MemoryEntry,
    },
};
use anyhow::{anyhow, Context, Result};
use std::path::Path;
use uuid::Uuid;

/// Locate an uploaded file by attachment UUID prefix under `uploads/{conversation_id}/`,
/// or `uploads/_no_conversation/` when the client uploaded before a conversation id existed.
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
            if name.starts_with(attachment_uid) {
                let p = entry.path();
                let meta = tokio::fs::symlink_metadata(&p).await?;
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

pub(super) fn to_conversation_view(conv: &StoredConversation) -> ConversationView {
    ConversationView {
        id: conv.id.to_string(),
        title: conv.title.clone(),
        created_at: conv.created_at.timestamp(),
        updated_at: conv.updated_at.timestamp(),
        messages: conv
            .messages
            .iter()
            .filter(|m| !m.is_summarized || m.is_summary)
            .map(MessageView::from)
            .collect(),
        profile_name: conv.profile_name.clone(),
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
