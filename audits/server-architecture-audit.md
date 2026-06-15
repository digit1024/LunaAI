# LunaAI Server Architecture Audit

> **Scope:** `src/server/` (6 files, ~3,195 lines, ~116 KB)
> **Date:** 2026-06-14
> **Auditor:** Automated static analysis

---

## 1. Architecture Overview

### 1.1 Module Inventory

| File | Lines | LOC % | Role |
|------|------:|------:|------|
| `handlers.rs` | 1,804 | 56% | Command dispatch, agent orchestration, business logic |
| `mod.rs` | 523 | 16% | Bootstrap, lifecycle, background workers |
| `http.rs` | 323 | 10% | Axum HTTP routes (REST + WS upgrade) |
| `dto.rs` | 266 | 8% | Wire protocol types (tagged JSON enums) |
| `conversation_subscriptions.rs` | 111 | 3% | Pub/sub fan-out for multi-client viewing |
| `websocket.rs` | 168 | 5% | WebSocket protocol loop |

### 1.2 Architecture Pattern

The server is a **single-process, single-node Axum WebSocket command bus** with a thin REST layer. It does not follow MVC. The closest named pattern is **Layered Architecture** — but only partially realised:

```
┌──────────────────────────────────────────────────────────┐
│  main.rs → server::run() → launch()   [Bootstrap]        │
└─────────────────────┬────────────────────────────────────┘
                      │
       ┌──────────────┼─────────────────┐
       ▼              ▼                 ▼
 Background       ServerContext     Axum Router
 Workers          (shared state)    (http.rs)
 (scheduler,                             │
  deep_sleep,                     ┌─────┴──────┐
  titles, audit)                  ▼            ▼
                              HTTP REST    WS upgrade
                              handlers     (websocket.rs)
                                               │
                                        ServerHandler
                                        (per-connection)
                                               │
                               ┌───────────────┼─────────────────┐
                               ▼               ▼                 ▼
                          SessionState    AgenticLoop       Services
                          (per WS conn)   (agent crate)     (stateless)
                               │               │                 │
                               └───────────────┼─────────────────┘
                                               ▼
                                        Storage (SQLite)
                                        Arc<Mutex<Storage>>
```

**External service integrations:**

```
ServerContext.mcp_registry ──► agentic-loop crate ──► MCP servers (stdio/http)
SessionState.llm_client    ──► OpenAI / Anthropic API  (HTTP, no circuit breaker)
ServerContext.embedding_provider ──► OpenAI Embeddings API
mod.rs LlmCallAuditObserver ──► SQLite (async drain channel)
spawn_title_generation_thread ──► LLM API (fire-and-forget)
```

**Potential bottlenecks:**

| Bottleneck | Location | Impact |
|------------|----------|--------|
| Global `Arc<Mutex<Storage>>` | `handlers.rs` throughout | All DB ops serialized; WS handlers contend on same lock |
| `to_bytes(body, usize::MAX)` | `http.rs:91` | Unbounded memory buffer for uploads |
| `spawn_agent_task` nested spawn | `handlers.rs:1235` | Unbounded agent tasks; no queue/semaphore |
| Background workers on Tokio default executor | `mod.rs:254–361` | Scheduler/deep-sleep can starve request tasks |
| `SmartContextManager::select_context` × 3 | `handlers.rs:499,515,545` | CPU-bound token counting on async thread |

---

## 2. Separation of Concerns

**Rating: 4 / 10**

The DTO and pub-sub layers are properly isolated. The rest is blurred:

| Layer | Status |
|-------|--------|
| HTTP routing (`http.rs`) | Clean |
| Wire protocol (`dto.rs`) | Clean |
| Pub-sub (`conversation_subscriptions.rs`) | Clean |
| WebSocket protocol (`websocket.rs`) | Clean |
| Bootstrap (`mod.rs`) | Slightly overloaded (4 background tasks inline) |
| Business logic (`handlers.rs`) | **Severely mixed** — orchestration, persistence, context management, auth co-location |

---

## 3. SOLID Compliance

**Overall SOLID rating: 3 / 10**

### 3.1 Single Responsibility Principle (SRP) — 3 / 10

**Violating modules:**

