# LunaAI Static Code Analysis Report

> **Scope:** Full workspace (`cosmic_llm`, `agentic-loop`, `luna_thin_ui`)
> **Date:** 2026-06-15
> **Tools:** `cargo check`, `cargo clippy`, `cargo audit`, pattern grep
> **Raw outputs:** `/tmp/luna-clippy.out`, `/tmp/luna-audit.txt`

---

## Executive Summary

| Metric | Result |
|--------|--------|
| **Build** | ✅ Compiles (`cargo check --workspace`) |
| **Clippy warnings** | ⚠️ **153** unique line-level warnings |
| **Security vulnerabilities** | 🔴 **6** (transitive deps) |
| **Unmaintained / unsound deps** | ⚠️ **7** advisory warnings |
| **Rust source files** | 104 files, ~25,176 lines |
| **Test functions** | 36 `#[test]` blocks across 9 files |
| **Production `unwrap`/`expect`** | 11 sites (excluding tests) |

**Overall health: Moderate.** The codebase compiles cleanly but carries significant dead code, style lint noise, and outdated transitive dependencies — especially in TLS (`rustls-webpki`) and serialization (`bytes`, `time`).

---

## 1. Project Metrics

### 1.1 Lines of Code by Crate

| Crate | Path | Lines |
|-------|------|------:|
| `cosmic_llm` (server + core) | `src/` | 17,241 |
| `luna_thin_ui` (desktop UI) | `luna_thin_ui/src/` | 6,941 |
| `agentic-loop` | `agentic-loop/src/` | 896 |
| **Total** | | **25,178** |

*(Excludes `vendor/` and `target/`)*

### 1.2 Largest Modules (cosmic_llm)

| Module | Approx. role |
|--------|--------------|
| `src/server/handlers/` | WebSocket command dispatch, agent orchestration |
| `src/storage/sqlite_storage_simple.rs` | SQLite persistence (545+ LOC, 15 clippy warnings) |
| `src/agentic/loop_engine.rs` | Agent loop (52 `.clone()` calls — hot path) |
| `src/llm/` | LLM provider adapters (OpenAI, Anthropic, Ollama, Gemini) |
| `luna_thin_ui/src/ui/app.rs` | Desktop app state (41 `.clone()` calls, 9 clippy warnings) |

---

## 2. Clippy Analysis

**Command run:**
```bash
unset ARGV0 && cargo clippy --workspace --all-targets --message-format=short
```

**Result:** 0 errors, 153 line-level warnings (161 total including crate summary lines).

### 2.1 Warning Categories

| Category | Count | Severity |
|----------|------:|----------|
| Unused imports | 39 | Low — auto-fixable |
| Redundant closures | 21 | Low — auto-fixable |
| Dead code (`never used` / `never constructed`) | 28 | Medium — indicates incomplete refactors |
| Hidden lifetimes (`elided_lifetime_in_paths`) | 9 | Low |
| Identical `if` blocks | 5 | Medium — logic smell |
| Too many function arguments (>7) | 4 | Medium — API design |
| Manual trait impls (`derivable_impls`) | 3 | Low — auto-fixable |
| Boolean simplification | 2 | Low |
| Complex type alias candidate | 1 | Low |
| Enum variant naming (`enum_variant_names`) | 1 | Low |

### 2.2 Top Files by Warning Count

| File | Warnings | Notable issues |
|------|:--------:|----------------|
| `src/server/handlers/mod.rs` | 8 | Unused imports, dead fields (`repos`, `llm_observer`) |
| `src/storage/sqlite_storage_simple.rs` | 15 | Multiple unused methods |
| `src/storage/repos.rs` | 6 | Unused traits (`ConversationRepo`, `MemoryRepo`, `MessageRepo`) |
| `src/storage/conversation_storage.rs` | 4 | Entire legacy `Storage` struct unused |
| `src/server/handlers/profile.rs` | 8 | Unused imports |
| `src/server/handlers/memory.rs` | 7 | Unused imports |
| `src/server/handlers/conversation.rs` | 6 | Unused imports |
| `src/server/handlers/agent.rs` | 6 | Unused imports |
| `src/mcp/conversions.rs` | 4 | 3 conversion helpers never used |
| `src/services/mcp_service.rs` | 2 | Entire `MCPService` unused |
| `src/llm/ollama.rs` | 6 | 3 stream response structs never constructed |
| `luna_thin_ui/src/ui/app.rs` | 9 | Redundant closures, unused imports |

### 2.3 Auto-fixable Suggestions

Clippy reports these fix counts if `--fix` is applied:

