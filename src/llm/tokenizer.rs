//! Token counting for different LLM models
//! 
//! This module provides accurate token counting across different model families:
//! - OpenAI/DeepSeek: Uses tiktoken (cl100k_base)
//! - Anthropic: Approximation using cl100k_base (similar tokenization)
//! - Gemini: Character-based estimation
//! - Ollama: Model-specific estimation

use crate::config::LlmProfile;
use crate::llm::Message;

/// Tokenizer type based on model backend
#[derive(Debug, Clone)]
pub enum TokenizerType {
    /// OpenAI/DeepSeek models (cl100k_base)
    Cl100kBase,
    /// Anthropic Claude (approximate - similar to cl100k_base)
    Anthropic,
    /// Google Gemini (approximate)
    Gemini,
    /// Ollama - depends on model name
    Ollama { model_name: String },
    /// Fallback: character-based estimation
    Estimation,
}

/// Token counter with model-specific tokenization
pub struct TokenCounter {
    tokenizer_type: TokenizerType,
    cl100k_encoder: Option<tiktoken_rs::CoreBPE>, // Cached encoder
}

impl TokenCounter {
    /// Create a new token counter for the given profile
    pub fn new(profile: &LlmProfile) -> Self {
        let tokenizer_type = Self::detect_tokenizer(profile);
        let mut counter = Self {
            tokenizer_type,
            cl100k_encoder: None,
        };

        // Initialize encoder if needed
        if matches!(counter.tokenizer_type, TokenizerType::Cl100kBase | TokenizerType::Anthropic) {
            counter.cl100k_encoder = tiktoken_rs::cl100k_base().ok();
        }

        counter
    }

    /// Detect the appropriate tokenizer for a given profile
    fn detect_tokenizer(profile: &LlmProfile) -> TokenizerType {
        match profile.backend.as_str() {
            "openai" | "deepseek" => {
                // All OpenAI/DeepSeek models use cl100k_base
                TokenizerType::Cl100kBase
            }
            "anthropic" => TokenizerType::Anthropic,
            "gemini" => TokenizerType::Gemini,
            "ollama" => TokenizerType::Ollama {
                model_name: profile.model.clone(),
            },
            _ => TokenizerType::Estimation,
        }
    }

    /// Count tokens in a text string
    pub fn count_tokens(&self, text: &str) -> usize {
        match &self.tokenizer_type {
            TokenizerType::Cl100kBase => {
                if let Some(encoder) = &self.cl100k_encoder {
                    encoder.encode_with_special_tokens(text).len()
                } else {
                    // Fallback to estimation if encoder failed to load
                    tracing::warn!("tiktoken encoder failed to load, using estimation");
                    Self::estimate_tokens(text)
                }
            }
            TokenizerType::Anthropic => {
                // Anthropic uses similar tokenization to cl100k_base
                // But not exactly the same. Use cl100k as approximation
                if let Some(encoder) = &self.cl100k_encoder {
                    // Add 5% buffer for differences
                    ((encoder.encode_with_special_tokens(text).len() as f32) * 1.05) as usize
                } else {
                    Self::estimate_tokens(text)
                }
            }
            TokenizerType::Gemini => {
                // Gemini tokenization is different, use estimation with adjustment
                Self::estimate_tokens_gemini(text)
            }
            TokenizerType::Ollama { model_name } => {
                // Ollama models vary - try to detect
                Self::estimate_tokens_ollama(text, model_name)
            }
            TokenizerType::Estimation => Self::estimate_tokens(text),
        }
    }

    /// Count tokens in a complete message (including role, tool calls, attachments, etc.)
    pub fn count_message_tokens(&self, msg: &Message) -> usize {
        let mut tokens = 0;

        // Role + formatting tokens (~4-10 tokens depending on API)
        // Different APIs format messages differently, but roughly:
        tokens += 5;

        // Content
        tokens += self.count_tokens(&msg.content);

        // Reasoning content (if present) - DeepSeek thinking tokens
        if let Some(ref reasoning) = msg.reasoning_content {
            tokens += self.count_tokens(reasoning);
            // Additional overhead for reasoning content structure
            tokens += 10;
        }

        // Tool calls (if present)
        if let Some(ref tool_calls) = msg.tool_calls {
            for tool_call in tool_calls {
                // Tool call structure overhead (JSON formatting, field names, etc.)
                tokens += 15;
                tokens += self.count_tokens(&tool_call.name);
                tokens += self.count_tokens(&tool_call.parameters.to_string());
            }
        }

        // Tool result overhead (if this is a tool result message)
        if msg.tool_call_id.is_some() {
            // Additional overhead for tool result structure
            tokens += 10;
        }

        // Attachments
        if let Some(ref attachments) = msg.attachments {
            for attachment in attachments {
                // File metadata overhead
                tokens += self.count_tokens(&attachment.file_name);
                tokens += self.count_tokens(&attachment.mime_type);
                tokens += 10; // Structure overhead

                // Content tokens
                if let Some(ref content) = attachment.content {
                    // Text file content
                    tokens += self.count_tokens(content);
                } else {
                    // Binary/image file - estimate from size
                    // Rough estimate: ~1 token per 4 bytes of base64
                    // Base64 encoding increases size by ~33%, so:
                    // tokens ≈ (file_size * 1.33) / 4
                    tokens += ((attachment.file_size as f32 * 1.33) / 4.0) as usize;
                }
            }
        }

        tokens
    }