#### `handlers.rs:133–1167` — `ServerHandler`
One impl block owns:
- 16 command dispatches (health, conversations, profiles, memories, messages, scheduling)
- Agent lifecycle (spawn, abort, timeout)
- Attachment resolution
- Context window management (summarize → RAG → truncate → emergency fallback)
- SQLite persistence triggers
- WebSocket event broadcasting

**Fix — split into focused handlers:**
```rust
// Instead of ServerHandler handling everything:

pub struct ConversationHandler<'a> { ctx: &'a Arc<ServerContext>, session: &'a mut SessionState, outbound: &'a UnboundedSender<ServerEvent> }
pub struct MemoryHandler<'a> { ctx: &'a Arc<ServerContext>, outbound: &'a UnboundedSender<ServerEvent> }
pub struct ProfileHandler<'a> { ctx: &'a Arc<ServerContext>, session: &'a mut SessionState, outbound: &'a UnboundedSender<ServerEvent> }

// ServerHandler becomes a thin dispatcher:
impl ServerHandler {
    pub async fn handle_command(&mut self, command: ClientCommand) {
        let result = match command {
            ClientCommand::SendMessage { .. } => ConversationHandler::new(&self.ctx, &mut self.session, &self.outbound).handle_send(content).await,
            ClientCommand::ListMemories { .. } => MemoryHandler::new(&self.ctx, &self.outbound).list().await,
            // ...
        };
        // single error handler
    }
}
```

#### `handlers.rs:381–612` — `run_agent_for_conversation` (232 lines)
Does: auto-summarize check → rebuild messages → memory RAG → inject prompts → token count → smart truncation × 2 → emergency fallback → spawn.

**Fix — extract a `ContextPipeline`:**
```rust
struct ContextPipeline<'a> {
    ctx: &'a Arc<ServerContext>,
    session: &'a SessionState,
    conversation_id: Uuid,
}

impl<'a> ContextPipeline<'a> {
    async fn build_messages(&self) -> Result<Vec<LlmMessage>> { ... }
    async fn maybe_summarize(&self, msgs: Vec<LlmMessage>) -> Result<Vec<LlmMessage>> { ... }
    async fn inject_memory(&self, msgs: &mut Vec<LlmMessage>) -> Result<()> { ... }
    async fn enforce_token_budget(&self, msgs: Vec<LlmMessage>) -> Vec<LlmMessage> { ... }
}
```

#### `mod.rs:30–149` — `launch()` (120 lines)
Does: config → db init → MCP connect → embedding init → 4 background task spawns → HTTP bind.

**Fix — extract an `AppInitializer`:**
```rust
async fn init_storage(config: &AppConfig) -> Arc<Mutex<Storage>> { ... }
async fn init_mcp(config: &AppConfig) -> Result<Arc<RwLock<MCPServerRegistry>>> { ... }
async fn init_server_context(...) -> Result<Arc<ServerContext>> { ... }
fn spawn_background_workers(ctx: Arc<ServerContext>) { ... }
```

---

### 3.2 Open/Closed Principle (OCP) — 4 / 10

#### `handlers.rs:156–205` — `handle_command` match
Adding a new client command requires:
1. Editing `ClientCommand` enum (`dto.rs`)
2. Adding a match arm in `handle_command`
3. Writing a `handle_*` method on `ServerHandler`

This is acceptable for a Rust enum-dispatch pattern, but the coupling of all handlers into one struct makes extensions surgical rather than additive.

**Fix — trait-based command handlers:**
```rust
#[async_trait]
pub trait CommandHandler {
    type Command;
    async fn handle(&self, cmd: Self::Command, outbound: &UnboundedSender<ServerEvent>) -> Result<()>;
}

// Registry:
struct CommandRegistry {
    handlers: HashMap<CommandKind, Box<dyn ErasedCommandHandler>>,
}
```

#### `handlers.rs:1644–1804` — `process_agent_update` match (160 lines)
New `AgentUpdate` variants require editing this function. No extension point.

**Fix — per-variant handler map or a visitor trait on `AgentUpdate`.**

---

### 3.3 Liskov Substitution Principle (LSP) — 8 / 10

No violations found. `Arc<dyn LlmClient>` and `Arc<dyn EmbeddingProvider>` are used consistently throughout. Trait objects are used correctly.

---

### 3.4 Interface Segregation Principle (ISP) — 4 / 10

