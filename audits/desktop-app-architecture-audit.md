# LunaAI Desktop App Architecture Audit

> **Scope:** `luna_thin_ui/src/` (43 files, ~7,000 lines)
> **Stack:** Rust · libcosmic (COSMIC desktop) · iced 0.14 TEA · tokio-tungstenite · reqwest · zbus
> **Date:** 2026-06-14
> **Auditor:** Automated static analysis

---

## 1. Architecture Overview

### 1.1 Module Inventory

| File | Lines | LOC % | Role |
|------|------:|------:|------|
| `ui/app.rs` | 1,872 | 27% | God object: model, update, subscription, event reducer, streaming, images |
| `ui/pages/history.rs` | 305 | 4.4% | Conversation list view |
| `client/ws_client.rs` | 318 | 4.6% | WebSocket background task + broadcast channel |
| `server/dto.rs` | 300 | 4.3% | Wire protocol mirror (maintained copy of server DTOs) |
| `ui/widgets/markdown_viewer.rs` | 380 | 5.5% | Markdown image viewer |
| `ui/widgets/message_bubble.rs` | 375 | 5.4% | User/assistant/summary bubble |
| `ui/pages/chat/message_list.rs` | 278 | 4.0% | Scrollable bubble list |
| `ui/handlers/chat.rs` | 262 | 3.8% | Chat input/send/attach/TTS handlers |
| `ui/pages/memories.rs` | 259 | 3.7% | Memory CRUD view |
| `utils/markdown_strip.rs` | 211 | 3.1% | TTS plain-text extraction |

### 1.2 Architecture Pattern — Elm/TEA

The app follows iced's **The Elm Architecture (TEA)** via libcosmic's `Application` trait, not MVC or MVVM:

```
┌─────────────────────────────────────────────────────────────────────────┐
│  view(app: &LunaThinApp) → Element<Message>                              │
│  pages/{chat,history,memories,settings,mcp} + widgets/*                  │
└──────────────────────┬──────────────────────────────────────────────────┘
                       │ user interactions emit Message
                       ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  update(msg: Message) → Task<Message>                                    │
│  1. handle_connection_messages                                           │
│  2. handle_chat_messages                                                 │
│  3. handle_navigation_messages                                           │
│  4. handle_history_memories_messages                                     │
│  5. handle_settings_messages                                             │
│  6. handle_server_event_messages  ──► handle_server_event() [237 lines] │
│  7–10. TTS, MCP, toggles, images (inline match)                         │
└──────────────────────┬──────────────────────────────────────────────────┘
                       │ mutates LunaThinApp, returns Task
                       ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  LunaThinApp (single mutable model, ~40 pub fields)                     │
└──────────────────────┬──────────────────────────────────────────────────┘
                       │
       ┌───────────────┴───────────────────────────────────┐
       ▼                                                   ▼
subscription()                                        Task::perform()
WS broadcast stream                                   (async work)
→ Message::ServerEvent                                → Message result
```

**External integrations:**

```
LunaWsClient ──tokio-tungstenite──► LunaAI server WebSocket
FileClient ──reqwest──► POST /api/attach-file, GET /api/mcp-servers
TtsClient ──zbus──► com.github.digit1024.ttsstt (DBus)
AudioService ──rodio──► embedded WAV assets
IconCache ──OnceLock──► filesystem SVG icons (Flatpak or dev path)
```

### 1.3 Data Flow: Server Event → UI

