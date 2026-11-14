# Luna Server Remediation Plan

> **Status (2026-11-13): Phases 1–4 complete.** All SEC/BUG/ARCH/SMELL findings
> addressed except a few explicitly-deferred cosmetic items (see Phase 4).
> Gate is green: clippy `--workspace -D warnings`, 261 tests, `cargo audit` exit 0.
> Upstream markdownify PR: https://github.com/Skardyy/mcat/pull/115 (SEC-1d).

Companion to [`server-code-review-2026-11.md`](server-code-review-2026-11.md).
Phased plan covering every finding (SEC-1..10, BUG-1..10, ARCH-1..5, SMELL-1..8).

**Decisions:** all findings, phased · markdownify chain → audit-ignore + upstream PR (vendor fork only if it stalls) · hardening → secure defaults with config escape hatches.
**Gate for every phase:** `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p cosmic_llm -p agentic-loop`, `cargo audit` all green.

---

## Phase 1 — P0: supply chain, critical exposure, durability — ✅ DONE (2026-11-13)

- [x] **SEC-1** Dependencies: targeted `cargo update` (rustls 0.23.46, rustls-webpki 0.103.17, webpki-root-certs 1.0.12, h2 0.4.20, tokio 1.54.1, openssl 0.10.82, quinn 0.11.11 + quinn-proto 0.11.19, pulldown-cmark 0.13.6, rtrb 0.3.6, rand 0.10.3, anyhow 1.0.104); `rusqlite 0.39 → 0.42` (libsqlite3-sys 0.40); `markdownify 0.3.7 → 0.3.8`. 32 → 5 vulns; remainder (`zip` ×4, `calamine` ×1) pinned by markdownify → `.cargo/audit.toml` ignores with reasons + expiry 2027-02-13.
- [x] **SEC-1 (CI)** `.github/workflows/audit.yml`: weekly scheduled `cargo audit`; `check.yml` now also runs `cargo test -p cosmic_llm -p agentic-loop`.
- [x] **SEC-2** `publish_image` sources confined to `uploads_dir()` ∪ `files_root()` (canonicalized prefix check); escape hatch `[agentic].publish_image_unrestricted_sources` (default false); helper `publish_source_in_allowed_root` + unit test; also fixed `static_dir()` being taken from `AppConfig::default()` instead of the live config.
- [x] **SEC-4** WS transport limits in `ws_handler`: `max_message_size(1 MiB)`, `max_frame_size(256 KiB)`, `max_write_buffer_size(32 MiB)` — cap now enforced during buffering, not after (tungstenite defaults were 64/16 MiB/unbounded).
- [x] **BUG-3** Scheduler durability: `recover_stuck_running_jobs()` (running→pending) runs at scheduler start; each job runs in its own `tokio::spawn` so a panic can't kill the scheduler loop (JoinError → job marked failed); "agent already running" early-return now finalizes the row (recurring → next run; one-off → failed); agent-task panic/cancel → job failed. Test: `stuck_running_jobs_recovered_to_pending`.
- [x] **SMELL-7** README: removed the nonexistent `--server` flag, documented actual CLI (`--config`, `--deep_sleep`, `--reorganize_memories`); `.gitignore`: dropped redundant per-file `.pyc` lines, added `/luna-thin`; `git rm --cached` 9 tracked `.pyc` files (staged, not committed).
- [x] **Harness (Phase 0)** Router-level tests in `src/server/http.rs`: `auth_middleware_gates_api_routes` (401 no-key/wrong-key/ws-pre-upgrade; 200 x-api-key + Bearer) and `static_route_requires_capability_token` (401 wrong token; 404 right token missing file) via `tower::ServiceExt::oneshot` + temp-DB `ServerContext` builder (`test_ctx`). `tower` gained the `util` feature.
- [x] **Lint debt (pre-existing, blocked `-D warnings`)**: 5 in server crate (`type_complexity` shell.rs → `ParsedArgs` alias; `too_many_arguments` spawn_worker.rs → loop built at call site; 3× `field_reassign_with_default` tools_policy tests → struct-literal init) + 4 in luna_thin_ui (3× `needless_return`, 1× `filter_next`).
- [x] **Test hygiene**: `#[ignore]` on two agentic-loop integration tests that spawn real `npx`/`uvx` MCP servers over the network and pin upstream tool counts (`test_initialize_from_config` had already drifted 14→16 and was failing) — documented, runnable via `-- --ignored`.
- [ ] **SEC-1d (outstanding)** Upstream PR to https://github.com/Skardyy/mcat bumping `zip → 9`, `calamine → 0.38`; then remove `.cargo/audit.toml` ignores. Fallback if stalled ~2 weeks: vendor patched fork + `[patch.crates-io]`.

**Verification snapshot:** clippy workspace `-D warnings` exit 0 · tests 113 lib + 130 bin + 2 agentic-loop passed (2 ignored) · `cargo audit` exit 0 · `cargo check --workspace` ok.

---

## Phase 2 — P1: agent & connection safety — ✅ DONE (2026-11-13)

