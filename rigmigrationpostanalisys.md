# RIG Migration Post-Analysis Report

**Generated:** After switching from AgenticLoop to Rig as the sole conversation engine.

**Scope:** Full analysis of every file under `src/` — 42 files reviewed by 4 subagents.

---

## Executive Summary

| Category | Count |
|----------|-------|
| **KEEP (no changes)** | 28 files |
| **MODIFY** | 13 files |
| **DELETE (candidate)** | 2 files |

**Critical finding:** ~~System prompts from `ContextService::inject_prompts` are never used by Rig~~ **FIXED:** Preamble support added; engine extracts system content and passes to pipeline.

---

## 1. Agentic & Config

### `src/agentic/mod.rs`
**Status:** MODIFY (or DELETE)  
**Summary:** Stub module; `loop_engine.rs` and `protocol.rs` were removed. MCP lives in `agentic-loop` crate.

**Changes:**
- [Line 1-3] Entire file — **delete** — Stub only; either remove module and re-exports from `lib.rs`/`main.rs`, or keep minimal placeholder.
- If keeping: Update comment to "Reserved for future extensions; MCP in agentic-loop crate."

---

### `src/config/mod.rs`
**Status:** KEEP  
**Summary:** Config types used by Rig, handlers, services. No AgenticLoop-specific logic.

**Changes:** No changes needed.

---

## 2. Embeddings & LLM

### `src/embeddings/mod.rs`
**Status:** KEEP  
**Summary:** `EmbeddingProvider` used for memory RAG and deep sleep. Referenced by `rig_tools` and `ServerContext`.

**Changes:** No changes needed.

---

### `src/lib.rs`
**Status:** MODIFY  
**Summary:** Re-exports `agentic`; otherwise fine for Rig-only setup.

**Changes:**
- [Line 1] `pub mod agentic;` — **delete** — If `agentic` module is removed. If stub is kept, leave as-is.

---

### `src/llm/mod.rs`
**Status:** KEEP  
**Summary:** `LlmClient`, `Message`, `Role`, `build_llm_client` used by ContextService, title generation, deep sleep, SessionState. Rig engine does not use them; other features do.

**Changes:** No changes needed.

---

### `src/llm/openai.rs`
**Status:** KEEP  
**Summary:** `OpenAIClient` used for summarization, title generation, deep sleep. Rig uses its own OpenAI provider.

**Changes:** No changes needed.

---

### `src/llm/anthropic.rs`
**Status:** KEEP  
**Summary:** `AnthropicClient` for anthropic preset (summarization, title generation, deep sleep).

**Changes:** No changes needed.

---

### `src/llm/ollama.rs`
**Status:** KEEP  
**Summary:** `OllamaClient` for Ollama profiles.

**Changes:** No changes needed.

---

### `src/llm/gemini.rs`
**Status:** KEEP  
**Summary:** `GeminiClient` for Gemini profiles.

**Changes:** No changes needed.

---

### `src/llm/context_manager.rs`
**Status:** KEEP  
**Summary:** `SmartContextManager` used by handlers and context_service for context selection before engine.

**Changes:** No changes needed.

---

### `src/llm/file_utils.rs`
**Status:** KEEP  
**Summary:** File attachments in messages. No AgenticLoop coupling.

**Changes:** No changes needed.

---

### `src/llm/tokenizer.rs`
**Status:** KEEP  
**Summary:** `TokenCounter` used by SmartContextManager, ContextService, handlers.

**Changes:** No changes needed.

---

## 3. MCP

### `src/mcp/mod.rs`
**Status:** MODIFY  
**Summary:** Re-exports `tool_call_to_params` and `tools_to_definitions` — not used. `rig_tools` builds `ToolDefinition` directly from `McpTool`.

**Changes:**
- [Line 1-2] Remove or deprecate `tool_call_to_params`, `tools_to_definitions` — REASON: Dead code for Rig.

---

### `src/mcp/conversions.rs`
**Status:** MODIFY  
**Summary:** Luna↔MCP conversions. Not used after Rig migration; `rig_tools` uses `McpTool` directly.

**Changes:**
- [Lines 1-151] Entire file — **keep or delete** — Dead code for Rig. If no future Luna↔MCP path planned, delete and remove from `mcp/mod.rs`.
- If keeping: Add `#[allow(dead_code)]` or `#[deprecated]` to silence warnings.