    /// Get the context window limit for a given profile
    pub fn get_context_limit(&self, profile: &LlmProfile) -> usize {
        // First, check if context_window_size is configured in profile
        if let Some(configured_size) = profile.context_window_size {
            return configured_size;
        }

        // Otherwise, auto-detect based on model name
        let model_lower = profile.model.to_lowercase();

        match profile.backend.as_str() {
            "openai" | "deepseek" => {
                if model_lower.contains("gpt-4o") || model_lower.contains("o3") {
                    128_000 // GPT-4o / O3
                } else if model_lower.contains("o1") {
                    200_000 // o1 models
                } else if model_lower.contains("gpt-4-turbo")
                    || model_lower.contains("gpt-4-1106")
                    || model_lower.contains("gpt-4-0125")
                {
                    128_000 // GPT-4 Turbo
                } else if model_lower.contains("gpt-4") {
                    8_192 // GPT-4 base
                } else if model_lower.contains("gpt-3.5-turbo") {
                    16_385 // GPT-3.5 Turbo
                } else if model_lower.contains("deepseek") {
                    // DeepSeek models typically have large context windows
                    if model_lower.contains("reasoner") {
                        131_072 // DeepSeek Reasoner: 128k context (131,072 tokens)
                    } else if model_lower.contains("chat") || model_lower.contains("v2") {
                        131_072 // DeepSeek Chat v2: 128k context
                    } else if model_lower.contains("v1") {
                        64_000 // DeepSeek Chat v1: 64k context
                    } else {
                        131_072 // Default for newer DeepSeek models: 128k
                    }
                } else {
                    4_096 // Default/safe for unknown models
                }
            }
            "anthropic" => {
                if model_lower.contains("claude-3-5-sonnet") || model_lower.contains("claude-3-opus") {
                    200_000
                } else if model_lower.contains("claude-3") {
                    200_000
                } else if model_lower.contains("claude-2") {
                    200_000
                } else {
                    100_000 // Older Claude
                }
            }
            "gemini" => {
                if model_lower.contains("1.5-pro") || model_lower.contains("1.5-flash") {
                    1_000_000 // Gemini 1.5
                } else if model_lower.contains("2.0") {
                    1_000_000 // Gemini 2.0
                } else {
                    32_768 // Gemini 1.0
                }
            }
            "ollama" => {
                // Ollama models vary - check model name
                let model_lower = model_lower.as_str();
                if model_lower.contains("llama3.1") || model_lower.contains("llama3") {
                    128_000 // Llama 3.1
                } else if model_lower.contains("mistral") || model_lower.contains("mixtral") {
                    32_768 // Mistral/Mixtral
                } else if model_lower.contains("qwen") {
                    32_768 // Qwen
                } else {
                    4_096 // Conservative default
                }
            }
            _ => 4_096, // Safe default
        }
    }

    /// Get a safe context limit (leaving 20% headroom for response generation)
    pub fn get_safe_context_limit(&self, profile: &LlmProfile) -> usize {
        let limit = self.get_context_limit(profile);
        // Leave 20% headroom for response generation
        ((limit as f32) * 0.8) as usize
    }

    /// Get the summarization threshold (0.0 - 1.0) from profile
    /// Default: 0.7 (70% of context window)
    pub fn get_summarize_threshold(&self, profile: &LlmProfile) -> f32 {
        profile.summarize_threshold
    }

    /// Get the token count at which summarization should trigger
    /// This is: context_window_size * summarize_threshold
    pub fn get_summarize_threshold_tokens(&self, profile: &LlmProfile) -> usize {
        let context_limit = self.get_context_limit(profile);
        let threshold = self.get_summarize_threshold(profile);
        ((context_limit as f32) * threshold) as usize
    }

    // Fallback estimation: ~4 characters per token (conservative)
    fn estimate_tokens(text: &str) -> usize {
        // Conservative: English text is ~4 chars/token, but code/markdown can be different
        // Use a heuristic that accounts for whitespace
        let char_count = text.chars().count();
        let word_count = text.split_whitespace().count();

        // Average: take the higher estimate
        let char_based = char_count / 4;
        let word_based = ((word_count as f32) * 1.3) as usize; // ~1.3 tokens per word

        char_based.max(word_based)
    }

    fn estimate_tokens_gemini(text: &str) -> usize {
        // Gemini tends to tokenize more aggressively (smaller tokens)
        // Estimate ~3.5 chars per token
        ((text.chars().count() as f32) / 3.5) as usize
    }

    fn estimate_tokens_ollama(text: &str, model_name: &str) -> usize {
        // Ollama models vary:
        // - Llama: SentencePiece, ~3-4 chars/token
        // - Mistral: Similar
        // - Code models: Can be very different

        let model_lower = model_name.to_lowercase();
        if model_lower.contains("code") {
            // Code models tokenize differently
            text.chars().count() / 5 // More tokens for code
        } else {
            // Default for most Ollama models
            text.chars().count() / 3
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens() {
        let text = "Hello, world! This is a test.";
        let tokens = TokenCounter::estimate_tokens(text);
        // Should be roughly 7-10 tokens
        assert!(tokens >= 5 && tokens <= 15);
    }

    #[test]
    fn test_count_tokens_openai() {
        let profile = LlmProfile {
            backend: "openai".to_string(),
            model: "gpt-4".to_string(),
            ..LlmProfile::default()
        };
        let counter = TokenCounter::new(&profile);
        let text = "Hello, world!";
        let tokens = counter.count_tokens(text);
        // Should be around 3-4 tokens with cl100k_base
        assert!(tokens > 0 && tokens < 10);
    }

    #[test]
    fn test_context_limits() {
        let profile = LlmProfile {
            backend: "openai".to_string(),
            model: "gpt-4o".to_string(),
            ..LlmProfile::default()
        };
        let counter = TokenCounter::new(&profile);
        let limit = counter.get_context_limit(&profile);
        assert_eq!(limit, 128_000);

        let safe_limit = counter.get_safe_context_limit(&profile);
        assert_eq!(safe_limit, 102_400); // 80% of 128k
    }
}