#### `ServerContext` — 11 fields, passed whole to every handler
`handlers.rs:44–58`:
```rust
pub struct ServerContext {
    pub config: Arc<AppConfig>,            // used everywhere
    pub server_cfg: Arc<ServerConfig>,     // used by agent spawn + auth
    pub prompt_manager: PromptManager,     // used only by run_agent_for_conversation
    pub storage: Arc<Mutex<Storage>>,      // used everywhere
    pub mcp_registry: Arc<RwLock<MCPServerRegistry>>, // used only by agent spawn + profile change
    pub subscriptions: Arc<ConversationSubscriptions>, // used by broadcasting
    pub schedule_service: Arc<ScheduleService>,       // used only by scheduled tasks
    pub default_allowed_tool_names: HashSet<String>,  // used only at session init
    pub embedding_provider: Option<Arc<dyn EmbeddingProvider>>, // used only by memory RAG
    pub active_agent_runs: Arc<RwLock<HashMap<Uuid, AbortHandle>>>, // used by spawn + stop
}
```

`attach_file_handler` receives full `ServerContext` via Axum state but only uses `config` + `server_cfg`.

**Fix — split into focused sub-contexts:**
```rust
pub struct StorageCtx { pub storage: Arc<Mutex<Storage>> }
pub struct AgentCtx { pub mcp_registry: Arc<RwLock<MCPServerRegistry>>, pub schedule_service: Arc<ScheduleService>, pub active_agent_runs: Arc<RwLock<HashMap<Uuid, AbortHandle>>> }
pub struct SessionCtx { pub config: Arc<AppConfig>, pub server_cfg: Arc<ServerConfig>, pub default_allowed_tool_names: HashSet<String> }
// ServerContext composes these; individual handlers take only what they need.
```

---

### 3.5 Dependency Inversion Principle (DIP) — 3 / 10

#### Direct `Arc<Mutex<Storage>>` — no storage trait
`handlers.rs` calls `self.ctx.storage.lock().await` directly ~30 times. Every storage call is concrete.

**Fix:**
```rust
#[async_trait]
pub trait ConversationRepo: Send + Sync {
    async fn create_conversation(&self, title: &str, profile: Option<&str>) -> Result<Uuid>;
    async fn list_conversations(&self) -> Result<Vec<StoredConversation>>;
    // ...
}

// Storage implements ConversationRepo, MemoryRepo, MessageRepo separately.
// ServerContext holds Arc<dyn ConversationRepo>, etc.
```

#### Global observer via `set_llm_observer` — `mod.rs:90`
```rust
// Current (hidden global mutation):
crate::llm::set_llm_observer(Arc::new(LlmCallAuditObserver { tx: audit_tx }));

// Fix — pass observer explicitly through ServerContext or via constructor injection:
let llm_observer: Arc<dyn LlmObserver> = Arc::new(LlmCallAuditObserver { tx: audit_tx });
// store in ServerContext, inject into AgenticLoop::new()
```

#### Blocking FS in async handlers — `http.rs:145–147`
```rust
// Current:
std::fs::create_dir_all(&target_dir).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
std::fs::write(&path, &data).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

// Fix:
tokio::fs::create_dir_all(&target_dir).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
tokio::fs::write(&path, &data).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
```

Same issue at `http.rs:178–201` and `handlers.rs:1184–1194`.

---

## 4. Design Patterns

### 4.1 Patterns Present

| Pattern | Location | Correct? | Notes |
|---------|----------|----------|-------|
| **Command** | `ClientCommand` + `handle_command` match | Mostly | Match arms in single impl block limits extensibility |
| **DTO / Value Object** | `dto.rs` (all types) | Yes | Clean tagged-union JSON protocol |
| **Observer** | `LlmCallAuditObserver`, `ConversationSubscriptions::broadcast` | Yes | Correct |
| **Pub-Sub** | `conversation_subscriptions.rs` | Yes | Clean implementation |
| **Session** | `SessionState` per WS connection | Yes | Appropriate |
| **Abstract Factory / Builder** | `llm::build_llm_client()` | Yes | Hidden behind trait object |
| **Strategy** | `Arc<dyn LlmClient>`, `Arc<dyn EmbeddingProvider>` | Yes | Correct use of trait objects |
| **Template method (implicit)** | `run_agent_for_conversation` fixed pipeline | Implicit | Steps not overridable |

