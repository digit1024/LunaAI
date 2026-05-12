use super::*;
use crate::config::ModelPreset;
use crate::llm::observability::{classify_reqwest_error, CallOutcome, LlmCallSpan};
use crate::llm::tokenizer::TokenCounter;
use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::pin::Pin;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing;

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
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

#[derive(Debug, Serialize, Deserialize, Clone)]
struct AnthropicImageSource {
    #[serde(rename = "type")]
    source_type: String,
    media_type: String,
    data: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum AnthropicContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { source: AnthropicImageSource },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicResponseBlock>,
    #[serde(default)]
    usage: Option<AnthropicUsage>,
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct AnthropicUsage {
    #[serde(default)]
    input_tokens: Option<u32>,
    #[serde(default)]
    output_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum AnthropicResponseBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
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
    preset: ModelPreset,
}

impl AnthropicClient {
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

    fn extract_request_id(resp: &reqwest::Response) -> Option<String> {
        resp.headers()
            .get("x-request-id")
            .or_else(|| resp.headers().get("request-id"))
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    }

    fn push_user_attachments(blocks: &mut Vec<AnthropicContentBlock>, attachments: Vec<Attachment>) {
        for attachment in attachments {
            match attachment.mime_type.as_str() {
                mime if mime.starts_with("image/") => {
                    if let Some(data) = attachment.content {
                        blocks.push(AnthropicContentBlock::Image {
                            source: AnthropicImageSource {
                                source_type: "base64".to_string(),
                                media_type: attachment.mime_type,
                                data,
                            },
                        });
                    }
                }
                mime if mime.starts_with("text/") => {
                    if let Some(content) = attachment.content {
                        blocks.push(AnthropicContentBlock::Text {
                            text: format!("File: {}\nContent:\n{}", attachment.file_name, content),
                        });
                    }
                }
                _ => {
                    blocks.push(AnthropicContentBlock::Text {
                        text: format!(
                            "File attached: {} ({} bytes)",
                            attachment.file_name, attachment.file_size
                        ),
                    });
                }
            }
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
        let mut span = LlmCallSpan::start("anthropic", self.preset.model.clone(), true);
        let context_tokens = Self::estimate_context_tokens(&messages);
        span.set_input_shape(messages.len(), 0, context_tokens);

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
                tracing::debug!(
                    role = ?m.role,
                    content_length = m.content.len(),
                    attachment_count = m.attachments.as_ref().map(|a| a.len()).unwrap_or(0),
                    "Converting message to Anthropic format"
                );
                
                let mut content_blocks = Vec::new();
                if !m.content.is_empty() {
                    content_blocks.push(AnthropicContentBlock::Text { text: m.content });
                }
                if let Some(attachments) = m.attachments {
                    Self::push_user_attachments(&mut content_blocks, attachments);
                }
                if content_blocks.is_empty() {
                    content_blocks.push(AnthropicContentBlock::Text {
                        text: String::new(),
                    });
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
            model: self.preset.model.clone(),
            messages: anthropic_messages,
            max_tokens: max_tokens.or(self.preset.max_tokens),
            temperature: temperature.or(self.preset.temperature),
            system: system_prompt,
            tools: None,
            tool_choice: None,
            stream: true,
        };

        let response = match self
            .client
            .post(&self.preset.endpoint)
            .header("x-api-key", &self.preset.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                let (outcome, kind) = classify_reqwest_error(&e);
                span.finish_error(outcome, kind, e.to_string());
                return Err(LlmError::Http(e));
            }
        };

        let status = response.status();
        let request_id = Self::extract_request_id(&response);
        span.set_response_headers(status.as_u16(), request_id);

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            let msg = format!("Anthropic API error (HTTP {}): {}", status.as_u16(), error_text);
            span.finish_error(CallOutcome::HttpError, format!("http_{}", status.as_u16()), msg.clone());
            return Err(LlmError::Api(msg));
        }

