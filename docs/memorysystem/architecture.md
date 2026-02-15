# Memory System Architecture

## Schema

All memory-related data lives in the same SQLite database as conversations (`conversations.db` by default).

### Table: `memory`

Long-term memory entries. Content is indexed for full-text search.

| Column | Type | Description |
|--------|------|-------------|
| id | INTEGER PK | Auto-increment |
| content | TEXT NOT NULL | Fact or note to remember |
| category | TEXT | Optional tag (e.g. workflow, personal, work) |
| importance | INTEGER | 1–10, default 5 |
| created_at | INTEGER | Unix timestamp |
| updated_at | INTEGER | Set on create and on update (e.g. by Deep Sleep) |

### Virtual table: `memory_fts`

FTS5 virtual table for full-text search over `memory.content`. Kept in sync by triggers on INSERT/DELETE; UPDATE is handled manually in `update_memory()`.

- Queries use OR semantics over keywords; ranking via BM25.
- Used by Memory RAG (retrieval) and Deep Sleep (dedup of proposed new memories).

### Table: `conversation_memory_recalls`

Junction table: which memories were recalled (injected) in which conversation.

| Column | Type | Description |
|--------|------|-------------|
| conversation_id | TEXT | UUID of the conversation |
| memory_id | INTEGER | FK to memory(id) ON DELETE CASCADE |
| recalled_at | INTEGER | Unix timestamp of first recall |

Primary key: `(conversation_id, memory_id)` — each pair stored once. Index on `memory_id` for “conversations that saw this memory”.

### Table: `deep_sleep_state`

Key-value store for Deep Sleep progress.

| Key | Meaning |
|-----|--------|
| last_processed_message_id | Max message ID included in any processed conversation (watermark) |
| last_run_at | Unix timestamp of last cycle end |

---

## Component Map

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           Server (handle_send_message)                   │
├─────────────────────────────────────────────────────────────────────────┤
│  1. Load / seed used_ids from memory_dedup + get_recalled_memory_ids()   │
│  2. memory_rag::retrieve_memory_context(storage, user_message, used_ids)│
│  3. Insert memory system message into llm_messages                       │
│  4. record_memory_recalls(conversation_id, new_ids)                      │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  Memory RAG (src/services/memory_rag.rs)                                 │
│  - extract_keywords(user_message) → words len ≥ 3, dedup                │
│  - storage.search_memory(keywords, 10) → FTS5, BM25                      │
│  - Filter by used_ids → format system message → return (msg, new_ids)    │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  Storage (SqliteStorage / Storage wrapper)                               │
│  - memory: store_memory, search_memory, search_memory_by_category,      │
│            delete_memory, list_memory, update_memory                     │
│  - recalls: record_memory_recalls, get_recalled_memory_ids               │
│  - deep_sleep_state: get_deep_sleep_state, set_deep_sleep_state          │
│  - deep_sleep: get_conversations_with_messages_after, get_max_message_id │
└─────────────────────────────────────────────────────────────────────────┘
```

```
┌─────────────────────────────────────────────────────────────────────────┐
│  Deep Sleep loop (spawn_deep_sleep_loop, server/mod.rs)                  │
│  - Every 5 min: is_due(storage, interval_hours)?                         │
│  - If due: build LLM client from deep_sleep.profile, run_deep_sleep_cycle│
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  Deep Sleep service (src/services/deep_sleep_service.rs)                │
│  - run_deep_sleep_cycle(): loop until no more unprocessed conversations  │
│    - Step 1: summarize batch of conversations → session_digest           │
│    - Step 2: evaluate all memories vs digest (KEEP/UPDATE/DELETE)        │
│    - Step 3: extract new memories from digest, FTS5 dedup, store         │
│    - Persist watermark (last_processed_message_id)                      │
│  - is_due(): last_run_at + interval_hours <= now                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## In-Process State

- **memory_dedup** — `ServerContext.memory_dedup: Mutex<HashMap<Uuid, HashSet<i64>>>`. Per-conversation set of memory IDs already injected this run. Seeded from `get_recalled_memory_ids` when the set is empty (e.g. after restart). Updated by Memory RAG and persisted via `record_memory_recalls` for new IDs only.

---

## Configuration

### Deep Sleep (`[deep_sleep]` in config.toml)

| Option | Default | Description |
|--------|---------|-------------|
| enabled | false | Turn on the background loop |
| profile | (none) | LLM profile name for summarization and evaluation |
| interval_hours | 24 | Run cycle when this many hours since last_run_at |
| memory_batch_size | 20 | Memories per LLM call in Step 2 |
| max_conversations_per_run | 50 | Conversations per batch in Step 1 (cycle runs until backlog is done) |
| inter_call_delay_secs | 2 | Delay between LLM calls (e.g. for RPi4 thermal) |

Deep Sleep is off until `enabled = true` and `profile` is set. No separate config for Memory RAG or recalls; they use the default DB and run whenever the server runs.

---

## Built-in memory tools (LLM)

Exposed when `AgenticLoop` is constructed with storage:

- **store_memory** — content, optional category, optional importance.
- **search_memory** — keywords (array), FTS5 OR search.
- **search_memory_by_category** — category string.
- **delete_memory** — memory_id.

Implemented in `src/agentic/loop_engine.rs`; execution goes through `execute_memory_tool` and the storage wrapper, not MCP.