```
Server JSON over WebSocket
    │
    ▼
ws_client.rs read loop (line 201–243)
    serde_json::from_str::<ServerEvent>()
    │
    ▼
broadcast::Sender<ServerEvent> (capacity 10,000)
    │
    ▼
subscription() when ConnectionStatus::Connected (app.rs:1783–1787)
    Subscription::run_with(WsSubscriptionClient, ws_client_event_stream)
    │
    ▼
ws_client_event_stream() (app.rs:1428–1517)
    burst-coalesce: max 100 events OR 16ms frame budget
    on Lagged: drop + warn   on Closed: yield ServerDisconnected
    │
    ▼
Message::ServerEvent(event) → iced event queue
    │
    ▼
update() → handle_server_event_messages() → handle_server_event()
    mutates: messages, streaming_conversations, conversations,
             memories, connection_status, inline_error, image_cache, nav_model
    may return Task: ParseMarkdownChunk, fetch_missing_images
    │
    ▼
view() → chat_page() → message_list() → message_bubble()
    renders ChatMessage.markdown_items + image_cache
```

### 1.4 Potential Bottlenecks

| Bottleneck | Location | Impact |
|------------|----------|--------|
| `Message.clone()` × 8 per update cycle | `app.rs:1579–1635` | Clones entire message (including large assistant text) |
| `handle_server_event()` 237-line match | `app.rs:1179–1415` | All server events serialized through one fn on UI thread |
| `ParseMarkdownChunk` incremental parse | `app.rs:1700–1762` | Prevents freeze but adds frame latency on long histories |
| Broadcast capacity 10,000 | `ws_client.rs:146` | 10K buffered events possible; `Lagged` drops silently |
| `std::sync::Mutex` in tokio tasks | `ws_client.rs:157,276,288` | Can block executor thread under contention |
| Subscription gap on reconnect | `app.rs:1783` | Events during `Connecting` state are dropped |

---

## 2. Separation of Concerns

**Rating: 4 / 10**

| Layer | Status |
|-------|--------|
| Wire protocol (`server/dto.rs`) | Clean |
| WS networking (`client/ws_client.rs`) | Clean |
| HTTP client (`client/http_client.rs`) | Clean |
| Utility (`utils/markdown_strip.rs`) | Clean |
| View pages (`ui/pages/`) | Mostly clean; take `&LunaThinApp` so any page can read any field |
| View widgets (`ui/widgets/`) | Import `app::{Message, ImageState, ...}` — coupled to app layer |
| Handlers (`ui/handlers/`) | Overflow extraction; all handlers take `&mut LunaThinApp` |
| `ui/app.rs` | **Severely mixed** — model, update dispatcher, event reducer, streaming logic, image fetcher, WS bridge, subscription |

---

## 3. SOLID Compliance

**Overall SOLID rating: 3 / 10**

### 3.1 Single Responsibility Principle (SRP) — 2 / 10

#### `LunaThinApp` / `app.rs` — god object

`app.rs` (1,872 lines) is responsible for:
- Application **model** (40 fields, lines 398–481)
- **Message** enum (50+ variants, lines 40–154) with all domain types inside
- **Update dispatcher** (chained handlers, 1577–1766)
- **Server event reducer** (237-line match, 1179–1415)
- **Streaming delta application** (1015–1177)
- **Image fetching and caching** (708–824)
- **WS subscription** stream (1418–1518)
- **Navigation model** construction (609–700)
- **Markdown chunk parsing** (1700–1762)
- **Domain types**: `Page`, `ChatMessage`, `BubbleType`, `ConnectionStatus`, `TtsStatus`, `ImageState`, `MemoryDraft`, `NavItem`, `MenuAction`

The handlers module extracted ~850 lines but left all the **hard logic** in place.

**Fix — extract domain-specific state and reducers:**
```rust
// Instead of one struct with 40 fields, compose focused state modules:
pub struct LunaThinApp {
    pub core: Core,
    pub connectivity: ConnectivityState,
    pub conversation: ConversationState,
    pub history: HistoryState,
    pub memories: MemoriesState,
    pub ui: UiState,
    pub tts: TtsState,
    pub settings: SettingsState,
}

pub struct ConversationState {
    pub current_id: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub streaming: StreamingState,
    pub image_cache: HashMap<String, ImageState>,
}
```

#### `update()` — 8 sequential `message.clone()` calls