- [x] **SEC-3** `spawn_worker` restrictions: `[agentic].worker_profiles` allowlist (default `None` = parent profile only — model-chosen escalation rejected), `RunContext.depth` + `[agentic].max_worker_depth` (default 1; 0 disables), and all internal tools now run under `tokio::time::timeout` — `tool_call_timeout_secs` normally, `[agentic].worker_timeout_secs` (default 600) for workers. Both dispatch sites unified through one `dispatch_internal_tool` (also fixed a latent bug: the non-streaming path never dispatched `spawn_worker`). Test: `spawn_worker_args_enforce_profile_and_depth_limits`.
- [x] **SEC-5** Bounded channels: per-connection WS outbound queue (`WS_EVENT_CHANNEL_CAP=1024`; drop-new + debug log on full, prune on closed; `try_send` in `broadcast`/`broadcast_excluding`), LLM-audit channel (`LLM_AUDIT_CHANNEL_CAP=8192`, drop+warn; `record()` is sync from `Drop`), tungstenite write buffer 32 MiB. *Deviation:* per-run `AgentUpdate` channel stays unbounded for now — converting its ~30 send sites belongs to the Phase-4 dispatch rewrite; it is per-run and lifetime-bounded by the stream timeout.
- [x] **BUG-1** Atomic agent registration: check+spawn+insert under one write-lock scope (entry API) in `try_spawn_tracked_agent` and `run_scheduled_task`.
- [x] **BUG-2** Delete/truncate lifecycle: running agent aborted, `ConversationDeleted` broadcast to other viewers, subscriptions cleared, `uploads/{conv}` + `static/{conv}` removed best-effort; truncate aborts the agent and re-syncs other viewers. (`_no_conversation` upload orphans → Phase 4 hash index.)
- [x] **SEC-8** `init_storage` fails fast (no temp-dir DB); removed dead `impl Default for Storage` (temp fallback + `process::exit`) and unused `Storage::new`/`new_default`.
- [x] **SEC-10** Content responses: `X-Content-Type-Options: nosniff`, `Cache-Control: no-store`, `Content-Disposition` (attachment for SVG/HTML), `Content-Security-Policy: sandbox` on static SVGs. *Deferred (evaluated):* `static_token` out of URL path — needs coordinated mobile+thin-UI changes; token is per-boot ephemeral, risk accepted.
- [x] **BUG-4** FTS5 escaped via `fts_match_query` (per-token quoted phrases) in `search_history` + `search_memory_paginated`. Tests: `fts_query_escapes_user_input`, `search_with_fts_syntax_chars_does_not_error`. Trade-off: FTS operators become literal (documented).
- [x] **BUG-5** `@file:` reads bounded (`File::take`; budget+slack plain / 4 MiB scan cap for ranges; binary stubs no longer read the file — size from metadata); static handler canonicalize → `tokio::fs`. *Remainder:* full async migration of `context_refs` folded into Phase 4.
- [x] **BUG-6** `handle_send_message` reordered: resolve attachments (local) → persist → **`MessageAccepted` + subscribe** → vision describe → patch metadata via new `update_message_attachments` → RAG index → agent. Vision-failure fallback (base64 rehydration) unchanged.
- [x] **BUG-8** `MAX_UPLOAD_BYTES = MAX_DOC_BYTES + 1 MiB` (100 MiB doc cap now reachable). **BUG-9** `StopStreaming{None}` → session's active conversation (error if none); no `"unknown"` events; others notified via `broadcast_excluding`. **BUG-10** title limit counts chars.
- [x] **SMELL-8** `parse_multipart_boundary` (param order, case, quoted values). Tests: `parses_multipart_boundary_variants`.
- [x] **ARCH-3** Graceful shutdown: SIGINT/SIGTERM → `with_graceful_shutdown` → abort outstanding agent runs → clean exit.
- [x] **ARCH-4** `[agentic].max_turns` (default 50) enforced in both loops; clean `ModelError` on exhaustion.
- [ ] **SMELL-4** upload hash index → moved to Phase 4 (schema addition; pairs with orphan tracking).

## Phase 3 — P2: policy & observability — ✅ DONE (2026-11-13)

