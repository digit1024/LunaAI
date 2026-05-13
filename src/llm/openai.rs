use super::*;
use crate::config::ModelPreset;
use crate::llm::observability::{classify_reqwest_error, CallOutcome, LlmCallSpan};
use crate::llm::tokenizer::TokenCounter;
use futures::{Stream, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;
use std::time::{Duration, Instant};
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

/// Bounded byte-buffer that keeps the most recent N bytes of an SSE
/// response. We log a snippet of this on every degraded outcome
/// (stream-truncated / length-truncated / empty-tool-calls) so we can
/// forensically determine what the provider actually sent without having
/// to repro the bug. 32 KB is plenty for one assistant turn.
const SSE_BODY_CAPTURE_CAP: usize = 32 * 1024;

struct SseBodyCapture {
    buf: Vec<u8>,
    cap: usize,
    /// Total bytes seen (not just retained). Useful to tell users
    /// "the snippet you're looking at is the tail of N total bytes".
    total: usize,
}

impl SseBodyCapture {
    fn new(cap: usize) -> Self {
        Self {
            buf: Vec::with_capacity(cap.min(8 * 1024)),
            cap,
            total: 0,
        }
    }

    fn append(&mut self, bytes: &[u8]) {
        self.total = self.total.saturating_add(bytes.len());
        if bytes.len() >= self.cap {
            self.buf.clear();
            self.buf.extend_from_slice(&bytes[bytes.len() - self.cap..]);
            return;
        }
        let overflow = (self.buf.len() + bytes.len()).saturating_sub(self.cap);
        if overflow > 0 {
            self.buf.drain(..overflow);
        }
        self.buf.extend_from_slice(bytes);
    }

    fn snapshot_lossy(&self) -> String {
        String::from_utf8_lossy(&self.buf).into_owned()
    }
}

/// Trim a long body snapshot to `head` + `tail` chars with an elision
/// marker in the middle, so degraded-outcome logs don't flood when the
/// captured tail is 32 KB. Keep `head` and `tail` smallish (≤4 KB total
/// is plenty for figuring out which provider quirk hit).
fn body_snippet(body: &str, head: usize, tail: usize) -> String {
    let chars: Vec<char> = body.chars().collect();
    if chars.len() <= head + tail + 64 {
        return body.to_string();
    }
    let head_str: String = chars.iter().take(head).collect();
    let tail_str: String = chars.iter().skip(chars.len() - tail).collect();
    format!(
        "{head_str}\n...[elided {} chars]...\n{tail_str}",
        chars.len() - head - tail
    )
}

/// Log a captured response body on a degraded outcome. The full body
/// goes to `target: "llm.body"` at DEBUG (so users can opt in via
/// `RUST_LOG=llm.body=debug`); a head/tail snippet goes to the main
/// `target: "llm_call"` at WARN with the call_id so it always shows up
/// in the audit trail.
fn log_response_body_on_failure(
    capture: &SseBodyCapture,
    call_id: uuid::Uuid,
    provider: &str,
    model: &str,
    reason: &str,
) {
    let body = capture.snapshot_lossy();
    let snippet = body_snippet(&body, 1024, 1024);
    tracing::warn!(
        target: "llm_call",
        call_id = %call_id,
        provider = provider,
        model = %model,
        reason = reason,
        captured_bytes = capture.buf.len(),
        total_bytes = capture.total,
        body_snippet = %snippet,
        "Captured response body snippet on degraded outcome"
    );
    tracing::debug!(
        target: "llm.body",
        call_id = %call_id,
        reason = reason,
        body = %body,
        "Full captured response body on degraded outcome"
    );
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

    /// Parse `Retry-After`. Per RFC 7231 it may be seconds OR an HTTP-date;
    /// every LLM provider we talk to uses the seconds form so that's all
    /// we honor. Returns `None` when missing or unparseable.
    fn extract_retry_after(resp: &reqwest::Response) -> Option<Duration> {
        let raw = resp.headers().get("retry-after")?.to_str().ok()?;
        let trimmed = raw.trim();
        if let Ok(seconds) = trimmed.parse::<u64>() {
            return Some(Duration::from_secs(seconds));
        }
        if let Ok(seconds_f) = trimmed.parse::<f64>() {
            if seconds_f.is_finite() && seconds_f >= 0.0 {
                return Some(Duration::from_millis((seconds_f * 1000.0) as u64));
            }
        }
        None
    }

    /// Map a non-2xx response to a properly classified `LlmError`. 429 and
    /// 500/503 (the codes DeepSeek docs explicitly mark retryable) become
    /// transient variants the agentic loop can retry. Everything else stays
    /// terminal so we never burn retries on bad auth / malformed requests.
    fn classify_http_error(
        status: reqwest::StatusCode,
        retry_after: Option<Duration>,
        body: String,
    ) -> (LlmError, CallOutcome, String) {
        let code = status.as_u16();
        let message = format!("OpenAI API error (HTTP {}): {}", code, body);
        match code {
            429 => (
                LlmError::RateLimited {
                    retry_after,
                    message,
                },
                CallOutcome::HttpError,
                "http_429_rate_limited".to_string(),
            ),
            500 | 503 => (
                LlmError::ServerBusy {
                    status: code,
                    retry_after,
                    message,
                },
                CallOutcome::HttpError,
                format!("http_{}_server_busy", code),
            ),
            _ => (
                LlmError::Api(message),
                CallOutcome::HttpError,
                format!("http_{}", code),
            ),
        }
    }

    /// True if this chunk has any line that proves the provider is alive:
    /// an SSE comment (`:` prefix, includes DeepSeek's `: keep-alive`) or
    /// a `data:` line. Used to bump `last_event` so a long-queued DeepSeek
    /// request doesn't show "last activity hours ago" in truncation logs.
    fn chunk_has_liveness_signal(chunk: &str) -> bool {
        for line in chunk.lines() {
            let trimmed = line.trim_start();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with(':') || trimmed.starts_with("data:") {
                return true;
            }
        }
        false
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
        let retry_after = Self::extract_retry_after(&response);
        span.set_response_headers(status.as_u16(), request_id);

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            let (err, outcome, kind) = Self::classify_http_error(status, retry_after, error_text);
            span.finish_error(outcome, kind, err.to_string());
            return Err(err);
        }

        // Stream into a channel so we can run truncation detection out-of-band.
        let mut bytes_stream = response.bytes_stream();
        let (tx, rx) = mpsc::unbounded_channel::<Result<String, LlmError>>();

        let call_id = span.call_id();
        let model_for_log = self.preset.model.clone();
        tokio::spawn(async move {
            let mut saw_done = false;
            let mut bytes_in: usize = 0;
            let mut last_event = Instant::now();
            let mut finish_reason: Option<String> = None;
            let mut usage: Option<OpenAIUsage> = None;
            let mut tool_states: HashMap<usize, StreamedToolCallState> = HashMap::new();
            let mut content_chars: usize = 0;
            let mut body_capture = SseBodyCapture::new(SSE_BODY_CAPTURE_CAP);

            while let Some(chunk_result) = bytes_stream.next().await {
                match chunk_result {
                    Ok(chunk) => {
                        bytes_in += chunk.len();
                        span.observe_bytes(chunk.len());
                        body_capture.append(&chunk);
                        let chunk_str = match String::from_utf8(chunk.to_vec()) {
                            Ok(s) => s,
                            Err(e) => {
                                let msg = format!("Invalid UTF-8 in stream: {}", e);
                                log_response_body_on_failure(
                                    &body_capture,
                                    call_id,
                                    "openai",
                                    &model_for_log,
                                    "utf8",
                                );
                                span.finish_error(CallOutcome::Parse, "utf8", msg.clone());
                                let _ = tx.send(Err(LlmError::Api(msg)));
                                return;
                            }
                        };

                        // P2: count SSE comment / data lines as liveness.
                        if Self::chunk_has_liveness_signal(&chunk_str) {
                            last_event = Instant::now();
                        }

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
                            content_chars += content.chars().count();
                            if tx.send(Ok(content)).is_err() {
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        log_response_body_on_failure(
                            &body_capture,
                            call_id,
                            "openai",
                            &model_for_log,
                            "transport_error",
                        );
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
                log_response_body_on_failure(
                    &body_capture,
                    call_id,
                    "openai",
                    &model_for_log,
                    "stream_truncated",
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

            // P1: finish_reason=length with no content is degraded.
            if finish_reason.as_deref() == Some("length") && content_chars == 0 {
                let reason = "model hit max_tokens before emitting any content".to_string();
                log_response_body_on_failure(
                    &body_capture,
                    call_id,
                    "openai",
                    &model_for_log,
                    "length_truncated",
                );
                span.finish_error(
                    CallOutcome::StreamTruncated,
                    "length_truncated",
                    reason,
                );
                let _ = tx.send(Err(LlmError::LengthTruncated {
                    partial_tool_calls: 0,
                    content_chars: 0,
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
        let retry_after = Self::extract_retry_after(&response);
        span.set_response_headers(status.as_u16(), request_id);

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            let (err, outcome, kind) = Self::classify_http_error(status, retry_after, error_text);
            span.finish_error(outcome, kind, err.to_string());
            return Err(err);
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
                    // P1: don't silently fall back to `{}` when the model
                    // returned malformed arguments JSON. Log a warning so
                    // the bad call shows up in the audit trail instead of
                    // executing a tool with empty params.
                    let parameters = match serde_json::from_str::<serde_json::Value>(
                        &tc.function.arguments,
                    ) {
                        Ok(v) => v,
                        Err(parse_err) => {
                            tracing::warn!(
                                tool_name = %tc.function.name,
                                argument_chars = tc.function.arguments.len(),
                                error = %parse_err,
                                "Tool call arguments JSON failed to parse; defaulting to empty params"
                            );
                            serde_json::Value::Object(Default::default())
                        }
                    };
                    ToolCall {
                        id: tc.id.clone(),
                        name: tc.function.name.clone(),
                        parameters,
                    }
                })
                .collect()
        } else {
            Vec::new()
        };

        let reasoning_content = choice.message.reasoning_content.clone();

        let finish_reason_str = choice.finish_reason.as_deref();

        // Fix 1/4 — non-streaming twin of EmptyToolCallsCompletion.
        // DeepSeek's phantom-tool-calls bug also shows up here: response
        // body has finish_reason="tool_calls" with zero tool_calls and
        // empty content. Surface as transient so the loop can retry once.
        if finish_reason_str == Some("tool_calls")
            && content.is_empty()
            && tool_calls.is_empty()
        {
            let msg = "provider signalled tool-call intent (finish_reason=tool_calls) but emitted no tool_calls or content".to_string();
            tracing::warn!(
                target: "llm.body",
                response = ?response_data,
                "Empty tool-calls completion captured (non-streaming)"
            );
            span.finish_error(
                CallOutcome::StreamTruncated,
                "empty_tool_calls_completion",
                msg,
            );
            return Err(LlmError::EmptyToolCallsCompletion {
                partial_tool_calls: 0,
            });
        }

        // P1: same length-truncated guard as the streaming path. If the
        // provider tells us max_tokens stopped generation and we have
        // nothing usable to show, return LengthTruncated so the loop can
        // surface a specific error instead of an empty assistant turn.
        if finish_reason_str == Some("length") && content.is_empty() && tool_calls.is_empty() {
            let msg = "model hit max_tokens before producing usable output".to_string();
            span.finish_error(CallOutcome::StreamTruncated, "length_truncated", msg);
            return Err(LlmError::LengthTruncated {
                partial_tool_calls: 0,
                content_chars: 0,
            });
        }

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
        let retry_after = Self::extract_retry_after(&response);
        span.set_response_headers(status.as_u16(), request_id);

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            let (err, outcome, kind) = Self::classify_http_error(status, retry_after, error_text);
            span.finish_error(outcome, kind, err.to_string());
            return Err(err);
        }

        let mut bytes_stream = response.bytes_stream();
        let (tx, rx) = mpsc::unbounded_channel();

        let call_id = span.call_id();
        let model_for_log = self.preset.model.clone();
        tokio::spawn(async move {
            let mut tool_states: HashMap<usize, StreamedToolCallState> = HashMap::new();
            let mut saw_done = false;
            let mut bytes_in: usize = 0;
            let mut last_event = Instant::now();
            let mut finish_reason: Option<String> = None;
            let mut usage: Option<OpenAIUsage> = None;
            let mut content_chars: usize = 0;
            let mut completed_tool_calls: usize = 0;
            let mut body_capture = SseBodyCapture::new(SSE_BODY_CAPTURE_CAP);

            while let Some(chunk_result) = bytes_stream.next().await {
                match chunk_result {
                    Ok(chunk) => {
                        bytes_in += chunk.len();
                        span.observe_bytes(chunk.len());
                        body_capture.append(&chunk);
                        match String::from_utf8(chunk.to_vec()) {
                            Ok(chunk_str) => {
                                // P2: SSE comment lines (DeepSeek emits
                                // `: keep-alive` while a request is queued)
                                // are evidence of provider liveness too.
                                if Self::chunk_has_liveness_signal(&chunk_str) {
                                    last_event = Instant::now();
                                }

                                let events = OpenAIClient::parse_stream_chunk(
                                    &chunk_str,
                                    &mut tool_states,
                                    &mut finish_reason,
                                    &mut usage,
                                    &mut saw_done,
                                );
                                for event in events {
                                    match &event {
                                        ChatStreamEvent::ToolCallDelta(_) => {
                                            span.observe_tool_call();
                                            completed_tool_calls += 1;
                                        }
                                        ChatStreamEvent::ContentDelta(c) => {
                                            content_chars += c.chars().count();
                                        }
                                        ChatStreamEvent::ReasoningContentDelta(_) => {}
                                    }
                                    if tx.send(Ok(event)).is_err() {
                                        return;
                                    }
                                }
                            }
                            Err(err) => {
                                let msg = format!("Invalid UTF-8 in stream: {}", err);
                                log_response_body_on_failure(
                                    &body_capture,
                                    call_id,
                                    "openai",
                                    &model_for_log,
                                    "utf8",
                                );
                                span.finish_error(CallOutcome::Parse, "utf8", msg.clone());
                                let _ = tx.send(Err(LlmError::Api(msg)));
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        log_response_body_on_failure(
                            &body_capture,
                            call_id,
                            "openai",
                            &model_for_log,
                            "transport_error",
                        );
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
                log_response_body_on_failure(
                    &body_capture,
                    call_id,
                    "openai",
                    &model_for_log,
                    "stream_truncated",
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

            // P1: warn for partial tool-call states that never produced a
            // valid JSON. With the old code they vanished silently.
            let partial_tool_calls: usize = tool_states
                .values()
                .filter(|s| s.name.is_some() || !s.arguments.is_empty())
                .count();
            if partial_tool_calls > 0 {
                for state in tool_states.values() {
                    if state.name.is_none() && state.arguments.is_empty() {
                        continue;
                    }
                    tracing::warn!(
                        call_id = %call_id,
                        tool_name = state.name.as_deref().unwrap_or("?"),
                        argument_chars = state.arguments.len(),
                        finish_reason = finish_reason.as_deref().unwrap_or(""),
                        "Dropping partial tool call: argument JSON was incomplete when the stream ended"
                    );
                }
            }

            // Fix 1 + Fix 3 (DeepSeek phantom-tool-calls bug): the provider
            // signalled tool-call intent (finish_reason=tool_calls or
            // partial deltas in tool_states) but we never emitted a single
            // fully-parsed tool call and no text content. Mark transient
            // so the loop can re-roll the turn.
            let intent_to_call_tools = finish_reason.as_deref() == Some("tool_calls")
                || partial_tool_calls > 0;
            if intent_to_call_tools && content_chars == 0 && completed_tool_calls == 0 {
                let reason = format!(
                    "provider signalled tool-call intent (finish_reason={:?}, {} partial tool call(s)) but emitted no usable output",
                    finish_reason.as_deref().unwrap_or("?"),
                    partial_tool_calls
                );
                log_response_body_on_failure(
                    &body_capture,
                    call_id,
                    "openai",
                    &model_for_log,
                    "empty_tool_calls_completion",
                );
                span.finish_error(
                    CallOutcome::StreamTruncated,
                    "empty_tool_calls_completion",
                    reason,
                );
                let _ = tx.send(Err(LlmError::EmptyToolCallsCompletion {
                    partial_tool_calls,
                }));
                return;
            }

            // P1: finish_reason=length with no usable output is a degraded
            // outcome too; tell the user instead of returning an empty turn.
            if finish_reason.as_deref() == Some("length")
                && content_chars == 0
                && completed_tool_calls == 0
            {
                let reason = format!(
                    "model hit max_tokens before producing usable output ({} partial tool call(s) dropped)",
                    partial_tool_calls
                );
                log_response_body_on_failure(
                    &body_capture,
                    call_id,
                    "openai",
                    &model_for_log,
                    "length_truncated",
                );
                span.finish_error(
                    CallOutcome::StreamTruncated,
                    "length_truncated",
                    reason,
                );
                let _ = tx.send(Err(LlmError::LengthTruncated {
                    partial_tool_calls,
                    content_chars: 0,
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