`app.rs:1577–1640`: Every message is cloned up to 8 times (once per handler in the chain) before a match arm consumes it. For `Message::ServerEvent(ServerEvent::AssistantComplete { full_text, .. })`, this clones a potentially large string 8 times per event.

**Fix — pattern-match once, dispatch by domain:**
```rust
fn update(&mut self, message: Message) -> app::Task<Message> {
    match message {
        Message::Connect | Message::Disconnect | Message::AutoReconnect
        | Message::ServerConnected | Message::ServerDisconnected
        | Message::ConnectionEstablished | Message::ConnectionFailed(_)
        | Message::ServerError(_) => self.connection.update(message),

        Message::SendMessage | Message::StopMessage | Message::InputChanged(_)
        | Message::AttachFile | Message::FileSelected(_) | Message::UploadSuccess { .. }
        | Message::FileUploadError(_) | Message::CopyMessage(_) => self.chat.update(message),

        Message::ServerEvent(event) => self.handle_server_event(event),
        // ...
    }
}
```

---

### 3.2 Open/Closed Principle (OCP) — 4 / 10

#### `Message` enum + handler chain
Adding a new user action requires:
1. New variant in `Message` (app.rs)
2. New match arm in one of the handler functions
3. Sometimes new fields on `LunaThinApp`

The chain-of-responsibility via `if let Some(task) = handle_*(self, message.clone())` is open to new handlers but each handler is a closed match-arm set.

#### `handle_server_event()` — 237-line match
New `ServerEvent` variants from the server require editing this match. No extension point.

---

### 3.3 Liskov Substitution Principle (LSP) — 8 / 10

No violations. `LunaWsClient`, `FileClient`, `TtsClient` are concrete, used directly. No trait objects misused.

---

### 3.4 Interface Segregation Principle (ISP) — 3 / 10

#### All page functions take `&LunaThinApp`
`pages/chat/top_panel.rs`, `pages/history.rs`, `pages/memories.rs`, etc. all accept full `&LunaThinApp`. A top panel needs only `connection_status`, `profiles`, `current_profile` — it gets all 40 fields.

**Fix — introduce narrow view-model structs:**
```rust
pub struct ChatTopPanelProps<'a> {
    pub connection_status: &'a ConnectionStatus,
    pub profiles: &'a [String],
    pub current_profile: &'a str,
    pub is_streaming: bool,
}

// top_panel.rs:
pub fn top_panel(props: ChatTopPanelProps<'_>) -> Element<'_, Message> { ... }
```

#### `Message` enum — too broad
`handle_connection_messages` receives all 50 variants but only handles ~8. The `if let Some` chain means irrelevant messages are cloned and passed to irrelevant handlers.

---

### 3.5 Dependency Inversion Principle (DIP) — 4 / 10

#### `TtsClient` created with `Arc<Connection>` directly
`services/tts_client.rs:20–28` creates a concrete DBus connection. The app holds `Option<Arc<TtsClient>>` directly with no trait, making TTS untestable and non-swappable.

```rust
// Current:
pub struct TtsClient { connection: Arc<Connection> }

// Fix — trait:
#[async_trait]
pub trait TtsSpeaker: Send + Sync {
    async fn speak(&self, text: &str) -> Result<()>;
    async fn stop(&self) -> Result<()>;
}
// App holds Option<Arc<dyn TtsSpeaker>>
```

#### Widgets importing concrete `app::Message`
`widgets/message_bubble.rs`, `widgets/markdown_viewer.rs`, `widgets/menu_bar.rs` all import `crate::ui::app::Message`. Widgets are permanently coupled to one app's message type — they can't be reused.

**Fix — parameterise widgets by message type** (standard iced pattern):
```rust
// message_bubble.rs:
pub fn user_bubble<'a, M: Clone + 'a>(
    content: &'a ChatMessage,
    on_copy: impl Fn(String) -> M + 'a,
    on_tts: impl Fn(String) -> M + 'a,
) -> Element<'a, M> { ... }
```

