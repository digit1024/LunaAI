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
    system: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
    stream: bool,
}

/// Anthropic prompt-cache breakpoint marker (`{"type":"ephemeral"}`).
#[derive(Debug, Serialize, Deserialize, Clone)]
struct CacheControl {
    #[serde(rename = "type")]
    kind: String,
}

impl CacheControl {
    fn ephemeral() -> Self {
        Self { kind: "ephemeral".to_string() }
    }
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
    Text {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    #[serde(rename = "image")]
    Image {
        source: AnthropicImageSource,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
}

impl AnthropicContentBlock {
    fn set_cache_control(&mut self, cc: CacheControl) {
        match self {
            AnthropicContentBlock::Text { cache_control, .. }
            | AnthropicContentBlock::Image { cache_control, .. }
            | AnthropicContentBlock::ToolUse { cache_control, .. }
            | AnthropicContentBlock::ToolResult { cache_control, .. } => *cache_control = Some(cc),
        }
    }
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
                    if let Some(description) = &attachment.description {
                        blocks.push(AnthropicContentBlock::Text {
                            text: format!("[Image: {}]\n{}", attachment.file_name, description),
                            cache_control: None,
                        });
                    } else if let Some(data) = attachment.content {
                        blocks.push(AnthropicContentBlock::Image {
                            source: AnthropicImageSource {
                                source_type: "base64".to_string(),
                                media_type: attachment.mime_type,
                                data,
                            },
                            cache_control: None,
                        });
                    }
                }
                mime if mime.starts_with("text/") => {
                    if let Some(content) = attachment.content {
                        blocks.push(AnthropicContentBlock::Text {
                            text: format!("File: {}\nContent:\n{}", attachment.file_name, content),
                            cache_control: None,
                        });
                    }
                }
                _ => {
                    blocks.push(AnthropicContentBlock::Text {
                        text: format!(
                            "File attached: {} ({} bytes)",
                            attachment.file_name, attachment.file_size
                        ),
                        cache_control: None,
                    });
                }
            }
        }
    }
}

/// Split messages into the single Anthropic `system` slot and the turn list.
///
/// Anthropic exposes exactly one system prompt; every `System` message after
/// the first is discarded (the provider has no per-message system role). This
/// is why compaction summaries must never be stored as `system` — they would
/// be silently dropped here.
fn split_system_and_turns(messages: Vec<Message>) -> (Option<String>, Vec<Message>) {
    let mut system_prompt: Option<String> = None;
    let mut turns: Vec<Message> = Vec::new();
    for msg in messages {
        match msg.role {
            Role::System => {
                if system_prompt.is_none() {
                    system_prompt = Some(msg.content);
                }
            }
            _ => turns.push(msg),
        }
    }
    (system_prompt, turns)
}

/// Anthropic allows up to 4 cache breakpoints. "system_and_3": one on the system
/// prompt plus one on the last block of each of the last 3 messages, so a rolling
/// window re-establishes cache hits every turn (~75% input-cost savings on
/// multi-turn conversations).
const CACHE_BREAKPOINT_MESSAGES: usize = 3;

fn apply_prompt_caching(messages: &mut [AnthropicMessage]) {
    let start = messages.len().saturating_sub(CACHE_BREAKPOINT_MESSAGES);
    for msg in messages.iter_mut().skip(start) {
        if let Some(last) = msg.content.last_mut() {
            last.set_cache_control(CacheControl::ephemeral());
        }
    }
}

/// The system prompt as a single cached text block array.
fn cached_system_prompt(text: String) -> serde_json::Value {
    json!([{
        "type": "text",
        "text": text,
        "cache_control": CacheControl::ephemeral(),
    }])
}

