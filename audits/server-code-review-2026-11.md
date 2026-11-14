# Luna Server Code Review — November 2026

> **Remediation status:** Phase 1 (P0) implemented 2026-11-13 — SEC-1, SEC-2, SEC-4, BUG-3, SMELL-7 + CI hardening; Phases 2–4 pending. Tracking doc: [`server-remediation-plan.md`](server-remediation-plan.md).

- **Date:** 2026-11-13
- **Commit:** `d447627` (HEAD at review time)
- **Scope:** `src/server/**`, `src/main.rs`, plus server-executed support code: `src/agentic/**` (loop engine, spawn_worker), `src/tools/**` (embedded shell/time), `src/tools_policy.rs`, `src/services/context_refs.rs`, `src/config/mod.rs` (server-relevant parts), `src/storage/**` (access patterns), `Cargo.toml`/`Cargo.lock` (supply chain).
- **Method:** Manual review of all ~4,600 lines of `src/server` + supporting modules, `cargo clippy -p cosmic_llm --all-targets`, `cargo audit`, dependency-tree attribution (`cargo tree -p cosmic_llm -i <crate>`). No code was changed.
- **Predecessors:** `audits/server-architecture-audit.md` (June), `audits/static-code-analysis.md` (June). The handler split (`handlers/{agent,conversation,memory,profile,spawn}.rs`), content-authoritative upload policy, constant-time auth and shell hardening post-date those audits; this review reflects current code.

---

## Executive Summary

The server is in materially better shape than the June audit: auth is constant-time and refuses weak keys, uploads are content-classified, SQL is uniformly parameterized, the shell tool is argv-only with default-deny, and path handling uses canonicalize + prefix checks with symlink rejection. Clippy is clean under `-D warnings`.

The remaining risk concentrates in four places:

1. **Supply chain** — `cargo audit` currently reports 32 vulnerabilities (CI runs `cargo audit` and must be red). Crates in the *server binary's* tree include `zip 8.6.0` (one **critical** 9.1), `calamine 0.36.1` (**high** 7.5), `libsqlite3-sys`, `rustls`, `h2`. Both `zip` and `calamine` sit under `markdownify`, which parses **user-uploaded documents** — the untrusted-input path.
2. **Untrusted-input → agent-tool chain** — `publish_image` will copy *any image-extension file on the server filesystem* into the web-served static dir; `spawn_worker` lets the model pick *any configured profile* (tool-policy escalation) with no depth limit; `schedule_task` gives injected instructions restart-surviving persistence. Each is individually defensible; combined they form a realistic prompt-injection escalation ladder.
3. **Resource-exhaustion gaps** — the WebSocket 1 MiB app-level cap is checked *after* tungstenite buffers up to 64 MiB; all event channels are unbounded (slow client = unbounded memory); 50 MiB uploads are fully buffered plus an O(dir) byte-compare dedupe scan.
4. **Single global `Arc<Mutex<Storage>>`** — one SQLite connection behind one mutex serializes every handler, the scheduler, title generation, deep sleep, memory RAG and agent persistence. It is the central throughput bottleneck and the reason several handlers do awkward lock-dance choreography.

Plus a cluster of lifecycle/correctness bugs: TOCTOU double-agent spawn, conversation delete/truncate not cancelling running agents or cleaning files, scheduled jobs stuck in `running` forever after a crash, and README drift (`--server` flag no longer exists).

### Findings Index

