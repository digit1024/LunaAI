//! Model vision-capability heuristics.
//!
//! Used to decide whether an image can be sent natively to the main model or
//! should be described by the auxiliary vision model. Conservative: unknown
//! models report `false`, so callers describe rather than risk a rejected or
//! silently-ignored image.

use crate::config::ModelPreset;

/// True when `preset` is heuristically known to accept image input.
pub fn supports_vision(preset: &ModelPreset) -> bool {
    let model = preset.model.to_ascii_lowercase();
    let any = |needles: &[&str]| needles.iter().any(|n| model.contains(n));

    match preset.backend.as_str() {
        "openai" => any(&[
            "gpt-4o", "gpt-4.1", "gpt-4-turbo", "gpt-4-vision", "gpt-5", "o3", "o4",
        ]),
        "anthropic" => any(&[
            "claude-3", "claude-4", "claude-sonnet", "claude-opus", "claude-haiku",
        ]),
        "gemini" => model.contains("gemini"),
        // Local models: only the explicitly multimodal families.
        "ollama" => any(&[
            "llava", "bakllava", "vision", "-vl", "minicpm-v", "moondream",
        ]),
        // OpenRouter and other OpenAI-compatible endpoints: match known slugs.
        _ => any(&[
            "gpt-4o", "gpt-4.1", "gpt-4-turbo", "claude-3", "claude-4", "gemini",
            "llava", "vision", "-vl", "qwen-vl", "qwen2-vl", "pixtral",
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(backend: &str, model: &str) -> ModelPreset {
        ModelPreset {
            backend: backend.to_string(),
            model: model.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn known_vision_models_report_true() {
        assert!(supports_vision(&preset("openai", "gpt-4o-mini")));
        assert!(supports_vision(&preset("anthropic", "claude-sonnet-4-5")));
        assert!(supports_vision(&preset("gemini", "gemini-2.5-flash")));
        assert!(supports_vision(&preset("ollama", "llama3.2-vision:11b")));
        assert!(supports_vision(&preset("openrouter", "qwen/qwen2-vl-7b")));
    }

    #[test]
    fn text_only_models_report_false() {
        assert!(!supports_vision(&preset("openai", "gpt-3.5-turbo")));
        assert!(!supports_vision(&preset("deepseek", "deepseek-reasoner")));
        assert!(!supports_vision(&preset("ollama", "llama3:8b")));
        // Unknown/blank model is conservative.
        assert!(!supports_vision(&preset("custom", "")));
    }
}
