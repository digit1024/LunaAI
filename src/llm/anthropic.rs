use super::*;
use crate::config::LlmProfile;
use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::pin::Pin;

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
    stream: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct AnthropicMessage {
    role: String,
    content: Vec<AnthropicContentBlock>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum AnthropicContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse { id: String, name: String, input: serde_json::Value },
    #[serde(rename = "tool_result")]
    ToolResult { tool_use_id: String, #[serde(skip_serializing_if = "Option::is_none")] content: Option<String>, #[serde(skip_serializing_if = "Option::is_none")] is_error: Option<bool> },
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicResponseBlock>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum AnthropicResponseBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse { id: String, name: String, input: serde_json::Value },
}

// Streaming event minimal structs (we only care about text deltas)
#[derive(Debug, Deserialize)]
struct AnthropicSseDelta {
    delta: Option<AnthropicDelta>,
}

#[derive(Debug, Deserialize)]
struct AnthropicDelta {
    text: Option<String>,
}

#[derive(Debug, Serialize)]
struct AnthropicToolDefinition {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

pub struct AnthropicClient {
    client: Client,
    profile: LlmProfile,
}

impl AnthropicClient {
    pub fn new(profile: LlmProfile) -> Self {
        Self {
            client: Client::new(),
            profile,
        }
    }
}

#[async_trait]
impl LlmClient for AnthropicClient {

    async fn send_message_stream(
        &self,
        messages: Vec<Message>,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LlmError>> + Send>>, LlmError> {
        // Extract first system prompt if present; Anthropic expects it separately
        let mut system_prompt: Option<String> = None;
        let mut user_assistant: Vec<Message> = Vec::new();
        for msg in messages {
            match msg.role {
                Role::System => {
                    if system_prompt.is_none() {
                        system_prompt = Some(msg.content);
                    }
                }
                _ => user_assistant.push(msg),
            }
        }

        let anthropic_messages: Vec<AnthropicMessage> = user_assistant
            .into_iter()
            .map(|m| {
                println!("🔍 DEBUG: Converting message to Anthropic: role={:?}, content={}, attachments={:?}", 
                    m.role, m.content, m.attachments);
                
                let mut content_blocks = vec![AnthropicContentBlock::Text { text: m.content }];
                
                // Handle attachments
                if let Some(attachments) = m.attachments {
                    for attachment in attachments {
                        match attachment.mime_type.as_str() {
                            mime if mime.starts_with("image/") => {
                                // For images, we need to read and encode them
                                if let Some(content) = &attachment.content {
                                    content_blocks.push(AnthropicContentBlock::Text { 
                                        text: format!("[Image: {} - {} bytes]", attachment.file_name, attachment.file_size)
                                    });
                                }
                            }
                            mime if mime.starts_with("text/") => {
                                // For text files, include content in text
                                if let Some(content) = &attachment.content {
                                    content_blocks.push(AnthropicContentBlock::Text { 
                                        text: format!("File: {}\nContent:\n{}", attachment.file_name, content)
                                    });
                                }
                            }
                            _ => {
                                // For other files, just mention them
                                content_blocks.push(AnthropicContentBlock::Text { 
                                    text: format!("File attached: {} ({} bytes)", attachment.file_name, attachment.file_size)
                                });
                            }
                        }
                    }
                }
                
                AnthropicMessage {
                    role: match m.role {
                        Role::User => "user".to_string(),
                        Role::Assistant => "assistant".to_string(),
                        Role::System => "user".to_string(),
                        Role::Tool => "user".to_string(),
                    },
                    content: content_blocks,
                }
            })
            .collect();

        let request = AnthropicRequest {
            model: self.profile.model.clone(),
            messages: anthropic_messages,
            max_tokens: max_tokens.or(self.profile.max_tokens).unwrap_or(1000),
            temperature: temperature.or(self.profile.temperature),
            system: system_prompt,
            tools: None,
            tool_choice: None,
            stream: true,
        };

        let response = self
            .client
            .post(&self.profile.endpoint)
            .header("x-api-key", &self.profile.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(LlmError::Api(format!("Anthropic API error: {}", error_text)));
        }

        let stream = response.bytes_stream();
        let stream = futures::StreamExt::map(stream, |chunk_result| {
            chunk_result
                .map_err(|e| LlmError::Http(e))
                .and_then(|chunk| {
                    let chunk_str = String::from_utf8(chunk.to_vec())
                        .map_err(|e| LlmError::Api(format!("Invalid UTF-8: {}", e)))?;

                    // SSE format: lines beginning with "data: "
                    let mut content = String::new();
                    for line in chunk_str.lines() {
                        if let Some(data) = line.strip_prefix("data: ") {
                            if data == "[DONE]" { continue; }
                            // Try parse minimal delta structure
                            if let Ok(delta) = serde_json::from_str::<AnthropicSseDelta>(data) {
                                if let Some(d) = delta.delta {
                                    if let Some(t) = d.text { content.push_str(&t); }
                                }
                            }
                        }
                    }

                    if content.is_empty() { Ok(None) } else { Ok(Some(content)) }
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
        // Extract first system prompt if present
        let mut system_prompt: Option<String> = None;
        let mut user_assistant: Vec<Message> = Vec::new();
        for msg in messages {
            match msg.role {
                Role::System => {
                    if system_prompt.is_none() { system_prompt = Some(msg.content); }
                }
                _ => user_assistant.push(msg),
            }
        }

        let mut anthropic_messages: Vec<AnthropicMessage> = Vec::new();
        for m in user_assistant.into_iter() {
            match m.role {
                Role::User => {
                    println!("🔍 DEBUG: Converting message to Anthropic (tools): role={:?}, content={}, attachments={:?}", 
                        m.role, m.content, m.attachments);
                    
                    let mut content_blocks = vec![AnthropicContentBlock::Text { text: m.content }];
                    
                    // Handle attachments
                    if let Some(attachments) = m.attachments {
                        for attachment in attachments {
                            match attachment.mime_type.as_str() {
                                mime if mime.starts_with("image/") => {
                                    // For images, we need to read and encode them
                                    if let Some(content) = &attachment.content {
                                        content_blocks.push(AnthropicContentBlock::Text { 
                                            text: format!("[Image: {} - {} bytes]", attachment.file_name, attachment.file_size)
                                        });
                                    }
                                }
                                mime if mime.starts_with("text/") => {
                                    // For text files, include content in text
                                    if let Some(content) = &attachment.content {
                                        content_blocks.push(AnthropicContentBlock::Text { 
                                            text: format!("File: {}\nContent:\n{}", attachment.file_name, content)
                                        });
                                    }
                                }
                                _ => {
                                    // For other files, just mention them
                                    content_blocks.push(AnthropicContentBlock::Text { 
                                        text: format!("File attached: {} ({} bytes)", attachment.file_name, attachment.file_size)
                                    });
                                }
                            }
                        }
                    }
                    
                    anthropic_messages.push(AnthropicMessage {
                        role: "user".to_string(),
                        content: content_blocks,
                    });
                }
                Role::Assistant => {
                    let mut content_blocks: Vec<AnthropicContentBlock> = Vec::new();
                    if !m.content.is_empty() {
                        content_blocks.push(AnthropicContentBlock::Text { text: m.content });
                    }
                    if let Some(tool_calls) = m.tool_calls.clone() {
                        for tc in tool_calls.into_iter() {
                            content_blocks.push(AnthropicContentBlock::ToolUse { id: tc.id, name: tc.name, input: tc.parameters });
                        }
                    }
                    if content_blocks.is_empty() {
                        content_blocks.push(AnthropicContentBlock::Text { text: String::new() });
                    }
                    anthropic_messages.push(AnthropicMessage { role: "assistant".to_string(), content: content_blocks });
                }
                Role::Tool => {
                    // Convert tool result message into a user message with a tool_result block
                    let is_error = m.content.starts_with("Error: ");
                    let content_text = if is_error { Some(m.content.trim_start_matches("Error: ").to_string()) } else { Some(m.content.clone()) };
                    let tool_use_id = m.tool_call_id.unwrap_or_else(|| "unknown_tool_use".to_string());
                    anthropic_messages.push(AnthropicMessage {
                        role: "user".to_string(),
                        content: vec![AnthropicContentBlock::ToolResult { tool_use_id, content: content_text, is_error: Some(is_error) }],
                    });
                }
                Role::System => {
                    // already extracted above; ignore
                }
            }
        }

        let has_tools = !available_tools.is_empty();
        let tools = if !has_tools {
            None
        } else {
            Some(available_tools.into_iter().map(|t| {
                let mut schema = t.parameters;
                if !schema.is_object() {
                    schema = serde_json::json!({"type":"object","properties":{}});
                }
                AnthropicToolDefinition { name: t.name, description: t.description, input_schema: schema }
            }).collect())
        };

        let request = AnthropicRequest {
            model: self.profile.model.clone(),
            messages: anthropic_messages,
            max_tokens: max_tokens.or(self.profile.max_tokens).unwrap_or(1000),
            temperature: temperature.or(self.profile.temperature),
            system: system_prompt,
            tools,
            tool_choice: if has_tools { Some(json!({"type": "auto"})) } else { None },
            stream: false,
        };

        let response = self
            .client
            .post(&self.profile.endpoint)
            .header("x-api-key", &self.profile.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(LlmError::Api(format!("Anthropic API error: {}", error_text)));
        }

        let response_data: AnthropicResponse = response.json().await?;
        let mut content = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        for block in response_data.content.into_iter() {
            match block {
                AnthropicResponseBlock::Text { text } => content.push_str(&text),
                AnthropicResponseBlock::ToolUse { id, name, input } => {
                    tool_calls.push(ToolCall { id, name, parameters: input });
                }
            }
        }

        Ok(ChatResponse { content, tool_calls })
    }
}