| ID | Severity | Category | Finding | Location |
|----|----------|----------|---------|----------|
| SEC-1 | Critical (supply chain) | Dependencies | 32 audit vulns; `zip`/`calamine`/`libsqlite3-sys`/`rustls`/`h2` in server tree; CI `cargo audit` failing | `Cargo.lock`, `.github/workflows/check.yml:26` |
| SEC-2 | High | Data exposure | `publish_image` reads arbitrary filesystem paths (any image ext, incl. `.svg`) into web-served static dir — model-controlled | `src/agentic/loop_engine.rs:1458–1601` |
| SEC-3 | High | Privilege escalation | `spawn_worker` accepts model-chosen `profile` → tools policy of *any* profile; no depth limit; no own timeout | `src/agentic/loop_engine.rs:453–500,1048–1067`, `src/agentic/spawn_worker.rs` |
| SEC-4 | Medium | DoS | WS size cap applied post-buffer (tungstenite default 64 MiB/msg, 16 MiB/frame) | `src/server/websocket.rs:13,56–74`, `src/server/http.rs:392–403` |
| SEC-5 | Medium | DoS | Unbounded channels everywhere: WS outbound, agent updates, LLM audit — slow consumer ⇒ unbounded memory | `websocket.rs:25`, `spawn.rs:46`, `mod.rs:150`, `spawn_worker.rs:94` |
| SEC-6 | Medium | Info leak | `@file:`/`@folder:` root defaults to `$HOME` with a small denylist (misses `.git-credentials`, `.kube`, `.docker`, browser stores, `~/.config/**` tokens); anyone who can send a message (e.g. open Telegram bridge) can inline them into the prompt | `src/services/context_refs.rs:156–185`, `src/config/mod.rs:989–996` |
| SEC-7 | Medium | Persistence | `schedule_task` lets injected instructions survive restarts as autonomous cron jobs; no confirm/visibility gate | `loop_engine.rs:1602+`, `src/server/mod.rs:323–368` |
| SEC-8 | Low-Med | Local attack | Silent DB fallback to predictable shared path `/tmp/cosmic_llm_server.db` (symlink/pre-create by other local users; silent data split) | `src/server/mod.rs:134–145` |
| SEC-9 | Low | Auth hardening | No rate-limit/lockout on API-key attempts; no TLS (plaintext key on wire if exposed); weak-key check only rejects empty/`LUna` | `src/server/http.rs:80–99`, `src/config/mod.rs:308–326` |
| SEC-10 | Low | Headers | Static/attachment responses lack `X-Content-Type-Options: nosniff`, `Content-Disposition`; SVG served inline as `image/svg+xml`; static token travels in URL path (log leakage) | `http.rs:264–288,427–457` |
| BUG-1 | High | Race | TOCTOU in `try_spawn_tracked_agent`: check-then-insert on `active_agent_runs` allows two concurrent agents per conversation | `src/server/handlers/agent.rs:151–177` |
| BUG-2 | High | Lifecycle | Delete/truncate conversation does not abort the running agent, does not unsubscribe viewers, does not clean `uploads/{id}/` or `static/{id}/` | `handlers/conversation.rs:198–265` |
| BUG-3 | High | Durability | Crash mid-job leaves scheduled job `status='running'` forever — never re-picked (recurring jobs silently die) | `sqlite_storage_simple.rs:1812–1845`, `mod.rs:323–368` |
| BUG-4 | Medium | Robustness | FTS5 `MATCH` receives raw user query — FTS syntax (`"`, `NEAR(`, `col:`) surfaces as server errors instead of zero results | `sqlite_storage_simple.rs:857–904,1206–1231`, `handlers/conversation.rs:85–92` |
| BUG-5 | Medium | Blocking | Blocking `std::fs` I/O inside async paths: `@file:` expansion reads full files synchronously; static handler canonicalizes synchronously | `context_refs.rs:195–200`, `http.rs:419–420` |
| BUG-6 | Low-Med | Latency | `handle_send_message` does attachment I/O + vision LLM call + RAG embedding **before** `MessageAccepted` — client sees no ack for the whole pipeline; connection can't process other commands meanwhile | `handlers/agent.rs:208–397` |
| BUG-7 | Low | Consistency | Conversation profile is force-overwritten by session profile on send, but restored from conversation on load — two clients with different profiles fight over one conversation | `agent.rs:224–240` vs `conversation.rs:38–54` |
| BUG-8 | Low | Limits | `MAX_DOC_BYTES` (100 MiB) unreachable: upload body capped at `MAX_UPLOAD_BYTES` (50 MiB) incl. multipart overhead | `src/llm/attachment_limits.rs:7,14` |
| BUG-9 | Low | Semantics | `StopStreaming{conversation_id: None}` stops nothing and broadcasts `StreamingStopped{conversation_id:"unknown"}` | `agent.rs:398–418` |
| BUG-10 | Low | Validation | Rename title limit checks `str::len()` (bytes) but error says "characters" | `conversation.rs:152–162` |
| ARCH-1 | High | Bottleneck | Single `Arc<Mutex<Storage>>` / one rusqlite `Connection` serializes all reads+writes across the whole server | `storage/storage_wrapper.rs:11–13`, `server/mod.rs:144` |
| ARCH-2 | Medium | Scheduling | Scheduler executes jobs sequentially and awaits the full agent run (up to `stream_timeout_secs`, default 600 s) inside the 45 s tick loop | `mod.rs:323–368`, `spawn.rs:116–332` |
| ARCH-3 | Medium | Shutdown | No graceful shutdown (`axum::serve` without signal handling); Ctrl-C aborts agents mid-write | `mod.rs:69–78` |
| ARCH-4 | Medium | Runaway guard | Agentic `'turn: loop` has no max-iterations/turn cap; only the outer 10-min timeout bounds it | `loop_engine.rs:657+` |
| ARCH-5 | Low | Duplication | Tool-policy enforcement + update-processing duplicated between streaming and non-streaming paths | `loop_engine.rs:~1008 vs ~2011` |
| SMELL-1 | — | DRY | Upload resolution by uid implemented 3× (`http.rs:241`, `helpers.rs:84`, `loop_engine.rs:239`) with subtly different rules | see locations |
| SMELL-2 | — | DRY | `PersistenceContext` (spawn.rs:334) ≈ `WorkerPersistence` (spawn_worker.rs:183); rowid→UUID mapping duplicated (`helpers.rs:141` vs `storage_wrapper.rs:86`) | see locations |
| SMELL-3 | — | God files | `loop_engine.rs` 2,268 lines; `sqlite_storage_simple.rs` 2,331 lines; `http.rs` mixes routing, upload handler, static server | — |
| SMELL-4 | — | Perf | `find_duplicate` reads every file in the target dir byte-for-byte per upload (up to 50 MiB each) | `http.rs:215–238` |
| SMELL-5 | — | Silent failures | `let _ = outbound.send(...)` swallows closed-channel errors in many places; dead `truncate_conversation` stub returns `Ok(0)` | `agent.rs`, `context_pipeline.rs`, `sqlite_storage_simple.rs:981–988` |
| SMELL-6 | — | Logging | Raw WS payloads logged at ERROR (parse failures) and DEBUG (full requests) — PII/secrets land in JSON log files | `websocket.rs:115–167` |
| SMELL-7 | — | Repo hygiene | Tracked `__pycache__/*.pyc` (currently dirty); 53 MB `luna-thin` binary and 1 MB `agentic_tool_calls.log` in working tree; README documents a `--server` flag that no longer exists; `ServerConfig.enabled` unused by the server | `git status`, `README.md:115`, `src/main.rs:18–39` |
| SMELL-8 | — | Multipart | Boundary parsed via `strip_prefix("multipart/form-data; boundary=")` — breaks on quoted boundaries or reordered params | `http.rs:110–116` |