### 4.2 Missing Patterns (High-Value)

| Pattern | Where Needed | Benefit |
|---------|-------------|---------|
| **Repository** | Storage access throughout `handlers.rs` | Decouples business logic from SQLite; enables testing with in-memory fakes |
| **Middleware chain** | Auth in every HTTP handler | Axum `middleware::from_fn` would centralise auth in one place |
| **Rate limiting** | Entire server | No protection against WS message floods or upload floods |
| **Circuit breaker** | LLM / MCP calls in `spawn_agent_task` | External API failures cascade silently |
| **Semaphore / bounded task pool** | `spawn_agent_task` | Unbounded task spawning under load |
| **Unit of Work** | Multi-step storage ops | Prevents partial writes across multiple `storage.lock()` calls |

### 4.3 Pattern Assessment

#### Singleton — `Arc<Mutex<Storage>>`
Correct use: single SQLite connection shared safely. But the lock is held across long operations (attachment setup + message save in `handle_send_message:785–890`), creating unnecessary serialization.

**Fix — reduce lock scope:**
```rust
// Current (lock held for entire setup block):
let storage = self.ctx.storage.lock().await;
// ... 100 lines of work ...

// Fix: take lock for each individual operation:
let conversation = {
    let storage = self.ctx.storage.lock().await;
    storage.get_conversation(id)?
};
// ... non-storage work ...
{
    let storage = self.ctx.storage.lock().await;
    storage.save_message(&msg)?;
}
```

---

## 5. Anti-Patterns Found

### AP-1: God Method — `run_agent_for_conversation` | Severity: 9/10

**Location:** `handlers.rs:381–612` (232 lines)

Five distinct responsibilities in one `async fn`:
1. Auto-summarize decision (391–447)
2. Memory RAG injection (450–473)
3. Token counting and context usage report (475–490)
4. Smart truncation with stale `total_tokens` (492–568)
5. Emergency message-drop fallback (576–608)

Critical bug: `total_tokens` is computed at line 477 from the pre-RAG message list, but then used at lines 507/538 to gate the post-RAG, post-inject message list (`agent_messages`). After memory injection grows the context, `total_tokens` is stale and the `safe_limit` branch (538) may be skipped.

**Fix:**
```rust
// Recount tokens after every mutation:
let total_tokens = count_tokens(&agent_messages, &token_counter);
if total_tokens > token_counter.get_safe_context_limit(preset) {
    agent_messages = SmartContextManager::select_context(agent_messages, &token_counter, preset);
}
```

---

### AP-2: Silent Channel-Send Failures | Severity: 7/10

**Location:** `handlers.rs` — 20+ occurrences, e.g. lines 208, 248, 470, 530, 561, 623, 641, 666, 709, 731

Pattern:
```rust
let _ = self.outbound.send(ServerEvent::Info { message: "...".into() });
```

The `_` discards `SendError`, which means if the WS writer task is dead, the connection silently stops receiving events but the handler keeps running. The only time it's detected is when `broadcast` prunes dead senders (best case) or never.

**Fix:**
```rust
fn send_event(&self, event: ServerEvent) -> Result<()> {
    self.outbound.send(event).map_err(|_| anyhow!("WebSocket outbound channel closed"))
}
// Use ? propagation; the caller can handle or abort the handler.
```

---

### AP-3: Blocking I/O in Async Handlers | Severity: 8/10

**Locations:**
- `http.rs:145–147` — `std::fs::create_dir_all`, `std::fs::write` in `attach_file_handler`
- `http.rs:178–201` — `std::fs::read_dir` in `remove_file_handler`
- `handlers.rs:1184–1194` — `std::fs::read_dir`, `symlink_metadata` in `resolve_attachment_upload_path`

Synchronous filesystem calls block the Tokio executor thread, stalling other requests.

**Fix:** Replace all `std::fs::*` in async functions with `tokio::fs::*`.

---

### AP-4: Unbounded Upload Buffer | Severity: 9/10

**Location:** `http.rs:91`

```rust
let body_bytes = axum::body::to_bytes(body, usize::MAX).await...;
```

No size limit. A 10 GB upload OOMs the server.