| Target | Fixable |
|--------|--------:|
| `cosmic_llm` lib | 26 |
| `cosmic_llm` bin | 39 |
| `agentic-loop` lib | 8 |
| `luna_thin_ui` lib | 27 |

**Quick win command:**
```bash
unset ARGV0 && cargo clippy --workspace --fix --allow-dirty --allow-staged
```

Review the diff before committing — `--fix` can change semantics in edge cases.

---

## 3. Security Audit (`cargo audit`)

**Command run:**
```bash
unset ARGV0 && cargo audit
```

**Dependencies scanned:** 885 crates in `Cargo.lock`

### 3.1 Vulnerabilities (action required)

| Crate | Version | Advisory | Severity | Fix |
|-------|---------|----------|----------|-----|
| `bytes` | 1.10.1 | [RUSTSEC-2026-0007](https://github.com/advisories/GHSA-434x-w66g-qw3r) — integer overflow in `BytesMut::reserve` | — | ≥ 1.11.1 |
| `rustls-webpki` | 0.103.7 | [RUSTSEC-2026-0049](https://rustsec.org/advisories/RUSTSEC-2026-0049) — faulty CRL matching | — | ≥ 0.103.10 |
| `rustls-webpki` | 0.103.7 | [RUSTSEC-2026-0099](https://rustsec.org/advisories/RUSTSEC-2026-0099) — wildcard name constraints | — | ≥ 0.103.12 |
| `rustls-webpki` | 0.103.7 | [RUSTSEC-2026-0098](https://rustsec.org/advisories/RUSTSEC-2026-0098) — URI name constraints | — | ≥ 0.103.12 |
| `rustls-webpki` | 0.103.7 | [RUSTSEC-2026-0104](https://rustsec.org/advisories/RUSTSEC-2026-0104) — panic in CRL parsing | — | ≥ 0.103.13 |
| `time` | 0.3.44 | [RUSTSEC-2026-0009](https://rustsec.org/advisories/RUSTSEC-2026-0009) — DoS via stack exhaustion | **6.8 medium** | ≥ 0.3.47 |

> **Note:** All six are **transitive** dependencies. Run `cargo update` and check whether direct dependency bumps (especially `reqwest`, `axum`, `tokio-tungstenite`) pull in patched versions. Consider adding a `[patch]` or `cargo update -p bytes time` after checking compatibility.

### 3.2 Unmaintained / Unsound (informational)

| Crate | Version | Advisory | Type |
|-------|---------|----------|------|
| `bincode` | 1.3.3 | [RUSTSEC-2025-0141](https://rustsec.org/advisories/RUSTSEC-2025-0141) | Unmaintained |
| `paste` | 1.0.15 | [RUSTSEC-2024-0436](https://rustsec.org/advisories/RUSTSEC-2024-0436) | Unmaintained |
| `proc-macro-error2` | 2.0.1 | [RUSTSEC-2026-0173](https://rustsec.org/advisories/RUSTSEC-2026-0173) | Unmaintained |
| `yaml-rust` | 0.4.5 | [RUSTSEC-2024-0320](https://rustsec.org/advisories/RUSTSEC-2024-0320) | Unmaintained |
| `rand` | 0.8.5 / 0.9.2 / 0.10.0 | [RUSTSEC-2026-0097](https://rustsec.org/advisories/RUSTSEC-2026-0097) | Unsound (custom logger edge case) |

---

## 4. Panic / Error-Handling Patterns

### 4.1 Production `unwrap()` / `expect()` Sites

| File | Line | Pattern | Risk |
|------|-----:|---------|------|
| `src/main.rs` | 126, 133, 152, 154, 182, 215, 217, 238 | Startup `expect` | ✅ Acceptable at process init |
| `src/mcp/conversions.rs` | 43 | `expect("Failed to create minimal ToolInputSchema")` | ⚠️ Low — schema is static |
| `luna_thin_ui/src/client/ws_client.rs` | 157, 276, 288, 293, 300, 310 | `Mutex::lock().unwrap()` | ⚠️ Medium — panics on poison |
| `luna_thin_ui/src/ui/app.rs` | 943 | `tool_call_id.clone().unwrap()` | 🔴 High — can panic on bad server data |
| `luna_thin_ui/src/ui/pages/memories.rs` | 214 | `expect("edit card without draft")` | ⚠️ Medium — UI state invariant |

**Recommendation:** Replace `app.rs:943` with graceful fallback or error UI. Use `lock().unwrap_or_else(|e| e.into_inner())` or `parking_lot::Mutex` in `ws_client.rs`.

### 4.2 Other Patterns

| Pattern | Count | Notes |
|---------|------:|-------|
| `todo!` / `unimplemented!` | 1 | `src/services/tool_call_manager.rs` |
| `unsafe { }` blocks | 1 | `src/storage/sqlite_storage_simple.rs` (sqlite-vec extension load) |
| `panic!` | 1 | `agentic-loop/src/mcp_config/mod.rs` (test module) |
| `#[allow(...)]` suppressions | 19 | Mostly in storage, LLM, vendor code |

---

## 5. Dead Code Hotspots

The storage layer has the largest dead-code footprint — likely from an incomplete migration to `sqlite_storage_simple.rs`:

```
src/storage/conversation_storage.rs  → legacy Conversation, Storage, ConversationIndex (unused)
src/storage/repos.rs                 → trait abstractions never wired in
src/services/mcp_service.rs          → entire service unused
src/services/context_service.rs      → desktop-specific methods unused in server build
src/mcp/conversions.rs               → 3 of 4 conversion helpers unused
src/llm/ollama.rs                    → streaming types defined but not used
```

**Impact:** Increases compile time, confuses contributors, hides real API surface.

---

## 6. Clone Density (performance smell)

High `.clone()` counts suggest unnecessary allocations in hot paths:

| File | `.clone()` calls |
|------|-----------------:|
| `src/agentic/loop_engine.rs` | 52 |
| `luna_thin_ui/src/ui/app.rs` | 41 |
| `src/llm/openai.rs` | 24 |
| `src/server/mod.rs` | 21 |
| `src/services/message_converter.rs` | 16 |
| `agentic-loop/src/mcp_servers_registry/mod.rs` | 16 |

Not all clones are bad (UI state, message passing), but `loop_engine.rs` and `openai.rs` warrant profiling review.

---

## 7. Test Coverage Snapshot

| File | `#[test]` count |
|------|----------------:|
| `src/services/memory_rag.rs` | 6 |
| `src/services/context_service.rs` | 7 |
| `luna_thin_ui/src/utils/markdown_strip.rs` | 7 |
| `src/llm/tokenizer.rs` | 3 |
| `src/storage/sqlite_storage_simple.rs` | 3 |
| Others | 10 |

**Gap:** No integration tests for `src/server/`, `luna_thin_ui/src/client/`, or LLM provider modules beyond tokenizer.

---

## 8. Prioritized Recommendations

### P0 — Security (this week)

1. Run `cargo update` and verify patched versions of `bytes`, `time`, `rustls-webpki`.
2. Re-run `cargo audit` until 0 vulnerabilities.
3. Consider adding `cargo audit` to CI (`.github/workflows/`).

### P1 — Correctness (this sprint)

4. Fix `luna_thin_ui/src/ui/app.rs:943` — replace bare `unwrap()` on optional `tool_call_id`.
5. Harden `ws_client.rs` mutex locking against poison panics.

### P2 — Maintainability (next sprint)

6. Run `cargo clippy --fix` for 100 auto-fixable warnings.
7. Remove or gate dead code behind `#[cfg(...)]` — especially `conversation_storage.rs` legacy types and unused repo traits.
8. Clean up unused imports in `src/server/handlers/` (likely leftover from recent handler split).

### P3 — Quality (ongoing)

9. Add `#![warn(clippy::pedantic)]` or `-D warnings` in CI incrementally.
10. Add integration tests for server WebSocket handlers.
11. Profile clone-heavy paths in `loop_engine.rs` and `openai.rs`.

---

## 9. Re-run Instructions

Save this script as `scripts/static-analysis.sh`:

```bash
#!/usr/bin/env zsh
set -euo pipefail
unset ARGV0
cd "$(dirname "$0")/.."

REPORT_DIR="audits"
mkdir -p "$REPORT_DIR"

echo "=== cargo check ==="
cargo check --workspace 2>&1 | tee "$REPORT_DIR/check.log"

echo "=== cargo clippy ==="
cargo clippy --workspace --all-targets --message-format=short 2>&1 \
  | tee "$REPORT_DIR/clippy.log"

echo "=== cargo audit ==="
cargo audit 2>&1 | tee "$REPORT_DIR/audit.log" || true

echo "=== Summary ==="
echo "Clippy warnings: $(grep -cE '\.rs:[0-9]+:[0-9]+: warning:' "$REPORT_DIR/clippy.log" || echo 0)"
echo "Audit vulnerabilities: $(grep -c '^Crate:' "$REPORT_DIR/audit.log" || echo 0)"
echo "Logs written to $REPORT_DIR/"
```

---

## Appendix: Related Audits

| Report | Focus |
|--------|-------|
| `audits/server-architecture-audit.md` | Server module architecture, bottlenecks, SoC |
| `audits/desktop-app-architecture-audit.md` | Desktop UI architecture |

This report complements those with toolchain-level static analysis (lints, deps, patterns).