#[async_trait]
impl LlmClient for AnthropicClient {
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
        let (system_prompt, user_assistant) = split_system_and_turns(messages);

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
                        content_blocks.push(AnthropicContentBlock::Text {
                            text: m.content,
                            cache_control: None,
                        });
                    }
                    if let Some(attachments) = m.attachments {
                        Self::push_user_attachments(&mut content_blocks, attachments);
                    }
                    if content_blocks.is_empty() {
                        content_blocks.push(AnthropicContentBlock::Text {
                            text: String::new(),
                            cache_control: None,
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
                        content_blocks.push(AnthropicContentBlock::Text {
                            text: m.content,
                            cache_control: None,
                        });
                    }
                    if let Some(tool_calls) = m.tool_calls.clone() {
                        for tc in tool_calls.into_iter() {
                            content_blocks.push(AnthropicContentBlock::ToolUse {
                                id: tc.id,
                                name: tc.name,
                                input: tc.parameters,
                                cache_control: None,
                            });
                        }
                    }
                    if content_blocks.is_empty() {
                        content_blocks.push(AnthropicContentBlock::Text {
                            text: String::new(),
                            cache_control: None,
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
                            cache_control: None,
                        }],
                    });
                }
                Role::System => {
                    // already extracted above; ignore
                }
            }
        }

        // Rolling cache breakpoints on the tail of the transcript.
        apply_prompt_caching(&mut anthropic_messages);

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
            system: system_prompt.map(cached_system_prompt),
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
                let _ = tx.send(Ok(ChatStreamEvent::Content(response.content)));
            }
            for tool_call in response.tool_calls {
                let _ = tx.send(Ok(ChatStreamEvent::ToolCall(tool_call)));
            }
        });

        Ok(Box::pin(UnboundedReceiverStream::new(rx)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::compaction::SUMMARY_MARKER;

    #[test]
    fn summary_survives_as_assistant_turn_not_dropped() {
        // Real system prompt first, then a compaction summary (assistant) and a
        // user turn. Only ONE system slot exists; the summary must land in the
        // turn list, not be discarded with the "extra" system messages.
        let messages = vec![
            Message::new(Role::System, "You are helpful.".into()),
            Message::new(
                Role::Assistant,
                format!("{}\n\n## Goal\nFix the bug", SUMMARY_MARKER),
            ),
            Message::new(Role::User, "continue".into()),
        ];

        let (system, turns) = split_system_and_turns(messages);

        assert_eq!(system.as_deref(), Some("You are helpful."));
        assert_eq!(turns.len(), 2, "summary + user must both survive");
        assert_eq!(turns[0].role, Role::Assistant);
        assert!(turns[0].content.contains(SUMMARY_MARKER));
        assert_eq!(turns[1].role, Role::User);
    }

    #[test]
    fn legacy_system_summary_after_prompt_would_be_dropped() {
        // Documents the old failure mode: a SECOND system message is discarded.
        // Compaction must therefore never store summaries as `system`.
        let messages = vec![
            Message::new(Role::System, "You are helpful.".into()),
            Message::new(Role::System, "[Previous conversation summary]: ...".into()),
            Message::new(Role::User, "continue".into()),
        ];

        let (system, turns) = split_system_and_turns(messages);

        assert_eq!(system.as_deref(), Some("You are helpful."));
        assert_eq!(turns.len(), 1, "the second system summary is dropped");
        assert_eq!(turns[0].role, Role::User);
    }

    #[test]
    fn prompt_caching_marks_only_the_last_three_messages() {
        let mut msgs: Vec<AnthropicMessage> = (0..5)
            .map(|i| AnthropicMessage {
                role: if i % 2 == 0 { "user".into() } else { "assistant".into() },
                content: vec![AnthropicContentBlock::Text {
                    text: format!("m{i}"),
                    cache_control: None,
                }],
            })
            .collect();

        apply_prompt_caching(&mut msgs);

        for m in &msgs[..2] {
            assert!(matches!(
                &m.content[0],
                AnthropicContentBlock::Text { cache_control: None, .. }
            ));
        }
        for m in &msgs[2..] {
            assert!(matches!(
                &m.content[0],
                AnthropicContentBlock::Text { cache_control: Some(_), .. }
            ));
        }
    }

    #[test]
    fn cached_system_prompt_is_a_marked_text_block() {
        let v = cached_system_prompt("system text".into());
        assert_eq!(v[0]["type"], "text");
        assert_eq!(v[0]["text"], "system text");
        assert_eq!(v[0]["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn image_with_description_renders_as_text_not_image() {
        let attachment = Attachment {
            file_path: "/tmp/x.png".to_string(),
            file_name: "x.png".to_string(),
            mime_type: "image/png".to_string(),
            file_size: 10,
            content: Some("QkFTRTY0".to_string()),
            description: Some("A red square with the word STOP".to_string()),
        };
        let mut blocks = Vec::new();
        AnthropicClient::push_user_attachments(&mut blocks, vec![attachment]);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            AnthropicContentBlock::Text { text, .. } => {
                assert!(text.contains("A red square"), "{text}");
            }
            other => panic!("expected a text block, got {other:?}"),
        }
    }
}
