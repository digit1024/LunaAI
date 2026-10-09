//! Auxiliary vision: describe image attachments with a separate model, so the
//! main model receives text rather than base64 image bytes. Descriptions are
//! cached by image content hash and computed once.

use crate::config::{AppConfig, ModelPreset, VisionMode};
use crate::llm::{Attachment, LlmClient, Message, Role};
use crate::storage::Storage;
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::Mutex;

/// True when the image `mime` should be described rather than embedded.
pub fn should_describe(config: &AppConfig, main_preset: &ModelPreset, mime: &str) -> bool {
    if !config.vision.enabled || !mime.starts_with("image/") {
        return false;
    }
    match config.vision.mode {
        VisionMode::Native => false,
        VisionMode::Always => true,
        VisionMode::Auto => !crate::llm::capabilities::supports_vision(main_preset),
    }
}

/// SHA-256 of the file at `path`, hex-encoded, or `None` if unreadable.
pub fn content_hash(path: &str) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Some(
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}

/// Return a description of `attachment`, reusing the cache when possible.
///
/// `instruction` lets the caller ask something specific ("how many fingers?",
/// "what color is the button?"); it defaults to a generic description. Cache
/// entries are keyed by image content **and** instruction, so a generic
/// description is never reused to answer a specific question. Reads (and
/// populates) `image_descriptions`; on a miss calls `client` once. Does not
/// mutate `attachment`.
pub async fn describe_cached(
    storage: &Arc<Mutex<Storage>>,
    client: &Arc<dyn LlmClient>,
    config: &AppConfig,
    attachment: &Attachment,
    instruction: Option<&str>,
) -> Result<String> {
    let instruction = instruction.map(str::trim).filter(|s| !s.is_empty());
    let cache_key = content_hash(&attachment.file_path).map(|hash| match instruction {
        Some(text) => format!("{hash}:{text}"),
        None => hash,
    });

    if let Some(key) = &cache_key {
        let guard = storage.lock().await;
        if let Ok(Some(cached)) = guard.get_image_description(key) {
            drop(guard);
            return Ok(cached);
        }
    }

    // Make sure the bytes are available for the request.
    let mut request_attachment = attachment.clone();
    if request_attachment.content.is_none() {
        let _ = crate::llm::file_utils::hydrate_attachments_for_llm(std::slice::from_mut(
            &mut request_attachment,
        ));
    }

    let user_text = instruction.unwrap_or("Describe this image.").to_string();
    let request = vec![
        Message::new(Role::System, config.vision.system_prompt.clone()),
        Message {
            role: Role::User,
            content: user_text,
            timestamp: None,
            is_prompt: false,
            tool_call_id: None,
            tool_calls: None,
            attachments: Some(vec![request_attachment]),
            reasoning_content: None,
        },
    ];

    let response = client
        .send_message_with_tools(request, vec![], Some(0.2), Some(config.vision.max_tokens))
        .await
        .map_err(|e| anyhow::anyhow!("vision describe failed: {e}"))?;

    let description = response.content.trim().to_string();
    if description.is_empty() {
        return Err(anyhow::anyhow!("vision model returned an empty description"));
    }

    if let Some(key) = &cache_key {
        let guard = storage.lock().await;
        let _ = guard.upsert_image_description(key, &description, chrono::Utc::now().timestamp());
    }

    Ok(description)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(enabled: bool, mode: VisionMode) -> AppConfig {
        let mut config = AppConfig::default();
        config.vision.enabled = enabled;
        config.vision.mode = mode;
        config
    }

    fn preset(backend: &str, model: &str) -> ModelPreset {
        ModelPreset {
            backend: backend.to_string(),
            model: model.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn disabled_never_describes() {
        let config = cfg(false, VisionMode::Always);
        assert!(!should_describe(&config, &preset("openai", "gpt-4o"), "image/png"));
    }

    #[test]
    fn always_describes_even_for_vision_models() {
        let config = cfg(true, VisionMode::Always);
        assert!(should_describe(&config, &preset("openai", "gpt-4o"), "image/png"));
    }

    #[test]
    fn auto_describes_only_for_non_vision_models() {
        let config = cfg(true, VisionMode::Auto);
        assert!(should_describe(&config, &preset("deepseek", "deepseek-reasoner"), "image/png"));
        assert!(!should_describe(&config, &preset("openai", "gpt-4o"), "image/png"));
    }

    #[test]
    fn non_images_are_never_described() {
        let config = cfg(true, VisionMode::Always);
        assert!(!should_describe(&config, &preset("openai", "gpt-4o"), "text/plain"));
    }

    #[test]
    fn content_hash_is_stable_and_distinct() {
        let dir = std::env::temp_dir().join(format!("luna_vision_hash_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.bin");
        let b = dir.join("b.bin");
        std::fs::write(&a, b"same").unwrap();
        std::fs::write(&b, b"diff").unwrap();
        let ha = content_hash(a.to_str().unwrap()).unwrap();
        let hb = content_hash(b.to_str().unwrap()).unwrap();
        assert_eq!(ha, content_hash(a.to_str().unwrap()).unwrap());
        assert_ne!(ha, hb);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
