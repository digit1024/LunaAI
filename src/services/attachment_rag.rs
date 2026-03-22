//! Chunk + vector index for large attachment text (requires embedding + sqlite-vec).

use crate::config::AttachmentRagConfig;
use crate::embeddings::EmbeddingProvider;
use crate::llm::file_utils::FileType;
use crate::storage::Storage;
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

fn chunk_text(s: &str, chunk_chars: usize, overlap: usize) -> Vec<String> {
    if chunk_chars == 0 {
        return if s.is_empty() {
            vec![]
        } else {
            vec![s.to_string()]
        };
    }
    let chars: Vec<char> = s.chars().collect();
    if chars.is_empty() {
        return vec![];
    }
    let mut out = Vec::new();
    let step = chunk_chars.saturating_sub(overlap).max(1);
    let mut i = 0;
    while i < chars.len() {
        let end = (i + chunk_chars).min(chars.len());
        let piece: String = chars[i..end].iter().collect();
        if !piece.trim().is_empty() {
            out.push(piece);
        }
        if end >= chars.len() {
            break;
        }
        i += step;
    }
    out
}

fn hash_text(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    format!("{:x}", h.finalize())
}

/// Full extracted text for indexing (not the truncated inline copy).
pub fn full_extract_for_indexing(path: &str) -> Option<String> {
    let p = Path::new(path);
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_string();
    let ft = FileType::from_extension(&ext);
    match ft {
        FileType::Document => markdownify::convert(p).ok(),
        FileType::Text | FileType::Unsupported => std::fs::read_to_string(p).ok(),
        FileType::Image => None,
    }
}

/// Trim attachment text for the LLM when over threshold; point model at search tool.
pub fn trim_extracted_text_for_inline(content: Option<String>, max_chars: usize) -> Option<String> {
    let Some(c) = content else {
        return None;
    };
    if c.chars().count() <= max_chars {
        return Some(c);
    }
    let snippet: String = c.chars().take(max_chars).collect();
    Some(format!(
        "{}\n\n[Document truncated for context; use the search_attachment_chunks tool to query the full indexed text.]",
        snippet
    ))
}

/// Build vector index when full extract is larger than `inline_max_chars`.
pub async fn index_large_attachment_if_needed(
    storage: Arc<Mutex<Storage>>,
    embedding_provider: Option<&dyn EmbeddingProvider>,
    rag: &AttachmentRagConfig,
    conversation_id: &str,
    attachment_uid: &str,
    file_name: &str,
    file_path: &str,
) -> Result<()> {
    let Some(provider) = embedding_provider else {
        return Ok(());
    };

    let Some(full_text) = full_extract_for_indexing(file_path) else {
        return Ok(());
    };
    if full_text.chars().count() <= rag.inline_max_chars {
        return Ok(());
    }

    let chunks = chunk_text(&full_text, rag.chunk_chars, rag.chunk_overlap);
    if chunks.is_empty() {
        return Ok(());
    }

    {
        let guard = storage.lock().await;
        guard
            .delete_attachment_doc_chunks_for(conversation_id, attachment_uid)
            .context("clear attachment chunk index")?;
    }

    let content_hash = hash_text(&full_text);
    for (idx, chunk) in chunks.iter().enumerate() {
        let emb = provider
            .embed(chunk)
            .await
            .with_context(|| format!("embed attachment chunk {}", idx))?;
        let guard = storage.lock().await;
        guard.insert_attachment_doc_chunk_with_embedding(
            conversation_id,
            attachment_uid,
            file_name,
            idx as i32,
            chunk,
            &content_hash,
            &emb,
        )?;
    }

    Ok(())
}
