# Conversation-scoped event broadcast — architecture

## Problem

1. **Single recipient**: Only the WebSocket connection that sent `SendMessage` receives streaming/tool events. Other clients with the same conversation open get nothing.
2. **Reconnect gap**: If the client disconnects and reconnects, the new connection has a new channel. The in-flight agent task still sends to the old (dead) channel, so the reconnected client receives no events until they reload the conversation.

## Goal

- **Multi-client**: All clients that are “watching” a conversation receive events for that conversation (streaming, tool calls, completion, errors).
- **Reconnect**: When a client reconnects and loads a conversation, they are added as a subscriber and receive all events for that conversation from that point (including any in-flight run). No need to “reattach” to a specific run — subscription is per conversation.

## Design

### 1. Conversation subscriptions (pub/sub)

Introduce a **conversation-scoped event bus** shared by all connections:

- **Subscribers**: For each conversation we keep a set of “subscribers” = `(connection_id, UnboundedSender<ServerEvent>)`.
- **Viewing**: Each connection is considered to be “viewing” at most one conversation at a time (the one they loaded or just sent a message to). Viewing a conversation makes that connection a subscriber for that conversation.
- **Broadcast**: When the agent produces an event (e.g. `AssistantDelta`, `ToolStarted`, `ConversationComplete`), we send it to **all** subscribers of that conversation instead of a single `out_tx`.

### 2. Event taxonomy

| Scope            | Events                                                                 | Delivery                |
|-----------------|------------------------------------------------------------------------|-------------------------|
| **Connection**  | `HealthOk`, `ConversationsList`, `ConversationLoaded`, `ProfileChanged`, `ProfilesList`, `Error` (parse/auth), `ConversationCreated`, `MessageAccepted`, `ConversationDeleted`, `StreamingStopped`, `SearchResults` | Single connection (`out_tx`) |
| **Conversation**| `StreamingStarted`, `AssistantDelta`, `ReasoningContentDelta`, `AssistantComplete`, `ToolPlanned`, `ToolStarted`, `ToolResult`, `ToolError`, `ConversationComplete`, `Error` (model/agent) | Broadcast to all subscribers of that conversation |

Connection-scoped events stay on `self.outbound`. Conversation-scoped events are sent via `subscriptions.broadcast(conversation_id, event)`.

### 3. Subscription lifecycle

| Action | Effect |
|--------|--------|
| **LoadConversation(conv_id)** | `set_viewing(connection_id, Some(conv_id))`: remove this connection from its previous conversation’s subscribers (if any); add it to `conv_id`’s subscribers. |
| **StartConversation** | No conversation yet; no subscription. |
| **SendMessage(Some(conv_id))** | Ensure this connection is viewing `conv_id` (they usually already loaded it) → `set_viewing(connection_id, Some(conv_id))`. |
| **SendMessage(None)** | New conversation is created; then `set_viewing(connection_id, Some(new_conv_id))` so the sender is a subscriber before the agent task starts. |
| **Connection closed** | `on_connection_closed(connection_id)`: remove this connection from whatever conversation it was viewing. |

So: one “viewing” conversation per connection; subscribing = adding that connection’s sender to that conversation’s subscriber list.

### 4. Agent task

Today the spawned task holds a **clone** of the handler’s `out_tx` and sends all events there. Change:

- The task receives: `subscriptions: Arc<ConversationSubscriptions>`, `conversation_id: Uuid`.
- It does **not** receive an `out_tx`.
- For every conversation-scoped event it calls `subscriptions.broadcast(conversation_id, event)`.
- Failed sends (e.g. receiver dropped) can be ignored or used to remove dead subscribers in a follow-up; at minimum, broadcast must not panic if a send fails.

### 5. StopStreaming

Today `StopStreaming` aborts all inflight tasks for that handler (all conversations). With broadcast, “inflight” is per conversation and not tied to a single connection. Options:

- **A** (recommended): Keep aborting only the tasks that **this connection** started (current behaviour: abort all in `session.inflight`). So if client A sends StopStreaming, we abort A’s inflight tasks; other clients’ views are unaffected except that the run stops and they get no more events (and we could send `StreamingStopped` via broadcast so all viewers see it).
- **B**: StopStreaming means “abort the in-flight run for this conversation” regardless of who started it (would require tracking run by conversation_id and aborting that run; all viewers get StreamingStopped).

We keep **A** for minimal change: StopStreaming still clears `session.inflight` for this connection. The run is aborted; we already send `StreamingStopped` to `self.outbound` — we should **broadcast** `StreamingStopped` so all subscribers see it. So: when we abort, broadcast `StreamingStopped` for that conversation.

