# Storage Layer Analysis: Eliminating storage_wrapper

This document analyzes how to remove `storage_wrapper.rs` and use wire DTOs (or Rig-aligned types) directly, avoiding the intermediate `FileConversation` / `StoredMessage` layer.

---

## 1. Current Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  HANDLERS (wire protocol)                                                        │
│  get_conversation, list_conversations, load_conversation                          │
└─────────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│  Storage (storage_wrapper)                                                        │
│  Returns: FileConversation with Vec<StoredMessage>                                │
└─────────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│  to_conversation_view() → ConversationView, MessageView (wire DTOs)               │
└─────────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────────┐
│  ENGINE PATH (Rig / LLM)                                                         │
│  build_llm_messages, run_scheduled_task                                           │
└─────────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│  load_conversation_messages() → Vec<sqlite::Message>  ← BYPASSES storage_wrapper  │
└─────────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│  MessageConverter::db_to_llm() → Vec<LlmMessage> → luna_messages_to_rig_history() │
│  → Vec<RigMessage>                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Key insight:** The engine path already bypasses `storage_wrapper` and uses `sqlite_storage_simple::Message` directly. Only the **wire path** (load/list conversations for clients) goes through `FileConversation` / `StoredMessage`.

---

## 2. Type Comparison

| Layer | Conversation | Message | ID | Timestamp |
|-------|---------------|---------|-----|-----------|
| **sqlite_storage_simple** | `Conversation` (id: String, created_at: i64) | `Message` (id: i64 rowid, created_at: i64) | i64 / String | i64 |
| **conversation_storage** | `FileConversation` (id: Uuid) | `StoredMessage` (id: Uuid, timestamp: DateTime) | Uuid | DateTime |
| **Wire (serverspec)** | `ConversationView` (id: String) | `MessageView` (id: String, timestamp: i64) | String | i64 |
| **Rig** | N/A | `RigMessage` (user/assistant/tool_result) | None | None |

**Rig's Message** is minimal: `user(content)`, `assistant(content)`, `tool_result(id, content)`. It has no id, timestamp, or tool metadata. It is **not** suitable as a storage or wire DTO.

**Wire DTOs** (`MessageView`, `ConversationView`) are the canonical client-facing format. They match `serverspec.md`.

---

## 3. What storage_wrapper Actually Does

| Method | Purpose | Depends on FileConversation? |
|--------|---------|------------------------------|
| `get_conversation` | Load conv + messages for wire | Yes – returns `FileConversation` |
| `list_conversations_paginated` | List for wire | Yes – returns `Vec<FileConversation>` |
| `add_message_to_conversation` | Persist user message | No – returns Uuid (rowid→UUID conversion) |
| `add_message_with_metadata` | Persist assistant/tool messages | No |
| `truncate_conversation` | Delete messages up to message_id | Yes – UUID→rowid lookup |
| `load_conversation_messages` | For engine + title gen | No – returns `Vec<sqlite::Message>` directly |
| Memory, scheduled jobs, etc. | Pass-through | No |

**Conversion logic in storage_wrapper:**

1. **rowid → UUID**: `format!("00000000-0000-0000-0000-{:012x}", rowid)` for stable message IDs
2. **i64 → DateTime**: `DateTime::from_timestamp(secs, 0)` for `StoredMessage`
3. **sqlite::Message → StoredMessage**: Field mapping (role, content, tool_calls, etc.)

---

## 4. Ways to Eliminate storage_wrapper

### Option A: Sqlite → Wire DTOs Directly

**Idea:** Add `From<&sqlite::Message> for MessageView` and build `ConversationView` from sqlite types. Storage returns wire-ready types.

**Changes:**

1. Add `message_rowid_to_string_id(rowid: i64) -> String` (same deterministic format).
2. Add `impl From<&sqlite_storage_simple::Message> for MessageView` using rowid as id.
3. Add `fn to_conversation_view_from_sqlite(conv: &sqlite::Conversation, messages: &[sqlite::Message]) -> ConversationView`.
4. `SqliteStorage` (or a thin facade) exposes:
   - `get_conversation_view(&self, id: &str) -> Option<ConversationView>`
   - `list_conversation_views(...) -> Vec<ConversationSummary>` (or full `ConversationView` if needed)
5. Handlers call these directly. Remove `storage_wrapper`, `FileConversation`, `StoredMessage`.

**Pros:** Single source of truth (sqlite types). Wire format is the only output.  
**Cons:** `ConversationSummary` needs `last_message_preview` – requires loading messages for list (or denormalizing in DB).

---

### Option B: Wire DTOs as Canonical Storage Output

**Idea:** Storage layer returns `ConversationView` / `MessageView` (or equivalent) as its public API. Sqlite is an implementation detail.