---

## 1. Security Vulnerabilities

### SEC-1 — Dependency vulnerabilities (Critical, actionable now)

`cargo audit` → **32 vulnerabilities, 24 warnings** (2026-11-13). Attribution to the **server binary** (`cargo tree -p cosmic_llm -i …`):

| Crate | In server tree via | Worst advisory | Note |
|---|---|---|---|
| `zip 8.6.0` | `markdownify`, `calamine` | **RUSTSEC-2026-0369 (9.1 critical)** — extract() applies permissions through symlinks; plus 0367 (SUID/SGID), 0368 (symlink escape), 0429 (process abort) | Parses **uploaded Office docs**; exploitability depends on whether `extract()` is used by markdownify/calamine, but upgrade is cheap |
| `calamine 0.36.1` | `markdownify` | **RUSTSEC-2026-0436 (7.5 high)** — panic + unbounded alloc on malformed CFB/OLE2 | Directly reachable by uploading a crafted `.xls/.xlsx` |
| `libsqlite3-sys 0.37.0` | `rusqlite (bundled)` | RUSTSEC-2026-0459 (6.8) — upstream SQLite CVE | Core storage |
| `rustls 0.23.32` | `reqwest`, `hyper-rustls`, `tokio-rustls` | RUSTSEC-2026-0285/0346/0348/0349 — handshake-level bugs, `Debug` leaks TLS secrets | All LLM API traffic |
| `rustls-webpki 0.103.13` / `webpki-root-certs 1.0.6` | `reqwest` | RUSTSEC-2026-0382/0383 — accepts distrusted intermediates | Cert validation |
| `h2 0.4.12` | `reqwest`/`hyper` | RUSTSEC-2026-0258 — unbounded empty DATA frames | Outbound HTTP/2 |
| `tokio 1.52.3` | direct | RUSTSEC-2026-0386 (unsound `AsyncFd`) | warning class |

`openssl`, `quinn-*`, `pulldown-cmark`, `rtrb` advisories belong to the `luna_thin_ui` subtree, not the server binary.

**Fix:** `cargo update` for `zip ≥9`, `calamine ≥0.38` (may need a `markdownify` bump or replacement — markdownify 0.3.7 pins old deps), `rusqlite ≥0.41` (or `libsqlite3-sys ≥0.39`), `rustls ≥0.23.46`, `h2 ≥0.4.16`. CI already runs `cargo audit` (`check.yml:26`) and is therefore currently failing — the pipeline will verify the fix. Note the June commit `2771077 "…clear cargo audit vulnerabilities"` shows this decays fast; consider `cargo deny` + a weekly scheduled audit run.

### SEC-2 — `publish_image`: model-controlled arbitrary file read → web publication (High)

`loop_engine.rs:1458` — the tool takes a `path` param and only checks: non-empty, extension ∈ {jpg,jpeg,png,gif,webp,bmp,svg}, is-a-file. **The source path is not confined** to `files_root`, the uploads dir, or the conversation — any readable file on the server with an image extension (screenshots, photos, scanned documents, `.svg` which is just text) is copied into `config_dir/static/{conv_id}/{uuid}.{ext}` and returned as a `luna-static:` marker that clients render via `/api/static/{token}/…`.

Threat model: this is exactly the payload of a prompt-injection exfiltration ("read `~/Pictures/…png`, publish it, put the link in your answer"). Unlike `@file:` expansion, there is **no denylist at all** here, and the result leaves the machine as a URL rather than as LLM text.

