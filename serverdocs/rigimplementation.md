# Rig Implementation

This document describes the **Rig-based conversation engine** in the Luna server. Rig ([rig-core](https://crates.io/crates/rig-core)) is the canonical LLM orchestration layer, replacing the previous direct LLM client integration.

---

## 1. Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  Client (WebSocket / HTTP)                                                   │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  handlers.rs: handle_send_message, handle_scheduled_job                       │
│  → engine.run_turn(ctx, TurnParams)                                          │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  server/engine.rs: RigEngine                                                 │
│  → ConversationEngine trait impl                                             │
│  → Builds RigConversationContext, calls run_turn_streaming()                 │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  rig_core/pipeline.rs: run_turn_streaming()                                  │
│  → Rig agent with .multi_turn(10) for tool loops                             │
│  → OpenAI-compatible Chat Completions API                                    │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                    ┌───────────────┼───────────────┐
                    ▼               ▼               ▼
            ┌──────────────┐ ┌──────────────┐ ┌──────────────┐
            │ MCP tools    │ │ schedule_task│ │ store_memory │
            │ (rig_tools)  │ │ cancel_*     │ │ search_*     │
            └──────────────┘ └──────────────┘ └──────────────┘
```

---

## 2. Components

### 2.1 `rig_core` Module

| File | Purpose |
|------|---------|
| `pipeline.rs` | `run_turn_streaming()` – main entry point. Builds Rig agent, runs multi-turn chat with tools. |
| `adapters.rs` | `luna_messages_to_rig_history()` – converts Luna `Message[]` to Rig `Message[]`. |
| `mod.rs` | Re-exports `RigConversationContext`, `run_turn_streaming`, `luna_messages_to_rig_history`. |

**Non-streaming path** (`run_turn`) exists for spike/testing (`examples/rig_spike.rs`) but is not used by the server.

### 2.2 Pipeline (`pipeline.rs`)

- **`RigConversationContext`**: Input for a turn:
  - `messages`: Luna chat history (excludes System; System messages are in `preamble`)
  - `user_message`: User text for this turn
  - `preset`: Model preset (backend, model, endpoint, api_key, temperature, max_tokens)
  - `preamble`: System prompt (from ContextService::inject_prompts)
  - `mcp_tools`: Vec of `ToolDyn` (MCP + internal tools)

- **`run_turn_streaming()`**:
  - Builds OpenAI-compatible client from `preset.endpoint` (base URL derived from `/chat/completions`)
  - Creates agent with preamble, temperature, max_tokens, tools
  - Calls `agent.stream_chat().multi_turn(10)` for tool loops
  - Returns `Stream<Item = Result<String, anyhow::Error>>`

### 2.3 Adapters (`adapters.rs`)

- **`luna_messages_to_rig_history()`**: Used by pipeline.
  - `User` → `RigMessage::user(content)`
  - `Assistant` → `RigMessage::assistant(content)`
  - `System` → skipped (handled via preamble)
  - `Tool` → `RigMessage::tool_result(id, content)`

- **Legacy/unused** (post-migration): `storage_messages_to_rig_history`, `rig_assistant_to_view`, `stored_message_to_rig`, `RigAssistantView` – kept for potential future HTTP/direct paths.

### 2.4 Engine (`server/engine.rs`)

- **`RigEngine`**: Implements `ConversationEngine`.
- **`TurnParams`**:
  - `conversation_id`, `agent_messages`, `profile_name`, `allowed_tool_names`
  - `llm_client`: Legacy field; Rig uses its own OpenAI client internally.

- **Flow**:
  1. Resolve profile → `ModelPreset`
  2. Extract preamble from System messages
  3. Split history vs user message
  4. `build_all_tools()` → MCP + internal tools
  5. `run_turn_streaming(rig_ctx)` → stream
  6. Consume stream, broadcast `AssistantDelta`, `AssistantComplete`, `ConversationComplete`
  7. Persist assistant message via `storage.add_message_with_metadata()`

### 2.5 Tools (`server/rig_tools.rs`)

- **`MCPToolWrapper`**: Wraps MCP tools as `ToolDyn`. Emits `tool_started`, `tool_result`, `tool_error`.
- **Internal tools**:
  - `schedule_task`: Schedule reminders, recurring tasks
  - `cancel_scheduled_task`: Cancel by job id
  - `store_memory`: Store memories with embeddings
  - `search_memory`: Semantic search
  - `search_memory_by_category`: Filter by category
  - `delete_memory`: Delete memory by id

- **`build_all_tools()`**: Builds all enabled tools based on `allowed_tool_names`.

---

## 3. Wire Protocol

- **ServerEvent** (see `serverdocs/serverspec.md`): `streaming_started`, `assistant_delta`, `assistant_complete`, `conversation_complete`, `tool_started`, `tool_result`, `tool_error`.
- Rig tools emit these events; clients receive them via WebSocket subscriptions.

---

## 4. Configuration

- **Rig is the default and only engine**; no profile configuration required. All profiles use Rig.
- `preset.endpoint`: Full URL to chat completions (e.g. `https://api.openai.com/v1/chat/completions`).
- DeepSeek, OpenRouter, etc. supported via custom endpoints.

---

## 5. Traceability

Rig uses the `tracing` crate. Luna enables Rig spans by default:

| Level | What you see |
|-------|--------------|
| `rig=info` | Operation spans (start/end of LLM calls, tool invocations) |
| `rig=trace` | Request/response payloads (full messages, tokens) |

**Custom spans**

- `luna.run_turn{conversation_id, model}` – wraps the whole turn (engine + pipeline)
- `run_turn_streaming{model, history_len}` – pipeline entry

**Enable verbose Rig logs**

```bash
RUST_LOG=info,rig=trace cargo run
```

---

## 6. Integrating with Langfuse (or other backends)

Rig emits OpenTelemetry-compatible traces. You can send them to **Langfuse**, **Arize Phoenix**, or any OTLP-capable backend.

### Option A: Direct Langfuse (no collector)

Use [opentelemetry-langfuse](https://crates.io/crates/opentelemetry-langfuse) – it talks to Langfuse directly via OTLP/HTTP.

**1. Add deps** (`Cargo.toml`):

```toml
opentelemetry = { version = "0.31", features = ["trace"] }
opentelemetry_sdk = { version = "0.31", features = ["rt-tokio"] }
opentelemetry-langfuse = "0.6"
tracing-opentelemetry = "0.30"
```

**2. Init in `main.rs`** (before `server::run`):

```rust
use opentelemetry_langfuse::ExporterBuilder;
use opentelemetry_sdk::trace::{BatchSpanProcessor, SdkTracerProvider};
use opentelemetry_sdk::{runtime::Tokio, Resource};
use opentelemetry::global;

// Optional: only when LANGFUSE_* env vars are set
if std::env::var("LANGFUSE_PUBLIC_KEY").is_ok() {
    let exporter = ExporterBuilder::from_env()?.build()?;
    let provider = SdkTracerProvider::builder()
        .with_span_processor(BatchSpanProcessor::builder(exporter, Tokio).build())
        .with_resource(Resource::builder().with_service_name("luna-server").build())
        .build();
    global::set_tracer_provider(provider);

    tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("luna")))
        .with(tracing_subscriber::fmt::layer())
        .with(EnvFilter::from_default_env())
        .try_init()?;
} else {
    // existing tracing() setup
}
```

**3. Env vars**:

```bash
export LANGFUSE_PUBLIC_KEY="pk-lf-..."
export LANGFUSE_SECRET_KEY="sk-lf-..."
export LANGFUSE_HOST="https://cloud.langfuse.com"   # or self-hosted URL
```

Rig spans (LLM calls, tool invocations) and Luna spans (`luna.run_turn`) will appear in Langfuse.

### Option B: OpenTelemetry Collector → any backend

Use a generic OTLP collector and route to Langfuse, Phoenix, Jaeger, etc.

**1. Add deps**:

```toml
opentelemetry = { version = "0.31", features = ["trace"] }
opentelemetry_sdk = { version = "0.31", features = ["rt-tokio"] }
opentelemetry-otlp = { version = "0.31", features = ["http", "tonic", "trace"] }
tracing-opentelemetry = "0.30"
```

**2. Export to collector** (e.g. `http://localhost:4318`):

```rust
let exporter = opentelemetry_otlp::SpanExporter::builder()
    .with_http()
    .with_endpoint("http://localhost:4318/v1/traces")
    .build()?;
// ... SdkTracerProvider with exporter, same pattern as above
```

**3. Collector config** – route to Langfuse, Phoenix, etc. Example for Langfuse:

```yaml
receivers:
  otlp:
    protocols:
      http:
        endpoint: 0.0.0.0:4318

exporters:
  otlphttp/langfuse:
    endpoint: "https://cloud.langfuse.com/api/public/otel"
    headers:
      Authorization: "Basic ${LANGFUSE_AUTH_BASE64}"

service:
  pipelines:
    traces:
      receivers: [otlp]
      exporters: [otlphttp/langfuse]
```

### Other backends

| Backend | Notes |
|---------|-------|
| **Langfuse** | Option A or B; GenAI traces, token usage, costs |
| **Arize Phoenix** | OTLP collector → Phoenix |
| **Langflow** | Different product (workflow builder); use OTLP collector if it supports it |
| **Jaeger / Zipkin** | OTLP collector → Jaeger/Zipkin exporter |

---

## 7. Migration Notes

- **Removed**: Direct `LlmClient` usage in engine; `send_message_stream`, `send_message_stream_with_tools` are unused.
- **Legacy storage**: `conversation_storage.rs` (file-based) is largely superseded by `sqlite_storage_simple` + `storage_wrapper`.
- **Legacy MCP conversions**: `mcp/conversions.rs` `tool_call_to_params`, `tools_to_definitions` etc. unused after Rig migration.
