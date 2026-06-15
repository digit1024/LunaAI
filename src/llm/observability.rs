//! LLM observability: shared HTTP client, structured per-call spans, and
//! an optional sink (`LlmObserver`) that persists call records to durable
//! storage (typically the SQLite `llm_calls` table).
//!
//! Why this module exists:
//! - Every LLM client used to build its own `reqwest::Client::new()` with
//!   no timeouts and no TCP keepalive. When a provider silently dropped
//!   the stream (most often DeepSeek), the byte stream just ended and the
//!   agentic loop treated it as a successful (empty) completion. Adding
//!   timeouts here is the fix.
//! - Logging was scattered `debug!()` calls with no per-call correlation
//!   id. `LlmCallSpan` gives every HTTP round-trip a `call_id` and emits
//!   one structured event per phase (sent / first byte / done / error).

use chrono::Utc;
use reqwest::Client;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// One canonical `reqwest::Client` shared across every LLM provider. Built
/// lazily on first use. Sets the timeouts and keepalive defaults that
/// `reqwest::Client::new()` does *not* set, which is what allowed silent
/// mid-stream drops to look like clean completions.
pub fn shared_http_client() -> Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .pool_idle_timeout(Some(Duration::from_secs(60)))
                .tcp_keepalive(Some(Duration::from_secs(30)))
                .http2_keep_alive_interval(Duration::from_secs(20))
                .http2_keep_alive_while_idle(true)
                .http2_keep_alive_timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_else(|err| {
                    tracing::error!(error = %err, "Failed to build shared reqwest client; falling back to default");
                    Client::new()
                })
        })
        .clone()
}

/// Sink for completed `LlmCallSpan` events. Implemented by the storage
/// layer to persist call rows into the `llm_calls` table.
pub trait LlmObserver: Send + Sync {
    fn record(&self, record: LlmCallRecord);
}

static OBSERVER: OnceLock<Arc<dyn LlmObserver>> = OnceLock::new();

/// Register the global observer. Should be called once at server startup.
/// Subsequent calls are no-ops (logged at debug).
pub fn set_llm_observer(observer: Arc<dyn LlmObserver>) {
    if OBSERVER.set(observer).is_err() {
        tracing::debug!("LlmObserver already set, ignoring second registration");
    }
}

pub fn llm_observer() -> Option<&'static Arc<dyn LlmObserver>> {
    OBSERVER.get()
}

/// One row in the call audit log. All times are UTC unix seconds.
#[derive(Debug, Clone)]
pub struct LlmCallRecord {
    pub call_id: Uuid,
    pub conversation_id: Option<Uuid>,
    pub provider: String,
    pub model: String,
    pub streaming: bool,
    pub started_at: i64,
    pub finished_at: i64,
    pub duration_ms: u64,
    pub ttfb_ms: Option<u64>,
    pub http_status: Option<u16>,
    pub request_id: Option<String>,
    pub messages_in: usize,
    pub tool_count: usize,
    pub context_tokens_estimate: Option<usize>,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
    pub finish_reason: Option<String>,
    pub tool_calls_emitted: usize,
    pub bytes_in: usize,
    pub outcome: CallOutcome,
    pub error_kind: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallOutcome {
    Success,
    StreamTruncated,
    HttpError,
    NetworkError,
    Timeout,
    Parse,
}

impl CallOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            CallOutcome::Success => "success",
            CallOutcome::StreamTruncated => "stream_truncated",
            CallOutcome::HttpError => "http_error",
            CallOutcome::NetworkError => "network_error",
            CallOutcome::Timeout => "timeout",
            CallOutcome::Parse => "parse_error",
        }
    }
}

/// Per-call observability scope. Build at the top of every `send_*` impl,
/// call setters as info becomes available, then `finish_success()` or
/// `finish_error()` exactly once. The drop guard catches the case where
/// the future is cancelled before either is called and emits a synthetic
/// failure event so cancellations aren't silently lost.
pub struct LlmCallSpan {
    call_id: Uuid,
    provider: &'static str,
    model: String,
    streaming: bool,
    started: Instant,
    started_at_unix: i64,
    ttfb: Option<Instant>,
    http_status: Option<u16>,
    request_id: Option<String>,
    messages_in: usize,
    tool_count: usize,
    context_tokens_estimate: Option<usize>,
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    total_tokens: Option<u32>,
    finish_reason: Option<String>,
    tool_calls_emitted: usize,
    bytes_in: usize,
    finished: bool,
}