---

## 4. Prompts & Rig Core

### `src/prompts.rs`
**Status:** KEEP  
**Summary:** Prompt config and manager used by `ContextService::inject_prompts` and `ServerContext`.

**Changes:** No changes needed.

---

### `src/rig_core/mod.rs`
**Status:** KEEP  
**Summary:** Rig module entry point; exports adapters and pipeline.

**Changes:** No changes needed.

---

### `src/rig_core/adapters.rs`
**Status:** KEEP  
**Summary:** Luna ↔ Rig conversion. System messages skipped (Rig uses preamble).

**Changes:** No changes needed.

---

### `src/rig_core/pipeline.rs`
**Status:** MODIFY  
**Summary:** Pipeline correct but uses hardcoded preamble; ignores system prompt from `ContextService::inject_prompts`.

**Changes:**
- [Line 18-27] `struct RigConversationContext` — **modify** — Add `preamble: String` (or `Option<String>`).
- [Line 60-101] `fn run_turn` — **modify** — Use `context.preamble` instead of hardcoded `"You are a helpful assistant."`.
- [Line 105-170] `fn run_turn_streaming` — **modify** — Same preamble handling.
- [Line 4] Doc comment — **modify** — Remove `engine = "rig"` wording; Rig is now the only engine.

---

## 5. Server

### `src/server/mod.rs`
**Status:** KEEP  
**Summary:** Uses `RigEngine`, `PromptManager`, `agentic_loop` for MCP only.

**Changes:** No changes needed.

---

### `src/server/engine.rs`
**Status:** MODIFY  
**Summary:** `RigEngine` wired correctly; system prompts dropped because pipeline uses hardcoded preamble.

**Changes:**
- [Line 59-68] `fn run_turn` — **modify** — Extract system content from `params.agent_messages` before building `history`, build preamble string, pass in `RigConversationContext`.
- [Line 15-22] `struct TurnParams` — **keep** — `llm_client` unused by Rig but used by SessionState for summarization; consider documenting.

---

### `src/server/dto.rs`
**Status:** KEEP  
**Summary:** DTOs for wire protocol; no engine-specific logic.

**Changes:** No changes needed.

---

### `src/server/handlers.rs`
**Status:** KEEP  
**Summary:** Uses `ConversationEngine` and `TurnParams` correctly.

**Changes:** No changes needed.

---

### `src/server/http.rs`
**Status:** KEEP  
**Summary:** HTTP routes; no engine-specific code.

**Changes:** No changes needed.

---

### `src/server/websocket.rs`
**Status:** KEEP  
**Summary:** WebSocket handling; no engine-specific logic.

**Changes:** No changes needed.

---

### `src/server/conversation_subscriptions.rs`
**Status:** KEEP  
**Summary:** Event broadcast for conversation subscribers.

**Changes:** No changes needed.

---

### `src/server/rig_tools.rs`
**Status:** KEEP  
**Summary:** Rig tool wrappers (MCP + internal tools). Correct for Rig-only architecture.

**Changes:** No changes needed.

---

### `src/main.rs`
**Status:** KEEP  
**Summary:** Entry point; `mod agentic` remains for MCP support.

**Changes:** No changes needed.

---

## 6. Services

### `src/services/mod.rs`
**Status:** MODIFY  
**Summary:** Re-exports unused services (`ToolCallManager`, `MCPService`).

**Changes:**
- [Line 16-17] Remove or mark `#[allow(dead_code)]` for `ToolCallManager`, `MCPService` — REASON: Neither used.
- [Line 16] Comment "ToolCallManager not used" — Update reference from loop_engine to rig_tools.

---

### `src/services/message_converter.rs`
**Status:** KEEP  
**Summary:** Used by handlers, rig_core adapters, context_service. Engine-agnostic.

**Changes:** No changes needed.

---

### `src/services/mcp_service.rs`
**Status:** MODIFY (or DELETE)  
**Summary:** `MCPService` never used. Tools policy uses `tools_policy::apply_tools_policy` directly.

**Changes:**
- [Line 10-24] `struct MCPService`, `impl MCPService::apply_profile_tool_defaults` — **delete** or add `#[allow(dead_code)]` — REASON: Dead code.
- **Alternative:** DELETE entire file and remove from `mod.rs` if no future use.

---

### `src/services/schedule_service.rs`
**Status:** KEEP  
**Summary:** Used by handlers, rig_tools, server/mod. Independent of engine.