**Fix:**
```rust
const MAX_UPLOAD_BYTES: usize = 50 * 1024 * 1024; // 50 MB
let body_bytes = axum::body::to_bytes(body, MAX_UPLOAD_BYTES)
    .await
    .map_err(|_| StatusCode::PAYLOAD_TOO_LARGE)?;
```

---

### AP-5: API Key in URL Path | Severity: 8/10

**Location:** `http.rs:277–284`

Route: `GET /api/static/{api_key}/{*path}` — the API key appears in server access logs, browser history, and HTTP `Referer` headers.

**Fix:** Move auth to standard header; use `authorize()` already defined at `http.rs:68–72`.

```rust
// Replace path-based key with header-based auth:
.route("/api/static/{*path}", get(static_file_handler))
// static_file_handler calls authorize(&ctx, &headers) as other handlers do
```

---

### AP-6: Timing-Unsafe API Key Comparison | Severity: 6/10

**Location:** `http.rs:70–72`

```rust
fn authorize(ctx: &ServerContext, headers: &HeaderMap) -> bool {
    extract_api_key(headers)
        .map(|k| k == ctx.server_cfg.api_key)  // string == is not constant-time
        .unwrap_or(false)
}
```

**Fix:**
```rust
use subtle::ConstantTimeEq;
fn authorize(ctx: &ServerContext, headers: &HeaderMap) -> bool {
    extract_api_key(headers)
        .map(|k| k.as_bytes().ct_eq(ctx.server_cfg.api_key.as_bytes()).into())
        .unwrap_or(false)
}
```
Add dependency: `subtle = "2"` to `Cargo.toml`.

---

### AP-7: `std::process::exit` in Library Code | Severity: 5/10

**Location:** `mod.rs:81`

```rust
.unwrap_or_else(|e| {
    tracing::error!(error = %e, "Failed to create temporary database");
    std::process::exit(1);  // kills process without cleanup
})
```

**Fix:** Propagate the error through `?` — `launch()` returns `Result<()>` and the caller can decide.

```rust
.map_err(|e| anyhow::anyhow!("Failed to create temporary database: {e}"))?
```

---

### AP-8: JSON Round-Trip Config Conversion | Severity: 4/10

**Location:** `mod.rs:167–171`

```rust
fn convert_mcp_config(config: &crate::config::MCPConfig) -> agentic_loop::MCPConfig {
    serde_json::from_value(serde_json::to_value(config).unwrap_or_default()).unwrap_or_default()
}
```

Double serialization silently produces a default config on any schema mismatch. Type mismatches disappear without error.

**Fix:** Implement `From<&crate::config::MCPConfig> for agentic_loop::MCPConfig` with explicit field mapping and return `Result`.

---

### AP-9: Dual Tracking of Agent Runs | Severity: 5/10

**Location:** `handlers.rs:1122–1152` (`handle_stop_streaming`)

Agent abort handles are tracked in both `session.inflight` (`Vec<JoinHandle>`) and `ctx.active_agent_runs` (`HashMap<Uuid, AbortHandle>`). Stop streaming aborts via `active_agent_runs`, but cleanup of `inflight` relies on `retain(|h| !h.is_finished())` polling. A race between abort and the `retain` sweep can leave dead handles.

**Fix:** Use only `active_agent_runs`; remove `inflight` from `SessionState`, or make the two canonical.

---

### AP-10: Zero Test Coverage | Severity: 10/10

No `#[cfg(test)]` modules exist anywhere in `src/server/`. The entire command dispatch, context pipeline, and persistence logic is untested.