impl LlmCallSpan {
    pub fn start(provider: &'static str, model: impl Into<String>, streaming: bool) -> Self {
        let call_id = Uuid::new_v4();
        let model = model.into();
        let started = Instant::now();
        let started_at_unix = Utc::now().timestamp();

        tracing::info!(
            target: "llm_call",
            call_id = %call_id,
            provider = provider,
            model = %model,
            streaming = streaming,
            phase = "send",
            "LLM call starting"
        );

        Self {
            call_id,
            provider,
            model,
            streaming,
            started,
            started_at_unix,
            ttfb: None,
            http_status: None,
            request_id: None,
            messages_in: 0,
            tool_count: 0,
            context_tokens_estimate: None,
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
            finish_reason: None,
            tool_calls_emitted: 0,
            bytes_in: 0,
            finished: false,
        }
    }

    pub fn call_id(&self) -> Uuid {
        self.call_id
    }

    pub fn set_input_shape(
        &mut self,
        messages_in: usize,
        tool_count: usize,
        context_tokens_estimate: Option<usize>,
    ) {
        self.messages_in = messages_in;
        self.tool_count = tool_count;
        self.context_tokens_estimate = context_tokens_estimate;
        tracing::info!(
            target: "llm_call",
            call_id = %self.call_id,
            provider = self.provider,
            model = %self.model,
            messages_in = messages_in,
            tool_count = tool_count,
            context_tokens_estimate = context_tokens_estimate.unwrap_or(0),
            phase = "request_prepared"
        );
    }

    pub fn set_response_headers(&mut self, status: u16, request_id: Option<String>) {
        let elapsed_ms = self.started.elapsed().as_millis() as u64;
        self.http_status = Some(status);
        self.request_id = request_id.clone();
        if self.ttfb.is_none() {
            self.ttfb = Some(Instant::now());
        }
        tracing::info!(
            target: "llm_call",
            call_id = %self.call_id,
            provider = self.provider,
            model = %self.model,
            http_status = status,
            request_id = request_id.as_deref().unwrap_or(""),
            ttfb_ms = elapsed_ms,
            phase = "headers"
        );
    }

    pub fn observe_bytes(&mut self, n: usize) {
        self.bytes_in += n;
        if self.ttfb.is_none() {
            self.ttfb = Some(Instant::now());
        }
    }

    pub fn observe_tool_call(&mut self) {
        self.tool_calls_emitted += 1;
    }

    pub fn set_usage(
        &mut self,
        prompt_tokens: Option<u32>,
        completion_tokens: Option<u32>,
        total_tokens: Option<u32>,
    ) {
        self.prompt_tokens = prompt_tokens.or(self.prompt_tokens);
        self.completion_tokens = completion_tokens.or(self.completion_tokens);
        self.total_tokens = total_tokens.or(self.total_tokens);
    }

    pub fn set_finish_reason(&mut self, reason: impl Into<String>) {
        self.finish_reason = Some(reason.into());
    }

    pub fn finish_success(mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        let duration_ms = self.started.elapsed().as_millis() as u64;
        let ttfb_ms = self
            .ttfb
            .map(|t| t.saturating_duration_since(self.started).as_millis() as u64);

        tracing::info!(
            target: "llm_call",
            call_id = %self.call_id,
            provider = self.provider,
            model = %self.model,
            http_status = self.http_status.unwrap_or(0),
            request_id = self.request_id.as_deref().unwrap_or(""),
            duration_ms = duration_ms,
            ttfb_ms = ttfb_ms.unwrap_or(0),
            prompt_tokens = self.prompt_tokens.unwrap_or(0),
            completion_tokens = self.completion_tokens.unwrap_or(0),
            total_tokens = self.total_tokens.unwrap_or(0),
            finish_reason = self.finish_reason.as_deref().unwrap_or(""),
            tool_calls = self.tool_calls_emitted,
            bytes_in = self.bytes_in,
            phase = "done",
            "LLM call complete"
        );

        self.emit_record(CallOutcome::Success, None, None, duration_ms, ttfb_ms);
    }