#### Global `OnceLock<Mutex<IconCache>>` — `icons.rs:11`
```rust
static ICON_CACHE: OnceLock<Mutex<IconCache>> = OnceLock::new();
```
Process-global mutable singleton; can't be replaced in tests; lock contention possible.

---

## 4. Design Patterns

### 4.1 Patterns Present

| Pattern | Location | Correct? | Notes |
|---------|----------|----------|-------|
| **TEA / Elm** | `Application` impl, `update()`, `view()` | Yes | Framework-mandated |
| **Command** | `ClientCommand` + `send_command()` | Yes | Fire-and-forget via tokio spawn |
| **Observer / Pub-Sub** | `broadcast::channel::<ServerEvent>` | Yes | Correct; coalescing is a nice touch |
| **Facade** | `LunaWsClient` over tokio-tungstenite | Yes | Good abstraction |
| **Singleton** | `ICON_CACHE` | Partial | Lock-wrapped global; testability concern |
| **Strategy (implicit)** | `handle_*(app, msg)` chain | Weak | Falls through by `if let Some` — not pluggable |
| **Session** | `ConnectionHandle` in ws_client | Yes | Cleared on disconnect |

### 4.2 Missing Patterns

| Pattern | Where Needed | Benefit |
|---------|-------------|---------|
| **View-model / Props struct** | Page/widget boundaries | Decouple view from `LunaThinApp` |
| **Domain events → UI events separation** | `ServerEvent` directly in `Message` | Server protocol changes ripple to all UI code |
| **Repository** | History/Memory page data | Currently fetched ad-hoc from server, no local cache abstraction |
| **Circuit breaker** | WS reconnect loop | Exponential backoff exists (`connection.rs`) but no attempt limit |
| **Command bus** | `update()` chain | Enum routing is fragile at scale |

### 4.3 WS subscription coalescing — positive pattern

`ws_client_event_stream()` (`app.rs:1467–1510`) batches up to 100 events per 16ms frame — correctly prevents UI freeze during rapid streaming. Worth preserving.

---

## 5. Anti-Patterns Found

### AP-1: God Object — `app.rs` | Severity: 9/10

**Location:** `app.rs` — 1,872 lines

`LunaThinApp` has 40+ `pub` fields covering 8 distinct domains (connectivity, chat, history, memories, streaming, TTS, images, settings). The struct has no encapsulation — every handler can read and write any field. `app.rs` is simultaneously Model, Reducer, Subscription, and Service layer.

**Fix:** Compose focused sub-state as shown in SRP section. Even extracting `ConversationState` and `ConnectivityState` would reduce field scatter significantly.

---

### AP-2: `message.clone()` × 8 per update | Severity: 8/10

**Location:** `app.rs:1579, 1584, 1589, 1594, 1599, 1604, 1609, 1635, 1640`

```rust
if let Some(task) = handle_connection_messages(self, message.clone()) { return task; }
if let Some(task) = handle_chat_messages(self, message.clone()) { return task; }
if let Some(task) = handle_navigation_messages(self, message.clone()) { return task; }
// ... ×8 more
```

`Message::ServerEvent(ServerEvent::AssistantComplete { full_text, .. })` carries a full assistant response — cloned 8 times before being consumed. At 60fps streaming, this is 480 string clones/second minimum.

**Fix:** Dispatch by discriminant first, then forward once:
```rust
fn update(&mut self, message: Message) -> app::Task<Message> {
    match &message {
        Message::Connect | Message::Disconnect | Message::ServerConnected
        | Message::ServerDisconnected | Message::AutoReconnect => {
            return handlers::connection::handle(self, message);
        }
        Message::SendMessage | Message::StopMessage | Message::InputChanged(_) => {
            return handlers::chat::handle(self, message);
        }
        Message::ServerEvent(_) => {
            return handlers::server_events::handle(self, message);
        }
        // ...
    }
    app::Task::none()
}
```