Secondary issue: `.svg` is accepted and served inline (`http.rs:451–456` uses `mime_guess`, no `Content-Disposition`, no `nosniff`) — stored-XSS on the server's own origin for anyone with the static token.

**Fix (suggested):** restrict sources to `uploads/` + `files_root` (reuse `resolve_workspace_path`, which already exists at `loop_engine.rs:266` and is used elsewhere); drop `svg` or serve it with `Content-Disposition: attachment` + `X-Content-Type-Options: nosniff` + a restrictive CSP on `/api/static/*`.

### SEC-3 — `spawn_worker`: cross-profile tool escalation, unbounded recursion, no timeout (High)

- `parse_spawn_worker_args` (`loop_engine.rs:453–500`) accepts a `profile` name **chosen by the model** and only checks it exists. `resolve_worker_profile` then computes the worker's `allowed_tool_names` from *that profile's* policy (`spawn_worker.rs:140–157`). A run restricted to a low-privilege policy (e.g. no shell) can therefore execute work under any other configured profile (e.g. one with `shell_execute` + `enabled_tools=["*"]`) simply by naming it. The tool description even advertises this ("may differ from the current session"). Combined with SEC-6/SEC-7 and any prompt injection, this is the shortest path to code execution.
- **No depth limit**: a worker has `spawn_worker` in its own toolset if its policy allows it — the doc comment brags "nested workers are safe to spawn". Each level costs LLM calls and an internal conversation row; fan-out is bounded only by the outer 10-minute stream timeout and token budget.
- **No per-call timeout**: internal tools like `spawn_worker` bypass `execute_tool_with_retry` (which wraps MCP tools with `tool_call_timeout_secs`, `loop_engine.rs:1069–1073` vs `1717+`). A worker runs until the *whole* agent stream timeout fires.

**Fix (suggested):** allow-list worker profiles in config (default: current profile only, or intersect parent ∩ worker policies); add a `depth` field in `RunContext` capped at 1–2; wrap internal tool dispatch in the same per-call timeout as MCP tools.

### SEC-4 — WebSocket frame limits applied after buffering (Medium)

`websocket.rs:13` defines `MAX_WS_TEXT_BYTES = 1 MiB`, but the check at line 60 runs on an **already fully reassembled** `Message::Text`. `ws_handler` (`http.rs:392–403`) never calls `.max_message_size()` / `.max_frame_size()`, so tungstenite 0.29 defaults apply: **64 MiB per message, 16 MiB per frame** (verified in the vendored source). An authenticated client (or N of them) can force 64 MiB allocations per connection before rejection. There is also no connection-count cap.

**Fix:** `ws.max_message_size(MAX_WS_TEXT_BYTES).max_frame_size(64 * 1024)` in `ws_handler`; then the app-level check becomes a redundant safety net.

### SEC-5 — Unbounded channels on every hot path (Medium)

- Per-connection WS outbound: `websocket.rs:25` (`mpsc::unbounded_channel::<ServerEvent>`), fed by `broadcast()` for every stream delta.
- Per-run agent updates: `spawn.rs:46`.
- LLM audit: `mod.rs:150` (deliberate, documented — but still unbounded).

A client that authenticates, subscribes to a busy conversation and then stops reading (TCP zero-window) causes the writer task to block on `send().await` while the channel grows without limit — streaming deltas are small but a 10-minute agent run across several viewers accumulates. Memory growth is unbounded and invisible until OOM.

**Fix:** bounded channels with an explicit policy (drop-oldest deltas, or disconnect on `try_send` overflow for N consecutive failures). `broadcast()` already prunes senders whose channel *closed*; it never sees a merely-full one.

### SEC-6 — `@file:` expansion: HOME-wide read with a porous denylist (Medium)

`context_refs.rs` is well built (canonicalize-before-allow, symlink-escape rejection, byte/ref budgets, boundary check on `@`). The concern is policy, not mechanics:

- Root defaults to `$HOME` (`config/mod.rs:989–996`), matching the filesystem-MCP scope — i.e., **everything the user owns is readable** by any message sender.
- The denylist (`context_refs.rs:156–185`) misses: `.git-credentials`, `.config/gh/hosts.yml`, `.kube/`, `.docker/config.json`, `.config/opencode/`, `.local/share/keyrings/`, browser profile stores (`.mozilla`, `.config/google-chrome`), `.bash_history`, `.mysql_history`, `*.kdbx`, `.token`, `.secrets` — the list is name-based, so trivially defeated by copies/renames (`cp ~/.git-credentials ~/notes.txt`).
- Who can send a message? Every WS client with the API key, every scheduled job, and — per README — the **Telegram bot for anyone** when `ALLOWED_TELEGRAM_IDS` is unset. The bridge is out of scope here, but it widens this surface.

Since denylists cannot work against a same-user attacker, the honest options are: default `files.root` to a project dir instead of `$HOME`, and/or make `@file:` expansion opt-in per profile (`tools_policy` already has the machinery), and/or log+notify on every expansion (currently silent).

