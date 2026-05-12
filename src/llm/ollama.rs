use super::*;
use crate::config::ModelPreset;
use crate::llm::observability::{classify_reqwest_error, CallOutcome, LlmCallSpan};
use crate::llm::tokenizer::TokenCounter;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use tracing;

// Ollama uses OpenAI-compatible API format
#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OllamaTool>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaMessage {
    role: String,
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OllamaToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaTool {
    r#type: String,
    function: OllamaToolFunction,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaToolFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaToolCall {
    id: String,
    r#type: String,
    function: OllamaToolCallFunction,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaToolCallFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    choices: Vec<OllamaChoice>,
}

#[derive(Debug, Deserialize)]
struct OllamaChoice {
    message: OllamaMessage,
}

#[derive(Debug, Deserialize)]
struct OllamaStreamResponse {
    choices: Vec<OllamaStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct OllamaStreamChoice {
    delta: OllamaDelta,
}

#[derive(Debug, Deserialize)]
struct OllamaDelta {
    content: Option<String>,
}

pub struct OllamaClient {
    client: Client,
    preset: ModelPreset,
}

impl OllamaClient {
    pub fn new(preset: ModelPreset) -> Self {
        Self {
            client: super::observability::shared_http_client(),
            preset,
        }
    }

    fn estimate_context_tokens(messages: &[Message]) -> Option<usize> {
        let counter = TokenCounter::cl100k();
        let mut total = 0usize;
        for m in messages {
            total = total.saturating_add(counter.count_message_tokens(m));
        }
        Some(total)
    }
}

#[async_trait]
impl LlmClient for OllamaClient {
    async fn send_message_stream(
        &self,
        messages: Vec<Message>,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LlmError>> + Send>>, LlmError> {
        let mut span = LlmCallSpan::start("ollama", self.preset.model.clone(), true);
        let context_tokens = Self::estimate_context_tokens(&messages);
        span.set_input_shape(messages.len(), 0, context_tokens);

        let ollama_messages: Vec<OllamaMessage> = messages
            .into_iter()
            .map(|msg| {
                tracing::debug!(
                    role = ?msg.role,
                    content_length = msg.content.len(),
                    attachment_count = msg.attachments.as_ref().map(|a| a.len()).unwrap_or(0),
                    "Converting message to Ollama format"
                );
                
                // Handle attachments by including them in the content
                let mut content = msg.content;
                if let Some(attachments) = msg.attachments {
                    for attachment in attachments {
                        match attachment.mime_type.as_str() {
                            mime if mime.starts_with("image/") => {
                                content.push_str(&format!("\n[Image: {} - {} bytes]", attachment.file_name, attachment.file_size));
                            }
                            mime if mime.starts_with("text/") => {
                                if let Some(file_content) = &attachment.content {
                                    content.push_str(&format!("\n\nFile: {}\nContent:\n{}", attachment.file_name, file_content));
                                }
                            }
                            _ => {
                                content.push_str(&format!("\nFile attached: {} ({} bytes)", attachment.file_name, attachment.file_size));
                            }
                        }
                    }
                }
                
                OllamaMessage {
                    role: match msg.role {
                        Role::User => "user".to_string(),
                        Role::Assistant => "assistant".to_string(),
                        Role::System => "system".to_string(),
                        Role::Tool => "tool".to_string(),
                    },
                    content: Some(content),
                    tool_calls: None,
                    tool_call_id: msg.tool_call_id,
                }
            })
            .collect();

        let request = OllamaRequest {
            model: self.preset.model.clone(),
            messages: ollama_messages,
            temperature: temperature.or(self.preset.temperature),
            max_tokens: max_tokens.or(self.preset.max_tokens),
            stream: true,
            tools: None,
        };

        let mut request_builder = self
            .client
            .post(&self.preset.endpoint)
            .header("Content-Type", "application/json");

        // Only add authorization header if API key is provided
        if !self.preset.api_key.is_empty() {
            request_builder =
                request_builder.header("Authorization", format!("Bearer {}", self.preset.api_key));
        }

        let response = match request_builder.json(&request).send().await {
            Ok(r) => r,
            Err(e) => {
                let (outcome, kind) = classify_reqwest_error(&e);
                span.finish_error(outcome, kind, e.to_string());
                return Err(LlmError::Http(e));
            }
        };

        let status = response.status();
        span.set_response_headers(status.as_u16(), None);

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            let msg = format!("Ollama API error (HTTP {}): {}", status.as_u16(), error_text);
            span.finish_error(CallOutcome::HttpError, format!("http_{}", status.as_u16()), msg.clone());
            return Err(LlmError::Api(msg));
        }

        // Ollama's local server is the most reliable provider; close span
        // optimistically. transport-level keepalive still applies.
        span.finish_success();

        let stream = response.bytes_stream();
        let stream = futures::StreamExt::map(stream, |chunk_result| {
            chunk_result
                .map_err(|e| LlmError::Http(e))
                .and_then(|chunk| {
                    let chunk_str = String::from_utf8(chunk.to_vec())
                        .map_err(|e| LlmError::Api(format!("Invalid UTF-8: {}", e)))?;

                    // Parse SSE format
                    let lines: Vec<&str> = chunk_str.lines().collect();
                    let mut content = String::new();

                    for line in lines {
                        if line.starts_with("data: ") {
                            let data = &line[6..]; // Remove "data: " prefix
                            if data == "[DONE]" {
                                break;
                            }

                            // Parse JSON
                            if let Ok(stream_response) =
                                serde_json::from_str::<OllamaStreamResponse>(data)
                            {
                                if let Some(choice) = stream_response.choices.first() {
                                    if let Some(content_delta) = &choice.delta.content {
                                        content.push_str(content_delta);
                                    }
                                }
                            }
                        }
                    }

                    if content.is_empty() {
                        Ok(None)
                    } else {
                        Ok(Some(content))
                    }
                })
        });
        let stream = futures::StreamExt::filter_map(stream, |result| async move {
            match result {
                Ok(Some(content)) => Some(Ok(content)),
                Ok(None) => None,
                Err(e) => Some(Err(e)),
            }
        });

        Ok(Box::pin(stream))
    }

    async fn send_message_with_tools(
        &self,
        messages: Vec<Message>,
        available_tools: Vec<ToolDefinition>,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
    ) -> Result<ChatResponse, LlmError> {
        let mut span = LlmCallSpan::start("ollama", self.preset.model.clone(), false);
        let context_tokens = Self::estimate_context_tokens(&messages);
        span.set_input_shape(messages.len(), available_tools.len(), context_tokens);

        let ollama_messages: Vec<OllamaMessage> = messages
            .into_iter()
            .map(|msg| {
                tracing::debug!(
                    role = ?msg.role,
                    content_length = msg.content.len(),
                    attachment_count = msg.attachments.as_ref().map(|a| a.len()).unwrap_or(0),
                    "Converting message to Ollama format (tools)"
                );
                
                let tool_calls = if let Some(tool_calls) = msg.tool_calls {
                    Some(tool_calls.into_iter().map(|tc| OllamaToolCall {
                        id: tc.id,
                        r#type: "function".to_string(),
                        function: OllamaToolCallFunction {
                            name: tc.name,
                            arguments: serde_json::to_string(&tc.parameters).unwrap_or_else(|_| "{}".to_string()),
                        },
                    }).collect())
                } else {
                    None
                };
                
                // Handle attachments by including them in the content
                let mut content = msg.content;
                if let Some(attachments) = msg.attachments {
                    for attachment in attachments {
                        match attachment.mime_type.as_str() {
                            mime if mime.starts_with("image/") => {
                                content.push_str(&format!("\n[Image: {} - {} bytes]", attachment.file_name, attachment.file_size));
                            }
                            mime if mime.starts_with("text/") => {
                                if let Some(file_content) = &attachment.content {
                                    content.push_str(&format!("\n\nFile: {}\nContent:\n{}", attachment.file_name, file_content));
                                }
                            }
                            _ => {
                                content.push_str(&format!("\nFile attached: {} ({} bytes)", attachment.file_name, attachment.file_size));
                            }
                        }
                    }
                }
                
                OllamaMessage {
                    role: match msg.role {
                        Role::User => "user".to_string(),
                        Role::Assistant => "assistant".to_string(),
                        Role::System => "system".to_string(),
                        Role::Tool => "tool".to_string(),
                    },
                    content: Some(content),
                    tool_calls,
                    tool_call_id: msg.tool_call_id,
                }
            })
            .collect();

        let has_tools = !available_tools.is_empty();
        let tools = if !has_tools {
            None
        } else {
            Some(
                available_tools
                    .into_iter()
                    .map(|tool| OllamaTool {
                        r#type: "function".to_string(),
                        function: OllamaToolFunction {
                            name: tool.name,
                            description: tool.description,
                            parameters: tool.parameters,
                        },
                    })
                    .collect(),
            )
        };

        let request = OllamaRequest {
            model: self.preset.model.clone(),
            messages: ollama_messages,
            temperature: temperature.or(self.preset.temperature),
            max_tokens: max_tokens.or(self.preset.max_tokens),
            stream: false,
            tools,
        };

        let mut request_builder = self
            .client
            .post(&self.preset.endpoint)
            .header("Content-Type", "application/json");

        // Only add authorization header if API key is provided
        if !self.preset.api_key.is_empty() {
            request_builder =
                request_builder.header("Authorization", format!("Bearer {}", self.preset.api_key));
        }

        let response = match request_builder.json(&request).send().await {
            Ok(r) => r,
            Err(e) => {
                let (outcome, kind) = classify_reqwest_error(&e);
                span.finish_error(outcome, kind, e.to_string());
                return Err(LlmError::Http(e));
            }
        };

        let status = response.status();
        span.set_response_headers(status.as_u16(), None);

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            let msg = format!("Ollama API error (HTTP {}): {}", status.as_u16(), error_text);
            span.finish_error(CallOutcome::HttpError, format!("http_{}", status.as_u16()), msg.clone());
            return Err(LlmError::Api(msg));
        }

        let response_data: OllamaResponse = match response.json().await {
            Ok(r) => r,
            Err(e) => {
                let (outcome, kind) = classify_reqwest_error(&e);
                span.finish_error(outcome, kind, e.to_string());
                return Err(LlmError::Http(e));
            }
        };

        let choice = match response_data.choices.first() {
            Some(c) => c,
            None => {
                let msg = "No response from Ollama".to_string();
                span.finish_error(CallOutcome::Parse, "empty_choices", msg.clone());
                return Err(LlmError::Api(msg));
            }
        };

        let content = choice.message.content.clone().unwrap_or_default();

        let tool_calls = if let Some(tool_calls) = &choice.message.tool_calls {
            tool_calls
                .iter()
                .map(|tc| {
                    span.observe_tool_call();
                    ToolCall {
                        id: tc.id.clone(),
                        name: tc.function.name.clone(),
                        parameters: serde_json::from_str(&tc.function.arguments).unwrap_or_default(),
                    }
                })
                .collect()
        } else {
            Vec::new()
        };

        span.finish_success();
        Ok(ChatResponse {
            content,
            tool_calls,
            reasoning_content: None,
        })
    }
}