---

### AP-3: API Key Logged in Debug Mode | Severity: 8/10

**Location:** `ws_client.rs:114–116`

```rust
tracing::debug!(
    "Auth headers: x-api-key={}, authorization=Bearer ...",
    config.api_key   // ← full key in log output
);
```

Debug logs go to stdout / system journal. The API key is a long-lived shared secret.

**Fix:**
```rust
tracing::debug!(
    "Auth headers: x-api-key=[REDACTED], authorization=Bearer [REDACTED]"
);
```

---

### AP-4: `std::sync::Mutex` inside Tokio tasks | Severity: 7/10

**Location:** `ws_client.rs:157, 276, 288, 293, 300, 310`

`handle_slot: Arc<Mutex<Option<Arc<ConnectionHandle>>>>` uses `std::sync::Mutex` (OS blocking) inside `async fn`. Under lock contention the Tokio worker thread blocks, starving other tasks on the same thread.

**Fix:**
```rust
// Replace:
use std::sync::Mutex;
// With:
use tokio::sync::Mutex;
// And add .await to all lock acquisitions
```

---

### AP-5: Dead Code | Severity: 6/10

Multiple confirmed dead code locations:

| Symbol | Location | Evidence |
|--------|----------|---------|
| `Message::ScrollToBottom` | `app.rs:94` | Defined; never matched in `update()` |
| `INIT_CONNECT: Once` block | `app.rs:1623–1631` | Closure body is empty; does nothing |
| `LunaThinApp.streaming_content` / `.reasoning_content` | `app.rs:433–434` | Reset to empty string; never written; comment says "like mobile app" |
| `Message::RegenerateMessage` | `app.rs:129` | Handler logs only (chat.rs:106–109); button removed from UI (message_list.rs:170–171) |
| `ToolCallMessage` enum | `tool_call.rs:14–16` | Exported via mod; unused by app |
| `TtsClient::subscribe_status()` | `tts_client.rs:116` | Implemented; app has TODO (app.rs:1771–1772); optimistic status only |
| `Message::ConnectionEstablished` | `app.rs:152` | Matched in handler (connection.rs:71) but never produced by any task |

**Fix:** Delete all dead variants and fields. `streaming_content` and `reasoning_content` are the most confusing since they look like active state but are always empty.

---

### AP-6: Dual Input State | Severity: 6/10

**Location:** `app.rs:449–450` vs `chat_page.input_content` (`pages/chat/mod.rs:19`)

```rust
pub input_text: String,                           // synced from editor, used by send
pub chat_page: ChatPageState,                     // owns text_editor::Content
//   ChatPageState.input_content: text_editor::Content
```

Two representations of the same text. `input_text` is set from the editor via `InputActionPerformed` handler; `input_content` is the widget's own state. They can drift (e.g., `RetryMessage` restores `input_text` at app.rs:1220 but must also clear `input_content` at chat.rs:174).

**Fix:** Use only `text_editor::Content` as the source of truth; extract text at send time via `.text()`.

---

### AP-7: `expect` in View Path | Severity: 7/10

**Location:** `memories.rs:214`

```rust
fn edit_memory_card(app: &LunaThinApp) -> Element<Message> {
    let draft = app.editing_memory.as_ref().expect("edit card without draft");
    // ...
}
```

A panic in `view()` crashes the entire COSMIC app. The `expect` fires if `editing_memory` is `None` while the page is rendered (possible if navigation clears it asynchronously).

**Fix:**
```rust
let Some(draft) = app.editing_memory.as_ref() else {
    return widget::text("No memory selected.").into();
};
```

---

### AP-8: Fire-and-Forget Spawns with Silent Errors | Severity: 7/10

**Locations:**

| Location | Code |
|----------|------|
| `app.rs:826–831` | `send_command()` — tokio spawn, no error handling |
| `app.rs:834–839` | `set_ws_streaming()` — same |
| `connection.rs:64–67` | `Disconnect` — spawn without awaiting |
| `audio.rs:14` | `play_sound()` — spawn without cancellation on app exit |