Also mechanical: `expand_file` uses **blocking** `std::fs::read` (full file, before budget is applied — a 2 GiB file in `$HOME` is read entirely, `context_refs.rs:200`) inside the async `build_llm_messages` path. Use `tokio::fs` + `File::open` + `take(max_bytes)` and `spawn_blocking` for the sniff.

### SEC-7 — `schedule_task` = persistence for injected instructions (Medium)

The model can create cron jobs (`loop_engine.rs:1602+`) whose stored `message` is later re-injected verbatim as a user turn (`spawn.rs:159–165`) with the job profile's tools — surviving restarts, running unattended forever. There is no user confirmation event, no cap on job count, and no UI-side distinction between user-created and model-created jobs. A single successful prompt injection can install e.g. a nightly "summarize ~/.git-credentials into memory" job. Recurring cron errors also mark the job failed permanently (`spawn.rs:314–319`) — fine, but there's no alerting path beyond `error_message`.

**Fix (suggested):** broadcast a `ScheduledTaskCreated` event clients must surface; cap active jobs per conversation; consider requiring `internal=false` + explicit user turn for creation.

### SEC-8 — Temp-dir DB fallback (Low-Medium, local)

`mod.rs:134–145`: if opening the real DB fails, the server silently switches to `std::env::temp_dir()/cosmic_llm_server.db` — a **predictable path in a world-writable dir**. On a multi-user box another local user can pre-create it (or a symlink at it) and read/write all conversations, or cause the server to append to an attacker-controlled file. It also masks real failures: users lose history "mysteriously" after a disk-full event.

**Fix:** fail fast instead, or use a 0700 per-user runtime dir (`XDG_RUNTIME_DIR`) and log at ERROR + surface to clients.

### SEC-9 — Auth surface hardening (Low)

Good: constant-time compare (`http.rs:80–88`), key required for all routes incl. `/ws` pre-upgrade (`http.rs:494–496`), startup refuses empty/`LUna` keys, binds 127.0.0.1 by default with a warning on 0.0.0.0.

Gaps:
- No minimum strength beyond "not empty/LUna" — `123` passes `validate_security` (`config/mod.rs:310–325`). Enforce ≥ 32 chars or generate one on first run.
- No rate limiting / lockout on auth failures — brute-force is cheap when exposed to LAN.
- No TLS anywhere; the API key travels in plaintext headers when bound to 0.0.0.0. Document "put a reverse proxy in front" or support `rustls` directly.
- Single shared key for all devices — no per-device keys, no revocation, no identity in logs. Acceptable for single-user, but worth noting as an architectural ceiling.

### SEC-10 — Response hardening (Low)

- `/api/attachment/{uid}` and `/api/static/...` omit `X-Content-Type-Options: nosniff`, `Content-Disposition`, CSP, `Cache-Control` (attachment bytes are cacheable by intermediaries).
- `static_token` rides in the URL path — it will appear in proxy logs, browser history and `Referer` if pages there link outward. Prefer a short-lived signed query param or a header-based fetch from clients.

---

## 2. Correctness / Robustness Bugs

### BUG-1 — Double-agent TOCTOU (`agent.rs:151–177`)

`try_spawn_tracked_agent` takes a **read** lock to check `contains_key`, drops it, spawns, then takes a **write** lock to insert. Two `SendMessage`s racing from two connections (mobile + desktop on the same conversation — a supported scenario per `conversation_subscriptions.rs` docs) can both pass the check → two agents persisting interleaved assistant/tool rows into one conversation. `run_scheduled_task` (`spawn.rs:277–300`) has the same pattern. Fix: hold the write lock across check-and-insert, or use `entry().or_try_insert_with()`-style atomicity (do the cheap pre-work first, then a single write-locked check-insert-spawn).

### BUG-2 — Delete/truncate lifecycle gaps (`conversation.rs:198–265`)