**Changes:** No changes needed.

---

### `src/services/context_service.rs`
**Status:** KEEP  
**Summary:** Used by handlers for inject_prompts, summarization, context checks.

**Changes:** No changes needed.

---

### `src/services/memory_rag.rs`
**Status:** KEEP  
**Summary:** Used by handlers for `retrieve_memory_context`.

**Changes:** No changes needed.

---

### `src/services/deep_sleep_service.rs`
**Status:** KEEP  
**Summary:** Used for memory maintenance. Independent of engine.

**Changes:** No changes needed.

---

### `src/services/tool_call_manager.rs`
**Status:** MODIFY (or DELETE)  
**Summary:** Never instantiated; contains `todo!()`. Tool execution handled by `rig_tools` via Rig.

**Changes:**
- [Line 33] Comment "agentic/loop_engine.rs" — **modify** to "server/rig_tools.rs" — REASON: loop_engine removed.
- [Line 35-36] `fn execute_tool_call` with `todo!()` — **delete** or keep with `#[allow(dead_code)]` — REASON: Never called.
- **Alternative:** DELETE entire file and remove from `mod.rs`.

---

## 7. Storage

### `src/storage/mod.rs`
**Status:** KEEP  
**Summary:** Module layout fine; re-exports correct.

**Changes:** No changes needed.

---

### `src/storage/conversation_storage.rs`
**Status:** MODIFY  
**Summary:** Contains legacy AgenticLoop structures (Turn, file-based Storage, `rebuild_llm_messages`).

**Changes:**
- [Line 173-179] `struct Storage` — **delete** — Legacy file-based; `storage_wrapper::Storage` (SQLite) is the only implementation.
- [Line 181-189] `impl Default for Storage` — **delete**.
- [Line 192-399] `impl Storage` (entire block) — **delete** — File-based implementation.
- [Line 113-161] `fn rebuild_llm_messages` — **delete** — Unused; `MessageConverter::db_to_llm` used instead.
- [Line 12-36] `struct Turn`, `ToolCallInfo`, `ToolCallStatus` — **keep** — Referenced by storage_wrapper.
- **Note:** `add_turn_to_conversation` in storage_wrapper is a no-op. Consider deprecating Turn-related code.

---

### `src/storage/sqlite_storage_simple.rs`
**Status:** KEEP  
**Summary:** SQLite backend used by storage_wrapper. Compatible with RIG.

**Changes:** No changes needed.

---

### `src/storage/storage_wrapper.rs`
**Status:** MODIFY  
**Summary:** Wrapper correct for RIG. Turn handling is legacy.

**Changes:**
- [Line 247-255] `fn add_turn_to_conversation` — **modify** — Add comment that Turn storage is legacy/AgenticLoop; no-op intentional for SQLite.

---

### `src/storage/title_generation.rs`
**Status:** KEEP  
**Summary:** Title generation storage-agnostic; works with RIG.

**Changes:**
- [Line 104-105] Trailing comment "Function removed - not used anywhere" — **delete** — Remove dead comment.

---

## 8. Tools Policy & Types

### `src/tools_policy.rs`
**Status:** MODIFY  
**Summary:** Logic used by handlers and server; doc comment outdated.

**Changes:**
- [Line 22] Doc comment for `AppliedToolsPolicy` — **modify** — Replace "loop_engine" with "Rig engine" or "conversation engine".

---

### `src/types.rs`
**Status:** MODIFY  
**Summary:** `From<&StorageMessage> for LlmMessage` may be redundant with `MessageConverter::db_to_llm`.

**Changes:**
- [Line 12-56] `impl From<&StorageMessage> for LlmMessage` — **verify then modify or delete** — If no code uses `LlmMessage::from(&storage_msg)` or `(&storage_msg).into()`, delete impl.
- [Line 58-61] Trailing comments — **delete** — Remove dead comments.
- **Recommendation:** Grep for usage; if none, delete impl.

---

## Summary Table