WS send errors are fully silenced. If the channel is closed, commands disappear with no UI feedback.

**Fix — return `Task<Message>` from send helpers:**
```rust
pub(crate) fn send_command(&self, command: ClientCommand) -> app::Task<Message> {
    let ws_client = self.ws_client.clone();
    app::Task::perform(
        async move {
            let client = ws_client.read().await;
            client.send(command);
        },
        |_| cosmic::Action::App(Message::NoOp),
    )
}
```

---

### AP-9: Hard Exit via `std::process::exit` | Severity: 6/10

**Location:** `app.rs:1711`

```rust
Message::Quit => {
    std::process::exit(0);  // kills process without cleanup
}
```

No graceful WS disconnect, no TTS stop, no pending-upload cancellation.

**Fix — use iced's native exit mechanism:**
```rust
Message::Quit => {
    // Disconnect first
    if self.connection_status == ConnectionStatus::Connected {
        self.send_command(ClientCommand::Disconnect);
    }
    return window::get_oldest().then(|id| {
        window::close(id.unwrap_or(window::Id::MAIN)).into()
    });
}
```

---

### AP-10: Duplicate Navigation Logic | Severity: 5/10

MCP/Memories page load triggered from **two places**:

- `navigation.rs:17–28` — `handle_navigation_messages()` when `NavigateTo(Page::MCPServers)` / `Page::Memories`
- `app.rs:1840–1850` — `on_nav_select()` COSMIC nav hook

If nav triggers both paths, double-loads are possible. Each triggers a separate HTTP/WS request.

**Fix:** Centralise page-entry logic in a single `on_page_entered(&mut self, page: Page) -> Task<Message>` called from both sites.

---

### AP-11: `server/dto.rs` — Maintained Duplicate | Severity: 6/10

**Location:** `luna_thin_ui/src/server/dto.rs` vs `src/server/dto.rs`

Not a shared crate — two independently maintained copies. Divergences already exist:
- Thin UI has `MCPServerStatus`, `MCPServerInfo`, `MCPServersResponse` — not in server `dto.rs`
- Server has `From<&StoredMessage>` — not in thin UI copy
- `MessageView.tool_calls` uses different inner type (`ToolCallView` vs crate `ToolCall`)

Any protocol change requires updating both files. A missed sync causes silent JSON deserialization failures (unknown fields ignored by serde; missing fields produce `None`).

**Fix:** Extract into a shared `luna-protocol` crate:
```toml
# Cargo.toml (workspace)
[workspace]
members = ["luna-protocol", "src", "luna_thin_ui"]

# luna-protocol/Cargo.toml
[package]
name = "luna-protocol"
[dependencies]
serde = { version = "1", features = ["derive"] }
```

---

### AP-12: Reconnect Subscription Gap | Severity: 5/10

**Location:** `app.rs:1783–1787`

```rust
fn subscription(&self) -> Subscription<Message> {
    if self.connection_status == ConnectionStatus::Connected {
        // subscription active
    }
    // else: no subscription
}
```

During the `Connecting` state, the WS task may be sending events (e.g., `ServerConnected`) into the broadcast channel. Since the iced subscription only attaches after `Connected` is set, events emitted before the status flips are broadcast into the channel but never received by the subscription — they sit in the 10,000-capacity buffer until the subscription attaches.

**Fix:** Start subscription at `Connecting` state; ignore events until `Connected` is confirmed.

---

### AP-13: No Tests | Severity: 10/10

Zero `#[cfg(test)]` modules in the entire `luna_thin_ui/src/` tree. The only tested code is `utils/markdown_strip.rs` (lines 152–210).