- [x] **SEC-6** `@file:` denylist broadened (components +`.kube/.docker/.mozilla/.thunderbird/.pki`; names +`.git-credentials`, shell histories, NSS/keyring DBs, `logins.json`, ADC json, `.vault-token`, more key IDs; exts +`p8/jks/keystore/kdbx`; substrings +`/.config/gh/`, `/.config/opencode/`, `/.config/rclone/`, browser profiles, `/.local/share/keyrings/`). Every expansion and denial logged (path+bytes, never contents). Test: `denied_credentials_extended_list`. Per-profile opt-in left as-is (`[files].root` is the knob).
- [x] **SEC-7** `schedule_task` caps: ≤25 pending per conversation, ≤100 pending global. *Deviation:* no new `ScheduledTaskCreated` protocol event — a new `ServerEvent` variant breaks deserialization in older thin-UI clients; the tool result already surfaces via the existing `ToolResult` broadcast.
- [x] **SEC-9** Startup rejects api_keys < `[server].min_api_key_length` (default 32; live key is 46). Per-IP auth throttle: `auth_rate_limit_max` (30) failures / `auth_rate_limit_window_secs` (300) → 429 until window reset; served via `into_make_service_with_connect_info`; cleared on success; map self-cleans. TLS/reverse-proxy docs in README + sample config. Tests: `rejects_short_api_keys_unless_lowered`, `auth_throttle_locks_after_max_failures`, `auth_throttle_returns_429_after_repeated_failures`.
- [x] **BUG-7** Profile ping-pong fixed: conversation is source of truth; `SendMessage` adopts the conversation profile into the session (emits `ProfileChanged`) instead of rewriting the DB; legacy null-profile conversations still adopt the session profile.
- [x] **SMELL-5** Client-facing responses propagate send errors (`send_event(...)?` in load/list/search/memories); dead `truncate_conversation` stub, `UploadKind::max_bytes`, `impl Default for Storage` removed.
- [x] **SMELL-6** WS logging redacted: parse-failure logging ERROR→WARN; payloads truncated (200 chars warn / 2000 debug) via `truncate_for_log`.
- [ ] **SMELL-7 (rest)** `conversation_storage.rs` legacy facade fate → folded into Phase 4 storage split.

**Verification snapshot (end of Phase 3):** clippy workspace `-D warnings` exit 0 · tests 118 lib + 140 bin + 2 agentic-loop passed (2 env-dependent ignored) · `cargo audit` exit 0 · `cargo check --workspace` ok.

## Phase 4 — structural refactors — ✅ DONE (2026-11-13, two items deferred)

- [x] **ARCH-1** Storage concurrency, in two steps: (1) `SqliteStorage` internals — one dedicated writer connection (mutex) + 4 round-robin reader connections for 20 SELECT-only methods; WAL already on; poisoned-mutex-resilient guards. (2) The global `Arc<Mutex<Storage>>` is gone — all ~70 call sites now share a plain `Arc<Storage>`; reads run concurrently, writes serialize on the writer connection only. Trade-off documented in the commit: multi-call sequences that previously held the outer lock are no longer atomic across calls (per-statement consistency unaffected; single-user deployment).
- [x] **ARCH-2** Scheduler: due jobs run in a `JoinSet` under a 2-permit semaphore (was strictly sequential); each task wraps `run_scheduled_task` in `catch_unwind` and always returns `(job_id, outcome)`, so panics mark the job failed instead of stranding the row.
- [x] **ARCH-5 (partial) + SMELL-3 (partial)** Tool dispatch was unified in Phase 2 (`dispatch_internal_tool`); the 15 `*_tool_definition` constructors now live in `agentic/tool_definitions.rs` (loop_engine 2.6k → 2.3k lines). *Deferred:* splitting `sqlite_storage_simple.rs` by aggregate (mechanical, no behavior value on its own).
- [x] **SMELL-1** New `crate::uploads` module: one async, symlink-rejecting implementation of `{uid}.{ext}` lookup (scoped + scan-all variants) replacing the three divergent copies — the loop_engine copy had no symlink check.
- [x] **SMELL-2** `AgentPersistence` (agentic/persistence.rs) replaces the duplicated `PersistenceContext`/`WorkerPersistence`; canonical `rowid_to_message_uuid`/`message_uuid_to_rowid` in storage_wrapper replace four inline copies. *Deferred:* a full `MessageId` newtype (functions are now single-source; the newtype is churn without added safety).
- [x] **SMELL-4** `uploads_index` table (uid, conversation_key, sha256, ext, size, created_at): attach-file dedupe is an O(1) hash lookup instead of byte-comparing every file in the dir; remove-file and conversation-delete drop index rows. Pre-index files still resolve via directory scan (they just don't dedupe).
- [x] **SEC-5 remainder** `AgentUpdate` channels bounded (256) with awaited sends at all 35 sites — a stalled persistence consumer now backpressures the agent loop.
- [x] **SEC-1d** Upstream PR opened: https://github.com/Skardyy/mcat/pull/115 (calamine 0.36.1 → 0.38; zip 8→9 already merged upstream in #112, awaiting a published markdownify release). `.cargo/audit.toml` comments reference both. When markdownify ships the bump: update the dep, delete the 5 ignore entries.
- [ ] **Deferred (documented):** `sqlite_storage_simple.rs` split; `conversation_storage.rs` facade fate (SMELL-7 rest); full async migration of `context_refs` (reads are bounded since Phase 2 — remaining blocking window is milliseconds); `MessageId` newtype.

## Cross-cutting

- [ ] Tests still to add per phase: concurrent-SendMessage (BUG-1), scheduler collision (BUG-3 second path), WS transport-limit, bounded-channel backpressure (SEC-5), FTS escaping (BUG-4).
- [ ] Consider pinning GitHub Actions to full commit SHAs.
- [ ] Keep `.cargo/audit.toml` expiry honest (2027-02-13).