| File | Status | Primary Action |
|------|--------|----------------|
| `src/agentic/mod.rs` | MODIFY | Delete or keep minimal stub |
| `src/config/mod.rs` | KEEP | None |
| `src/embeddings/mod.rs` | KEEP | None |
| `src/lib.rs` | MODIFY | Remove `agentic` if deleted |
| `src/llm/mod.rs` | KEEP | None |
| `src/llm/openai.rs` | KEEP | None |
| `src/llm/anthropic.rs` | KEEP | None |
| `src/llm/ollama.rs` | KEEP | None |
| `src/llm/gemini.rs` | KEEP | None |
| `src/llm/context_manager.rs` | KEEP | None |
| `src/llm/file_utils.rs` | KEEP | None |
| `src/llm/tokenizer.rs` | KEEP | None |
| `src/mcp/mod.rs` | MODIFY | Remove unused exports |
| `src/mcp/conversions.rs` | MODIFY | Delete or mark dead |
| `src/prompts.rs` | KEEP | None |
| `src/rig_core/mod.rs` | KEEP | None |
| `src/rig_core/adapters.rs` | KEEP | None |
| `src/rig_core/pipeline.rs` | MODIFY | Add preamble support |
| `src/server/mod.rs` | KEEP | None |
| `src/server/engine.rs` | MODIFY | Extract system prompt, pass preamble |
| `src/server/dto.rs` | KEEP | None |
| `src/server/handlers.rs` | KEEP | None |
| `src/server/http.rs` | KEEP | None |
| `src/server/websocket.rs` | KEEP | None |
| `src/server/conversation_subscriptions.rs` | KEEP | None |
| `src/server/rig_tools.rs` | KEEP | None |
| `src/main.rs` | KEEP | None |
| `src/services/mod.rs` | MODIFY | Remove/gate unused exports |
| `src/services/message_converter.rs` | KEEP | None |
| `src/services/mcp_service.rs` | MODIFY/DELETE | Remove dead code or delete |
| `src/services/schedule_service.rs` | KEEP | None |
| `src/services/context_service.rs` | KEEP | None |
| `src/services/memory_rag.rs` | KEEP | None |
| `src/services/deep_sleep_service.rs` | KEEP | None |
| `src/services/tool_call_manager.rs` | MODIFY/DELETE | Update comment, remove dead code |
| `src/storage/mod.rs` | KEEP | None |
| `src/storage/conversation_storage.rs` | MODIFY | Remove file-based Storage, rebuild_llm_messages |
| `src/storage/sqlite_storage_simple.rs` | KEEP | None |
| `src/storage/storage_wrapper.rs` | MODIFY | Document Turn legacy |
| `src/storage/title_generation.rs` | KEEP | Remove dead comment |
| `src/tools_policy.rs` | MODIFY | Update doc comment |
| `src/types.rs` | MODIFY | Verify From impl; remove dead comments |

---

## Critical Fix: System Prompt Not Used by Rig

`ContextService::inject_prompts` adds system prompts to `agent_messages`, but:
1. `luna_messages_to_rig_history` drops `Role::System` messages
2. Pipeline uses hardcoded `"You are a helpful assistant."`

**Required changes:**
1. Add `preamble: String` to `RigConversationContext`
2. In `engine.rs`, extract system content from `params.agent_messages` and build preamble
3. In `pipeline.rs`, use `context.preamble` (or default) instead of hardcoded string

---

## Suggested Implementation Order

1. ~~**High priority:** System prompt / preamble (engine.rs, pipeline.rs, rig_core adapters)~~ **DONE**
2. ~~**Cleanup:** agentic stub, mcp conversions, mcp_service, tool_call_manager~~ **DONE**
3. ~~**Docs/comments:** tools_policy, storage_wrapper, title_generation, types~~ **DONE**
4. **Larger refactor:** conversation_storage (file-based Storage removal) — verify dependencies first — *deferred*

---

## Completed (this pass)

- **Preamble:** `RigConversationContext` now has `preamble: String`; engine extracts System messages and passes to pipeline; pipeline uses preamble instead of hardcoded string
- **agentic:** Updated comment
- **mcp:** Removed unused exports (`tool_call_to_params`, `tools_to_definitions`); removed `ToolInputSchema` import
- **services:** Removed MCPService, ToolCallManager exports; removed parse_run_at, validate_schedule exports; added `#[allow(dead_code)]` to MCPService
- **tool_call_manager:** Updated comment (loop_engine → rig_tools)
- **tools_policy:** Doc comment (loop_engine → Rig engine)
- **storage_wrapper:** Documented Turn legacy in `add_turn_to_conversation`
- **title_generation:** Removed dead comment
- **types:** Removed dead comments
