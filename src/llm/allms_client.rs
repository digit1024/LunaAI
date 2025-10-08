// src/llm/allms_client.rs

use crate::config::LlmProfile;
use crate::llm::{LlmError, Message, ChatResponse, Role};
use allms::llm::{LLMModel, AnthropicModels, GoogleModels, MistralModels, OpenAIModels};
use allms::Completions;
use anyhow::Result;

pub struct AllmsClient {
    profile: LlmProfile,
}

impl AllmsClient {
    pub fn new(profile: LlmProfile) -> Result<Self, LlmError> {
        Ok(Self { profile })
    }

    pub async fn send_message(&self, messages: Vec<Message>) -> Result<ChatResponse, LlmError> {
        // For now, we'll just serialize the messages into a single prompt string.
        // This is a simplification and will be improved later.
        let instructions = messages
            .into_iter()
            .map(|m| format!("{:?}: {}", m.role, m.content))
            .collect::<Vec<String>>()
            .join("\n");

        let api_key = self.profile.api_key.clone();
        let model_name = &self.profile.model;

        let content = match self.profile.backend.as_str() {
            "openai" | "deepseek" => {
                let model = OpenAIModels::try_from_str(model_name)
                    .ok_or_else(|| LlmError::Config(format!("Unsupported OpenAI model: {}", model_name)))?;
                let completions = Completions::new(model, &api_key, None, None);
                completions.get_answer::<String>(&instructions).await
            }
            "anthropic" => {
                let model = AnthropicModels::try_from_str(model_name)
                    .ok_or_else(|| LlmError::Config(format!("Unsupported Anthropic model: {}", model_name)))?;
                let completions = Completions::new(model, &api_key, None, None);
                completions.get_answer::<String>(&instructions).await
            }
            "google" => {
                let model = GoogleModels::try_from_str(model_name)
                    .ok_or_else(|| LlmError::Config(format!("Unsupported Google model: {}", model_name)))?;
                let completions = Completions::new(model, &api_key, None, None);
                completions.get_answer::<String>(&instructions).await
            }
            "mistral" => {
                let model = MistralModels::try_from_str(model_name)
                    .ok_or_else(|| LlmError::Config(format!("Unsupported Mistral model: {}", model_name)))?;
                let completions = Completions::new(model, &api_key, None, None);
                completions.get_answer::<String>(&instructions).await
            }
            _ => return Err(LlmError::Config(format!("Unsupported LLM backend: {}", self.profile.backend))),
        }
        .map_err(|e| LlmError::Api(e.to_string()))?;

        Ok(ChatResponse {
            content,
            tool_calls: Vec::new(), // Tool support to be added later
        })
    }
}