**Changes:**

1. Define `ConversationView` / `MessageView` in `dto.rs` (already done).
2. Add conversion: `sqlite::Message` → `MessageView` (id = rowid-to-string).
3. Storage trait/facade: `get_conversation(id) -> Option<ConversationView>`.
4. `SqliteStorage` implements this. No `FileConversation` / `StoredMessage`.

**Pros:** Handlers only see wire types. Clean boundary.  
**Cons:** Same as A for list + preview.

---

### Option C: Keep Sqlite Types, Add Conversion at Handler Boundary

**Idea:** Storage returns sqlite types. Handlers convert to wire DTOs in one place.

**Changes:**

1. Storage exposes `get_conversation(id) -> Option<(sqlite::Conversation, Vec<sqlite::Message>)>`.
2. Add `sqlite_to_conversation_view(conv, messages) -> ConversationView`.
3. Handlers call this. Remove `storage_wrapper`, `FileConversation`, `StoredMessage`.

**Pros:** Minimal changes. Conversion logic in one place.  
**Cons:** Handlers depend on sqlite types (or we add a small storage trait).

---

## 5. UUID / ID Handling

The wire protocol uses **opaque string IDs** (UUIDs in the reference impl). Current format:

- **Conversation:** UUID string (from DB `conversations.id`).
- **Message:** `00000000-0000-0000-0000-{rowid:012x}` (deterministic from sqlite rowid).

To remove storage_wrapper:

1. **Conversation ID:** Keep as-is (DB already stores UUID-like string).
2. **Message ID:** Extract `rowid_to_message_id(rowid: i64) -> String` and `message_id_to_rowid(s: &str) -> Option<i64>` into a small util.
3. Use these in:
   - `MessageView::from(&sqlite::Message)`
   - `truncate_conversation` (parse message_id to find rowid)

---

## 6. Recommended Path: Option C (Minimal Refactor)

| Step | Action |
|------|--------|
| 1 | Add `storage::id_utils` with `rowid_to_message_id`, `message_id_to_rowid` |
| 2 | Add `impl From<&sqlite::Message> for MessageView` using `rowid_to_message_id` |
| 3 | Add `fn to_conversation_view_sqlite(conv: &sqlite::Conversation, messages: &[sqlite::Message]) -> ConversationView` |
| 4 | Change `Storage` to wrap `SqliteStorage` but return `ConversationView` from `get_conversation` / `list_conversations_paginated` (or equivalent) |
| 5 | Update handlers to use new return types; remove `to_conversation_view(StoredConversation)` |
| 6 | Delete `FileConversation`, `StoredMessage`, `conversation_storage` (or reduce to Turn if still needed) |
| 7 | Inline storage_wrapper logic into `Storage` or merge into `SqliteStorage` + thin facade |

**Result:** One storage implementation, sqlite types internally, wire DTOs at the boundary. No `FileConversation` / `StoredMessage`.

---

## 7. Rig DTOs: Clarification

**Rig's `rig::message::Message`** is for the LLM API only. It is not a storage or wire DTO.

The pipeline flow is:

```
sqlite::Message → MessageConverter::db_to_llm → LlmMessage → luna_messages_to_rig_history → RigMessage
```

This path does **not** use storage_wrapper. To "use Rig DTOs directly" in storage:

- **Not feasible:** Rig Message has no id, timestamp, or metadata – cannot represent stored messages.
- **Feasible:** Use **wire DTOs** (`MessageView`, `ConversationView`) as the storage output format, and keep the existing `LlmMessage` → `RigMessage` conversion in the pipeline.

---

## 8. Files to Touch

| File | Change |
|------|--------|
| `src/storage/storage_wrapper.rs` | Remove or merge into SqliteStorage |
| `src/storage/conversation_storage.rs` | Remove `Conversation`, `StoredMessage`; keep `Turn` only if used |
| `src/server/dto.rs` | Add `From<&sqlite::Message> for MessageView` (or equivalent) |
| `src/server/handlers.rs` | Use new storage API, remove `to_conversation_view(StoredConversation)` |
| `src/storage/mod.rs` | Update exports |
| `src/types.rs` | `From<&StorageMessage> for LlmMessage` – keep (used by MessageConverter) |

---

## 9. Summary

| Question | Answer |
|----------|--------|
| Can we use Rig's Message as storage/wire DTO? | No – too minimal, no id/timestamp/metadata |
| Can we remove storage_wrapper? | Yes – by converting sqlite types to wire DTOs at the boundary |
| Best approach? | Option C: sqlite types internally, `ConversationView`/`MessageView` at handler boundary |
| Engine path impact? | None – already uses sqlite + MessageConverter |