        // NOTE: This streaming path doesn't currently parse Anthropic's
        // `message_stop` / final `usage` event, so it can't observe
        // truncation as precisely as OpenAI. The shared client's
        // tcp_keepalive + http2 ping configuration ensures dead
        // connections at least surface as a transport error rather than
        // a silent close. Finishing the span on stream end is good
        // enough until we add full SSE event parsing here.
        span.finish_success();

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
                            if data == "[DONE]" {
                                continue;
                            }
                            // Try parse minimal delta structure
                            if let Ok(delta) = serde_json::from_str::<AnthropicSseDelta>(data) {
                                if let Some(d) = delta.delta {
                                    if let Some(t) = d.text {
                                        content.push_str(&t);
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
        let mut span = LlmCallSpan::start("anthropic", self.preset.model.clone(), false);
        let context_tokens = Self::estimate_context_tokens(&messages);
        span.set_input_shape(messages.len(), available_tools.len(), context_tokens);

        // Extract first system prompt if present
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

        let mut anthropic_messages: Vec<AnthropicMessage> = Vec::new();
        for m in user_assistant.into_iter() {
            match m.role {
                Role::User => {
                    tracing::debug!(
                        role = ?m.role,
                        content_length = m.content.len(),
                        attachment_count = m.attachments.as_ref().map(|a| a.len()).unwrap_or(0),
                        "Converting message to Anthropic format (tools)"
                    );

                    let mut content_blocks = Vec::new();
                    if !m.content.is_empty() {
                        content_blocks.push(AnthropicContentBlock::Text { text: m.content });
                    }
                    if let Some(attachments) = m.attachments {
                        Self::push_user_attachments(&mut content_blocks, attachments);
                    }
                    if content_blocks.is_empty() {
                        content_blocks.push(AnthropicContentBlock::Text {
                            text: String::new(),
                        });
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
                            content_blocks.push(AnthropicContentBlock::ToolUse {
                                id: tc.id,
                                name: tc.name,
                                input: tc.parameters,
                            });
                        }
                    }
                    if content_blocks.is_empty() {
                        content_blocks.push(AnthropicContentBlock::Text {
                            text: String::new(),
                        });
                    }
                    anthropic_messages.push(AnthropicMessage {
                        role: "assistant".to_string(),
                        content: content_blocks,
                    });
                }
                Role::Tool => {
                    // Convert tool result message into a user message with a tool_result block
                    let is_error = m.content.starts_with("Error: ");
                    let content_text = if is_error {
                        Some(m.content.trim_start_matches("Error: ").to_string())
                    } else {
                        Some(m.content.clone())
                    };
                    let tool_use_id = m
                        .tool_call_id
                        .unwrap_or_else(|| "unknown_tool_use".to_string());
                    anthropic_messages.push(AnthropicMessage {
                        role: "user".to_string(),
                        content: vec![AnthropicContentBlock::ToolResult {
                            tool_use_id,
                            content: content_text,
                            is_error: Some(is_error),
                        }],
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
            Some(
                available_tools
                    .into_iter()
                    .map(|t| {
                        let mut schema = t.parameters;
                        if !schema.is_object() {
                            schema = serde_json::json!({"type":"object","properties":{}});
                        }
                        AnthropicToolDefinition {
                            name: t.name,
                            description: t.description,
                            input_schema: schema,
                        }
                    })
                    .collect(),
            )
        };

        let request = AnthropicRequest {
            model: self.preset.model.clone(),
            messages: anthropic_messages,
            max_tokens: max_tokens.or(self.preset.max_tokens),
            temperature: temperature.or(self.preset.temperature),
            system: system_prompt,
            tools,
            tool_choice: if has_tools {
                Some(json!({"type": "auto"}))
            } else {
                None
            },
            stream: false,
        };

        let response = match self
            .client
            .post(&self.preset.endpoint)
            .header("x-api-key", &self.preset.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                let (outcome, kind) = classify_reqwest_error(&e);
                span.finish_error(outcome, kind, e.to_string());
                return Err(LlmError::Http(e));
            }
        };

        let status = response.status();
        let request_id = Self::extract_request_id(&response);
        span.set_response_headers(status.as_u16(), request_id);

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            let msg = format!("Anthropic API error (HTTP {}): {}", status.as_u16(), error_text);
            span.finish_error(CallOutcome::HttpError, format!("http_{}", status.as_u16()), msg.clone());
            return Err(LlmError::Api(msg));
        }

        let response_data: AnthropicResponse = match response.json().await {
            Ok(r) => r,
            Err(e) => {
                let (outcome, kind) = classify_reqwest_error(&e);
                span.finish_error(outcome, kind, e.to_string());
                return Err(LlmError::Http(e));
            }
        };

        if let Some(u) = response_data.usage.as_ref() {
            let total = match (u.input_tokens, u.output_tokens) {
                (Some(a), Some(b)) => Some(a + b),
                _ => None,
            };
            span.set_usage(u.input_tokens, u.output_tokens, total);
        }
        if let Some(reason) = response_data.stop_reason {
            span.set_finish_reason(reason);
        }

        let mut content = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        for block in response_data.content.into_iter() {
            match block {
                AnthropicResponseBlock::Text { text } => content.push_str(&text),
                AnthropicResponseBlock::ToolUse { id, name, input } => {
                    span.observe_tool_call();
                    tool_calls.push(ToolCall {
                        id,
                        name,
                        parameters: input,
                    });
                }
            }
        }

        span.finish_success();
        Ok(ChatResponse {
            content,
            tool_calls,
            reasoning_content: None,
        })
    }

    async fn send_message_stream_with_tools(
        &self,
        messages: Vec<Message>,
        available_tools: Vec<ToolDefinition>,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<ChatStreamEvent, LlmError>> + Send>>, LlmError>
    {
        // Use non-streaming execution for now and map to stream events
        let response = self
            .send_message_with_tools(messages, available_tools, temperature, max_tokens)
            .await?;

        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            if !response.content.is_empty() {
                let _ = tx.send(Ok(ChatStreamEvent::ContentDelta(response.content)));
            }
            for tool_call in response.tool_calls {
                let _ = tx.send(Ok(ChatStreamEvent::ToolCallDelta(tool_call)));
            }
        });

        Ok(Box::pin(UnboundedReceiverStream::new(rx)))
    }
}