**Fix (minimal):** Start with unit tests for the pure functions and testable components:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_preview_unicode_safe() {
        assert_eq!(truncate_preview("hello world", 5), "hello…");
    }

    #[tokio::test]
    async fn test_conversation_subscriptions_broadcast_prunes_dead_senders() {
        let subs = ConversationSubscriptions::default();
        let id = ConnectionId::new();
        let conv_id = Uuid::new_v4();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        // drop rx to simulate dead connection
        subs.set_viewing(id, conv_id, tx).await;
        drop(_rx); // now sender is dead
        subs.broadcast(conv_id, ServerEvent::ConversationComplete { conversation_id: conv_id.to_string() }).await;
        // assert subscribers map is cleaned up
    }
}
```

---

## 6. Modularity Score

**5 / 10**

| Factor | Score | Reason |
|--------|------:|-------|
| DTO isolation | 9/10 | Completely clean; externally tagged enums |
| Pub-sub isolation | 9/10 | `conversation_subscriptions.rs` is cohesive |
| HTTP routing | 7/10 | Clean routes; auth duplication is minor |
| WebSocket loop | 8/10 | Protocol logic separate from business logic |
| Business logic | 2/10 | `handlers.rs` is a monolith (56% of server code) |
| Bootstrap | 5/10 | `launch()` too many responsibilities inline |
| Test coverage | 0/10 | No tests |

---

## 7. Prioritised Findings

| # | Finding | Severity | File / Lines |
|---|---------|:--------:|-------------|
| 1 | **Zero test coverage** | 10/10 | Entire `src/server/` |
| 2 | **Unbounded upload buffer (DoS)** | 9/10 | `http.rs:91` |
| 3 | **`run_agent_for_conversation` god method with stale token count bug** | 9/10 | `handlers.rs:381–612` |
| 4 | **Blocking I/O in async context** | 8/10 | `http.rs:145–147`, `http.rs:178–201`, `handlers.rs:1184–1194` |
| 5 | **API key exposed in URL path** | 8/10 | `http.rs:277–284` |
| 6 | **No storage abstraction (DIP violation)** | 8/10 | `handlers.rs` throughout |
| 7 | **Silent channel-send failures** | 7/10 | `handlers.rs` — 20+ sites |
| 8 | **`ServerHandler` god object (SRP violation)** | 7/10 | `handlers.rs:133–1167` |
| 9 | **`launch()` over-sized composition root** | 6/10 | `mod.rs:30–149` |
| 10 | **Timing-unsafe API key comparison** | 6/10 | `http.rs:70–72` |
| 11 | **No rate limiting** | 6/10 | Entire server |
| 12 | **Global LLM observer via `set_llm_observer`** | 5/10 | `mod.rs:90` |
| 13 | **`std::process::exit` in library code** | 5/10 | `mod.rs:81` |
| 14 | **Dual agent-run tracking** | 5/10 | `handlers.rs:1122–1152` |
| 15 | **JSON round-trip config conversion** | 4/10 | `mod.rs:167–171` |
| 16 | **`ServerContext` ISP violation** | 4/10 | `handlers.rs:44–58` |
| 17 | **No circuit breaker for LLM/MCP** | 4/10 | `handlers.rs:1205–1292` |

---

## 8. Remediation Roadmap

### Sprint 1 — Critical Safety (1–2 days)

1. **Cap upload size** (`http.rs:91`): `to_bytes(body, 50 * 1024 * 1024)`
2. **Replace blocking FS** (`http.rs:145, 178`, `handlers.rs:1184`): `tokio::fs::*`
3. **Remove API key from URL** (`http.rs:277`): Route to `/api/static/{*path}`, auth via header
4. **Fix stale `total_tokens`** (`handlers.rs:477–568`): Recount after each mutation step

### Sprint 2 — Architecture (3–5 days)

5. **Extract `ContextPipeline`** from `run_agent_for_conversation` — 5 clear methods
6. **Extract focused handlers** (`ConversationHandler`, `MemoryHandler`, `ProfileHandler`) from `ServerHandler`
7. **Centralise auth** in Axum middleware: `middleware::from_fn(auth_middleware)`
8. **Replace `let _ = self.outbound.send(...)` with `send_event(&self, event) -> Result<()>`**

### Sprint 3 — Testability & Abstractions (1 week)

9. **Introduce `ConversationRepo` / `MemoryRepo` traits** over `Storage`
10. **Remove `set_llm_observer` global** — inject via `ServerContext`
11. **Replace `std::process::exit`** with `?` propagation in `launch()`
12. **Add `subtle` crate and constant-time key comparison**
13. **Write first 10 unit tests**: `truncate_preview`, `to_conversation_view`, `ConversationSubscriptions` pruning, `ContextPipeline` steps

### Sprint 4 — Resilience (optional)

14. **Semaphore over `spawn_agent_task`** — bound concurrent agents per server
15. **Rate limiter** on WS command loop (`tower::limit::RateLimitLayer`)
16. **Circuit breaker** for LLM calls (e.g. `failsafe-rs`)
