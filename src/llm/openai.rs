use super::*;
use crate::config::ModelPreset;
use crate::llm::observability::{classify_reqwest_error, CallOutcome, LlmCallSpan};
use crate::llm::tokenizer::TokenCounter;
use futures::{Stream, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

#[derive(Debug, Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAITool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
    /// Per OpenAI: when set on a streaming request, the server appends a
    /// final SSE chunk containing `usage`. DeepSeek implements this; we
    /// use it as our authoritative "stream finished" signal in addition
    /// to `[DONE]`.
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<OpenAIStreamOptions>,
}

#[derive(Debug, Serialize)]
struct OpenAIStreamOptions {
    include_usage: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAIMessage {
    role: String,
    content: Option<serde_json::Value>,
    tool_calls: Option<Vec<OpenAIToolCall>>,
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_content: Option<String>,
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
struct OpenAIToolCallDelta {
    index: Option<usize>,
    id: Option<String>,
    r#type: Option<String>,
    function: Option<OpenAIToolCallFunctionDelta>,
}

#[derive(Debug, Deserialize)]
struct OpenAIToolCallFunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    choices: Vec<OpenAIChoice>,
    #[serde(default)]
    usage: Option<OpenAIUsage>,
    /// Server-reported routed model (sometimes differs from what we asked
    /// for; not surfaced today but useful for debugging via raw bodies).
    #[serde(default)]
    #[allow(dead_code)]
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    message: OpenAIMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct OpenAIUsage {
    #[serde(default)]
    prompt_tokens: Option<u32>,
    #[serde(default)]
    completion_tokens: Option<u32>,
    #[serde(default)]
    total_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamResponse {
    #[serde(default)]
    choices: Vec<OpenAIStreamChoice>,
    /// Present only on the final `[DONE]`-adjacent chunk when
    /// `stream_options.include_usage = true`.
    #[serde(default)]
    usage: Option<OpenAIUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamChoice {
    delta: OpenAIDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Default)]
struct StreamedToolCallState {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

impl StreamedToolCallState {
    fn try_into_tool_call(&self) -> Option<ToolCall> {
        let id = self.id.as_ref()?;
        let name = self.name.as_ref()?;
        let params: serde_json::Value = serde_json::from_str(&self.arguments).ok()?;
        // Generate UUID if provider returns empty ID (e.g., DeepSeek)
        let final_id = if id.is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            id.clone()
        };
        Some(ToolCall {
            id: final_id,
            name: name.clone(),
            parameters: params,
        })
    }
}

#[derive(Debug, Deserialize)]
struct OpenAIDelta {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAIToolCallDelta>>,
    /// DeepSeek/OpenAI use `reasoning_content`; OpenRouter/some providers use `reasoning`
    #[serde(alias = "reasoning")]
    reasoning_content: Option<String>,
}

pub struct OpenAIClient {
    client: Client,
    preset: ModelPreset,
}

impl OpenAIClient {
    pub fn new(preset: ModelPreset) -> Self {
        Self {
            client: super::observability::shared_http_client(),
            preset,
        }
    }

    fn estimate_context_tokens(messages: &[Message]) -> Option<usize> {
        // Use the existing cl100k counter; this is a rough estimate good
        // enough for "how big was the context we sent" telemetry.
        let counter = TokenCounter::cl100k();
        let mut total = 0usize;
        for m in messages {
            total = total.saturating_add(counter.count_message_tokens(m));
        }
        Some(total)
    }

    fn map_messages(messages: Vec<Message>) -> Vec<OpenAIMessage> {
        messages
            .into_iter()
            .map(|msg| {
                tracing::debug!(
                    role = ?msg.role,
                    content_length = msg.content.len(),
                    attachment_count = msg.attachments.as_ref().map(|a| a.len()).unwrap_or(0),
                    "Converting message to OpenAI format"
                );

                // Some models require reasoning_content present on assistant messages with tool_calls; compute before moving msg.tool_calls
                let reasoning_content = match (&msg.role, msg.tool_calls.as_ref(), &msg.reasoning_content) {
                    (Role::Assistant, Some(_), None) => Some(String::new()),
                    _ => msg.reasoning_content.clone(),
                };

                let tool_calls = msg.tool_calls.map(|tool_calls| {
                    tool_calls
                        .into_iter()
                        .map(|tc| OpenAIToolCall {
                            id: tc.id,
                            r#type: "function".to_string(),
                            function: OpenAIToolCallFunction {
                                name: tc.name,
                                arguments: serde_json::to_string(&tc.parameters)
                                    .unwrap_or_else(|_| "{}".to_string()),
                            },
                        })
                        .collect()
                });

                let content = if let Some(attachments) = &msg.attachments {
                    if !attachments.is_empty() {
                        let mut content_parts = vec![serde_json::json!({
                            "type": "text",
                            "text": msg.content
                        })];

                        for attachment in attachments {
                            match attachment.mime_type.as_str() {
                                mime if mime.starts_with("image/") => {
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
                                    if let Some(content) = &attachment.content {
                                        content_parts.push(serde_json::json!({
                                            "type": "text",
                                            "text": format!("File: {}\nContent:\n{}", attachment.file_name, content)
                                        }));
                                    }
                                }
                                _ => {
                                    content_parts.push(serde_json::json!({
                                        "type": "text",
                                        "text": format!("File attached: {} ({} bytes)", attachment.file_name, attachment.file_size)
                                    }));
                                }
                            }
                        }

                        Some(serde_json::Value::Array(content_parts))
                    } else {
                        Some(serde_json::Value::String(msg.content))
                    }
                } else {
                    Some(serde_json::Value::String(msg.content))
                };

                OpenAIMessage {
                    role: match msg.role {
                        Role::User => "user".to_string(),
                        Role::Assistant => "assistant".to_string(),
                        Role::System => "system".to_string(),
                        Role::Tool => "tool".to_string(),
                    },
                    content,
                    tool_calls,
                    tool_call_id: msg.tool_call_id,
                    reasoning_content,
                }
            })
            .collect()
    }

    /// Parse one HTTP chunk worth of SSE bytes. Pulls deltas, tool calls,
    /// `finish_reason`, terminal usage, and tracks whether `[DONE]` was
    /// seen so the caller can detect mid-stream provider drops.
    fn parse_stream_chunk(
        chunk: &str,
        tool_states: &mut HashMap<usize, StreamedToolCallState>,
        finish_reason: &mut Option<String>,
        usage: &mut Option<OpenAIUsage>,
        saw_done: &mut bool,
    ) -> Vec<ChatStreamEvent> {
        let mut events = Vec::new();
        for line in chunk.lines() {
            if !line.starts_with("data: ") {
                continue;
            }
            let payload = line[6..].trim();
            if payload.is_empty() {
                continue;
            }
            if payload == "[DONE]" {
                *saw_done = true;
                continue;
            }
            match serde_json::from_str::<OpenAIStreamResponse>(payload) {
                Ok(parsed) => {
                    if let Some(u) = parsed.usage {
                        *usage = Some(u);
                    }
                    for choice in parsed.choices {
                        if let Some(reason) = choice.finish_reason.clone() {
                            *finish_reason = Some(reason);
                        }
                        events.extend(Self::extract_choice_events(choice, tool_states));
                    }
                }
                Err(err) => {
                    tracing::warn!("Failed to parse OpenAI stream payload: {}", err);
                }
            }
        }
        events
    }

    fn extract_choice_events(
        choice: OpenAIStreamChoice,
        tool_states: &mut HashMap<usize, StreamedToolCallState>,
    ) -> Vec<ChatStreamEvent> {
        let mut events = Vec::new();

        if let Some(content_delta) = choice.delta.content {
            if !content_delta.is_empty() {
                events.push(ChatStreamEvent::ContentDelta(content_delta));
            }
        }

        if let Some(reasoning_delta) = choice.delta.reasoning_content {
            if !reasoning_delta.is_empty() {
                events.push(ChatStreamEvent::ReasoningContentDelta(reasoning_delta));
            }
        }

        if let Some(tool_deltas) = choice.delta.tool_calls {
            for delta in tool_deltas {
                let idx = delta.index.unwrap_or(0);
                let state = tool_states.entry(idx).or_default();
                if let Some(id) = delta.id {
                    state.id = Some(id);
                }
                if let Some(function) = delta.function {
                    if let Some(name) = function.name {
                        state.name = Some(name);
                    }
                    if let Some(arguments) = function.arguments {
                        state.arguments.push_str(&arguments);
                    }
                }
                if let Some(tool_call) = state.try_into_tool_call() {
                    events.push(ChatStreamEvent::ToolCallDelta(tool_call));
                    tool_states.remove(&idx);
                }
            }
        }
        events
    }

    fn extract_request_id(resp: &reqwest::Response) -> Option<String> {
        resp.headers()
            .get("x-request-id")
            .or_else(|| resp.headers().get("openai-request-id"))
            .or_else(|| resp.headers().get("x-amzn-RequestId"))
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    }
}

/// OpenAI allows max_tokens in [1, 65536]. Return None for 0 or out-of-range so we omit the field.
fn sanitize_max_tokens(v: Option<u32>) -> Option<u32> {
    let n = v?;
    if n == 0 || n > 65536 {
        tracing::warn!(max_tokens = n, "OpenAI max_tokens must be in [1, 65536]; omitting invalid value");
        return None;
    }
    Some(n)
}

#[async_trait]
impl LlmClient for OpenAIClient {
    async fn send_message_stream(
        &self,
        messages: Vec<Message>,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LlmError>> + Send>>, LlmError> {
        let mut span = LlmCallSpan::start("openai", self.preset.model.clone(), true);
        let messages_in = messages.len();
        let context_tokens = Self::estimate_context_tokens(&messages);
        span.set_input_shape(messages_in, 0, context_tokens);

        let openai_messages = Self::map_messages(messages);
        let max_tokens = sanitize_max_tokens(max_tokens.or(self.preset.max_tokens));

        let request = OpenAIRequest {
            model: self.preset.model.clone(),
            messages: openai_messages,
            temperature: temperature.or(self.preset.temperature),
            max_tokens,
            stream: true,
            tools: None,
            tool_choice: None,
            stream_options: Some(OpenAIStreamOptions { include_usage: true }),
        };

        if let Ok(payload) = serde_json::to_string(&request) {
            tracing::debug!(target: "llm.body", payload = %payload, "OpenAI stream request");
        }

        let response = match self
            .client
            .post(&self.preset.endpoint)
            .header("Authorization", format!("Bearer {}", self.preset.api_key))
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
            let msg = format!("OpenAI API error (HTTP {}): {}", status.as_u16(), error_text);
            span.finish_error(CallOutcome::HttpError, format!("http_{}", status.as_u16()), msg.clone());
            return Err(LlmError::Api(msg));
        }

        // Stream into a channel so we can run truncation detection out-of-band.
        let mut bytes_stream = response.bytes_stream();
        let (tx, rx) = mpsc::unbounded_channel::<Result<String, LlmError>>();

        tokio::spawn(async move {
            let mut saw_done = false;
            let mut bytes_in: usize = 0;
            let mut last_event = Instant::now();
            let mut finish_reason: Option<String> = None;
            let mut usage: Option<OpenAIUsage> = None;
            let mut tool_states: HashMap<usize, StreamedToolCallState> = HashMap::new();

            while let Some(chunk_result) = bytes_stream.next().await {
                match chunk_result {
                    Ok(chunk) => {
                        bytes_in += chunk.len();
                        span.observe_bytes(chunk.len());
                        let chunk_str = match String::from_utf8(chunk.to_vec()) {
                            Ok(s) => s,
                            Err(e) => {
                                let msg = format!("Invalid UTF-8 in stream: {}", e);
                                span.finish_error(CallOutcome::Parse, "utf8", msg.clone());
                                let _ = tx.send(Err(LlmError::Api(msg)));
                                return;
                            }
                        };

                        let mut content = String::new();
                        let events = OpenAIClient::parse_stream_chunk(
                            &chunk_str,
                            &mut tool_states,
                            &mut finish_reason,
                            &mut usage,
                            &mut saw_done,
                        );
                        for ev in events {
                            if let ChatStreamEvent::ContentDelta(c) = ev {
                                content.push_str(&c);
                            }
                        }
                        if !content.is_empty() {
                            last_event = Instant::now();
                            if tx.send(Ok(content)).is_err() {
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        let (outcome, kind) = classify_reqwest_error(&e);
                        span.finish_error(outcome, kind, e.to_string());
                        let _ = tx.send(Err(LlmError::Http(e)));
                        return;
                    }
                }
            }

            // Stream ended. Was it a clean end?
            if !saw_done && usage.is_none() {
                let age = last_event.elapsed().as_millis();
                let reason = format!(
                    "no [DONE] sentinel and no terminal usage frame received after {} bytes",
                    bytes_in
                );
                span.finish_error(
                    CallOutcome::StreamTruncated,
                    "stream_truncated",
                    reason.clone(),
                );
                let _ = tx.send(Err(LlmError::StreamTruncated {
                    bytes_read: bytes_in,
                    last_event_age_ms: age,
                    reason,
                }));
                return;
            }

            if let Some(u) = usage.as_ref() {
                span.set_usage(u.prompt_tokens, u.completion_tokens, u.total_tokens);
            }
            if let Some(reason) = finish_reason {
                span.set_finish_reason(reason);
            }
            span.finish_success();
        });

        Ok(Box::pin(UnboundedReceiverStream::new(rx)))
    }

    async fn send_message_with_tools(
        &self,
        messages: Vec<Message>,
        available_tools: Vec<ToolDefinition>,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
    ) -> Result<ChatResponse, LlmError> {
        let mut span = LlmCallSpan::start("openai", self.preset.model.clone(), false);
        let messages_in = messages.len();
        let tool_count = available_tools.len();
        let context_tokens = Self::estimate_context_tokens(&messages);
        span.set_input_shape(messages_in, tool_count, context_tokens);

        let openai_messages = Self::map_messages(messages);

        let has_tools = !available_tools.is_empty();
        let tools = if !has_tools {
            None
        } else {
            Some(
                available_tools
                    .into_iter()
                    .map(|tool| OpenAITool {
                        r#type: "function".to_string(),
                        function: OpenAIToolFunction {
                            name: tool.name,
                            description: tool.description,
                            parameters: tool.parameters,
                        },
                    })
                    .collect(),
            )
        };

        let max_tokens = sanitize_max_tokens(max_tokens.or(self.preset.max_tokens));
        let request = OpenAIRequest {
            model: self.preset.model.clone(),
            messages: openai_messages,
            temperature: temperature.or(self.preset.temperature),
            max_tokens,
            stream: false,
            tools,
            tool_choice: self.preset.tool_choice.as_ref().and_then(|v| v.as_str().map(String::from)).or_else(|| if has_tools { Some("auto".to_string()) } else { None }),
            stream_options: None,
        };

        if let Ok(payload) = serde_json::to_string(&request) {
            tracing::debug!(target: "llm.body", payload = %payload, "OpenAI tool request");
        }

        let response = match self
            .client
            .post(&self.preset.endpoint)
            .header("Authorization", format!("Bearer {}", self.preset.api_key))
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
            let msg = format!("OpenAI API error (HTTP {}): {}", status.as_u16(), error_text);
            span.finish_error(CallOutcome::HttpError, format!("http_{}", status.as_u16()), msg.clone());
            return Err(LlmError::Api(msg));
        }

        let response_data: OpenAIResponse = match response.json().await {
            Ok(r) => r,
            Err(e) => {
                let (outcome, kind) = classify_reqwest_error(&e);
                span.finish_error(outcome, kind, e.to_string());
                return Err(LlmError::Http(e));
            }
        };

        if let Some(u) = response_data.usage.as_ref() {
            span.set_usage(u.prompt_tokens, u.completion_tokens, u.total_tokens);
        }

        let choice = match response_data.choices.first() {
            Some(c) => c,
            None => {
                let msg = "No response from OpenAI".to_string();
                span.finish_error(CallOutcome::Parse, "empty_choices", msg.clone());
                return Err(LlmError::Api(msg));
            }
        };

        if let Some(reason) = choice.finish_reason.clone() {
            span.set_finish_reason(reason);
        }

        let content = match choice.message.content.clone() {
            Some(serde_json::Value::String(s)) => s,
            Some(serde_json::Value::Array(parts)) => {
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

        let reasoning_content = choice.message.reasoning_content.clone();
        span.finish_success();

        Ok(ChatResponse {
            content,
            tool_calls,
            reasoning_content,
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
        let mut span = LlmCallSpan::start("openai", self.preset.model.clone(), true);
        let messages_in = messages.len();
        let tool_count = available_tools.len();
        let context_tokens = Self::estimate_context_tokens(&messages);
        span.set_input_shape(messages_in, tool_count, context_tokens);

        let openai_messages = Self::map_messages(messages);

        let has_tools = !available_tools.is_empty();
        let tools = if !has_tools {
            None
        } else {
            Some(
                available_tools
                    .into_iter()
                    .map(|tool| OpenAITool {
                        r#type: "function".to_string(),
                        function: OpenAIToolFunction {
                            name: tool.name,
                            description: tool.description,
                            parameters: tool.parameters,
                        },
                    })
                    .collect(),
            )
        };

        let max_tokens = sanitize_max_tokens(max_tokens.or(self.preset.max_tokens));
        let request = OpenAIRequest {
            model: self.preset.model.clone(),
            messages: openai_messages,
            temperature: temperature.or(self.preset.temperature),
            max_tokens,
            stream: true,
            tools,
            tool_choice: self.preset.tool_choice.as_ref().and_then(|v| v.as_str().map(String::from)).or_else(|| if has_tools { Some("auto".to_string()) } else { None }),
            stream_options: Some(OpenAIStreamOptions { include_usage: true }),
        };

        if let Ok(payload) = serde_json::to_string(&request) {
            tracing::debug!(target: "llm.body", payload = %payload, "OpenAI streaming tool request");
        }

        let response = match self
            .client
            .post(&self.preset.endpoint)
            .header("Authorization", format!("Bearer {}", self.preset.api_key))
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
            let msg = format!("OpenAI API error (HTTP {}): {}", status.as_u16(), error_text);
            span.finish_error(CallOutcome::HttpError, format!("http_{}", status.as_u16()), msg.clone());
            return Err(LlmError::Api(msg));
        }

        let mut bytes_stream = response.bytes_stream();
        let (tx, rx) = mpsc::unbounded_channel();

        tokio::spawn(async move {
            let mut tool_states: HashMap<usize, StreamedToolCallState> = HashMap::new();
            let mut saw_done = false;
            let mut bytes_in: usize = 0;
            let mut last_event = Instant::now();
            let mut finish_reason: Option<String> = None;
            let mut usage: Option<OpenAIUsage> = None;

            while let Some(chunk_result) = bytes_stream.next().await {
                match chunk_result {
                    Ok(chunk) => {
                        bytes_in += chunk.len();
                        span.observe_bytes(chunk.len());
                        match String::from_utf8(chunk.to_vec()) {
                            Ok(chunk_str) => {
                                let events = OpenAIClient::parse_stream_chunk(
                                    &chunk_str,
                                    &mut tool_states,
                                    &mut finish_reason,
                                    &mut usage,
                                    &mut saw_done,
                                );
                                for event in events {
                                    if matches!(event, ChatStreamEvent::ToolCallDelta(_)) {
                                        span.observe_tool_call();
                                    }
                                    last_event = Instant::now();
                                    if tx.send(Ok(event)).is_err() {
                                        return;
                                    }
                                }
                            }
                            Err(err) => {
                                let msg = format!("Invalid UTF-8 in stream: {}", err);
                                span.finish_error(CallOutcome::Parse, "utf8", msg.clone());
                                let _ = tx.send(Err(LlmError::Api(msg)));
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        let (outcome, kind) = classify_reqwest_error(&e);
                        span.finish_error(outcome, kind, e.to_string());
                        let _ = tx.send(Err(LlmError::Http(e)));
                        return;
                    }
                }
            }

            // Stream ended. If we saw neither [DONE] nor a terminal usage
            // frame, the provider hung up on us. Surface that explicitly
            // so the agentic loop stops treating it as a clean completion.
            if !saw_done && usage.is_none() {
                let age = last_event.elapsed().as_millis();
                let reason = format!(
                    "no [DONE] sentinel and no terminal usage frame received after {} bytes",
                    bytes_in
                );
                span.finish_error(
                    CallOutcome::StreamTruncated,
                    "stream_truncated",
                    reason.clone(),
                );
                let _ = tx.send(Err(LlmError::StreamTruncated {
                    bytes_read: bytes_in,
                    last_event_age_ms: age,
                    reason,
                }));
                return;
            }

            if let Some(u) = usage.as_ref() {
                span.set_usage(u.prompt_tokens, u.completion_tokens, u.total_tokens);
            }
            if let Some(reason) = finish_reason {
                span.set_finish_reason(reason);
            }
            span.finish_success();
        });

        Ok(Box::pin(UnboundedReceiverStream::new(rx)))
    }
}