    pub fn finish_error(
        mut self,
        outcome: CallOutcome,
        kind: impl Into<String>,
        message: impl Into<String>,
    ) {
        if self.finished {
            return;
        }
        self.finished = true;
        let duration_ms = self.started.elapsed().as_millis() as u64;
        let ttfb_ms = self
            .ttfb
            .map(|t| t.saturating_duration_since(self.started).as_millis() as u64);
        let kind = kind.into();
        let message = message.into();

        tracing::error!(
            target: "llm_call",
            call_id = %self.call_id,
            provider = self.provider,
            model = %self.model,
            http_status = self.http_status.unwrap_or(0),
            request_id = self.request_id.as_deref().unwrap_or(""),
            duration_ms = duration_ms,
            ttfb_ms = ttfb_ms.unwrap_or(0),
            bytes_in = self.bytes_in,
            outcome = outcome.as_str(),
            error_kind = %kind,
            error_message = %message,
            phase = "failed",
            "LLM call failed"
        );

        self.emit_record(outcome, Some(kind), Some(message), duration_ms, ttfb_ms);
    }

    fn emit_record(
        &self,
        outcome: CallOutcome,
        error_kind: Option<String>,
        error_message: Option<String>,
        duration_ms: u64,
        ttfb_ms: Option<u64>,
    ) {
        let Some(observer) = llm_observer() else {
            return;
        };
        let conversation_id = super::CONVERSATION_ID
            .try_with(|c| *c)
            .unwrap_or(None);
        let finished_at = Utc::now().timestamp();
        let record = LlmCallRecord {
            call_id: self.call_id,
            conversation_id,
            provider: self.provider.to_string(),
            model: self.model.clone(),
            streaming: self.streaming,
            started_at: self.started_at_unix,
            finished_at,
            duration_ms,
            ttfb_ms,
            http_status: self.http_status,
            request_id: self.request_id.clone(),
            messages_in: self.messages_in,
            tool_count: self.tool_count,
            context_tokens_estimate: self.context_tokens_estimate,
            prompt_tokens: self.prompt_tokens,
            completion_tokens: self.completion_tokens,
            total_tokens: self.total_tokens,
            finish_reason: self.finish_reason.clone(),
            tool_calls_emitted: self.tool_calls_emitted,
            bytes_in: self.bytes_in,
            outcome,
            error_kind,
            error_message,
        };
        observer.record(record);
    }
}

impl Drop for LlmCallSpan {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        // The future got cancelled (or someone forgot to finish_*). Emit a
        // synthetic failure so dropped calls don't disappear from the audit
        // trail.
        let duration_ms = self.started.elapsed().as_millis() as u64;
        let ttfb_ms = self
            .ttfb
            .map(|t| t.saturating_duration_since(self.started).as_millis() as u64);
        tracing::warn!(
            target: "llm_call",
            call_id = %self.call_id,
            provider = self.provider,
            model = %self.model,
            duration_ms = duration_ms,
            phase = "dropped",
            "LLM call span dropped without finish_*; future was likely cancelled"
        );
        self.emit_record(
            CallOutcome::NetworkError,
            Some("cancelled".to_string()),
            Some("LLM call future dropped before completion".to_string()),
            duration_ms,
            ttfb_ms,
        );
    }
}

/// Classify a `reqwest::Error` into a coarse outcome bucket for the audit log.
pub fn classify_reqwest_error(err: &reqwest::Error) -> (CallOutcome, &'static str) {
    if err.is_timeout() {
        (CallOutcome::Timeout, "timeout")
    } else if err.is_connect() {
        (CallOutcome::NetworkError, "connect")
    } else if err.is_decode() {
        (CallOutcome::Parse, "decode")
    } else if err.is_body() {
        (CallOutcome::NetworkError, "body")
    } else if err.is_request() {
        (CallOutcome::NetworkError, "request")
    } else {
        (CallOutcome::NetworkError, "http")
    }
}