**Fix:** Start with pure functions:
```rust
// utils/markdown_strip.rs already has tests — good model.
// Add tests for:
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_assistant_delta_increments_content() {
        // Build minimal ChatMessage, call apply_assistant_delta, assert content
    }

    #[test]
    fn test_handle_server_event_conversation_complete_removes_streaming() {
        // Construct minimal app state, feed ConversationComplete event
    }
}
```

---

## 6. Modularity Score

**4 / 10**

| Factor | Score | Reason |
|--------|------:|-------|
| Wire protocol | 9/10 | Clean DTOs; concern is the duplicate, not the design |
| Network client | 8/10 | `LunaWsClient` is well-scoped; `std::sync::Mutex` is minor |
| HTTP client | 8/10 | Focused, thin |
| Utils | 9/10 | `markdown_strip` is pure + tested |
| Services (TTS) | 6/10 | Concrete DBus, no trait |
| View pages | 6/10 | Stateless fns but take full `&LunaThinApp` |
| View widgets | 5/10 | Import `app::Message` — locked to one app |
| Handlers | 4/10 | Overflow extraction; no real domain separation |
| `ui/app.rs` | 1/10 | God object containing 8 concerns |
| Test coverage | 1/10 | Only `markdown_strip.rs` has tests |

---

## 7. Prioritised Findings

| # | Finding | Severity | File / Lines |
|---|---------|:--------:|-------------|
| 1 | **Zero test coverage** (except markdown_strip) | 10/10 | Entire `luna_thin_ui/src/` |
| 2 | **`app.rs` god object** — 40 fields, 8 responsibilities | 9/10 | `app.rs` whole file |
| 3 | **`message.clone()` × 8** — O(handlers × message_size) per update | 8/10 | `app.rs:1579–1640` |
| 4 | **API key logged in debug output** | 8/10 | `ws_client.rs:114–116` |
| 5 | **`expect()` panic in view path** | 7/10 | `memories.rs:214` |
| 6 | **`std::sync::Mutex` inside tokio tasks** | 7/10 | `ws_client.rs:157,276,288` |
| 7 | **Fire-and-forget spawns with silent errors** | 7/10 | `app.rs:826–839`, `connection.rs:64–67` |
| 8 | **`server/dto.rs` maintained duplicate** — silent deserialization drift | 6/10 | `luna_thin_ui/src/server/dto.rs` |
| 9 | **Dead code** (`streaming_content`, `ScrollToBottom`, `INIT_CONNECT`, etc.) | 6/10 | `app.rs` multiple locations |
| 10 | **`std::process::exit(0)` without cleanup** | 6/10 | `app.rs:1711` |
| 11 | **Dual input state** (`input_text` + `input_content`) | 6/10 | `app.rs:449–450` |
| 12 | **Widgets coupled to `app::Message`** (ISP violation) | 5/10 | `widgets/message_bubble.rs`, `widgets/markdown_viewer.rs` |
| 13 | **Duplicate nav-load logic** | 5/10 | `navigation.rs:17–28` vs `app.rs:1840–1850` |
| 14 | **Reconnect subscription gap** | 5/10 | `app.rs:1783–1787` |
| 15 | **Pages accept full `&LunaThinApp`** (ISP violation) | 4/10 | All `pages/` files |
| 16 | **Global `ICON_CACHE` singleton** | 4/10 | `icons.rs:11` |
| 17 | **`TtsClient` not behind trait** (DIP violation) | 3/10 | `services/tts_client.rs` |

---

## 8. Architecture Diagram — Dependency Flow

