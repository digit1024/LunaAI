use super::*;
use super::rate_limiter::RateLimitHandler;
use crate::config::LlmProfile;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

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
    profile: LlmProfile,
}

impl OllamaClient {
    pub fn new(profile: LlmProfile) -> Self {
        Self {
            client: Client::new(),
            profile,
        }
    }

    /// Execute an API request with retry logic for rate limiting
    /// Note: Ollama is typically local and doesn't have rate limits, but we implement
    /// the same interface for consistency and potential future remote usage
    async fn execute_with_retry<F>(&self, request_fn: F) -> Result<reqwest::Response, LlmError>
    where
        F: Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<reqwest::Response, reqwest::Error>> + Send>>,
    {
        let rate_handler = RateLimitHandler::new(self.profile.clone());
        let mut attempt_count = 0;

        loop {
            let response = request_fn().await?;
            
            if response.status().is_success() {
                return Ok(response);
            }

            let status = response.status().as_u16();
            
            // Check if this is a rate limit error (unlikely for local Ollama)
            if RateLimitHandler::is_rate_limit_error(status) {
                // Extract rate limit info from headers
                let rate_limit_info = rate_handler.extract_rate_limit_info(response.headers(), attempt_count);
                
                // Handle rate limit with retry logic
                if let Err(e) = rate_handler.handle_rate_limit_error(rate_limit_info).await {
                    return Err(e);
                }
                
                attempt_count += 1;
                continue;
            }

            // For non-rate-limit errors, get the error text and return immediately
            let error_text = response.text().await.unwrap_or_default();
            return Err(LlmError::Api(format!("Ollama API error: {}", error_text)));
        }
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
        let ollama_messages: Vec<OllamaMessage> = messages
            .into_iter()
            .map(|msg| {
                println!("🔍 DEBUG: Converting message to Ollama: role={:?}, content={}, attachments={:?}", 
                    msg.role, msg.content, msg.attachments);
                
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
            model: self.profile.model.clone(),
            messages: ollama_messages,
            temperature: temperature.or(self.profile.temperature),
            max_tokens: max_tokens.or(self.profile.max_tokens),
            stream: true,
            tools: None,
        };

        let response = self.execute_with_retry(|| {
            let mut request_builder = self.client
                .post(&self.profile.endpoint)
                .header("Content-Type", "application/json");
            
            // Only add authorization header if API key is provided
            if !self.profile.api_key.is_empty() {
                request_builder = request_builder.header("Authorization", format!("Bearer {}", self.profile.api_key));
            }

            Box::pin(request_builder.json(&request).send())
        }).await?;

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
                            if let Ok(stream_response) = serde_json::from_str::<OllamaStreamResponse>(data) {
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
        let ollama_messages: Vec<OllamaMessage> = messages
            .into_iter()
            .map(|msg| {
                println!("🔍 DEBUG: Converting message to Ollama (tools): role={:?}, content={}, attachments={:?}", 
                    msg.role, msg.content, msg.attachments);
                
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
            Some(available_tools.into_iter().map(|tool| OllamaTool {
                r#type: "function".to_string(),
                function: OllamaToolFunction {
                    name: tool.name,
                    description: tool.description,
                    parameters: tool.parameters,
                },
            }).collect())
        };

        let request = OllamaRequest {
            model: self.profile.model.clone(),
            messages: ollama_messages,
            temperature: temperature.or(self.profile.temperature),
            max_tokens: max_tokens.or(self.profile.max_tokens),
            stream: false,
            tools,
        };

        let response = self.execute_with_retry(|| {
            let mut request_builder = self.client
                .post(&self.profile.endpoint)
                .header("Content-Type", "application/json");
            
            // Only add authorization header if API key is provided
            if !self.profile.api_key.is_empty() {
                request_builder = request_builder.header("Authorization", format!("Bearer {}", self.profile.api_key));
            }

            Box::pin(request_builder.json(&request).send())
        }).await?;

        let response_data: OllamaResponse = response.json().await?;

        let choice = response_data
            .choices
            .first()
            .ok_or_else(|| LlmError::Api("No response from Ollama".to_string()))?;

        let content = choice.message.content.clone().unwrap_or_default();
        
        let tool_calls = if let Some(tool_calls) = &choice.message.tool_calls {
            tool_calls.iter().map(|tc| ToolCall {
                id: tc.id.clone(),
                name: tc.function.name.clone(),
                parameters: serde_json::from_str(&tc.function.arguments).unwrap_or_default(),
            }).collect()
        } else {
            Vec::new()
        };

        Ok(ChatResponse {
            content,
            tool_calls,
        })
    }
}

