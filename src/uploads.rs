//! Upload lookup helpers shared by the HTTP handlers, chat handlers and the
//! agentic tools.
//!
//! Previously three near-duplicate implementations existed
//! (`server::http::find_upload_by_uid`, `server::handlers::helpers::
//! resolve_attachment_upload_path`, `agentic::loop_engine::
//! resolve_uploaded_attachment`) with subtly different rules — notably only
//! one of them rejected symlinks. This module is the single implementation:
//! symlink-rejecting, async, and scoped by conversation where known.

use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Subdir holding uploads made before a conversation id existed.
pub const NO_CONVERSATION_DIR: &str = "_no_conversation";

/// True when `name` is exactly `{attachment_uid}.{ext}` with one extension
/// segment (so `{uid}x.png`, `{uid}.tar.gz` and bare `{uid}` never match).
pub fn filename_matches(name: &str, attachment_uid: &str) -> bool {
    let Some((stem, ext)) = name.split_once('.') else {
        return false;
    };
    !ext.is_empty() && !ext.contains('.') && !stem.is_empty() && stem == attachment_uid
}

/// Locate an uploaded file by uid.
///
/// Search order: `uploads/{conversation_id}/` (when given), then
/// `uploads/_no_conversation/`. Symlinks are rejected. Returns None when no
/// matching regular file exists.
pub async fn find_scoped(
    uploads_base: &Path,
    conversation_id: Option<&Uuid>,
    uid: &str,
) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::with_capacity(2);
    if let Some(conv) = conversation_id {
        dirs.push(uploads_base.join(conv.to_string()));
    }
    dirs.push(uploads_base.join(NO_CONVERSATION_DIR));
    for dir in dirs {
        if let Some(p) = find_in_dir(&dir, uid).await {
            return Some(p);
        }
    }
    None
}

/// Locate an uploaded file by uid across ALL conversation subdirs of
/// `uploads_base` (for endpoints that only carry the uid). Symlinks rejected.
pub async fn find_any(uploads_base: &Path, uid: &str) -> Option<PathBuf> {
    let mut subdirs = tokio::fs::read_dir(uploads_base).await.ok()?;
    while let Ok(Some(sub)) = subdirs.next_entry().await {
        if !sub.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        if let Some(p) = find_in_dir(&sub.path(), uid).await {
            return Some(p);
        }
    }
    None
}

async fn find_in_dir(dir: &Path, uid: &str) -> Option<PathBuf> {
    let mut entries = tokio::fs::read_dir(dir).await.ok()?;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !filename_matches(&name, uid) {
            continue;
        }
        let p = entry.path();
        // Never serve/consume symlinks: an attacker-writable uploads tree must
        // not be able to redirect reads outside of it.
        match tokio::fs::symlink_metadata(&p).await {
            Ok(meta) if meta.file_type().is_symlink() => return None,
            Ok(_) => return Some(p),
            Err(_) => return None,
        }
    }
    None
}

/// Lowercase hex SHA-256 of `data` (dedupe index key).
pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_matches_exact_uid_dot_ext() {
        let uid = "11111111-2222-3333-4444-555555555555";
        assert!(filename_matches(&format!("{uid}.png"), uid));
        assert!(!filename_matches(&format!("{uid}x.png"), uid));
        assert!(!filename_matches(&format!("{uid}.tar.gz"), uid));
        assert!(!filename_matches(uid, uid));
        assert!(!filename_matches(&format!("{uid}."), uid));
    }

    #[tokio::test]
    async fn scoped_lookup_prefers_conversation_dir_and_rejects_symlinks() {
        let base = std::env::temp_dir().join(format!("luna_uploads_{}", Uuid::new_v4()));
        let conv = Uuid::new_v4();
        let conv_dir = base.join(conv.to_string());
        let no_conv = base.join(NO_CONVERSATION_DIR);
        tokio::fs::create_dir_all(&conv_dir).await.unwrap();
        tokio::fs::create_dir_all(&no_conv).await.unwrap();

        let uid = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
        tokio::fs::write(conv_dir.join(format!("{uid}.txt")), b"here")
            .await
            .unwrap();

        // Found in the conversation dir.
        assert_eq!(
            find_scoped(&base, Some(&conv), uid).await,
            Some(conv_dir.join(format!("{uid}.txt")))
        );
        // Also findable without conversation context via find_any.
        assert!(find_any(&base, uid).await.is_some());
        // Unknown uid -> None.
        assert!(find_scoped(&base, Some(&conv), "no-such-uid").await.is_none());

        // Symlinked entry is rejected.
        let link_uid = "bbbbbbbb-cccc-dddd-eeee-ffffffffffff";
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                conv_dir.join(format!("{uid}.txt")),
                conv_dir.join(format!("{link_uid}.txt")),
            )
            .unwrap();
            assert!(find_scoped(&base, Some(&conv), link_uid).await.is_none());
        }

        let _ = tokio::fs::remove_dir_all(&base).await;
    }
}
