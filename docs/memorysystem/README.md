# LunaAI Memory System

Overview of the long-term memory system: how memories are stored, recalled into conversations, and maintained over time.

## Background

LunaAI keeps a **long-term memory** store so the assistant can remember facts, preferences, and context across conversations. The system has three main parts:

1. **Memory store** — SQLite table `memory` with optional category and importance. Full-text search (FTS5) powers keyword-based retrieval.
2. **Memory RAG** — On each user message, relevant memories are retrieved and injected as a system message so the LLM can use them without extra tool calls. Deduplication ensures the same memory is not re-injected in the same conversation.
3. **Deep Sleep** — A periodic background job that (a) summarizes new conversations into a digest, (b) evaluates every existing memory against that digest (update/delete/keep), and (c) extracts new memories from the digest.

Memories can be created or updated by:
- The **LLM** via built-in tools (`store_memory`, `search_memory`, `search_memory_by_category`, `delete_memory`) when the user asks to remember something.
- **Deep Sleep** when it extracts new facts from conversation summaries.

Which memories were shown in which conversation is persisted in `conversation_memory_recalls`, so deduplication survives restarts and the data can be used for analytics or future features (e.g. “never recalled” as a staleness signal).

## Documentation Index

| Document | Description |
|----------|-------------|
| [Architecture](architecture.md) | Schema, storage, config, and component map |
| [Flows](flows.md) | Request flows and sequence diagrams (RAG, recalls, Deep Sleep) |

## Key Code Locations

| Area | Path |
|------|------|
| Memory RAG | `src/services/memory_rag.rs` |
| Deep Sleep service | `src/services/deep_sleep_service.rs` |
| Memory + recalls schema & CRUD | `src/storage/sqlite_storage_simple.rs` |
| RAG injection + recall recording | `src/server/handlers.rs` (`handle_send_message`) |
| Deep Sleep loop spawn | `src/server/mod.rs` (`spawn_deep_sleep_loop`) |
| Built-in memory tools | `src/agentic/loop_engine.rs` |
| Deep Sleep config | `src/config/mod.rs` (`DeepSleepConfig`) |

## Enabling and Running

- **Memory tools** — Always available when storage is wired into `AgenticLoop` (default server path).
- **Memory RAG** — Always on when the server runs; injection happens in `handle_send_message` before context selection.
- **Deep Sleep** — Opt-in via config. Set `[deep_sleep] enabled = true` and a `profile` (e.g. `ollama-local`). Runs on an interval (default 24h); first check 5 minutes after startup. Manual run: `./cosmic_llm --deep-sleep`.

## Monitoring

Logs use the `tracing` crate. Filter by `RUST_LOG`:

- `RUST_LOG=info` — Cycle start/end, batch progress, memory updates/deletes, new memories stored.
- `RUST_LOG=cosmic_llm::services::deep_sleep_service=debug` — Per-conversation and dedup details.

When running under systemd (e.g. `luna-server`), use `journalctl --user -u luna-server -f | grep "Deep Sleep"` to follow memory maintenance.