`handle_delete_conversation`:
- does **not** abort `active_agent_runs[uuid]` — the agent keeps running, and `add_message_with_metadata` keeps inserting rows for a deleted conversation (or fails per-message, spamming errors);
- does not unsubscribe other connections viewing it (`subscriptions` entry lives on);
- does not delete `uploads/{uuid}/` or `static/{uuid}/` — permanent orphaned files (also for `TruncateConversation`, which likewise doesn't stop the agent).

### BUG-3 — Scheduled jobs stuck in `running` after crash (`sqlite_storage_simple.rs:1812+`)

`get_due_scheduled_jobs` selects `status='pending'` only; `set_scheduled_job_running` flips to `running` before execution. A crash/restart (or a kill via BUG-2's abort path, or a panic inside the awaited task) leaves the row `running` forever — **recurring jobs silently stop recurring**. Add startup recovery: `UPDATE scheduled_jobs SET status='pending' WHERE status='running'` (with an error note), or a `running_since` staleness timeout.

### BUG-4 — Raw FTS5 syntax from users (`sqlite_storage_simple.rs:857–904`, `1206–1231`)

`search_history` and `search_memory_paginated` pass user text straight to `MATCH ?1`. Parameterized (no SQL injection), but FTS5 *query syntax* is unescaped: a search for `"foo`, `NEAR(a b)`, `x:y`, or `*` alone returns `SqliteError` → surfaced to the client as a generic Error event. Quote-and-join tokens (`"tok1" OR "tok2"`), or catch syntax errors and retry with quoted tokens.

### BUG-5 — Blocking I/O in async context

- `context_refs.rs:195–200` — `std::fs::metadata`/`read` of arbitrarily large files on the tokio worker (see SEC-6).
- `http.rs:419–420` — `std::fs::canonicalize` (two syscalls-heavy ops) inside the static-file handler; use `tokio::fs::canonicalize`.
- `helpers.rs:94` does `symlink_metadata` correctly via tokio — inconsistent style across the same codebase.

### BUG-6 — `handle_send_message` front-loads the slow work (`agent.rs:208–397`)

Order today: resolve attachments → **vision LLM describe (network, seconds)** → RAG index (embeddings, network) → persist message → `MessageAccepted` → spawn agent. A message with one image yields no ack for the full vision+embedding round-trip, and because the WS read loop processes commands sequentially (`websocket.rs:76–79`), that client can't even `StopStreaming` meanwhile. Persist + ack first, then do attachment enrichment inside the spawned task (it already has all inputs), or at least before spawning but after the ack.

### BUG-7 — Profile ping-pong (`agent.rs:224–240` vs `conversation.rs:38–54`)

`SendMessage` silently rewrites the conversation's stored profile to the *session's* profile; `LoadConversation` restores the session profile *from* the conversation. Two devices on different profiles sharing a conversation flip its identity on every load/send, changing which system prompt/compaction settings apply mid-history. Pick one direction (conversation wins on send; session change should be explicit `ChangeProfile` — which already updates the DB, `profile.rs:11–20`, making the send-path rewrite redundant).

### BUG-8..10 — small ones

- `MAX_DOC_BYTES = 100 MiB` can never be reached: the upload route caps the whole multipart body at `MAX_UPLOAD_BYTES = 50 MiB` (`attachment_limits.rs`, `http.rs:466`). Either align the constants or apply per-field limits via multer and a larger route cap.
- `StopStreaming{None}` → no abort, broadcasts `conversation_id: "unknown"` (`agent.rs:398–418`). Define None as "stop the session's active conversation" (it's tracked in `session.active_conversation_id`!) or reject it.
- Rename title check uses bytes (`title.len() > 200`) but the message says characters (`conversation.rs:152–162`) — a 67-char CJK title is rejected.
- Multipart boundary extraction (`http.rs:110–116`) fails on `boundary="quoted"` or extra params before `boundary=`; parse the Content-Type properly (or let `multer`/`axum::extract::Multipart` do it).
- `main.rs:49` — `HOME` fallback to `"."` for the log dir can scatter `./local/state/luna/logs` into CWD when run without HOME (systemd services). Use `dirs::state_dir()` consistently with the rest of the code.

---

## 3. Architecture Flaws

### ARCH-1 — The global storage mutex (High)

Everything shares `Arc<Mutex<Storage>>` wrapping **one** rusqlite `Connection` (`storage_wrapper.rs:11`, `mod.rs:144`). Consequences visible in the code:

- Every handler serializes on it — list, load, search, memory ops, agent persistence, scheduler, title thread, deep sleep, LLM audit writer. Reads can't run concurrently even though WAL is enabled (`wal_enabled=true`) — WAL's reader-concurrency is wasted with one connection.
- The lock-dance choreography ("load messages, drop guard, call LLM, re-lock") is repeated in `agent.rs`, `spawn.rs`, `mod.rs:508–554` — correct today, but one accidental `.await` under the guard is a stalled server. (No such bug found in this pass; the risk is structural.)
- `sqlite_busy_timeout_ms` is configured but nearly meaningless with a single connection.

**Direction:** a small pool — e.g. `deadpool-sqlite`/`r2d2-sqlite` with 1 writer + N readers, or a dedicated DB actor task with request/response channels. Even keeping the mutex but splitting `Storage` into `Reader`/`Writer` handles over separate connections would unlock concurrent reads. This also removes most `drop(guard)` fragility.

### ARCH-2 — Scheduler throughput & isolation (`mod.rs:323–368`, `spawn.rs:116–332`)

The 45 s tick fetches ≤10 due jobs and runs them **sequentially**, awaiting each full agent run (`handle.await`, `spawn.rs:301`) — up to `stream_timeout_secs` (600 s default) each. One hung job delays the rest by minutes; a burst of due jobs after downtime serializes for an hour. Job execution also shares the single storage mutex and the `active_agent_runs` keyspace with interactive chats.

Concrete stuck-row trace (a reachable instance of BUG-3): the scheduler marks the job `running` (`mod.rs:341–353`), then calls `run_scheduled_task`, which — if an interactive agent is already running for that conversation — logs a warning and `return Ok(())` (`spawn.rs:277–287`) **without any completion update**. The scheduler only finalizes rows on `Err`, so the job stays `status='running'` forever and a recurring job never gets its next run computed. Fix: mark failed/skipped + requeue before returning, and make finalization RAII-style so every exit path (including aborts and panics) closes the row.

**Direction:** `JoinSet` with a concurrency cap; move status bookkeeping into a guard/RAII type so every exit path (including panics via `catch_unwind` or aborts) finalizes the row.

### ARCH-3 — No graceful shutdown (`mod.rs:69–78`)

`axum::serve(listener, app).await` with no `with_graceful_shutdown`. Ctrl-C / systemd SIGTERM drops in-flight requests, aborts agent tasks mid-LLM-call (leaving half-persisted tool tails — the `repair_incomplete_tool_tail` logic exists precisely because this happens) and races the tracing non-blocking writer flush. Wire `tokio::signal` → graceful shutdown → abort outstanding agents → flush.

### ARCH-4 — No turn cap in the agentic loop (`loop_engine.rs:657+`)

The `'turn: loop` terminates on: model stops calling tools, empty-completion retries exhaust, or the outer timeout (10 min interactive). A model looping on cheap, fast tools (e.g. `search_memory`) can burn tokens/cost for the full window; a worker (SEC-3) has no cap at all beyond its parent's. Add a `max_turns` (config, default ~50) and a per-run token/cost budget with a clean `ModelError` on exhaustion.

### ARCH-5 — Streaming/non-streaming duplication (`loop_engine.rs`)

Tool dispatch + policy checks + update emission exist twice (`~1008–1090` streaming, `~2011+` non-streaming). Every new internal tool must be added in two places (13-way `if/else` chains on string names — also an OCP smell: replace with a dispatch table `HashMap<&str, ToolHandler>` registered once). Divergence between the paths is a matter of time; the June audit flagged the same structure pre-refactor.

### Positive architectural notes

- `ServerContext` is immutable-after-boot (`Arc`, no interior mutability except registries) — good.
- `conversation_subscriptions` cleanly separates fan-out from handler logic and prunes dead subscribers; tested.
- The `ContextPipeline` extraction (June's 232-line god-method) is a real improvement; `upload_policy` as a pure, table-driven, tested module is exactly right.
- Per-session `allowed_tool_names` snapshots (no registry mutation) correctly fixed the old cross-session trampling.

---

## 4. Code Smells / Maintainability

1. **Triple-duplicated upload lookup** — `http.rs:241 find_upload_by_uid` (recursive over all subdirs), `helpers.rs:84 resolve_attachment_upload_path` (conv dir + `_no_conversation`, symlink-rejecting), `loop_engine.rs:239 resolve_uploaded_attachment` (same two dirs, **no symlink check**, sync fs). Three behaviors for one concept; the loop_engine variant is the weakest. Consolidate into `upload_policy` or a small `uploads` module with one async impl.
2. **Near-duplicate persistence structs** — `PersistenceContext` (`spawn.rs:334–433`) vs `WorkerPersistence` (`spawn_worker.rs:183–305`): same pending-tool-calls bookkeeping, same JSON round-trips. Extract one `AgentPersistence` with an optional broadcast sink.
3. **God files** — `loop_engine.rs` (2,268 lines: tool definitions, arg parsing, dispatch, retry, vision, publishing, scheduling, memory tools) and `sqlite_storage_simple.rs` (2,331 lines). Split loop_engine into `tools/definitions.rs`, `tools/dispatch.rs`, `engine.rs`; split storage by aggregate (conversations, messages, memories, jobs, vec).
4. **`find_duplicate` scan cost** (`http.rs:215–238`) — reads *every* candidate file fully. Store a content hash (sha2 is already a dependency) in a small index (DB table `uploads(uid, conv, sha256, size)`) — dedupe becomes O(1) and also gives you orphan tracking for BUG-2.
5. **Swallowed errors** — `let _ = self.outbound.send(...)` (agent.rs:254, context_pipeline.rs:212,258; conversation.rs:67,117; memory.rs:34). At minimum `tracing::debug!` them; a closed channel mid-run currently produces silent divergence between viewers.
6. **Verbose logging of raw payloads** — `websocket.rs:115–167` logs full request text at DEBUG and, on parse failure, at **ERROR** (always on, via the default `info` filter). User messages can contain secrets; the JSON file log lives in `~/.local/state/luna/logs` unrotated-size-wise (daily rotation, no max-age). Truncate + redact, and downgrade parse-failure payload logging to DEBUG.
7. **Dead/misleading APIs** — `truncate_conversation` stub returning `Ok(0)` (`sqlite_storage_simple.rs:981–988`, `#[allow(dead_code)]`), `UploadKind::max_bytes` `#[allow(dead_code)]`, `conversation_storage.rs` legacy types retained for the wrapper's "file-based compatibility" facade that no longer wraps files.
8. **Magic ID scheme** — rowid→pseudo-UUID (`00000000-…-{rowid:012x}`) implemented in two places (`helpers.rs:141–154`, `storage_wrapper.rs:86–90`) and round-tripped via string parsing. One `MessageId` newtype with `From<i64>`/`TryInto<i64>` would kill a class of bugs (e.g. `Uuid::parse_str` on client-supplied message ids currently doubles as the rowid decoder in `truncate`).
9. **Repo hygiene** — tracked `__pycache__/*.pyc` files (dirty in `git status`); `luna-thin` (53 MB) and `agentic_tool_calls.log` (1 MB) sitting in the worktree (ignored, but they leak into archives/backups); README documents `cargo run --release -- --server` — the flag was removed (clap would error), server mode is now the binary's only mode; `ServerConfig.enabled` is never read by the server itself.
10. **Config-fallback masking** — `load_config_or_default` (`mod.rs:81–86`) continues with defaults on *any* load error. It's saved from disaster only because defaults fail `validate_security` — but a user with a typo'd config gets "refusing to start: api_key empty" instead of the real TOML error. Log the load error at ERROR and abort with the parse message.

---

## 5. Prioritized Action List

**P0 — do now (hours):**
1. `cargo update` / bump `markdownify`-chain deps to clear the 32 audit vulns; make CI green again (SEC-1).
2. Confine `publish_image` sources to uploads/files_root; drop or harden SVG serving (SEC-2).
3. Set `max_message_size`/`max_frame_size` on the WS upgrade (SEC-4).
4. Fix the scheduled-job stuck-`running` paths: startup recovery + finalize on the "agent already running" early-return (BUG-3/ARCH-2).

**P1 — next sprint (days):**
5. Restrict `spawn_worker` profiles (allow-list/intersection) + depth cap + per-call timeout for internal tools (SEC-3).
6. Atomic check-insert-spawn for `active_agent_runs` (BUG-1); abort agents + clean files + unsubscribe on delete/truncate (BUG-2).
7. Bounded WS/agent channels with overflow policy (SEC-5); ack `SendMessage` before vision/RAG work (BUG-6).
8. Remove or gate the temp-dir DB fallback (SEC-8); escape/quote FTS queries (BUG-4).
9. Graceful shutdown + agent abort sweep (ARCH-3); max-turn cap in the loop (ARCH-4).

**P2 — structural (weeks):**
10. Storage concurrency: writer+reader connections or pool behind narrower interfaces (ARCH-1).
11. Unify upload lookup, persistence structs, message-id scheme; split `loop_engine.rs` and `sqlite_storage_simple.rs`; dispatch table for internal tools (ARCH-5, SMELL-1/2/3/8).
12. Rethink `@file:` default root and denylist → per-profile opt-in with audit log (SEC-6); surface `schedule_task` creations to clients (SEC-7); API-key strength floor + optional TLS/docs for reverse proxy (SEC-9).

**Testing gaps worth closing:** no integration tests for the HTTP layer (auth middleware, upload policy end-to-end, static traversal is unit-only), no test for concurrent `SendMessage` (BUG-1), no scheduler crash-recovery test (BUG-3), no WS-limit test. The existing unit tests (`upload_policy`, `context_refs`, `helpers`, `conversation_subscriptions`, `shell`, `validator`) are good — extend that style upward with `axum::Router` + `tower::ServiceExt::oneshot` tests.

---

## 6. What's Already Good (keep)

- Constant-time API-key compare; auth enforced as a route layer on *all* routes including pre-upgrade WS; startup refusal of empty/legacy keys; loopback-by-default bind with an explicit warning on 0.0.0.0.
- Uniformly parameterized SQL (the only `format!`-built SQL is an IN-placeholder generator); FTS instead of LIKE scans.
- Content-authoritative upload policy with extension-mismatch rejection, per-kind caps, stored-name canonicalization to `{uuid}.{ext}`, symlink rejection on attachment resolution — plus real unit tests.
- Static file serving: component-allowlist join **and** canonicalize-prefix check (tested against traversal).
- Shell tool: argv-only (never a shell string), basename whitelist + full-match regexes with metachar rejection, default-deny interpreter list, `find -exec`/`awk system()`/`tar --checkpoint-action`/`git -c alias.!` argument policies, output caps, timeouts, `kill_on_drop`, optional directory confinement. This is one of the better embedded shell implementations.
- Tools policy as a pure per-run snapshot (`tools_policy.rs`) — sessions can't trample each other; enforcement duplicated at execution time as defense-in-depth (`is_tool_allowed` + rejection message).
- `overflow-checks = true` in release; CI runs `clippy -D warnings` + `cargo audit`; clippy is currently clean; SAFETY comment on the single `unsafe` block (sqlite-vec auto-extension registration — correctly scoped and documented).
- LLM audit log decoupled from the hot path via channel + background writer, failures never propagate.