### 6. Component overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     ConversationSubscriptions                            │
│  viewing: ConnectionId → ConversationId (optional)                      │
│  subscribers: ConversationId → Vec<(ConnectionId, Sender<ServerEvent>)>  │
│  set_viewing(conn, conv_id)   broadcast(conv_id, event)                  │
│  on_connection_closed(conn)                                              │
└─────────────────────────────────────────────────────────────────────────┘
         ▲                                    ▲
         │ set_viewing / on_connection_closed │ broadcast(conv_id, event)
         │                                    │
┌────────┴────────┐                 ┌────────┴────────┐
│ handle_ws_      │                 │ Agent task      │
│ upgraded        │                 │ (handle_send_   │
│ (per connection)│                 │  message spawn) │
└─────────────────┘                 └─────────────────┘
```

- **ServerContext** holds `subscriptions: Arc<ConversationSubscriptions>`.
- **handle_ws_upgraded** creates a `connection_id`, passes it to `ServerHandler::new(..., connection_id)`, and on exit calls `subscriptions.on_connection_closed(connection_id)`.
- **ServerHandler** stores `connection_id`, uses it in `handle_load_conversation`, `handle_send_message` (new conv), and optionally in `handle_start_conversation` when we don’t have a conv yet.
- **handle_send_message** (and any helper that emits conversation events) calls `set_viewing(connection_id, Some(conversation_uuid))` so the sender is subscribed, then spawns the task with `(subscriptions, conversation_id)`; the task uses `subscriptions.broadcast(conversation_id, event)` only.

### 7. Reconnect behaviour

- User had conversation X open, sent a message, then closed the app. Agent task is still running and broadcasting to whoever was in `subscribers(X)`. When the app closed we called `on_connection_closed(connection_id)`, so that connection was removed from `subscribers(X)`.
- User reopens, new WebSocket, new `connection_id`. They send `LoadConversation(X)`. We call `set_viewing(new_connection_id, Some(X))` → new connection is added to `subscribers(X)`.
- From that moment, all events for X (including the still-running run) are broadcast to the new connection too. So they receive the rest of the stream without reloading (and can still call LoadConversation to refresh state if needed).

### 8. Files to add/change

| File | Change |
|------|--------|
| **New: `src/server/conversation_subscriptions.rs`** | `ConversationSubscriptions` with `set_viewing`, `broadcast`, `on_connection_closed`; `ConnectionId` type; internal maps under `RwLock`. |
| **`src/server/mod.rs`** | Add `mod conversation_subscriptions`; add `subscriptions: Arc<ConversationSubscriptions>` to `ServerContext` construction. |
| **`src/server/websocket.rs`** | Pass `connection_id` into `ServerHandler::new`; after read loop exits call `ctx.subscriptions.on_connection_closed(connection_id)`. |
| **`src/server/handlers.rs`** | `ServerHandler`: add `connection_id: ConnectionId`; in `handle_load_conversation` call `set_viewing(connection_id, Some(uuid))`; in `handle_send_message` after resolving/creating conversation call `set_viewing(connection_id, Some(conversation_uuid))`; spawn task with `(ctx.subscriptions.clone(), conversation_uuid)` and use `subscriptions.broadcast` in `process_agent_update` (or equivalent). `handle_stop_streaming`: broadcast `StreamingStopped` for the conversation(s) being stopped. |

### 9. Edge cases

- **SendMessage without prior LoadConversation**: For existing conv we call `set_viewing(connection_id, Some(conversation_uuid))` before spawning so the sender is a subscriber. For new conv we do the same after creating it.
- **Broadcast send failure**: If `sender.send(event)` fails (receiver dropped), remove that `(connection_id, sender)` from the conversation’s list so we don’t keep trying. Optional: also call cleanup in a central place when we detect closed connections.
- **ConnectionId type**: Newtype or alias (e.g. `uuid::Uuid`) so we don’t mix up with conversation ids.

This design closes the multi-client and reconnect gaps while keeping connection-scoped responses on the existing single channel and only changing how conversation-scoped events are delivered.

### 10. Client changes (thin UI, mobile app)

**Protocol:** No new commands or event fields. Clients already send `LoadConversation` when opening a conversation and handle streaming events for the current view.

**Reconnect:** To receive the tail of an in-flight stream after reconnect, clients must re-send `LoadConversation(conversation_id)` for the conversation they had open so the server adds them as a subscriber.

- **Thin UI:** `on_connect()` now sends `LoadConversation { conversation_id }` when `current_conversation_id` is set, so after reconnect we re-subscribe and receive any in-flight stream.
- **Mobile app:** After silent reconnect, when `wasInChat && activeConversationId != null`, we always send `loadConversation(activeConversationId)` (not only when “conversation was lost”) so we re-subscribe and receive the tail of any in-flight stream.
