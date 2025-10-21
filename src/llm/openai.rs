use super::*;
use super::rate_limiter::RateLimitHandler;
use crate::config::LlmProfile;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

#[derive(Debug, Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    stream: bool,
    tools: Option<Vec<OpenAITool>>,
    tool_choice: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAIMessage {
    role: String,
    content: Option<serde_json::Value>,
    tool_calls: Option<Vec<OpenAIToolCall>>,
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAITool {
    r#type: String,
    function: OpenAIToolFunction,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAIToolFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAIToolCall {
    id: String,
    r#type: String,
    function: OpenAIToolCallFunction,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAIToolCallFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    choices: Vec<OpenAIChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    message: OpenAIMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamResponse {
    choices: Vec<OpenAIStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamChoice {
    delta: OpenAIDelta,
}

#[derive(Debug, Deserialize)]
struct OpenAIDelta {
    content: Option<String>,
}

pub struct OpenAIClient {
    client: Client,
    profile: LlmProfile,
}

impl OpenAIClient {
    pub fn new(profile: LlmProfile) -> Self {
        Self {
            client: Client::new(),
            profile,
        }
    }

    /// Execute an API request with retry logic for rate limiting
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
            
            // Check if this is a rate limit error
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
            return Err(LlmError::Api(format!("OpenAI API error: {}", error_text)));
        }
    }
}

#[async_trait]
impl LlmClient for OpenAIClient {

    async fn send_message_stream(
        &self,
        messages: Vec<Message>,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LlmError>> + Send>>, LlmError> {
        let openai_messages: Vec<OpenAIMessage> = messages
            .into_iter()
            .map(|msg| {
                println!("🔍 DEBUG: Converting message to OpenAI: role={:?}, content={}, attachments={:?}", 
                    msg.role, msg.content, msg.attachments);
                
                // Handle attachments for multimodal content
                let content = if let Some(attachments) = &msg.attachments {
                    if !attachments.is_empty() {
                        // Create multimodal content with text and images
                        let mut content_parts = vec![
                            serde_json::json!({
                                "type": "text",
                                "text": msg.content
                            })
                        ];
                        
                        for attachment in attachments {
                            match attachment.mime_type.as_str() {
                                mime if mime.starts_with("image/") => {
                                    // For images, we need to read and encode them
                                    if let Some(content) = &attachment.content {
                                        content_parts.push(serde_json::json!({
                                            "type": "image_url",
                                            "image_url": {
                                                "url": format!("data:{};base64,{}", attachment.mime_type, content)
                                            }
                                        }));
                                    }
                                }
                                mime if mime.starts_with("text/") => {
                                    // For text files, include content in text
                                    if let Some(content) = &attachment.content {
                                        content_parts.push(serde_json::json!({
                                            "type": "text",
                                            "text": format!("File: {}\nContent:\n{}", attachment.file_name, content)
                                        }));
                                    }
                                }
                                _ => {
                                    // For other files, just mention them
                                    content_parts.push(serde_json::json!({
                                        "type": "text",
                                        "text": format!("File attached: {} ({} bytes)", attachment.file_name, attachment.file_size)
                                    }));
                                }
                            }
                        }
                        
                        serde_json::Value::Array(content_parts)
                    } else {
                        serde_json::Value::String(msg.content)
                    }
                } else {
                    serde_json::Value::String(msg.content)
                };
                
                OpenAIMessage {
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

        let request = OpenAIRequest {
            model: self.profile.model.clone(),
            messages: openai_messages,
            temperature: temperature.or(self.profile.temperature),
            max_tokens: max_tokens.or(self.profile.max_tokens),
            stream: true,
            tools: None,
            tool_choice: None,
        };

        let response = self.execute_with_retry(|| {
            Box::pin(
                self.client
                    .post(&self.profile.endpoint)
                    .header("Authorization", format!("Bearer {}", self.profile.api_key))
                    .header("Content-Type", "application/json")
                    .json(&request)
                    .send()
            )
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
                            if let Ok(stream_response) = serde_json::from_str::<OpenAIStreamResponse>(data) {
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
        let openai_messages: Vec<OpenAIMessage> = messages
            .into_iter()
            .map(|msg| {
                println!("🔍 DEBUG: Converting message to OpenAI (tools): role={:?}, content={}, attachments={:?}", 
                    msg.role, msg.content, msg.attachments);
                
                let tool_calls = if let Some(tool_calls) = msg.tool_calls {
                    Some(tool_calls.into_iter().map(|tc| OpenAIToolCall {
                        id: tc.id,
                        r#type: "function".to_string(),
                        function: OpenAIToolCallFunction {
                            name: tc.name,
                            arguments: serde_json::to_string(&tc.parameters).unwrap_or_else(|_| "{}".to_string()),
                        },
                    }).collect())
                } else {
                    None
                };
                
                // Handle attachments for multimodal content
                let content = if let Some(attachments) = &msg.attachments {
                    if !attachments.is_empty() {
                        // Create multimodal content with text and images
                        let mut content_parts = vec![
                            serde_json::json!({
                                "type": "text",
                                "text": msg.content
                            })
                        ];
                        
                        for attachment in attachments {
                            match attachment.mime_type.as_str() {
                                mime if mime.starts_with("image/") => {
                                    // For images, we need to read and encode them
                                    if let Some(content) = &attachment.content {
                                        content_parts.push(serde_json::json!({
                                            "type": "image_url",
                                            "image_url": {
                                                "url": format!("data:{};base64,{}", attachment.mime_type, content)
                                            }
                                        }));
                                    }
                                }
                                mime if mime.starts_with("text/") => {
                                    // For text files, include content in text
                                    if let Some(content) = &attachment.content {
                                        content_parts.push(serde_json::json!({
                                            "type": "text",
                                            "text": format!("File: {}\nContent:\n{}", attachment.file_name, content)
                                        }));
                                    }
                                }
                                _ => {
                                    // For other files, just mention them
                                    content_parts.push(serde_json::json!({
                                        "type": "text",
                                        "text": format!("File attached: {} ({} bytes)", attachment.file_name, attachment.file_size)
                                    }));
                                }
                            }
                        }
                        
                        serde_json::Value::Array(content_parts)
                    } else {
                        serde_json::Value::String(msg.content)
                    }
                } else {
                    serde_json::Value::String(msg.content)
                };
                
                OpenAIMessage {
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
            Some(available_tools.into_iter().map(|tool| OpenAITool {
                r#type: "function".to_string(),
                function: OpenAIToolFunction {
                    name: tool.name,
                    description: tool.description,
                    parameters: tool.parameters,
                },
            }).collect())
        };

        let request = OpenAIRequest {
            model: self.profile.model.clone(),
            messages: openai_messages,
            temperature: temperature.or(self.profile.temperature),
            max_tokens: max_tokens.or(self.profile.max_tokens),
            stream: false,
            tools,
            tool_choice: if has_tools { Some("auto".to_string()) } else { None },
        };

        let response = self.execute_with_retry(|| {
            Box::pin(
                self.client
                    .post(&self.profile.endpoint)
                    .header("Authorization", format!("Bearer {}", self.profile.api_key))
                    .header("Content-Type", "application/json")
                    .json(&request)
                    .send()
            )
        }).await?;

        let response_data: OpenAIResponse = response.json().await?;

        let choice = response_data
            .choices
            .first()
            .ok_or_else(|| LlmError::Api("No response from OpenAI".to_string()))?;

        let content = match choice.message.content.clone() {
            Some(serde_json::Value::String(s)) => s,
            Some(serde_json::Value::Array(parts)) => {
                // For multimodal content, extract text parts
                let mut text_parts = Vec::new();
                for part in parts {
                    if let serde_json::Value::Object(obj) = part {
                        if let Some(serde_json::Value::String(text)) = obj.get("text") {
                            text_parts.push(text.clone());
                        }
                    }
                }
                text_parts.join(" ")
            }
            _ => String::new(),
        };
        
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

