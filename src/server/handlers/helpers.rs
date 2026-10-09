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
use std::sync::OnceLock;
use uuid::Uuid;

/// Truncate to at most `max_chars` characters (char-boundary safe).
fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let kept: String = text.chars().take(max_chars).collect();
    format!("{kept}\n[... truncated ...]")
}

fn attach_ref_regex() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"@attach:([0-9a-zA-Z-]{1,64})").expect("attach ref regex"))
}

/// Expand `@attach:<uid>` tokens in a user message at prompt-build time.
///
/// Text/document attachments are inlined at the token position; image tokens are
/// dropped (the image rides turn-level via message metadata). Unresolved tokens
/// stay verbatim. Never persisted — the transcript keeps the token.
pub(super) async fn expand_attach_tokens(
    content: &str,
    uploads_base: &Path,
    conversation_id: &Uuid,
    max_inline_chars: usize,
) -> String {
    if !content.contains("@attach:") {
        return content.to_string();
    }

    let mut out = String::with_capacity(content.len());
    let mut last = 0usize;
    for caps in attach_ref_regex().captures_iter(content) {
        let whole = caps.get(0).expect("whole match");
        out.push_str(&content[last..whole.start()]);
        last = whole.end();
        let uid = &caps[1];

        match resolve_attachment_upload_path(uploads_base, conversation_id, uid).await {
            Ok(path) => {
                let path_str = path.to_string_lossy().to_string();
                match crate::llm::file_utils::create_attachment(&path_str) {
                    Ok(att) if att.mime_type.starts_with("image/") => {
                        // Images cannot be position-placed; drop the token.
                    }
                    Ok(att) => {
                        let body = att
                            .content
                            .unwrap_or_else(|| format!("[attachment: {}]", att.file_name));
                        out.push_str("\n```\n");
                        out.push_str(&truncate_chars(&body, max_inline_chars));
                        out.push_str("\n```\n");
                    }
                    Err(_) => out.push_str(whole.as_str()),
                }
            }
            Err(_) => out.push_str(whole.as_str()),
        }
    }
    out.push_str(&content[last..]);
    out
}

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
            if !crate::server::upload_policy::filename_matches(&name, attachment_uid) {
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

    #[tokio::test]
    async fn attach_token_inlines_text_and_drops_image() {
        let base = std::env::temp_dir().join(format!("luna_attach_tok_{}", std::process::id()));
        let conv = Uuid::new_v4();
        let dir = base.join(conv.to_string());
        tokio::fs::create_dir_all(&dir).await.unwrap();

        let text_uid = "11111111-1111-1111-1111-111111111111";
        let img_uid = "22222222-2222-2222-2222-222222222222";
        tokio::fs::write(dir.join(format!("{text_uid}.txt")), b"hello inline")
            .await
            .unwrap();
        tokio::fs::write(dir.join(format!("{img_uid}.png")), b"\x89PNG\r\n\x1a\n\x00")
            .await
            .unwrap();

        let content = format!("before @attach:{text_uid} middle @attach:{img_uid} after");
        let out = expand_attach_tokens(&content, &base, &conv, 100_000).await;

        assert!(out.contains("hello inline"), "{out}");
        assert!(!out.contains(&format!("@attach:{text_uid}")), "{out}");
        assert!(!out.contains(&format!("@attach:{img_uid}")), "{out}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn unresolved_attach_token_is_literal() {
        let base = std::env::temp_dir().join(format!("luna_attach_tok2_{}", std::process::id()));
        let conv = Uuid::new_v4();
        let content = "keep @attach:deadbeefdeadbeefdeadbeefdeadbeef here";
        let out = expand_attach_tokens(content, &base, &conv, 100_000).await;
        assert_eq!(out, content);
        let _ = std::fs::remove_dir_all(&base);
    }
}