```
luna_thin_ui/src/
│
├── main.rs ──► ui::LunaThinApp (COSMIC app entry)
│
├── client/
│   ├── config.rs ─────────────────────────────────────────────────── toml/dirs
│   ├── http_client.rs ──► server::dto ──────────────────────────── reqwest
│   └── ws_client.rs ──► server::dto ──────────────────────────── tokio-tungstenite
│
├── server/
│   └── dto.rs ◄────── [DUPLICATE of src/server/dto.rs] ────────── serde
│
├── services/
│   └── tts_client.rs ─────────────────────────────────────────── zbus
│
├── utils/
│   └── markdown_strip.rs ─────────────────────────────────────── pulldown_cmark
│
├── resources.rs ──────────────────────────────────────────────── rust_embed
│
└── ui/
    ├── app.rs ◄─── GOD OBJECT: owns all of the below
    │   ├── imports: client/*, services/tts, server/dto, ui/handlers/*
    │   ├── imports: ui/pages/*, ui/widgets/*
    │   ├── defines: Message, LunaThinApp, Page, ChatMessage, ...
    │   └── implements: Application (init/update/view/subscription)
    │
    ├── handlers/ ──► app::{Message, LunaThinApp}   [take &mut LunaThinApp]
    │   ├── connection.rs
    │   ├── chat.rs
    │   ├── navigation.rs
    │   ├── history_memories.rs
    │   ├── settings.rs
    │   ├── server_events.rs  [3 lines of passthrough]
    │   └── tts.rs
    │
    ├── pages/ ──► app::{Message, LunaThinApp}      [take &LunaThinApp]
    │   ├── chat/{mod,top_panel,message_list,input_area}
    │   ├── history.rs
    │   ├── memories.rs
    │   ├── settings.rs
    │   └── mcp_servers.rs
    │
    ├── widgets/ ──► app::{Message, ChatMessage, ImageState}
    │   ├── message_bubble.rs  [18-arg function]
    │   ├── markdown_viewer.rs
    │   ├── tool_call.rs
    │   ├── typing_indicator.rs
    │   ├── error_banner.rs / info_banner.rs / menu_bar.rs / selectable_text.rs
    │
    ├── audio.rs ──► resources::AudioAssets ──── rodio
    └── icons.rs ──► ICON_CACHE (global OnceLock)
```

---

## 9. Remediation Roadmap

### Sprint 1 — Security & Crash Safety (1 day)

1. **Redact API key from debug log** (`ws_client.rs:116`): remove `config.api_key` from format string
2. **Guard `expect()` in view** (`memories.rs:214`): replace with early return `Element`
3. **Replace `std::process::exit`** (`app.rs:1711`): use iced window close API

### Sprint 2 — Performance & Correctness (2–3 days)

4. **Fix `message.clone()` × 8** (`app.rs:1579–1640`): single `match` dispatch by discriminant
5. **Replace `std::sync::Mutex`** in ws_client with `tokio::sync::Mutex`
6. **Remove dual input state**: use `text_editor::Content::text()` as sole truth; delete `input_text`
7. **Delete confirmed dead code**: `streaming_content`, `reasoning_content`, `ScrollToBottom`, `INIT_CONNECT` empty block, `RegenerateMessage` handler stub

### Sprint 3 — Architecture (1 week)

8. **Extract `ConversationState`** from `LunaThinApp`: `current_id`, `messages`, `streaming`, `image_cache`
9. **Extract `ConnectivityState`**: `ws_client`, `file_client`, `connection_status`, `user_disconnect_flag`, `reconnect_in_progress`
10. **Centralise page-entry logic** in `on_page_entered()` called from both nav sites
11. **Fix reconnect subscription gap**: start subscription at `Connecting`, not `Connected`
12. **Add `send_command` error propagation** via `Task<Message>` instead of fire-and-forget spawn

### Sprint 4 — Shared Protocol Crate (1 week)

13. **Extract `luna-protocol` crate**: shared DTO types, `ClientCommand`, `ServerEvent` — eliminates server/dto.rs duplicate and drift risk

### Sprint 5 — Testability (ongoing)

14. **Introduce `TtsSpeaker` trait** — enables mock TTS in tests
15. **Introduce page props structs** — narrow view-model inputs instead of `&LunaThinApp`
16. **Parameterise widgets by message type** — enables widget unit tests without app
17. **Write first 20 tests**: streaming delta application, server event reducer, markdown strip edge cases, WS reconnect state machine
