# Scheduling and background execution — architecture

## Refined idea

**User says:** “Remind me in one hour to call John”, “Every day at 9am send me a digest”, “Every two weeks, run the report”, or **“Every morning at 8am start a fresh conversation with the prompt: What’s on my calendar today?”**

**System does:**
1. **Now:** Agent understands intent and **schedules** a task (run_at, target, prompt/message), optionally with **recurrence**.
2. **Target:** Either **in an existing conversation** (conversation_id set: inject a reminder/task message) or **fresh conversation** (no conversation_id: create a new conversation and run with the given prompt as the first user message).
3. **At run_at:** Scheduler finds due jobs and runs them: either inject into existing conversation or create conversation + run agent with prompt.
4. **If recurring:** After execution, compute next run time and re-queue the same job.

So: scheduling = “deferred agent run” either **in a conversation** (reminder/task) or **as a new conversation** (fresh prompt); one-shot or recurring.

---

## Strong recommendations

| Recommendation | Why |
|----------------|-----|
| **Persist in SQLite** | Survives restarts; you already use SQLite and a background loop (title generation). Same pattern. |
| **One scheduler loop** | Single tokio task that wakes every N seconds (e.g. 30–60), queries due jobs, runs them. No cron dependency, no extra process. |
| **Execute = re-run agent** | At trigger time, inject a **user message** into the conversation and run the same `process_message` flow. Agent can use tools, stream, broadcast. No second “job execution” path. |
| **Agent schedules via a tool** | Don’t parse free text for “in 1 hour.” Give the agent a **tool** (e.g. `schedule_task`) it can call. Structured, reliable, auditable. |
| **Internal tool, not MCP** | `schedule_task` is server state (conversation_id, run_at, message). Implement it as an **internal tool** in the Luna server (in the agent loop), not as a separate MCP server. |
| **Target: in-conversation or new conversation** | Store either `conversation_id` (inject into that conversation) or leave it null and use `message` as the **full initial prompt** for a **new** conversation. Execution branch: if conversation_id present → inject; else → create conversation, add prompt as first user message, run agent. |
| **Optional: idempotency / lock** | When the scheduler picks a job, mark it “running” (or delete it) so a second tick doesn’t run it again. Use a single loop (no horizontal scaling) to avoid distributed locking for v1. |

---

## Architecture overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  User: "Remind me in 1 hour to call John"                                     │
└─────────────────────────────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  Agent (existing loop)                                                       │
│  • Understands intent                                                        │
│  • Calls internal tool: schedule_task(run_at, message, conversation_id)     │
└─────────────────────────────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  Internal tool handler (new)                                                  │
│  • Parse run_at (relative "in 1 hour" / absolute timestamp)                  │
│  • Insert row into scheduled_jobs                                            │
│  • Return: "Scheduled for 14:00 UTC"                                         │
└─────────────────────────────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  SQLite: scheduled_jobs                                                      │
│  id, conversation_id, run_at_utc_secs, message, profile_name, status, ...   │
└─────────────────────────────────────────────────────────────────────────────┘

         ═══════════════════════════════════════════════════════════════════
         Time passes (e.g. 1 hour)
         ═══════════════════════════════════════════════════════════════════
         │
         ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  Scheduler loop (new, like spawn_title_generation_thread)                    │
│  • Every 30–60 s: SELECT * FROM scheduled_jobs WHERE run_at_utc_secs <= ?  │
│    AND status = 'pending'                                                    │
│  • For each: mark running → run_scheduled_task(ctx, job) → mark completed    │
└─────────────────────────────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  run_scheduled_task(ctx, job)                                                │
│  • If conversation_id set: load conv, append "Scheduled task (due now): …"   │
│  • If conversation_id null: create new conversation, add job.message as     │
│    first user prompt (fresh conversation with prompt)                       │
│  • Spawn same agent task → broadcast to that conversation’s subscribers     │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Data model

**Table: `scheduled_jobs`**

| Column | Type | Purpose |
|--------|------|--------|
| `id` | TEXT (UUID) | Primary key |
| `conversation_id` | TEXT (UUID) **nullable** | If set: inject into this conversation. If **null**: start a **fresh conversation** with `message` as the first user prompt. |
| `run_at_utc_secs` | INTEGER | When to run (Unix timestamp); for recurring, this is the *next* run |
| `message` | TEXT | **In-conversation:** short task/reminder text to inject ("Call John"). **New conversation:** full initial user prompt ("What's on my calendar today? Summarize and send to email."). |
| `profile_name` | TEXT (nullable) | LLM profile. For in-conversation: default from conversation or config. For **new conversation** this should be set (required when conversation_id is null). |
| `title` | TEXT (nullable) | Optional. For **new conversation** only: initial title (e.g. "Daily calendar digest"). If null, use a truncated `message` or "Scheduled run". |
| `status` | TEXT | `pending` \| `running` \| `completed` \| `failed` \| `cancelled` |
| `created_at_utc_secs` | INTEGER | Audit |
| `updated_at_utc_secs` | INTEGER | Audit |
| `error_message` | TEXT (nullable) | If status = failed |
| `schedule` | TEXT (nullable) | **Cron + once.** `null` or `"once"` = run once at `run_at_utc_secs`, then mark completed. Non-null cron = recurring; `run_at_utc_secs` is the *next* run (scheduler updates it after each run). |

Optional: `created_by_message_id` (UUID) to link back to the user message that triggered the schedule.

**Schedule format (cron-like):**
- **Once:** `schedule` = null or `"once"`. Run at `run_at_utc_secs`; after run → status = `completed`.
- **Recurring:** `schedule` = **5-field cron** (minute hour day-of-month month day-of-week), UTC. Standard: `min hr dom mon dow` (0–59, 0–23, 1–31, 1–12, 0–6 Sun–Sat). After each run → compute next occurrence (e.g. via `cron` or `cron_next` crate), set `run_at_utc_secs` to that, `status` = `pending`.

**Execution target:**
- **conversation_id set** → “in-conversation”: load that conversation, append synthetic user message “Scheduled task (due now): {message}”, run agent.
- **conversation_id null** → “fresh conversation”: create a new conversation (title from `title` or truncated `message`), add `message` as the first user message, run agent. Recurring + fresh = new conversation per run (one thread per occurrence).

**Cron examples:**

| User intent | `schedule` (stored) | Notes |
|-------------|---------------------|--------|
| Once at 14:00 tomorrow | null or "once" | `run_at_utc_secs` = that timestamp |
| Every hour | `0 * * * *` | Minute 0 of every hour |
| Every day at 9am UTC | `0 9 * * *` | 0 9 * * * |
| Every Monday at 9am UTC | `0 9 * * 1` | 0=Sun, 1=Mon, … 6=Sat |
| Every 1st of month at 9am | `0 9 1 * *` | |

*Note:* Standard cron has no “every two weeks”. Options: two jobs (e.g. 1st and 3rd Monday), or a small extension later (e.g. `0 9 * * 1/2` = every 2nd Monday) if needed.

---

## Internal tool: `schedule_task`

**Why internal:** Scheduling is server-owned state (conversation, run_at). Implementing it inside the Luna server keeps persistence and execution in one place and avoids an extra MCP server.

**Tool schema (for LLM):**

```json
{
  "name": "schedule_task",
  "description": "Schedule a task to run at a later time, once or repeatedly. Use for reminders, 'do X in 1 hour', 'every day at 9am', or 'every morning start a fresh conversation with prompt X'.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "run_at": {
        "type": "string",
        "description": "When to run (first time): relative e.g. 'in 30 minutes', or ISO 8601 e.g. '2025-02-01T09:00:00Z'. For recurring, this is the first run."
      },
      "message": {
        "type": "string",
        "description": "In-conversation: short task/reminder (e.g. 'Call John'). New conversation: full initial user prompt (e.g. 'What is on my calendar today? Summarize and send to my email.')."
      },
      "new_conversation": {
        "type": "boolean",
        "description": "If true, at run time create a fresh conversation and use message as the first user prompt (no prior context). If false or omitted, inject into the current conversation as a reminder/task."
      },
      "title": {
        "type": "string",
        "description": "Optional. For new_conversation only: title of the new conversation (e.g. 'Daily calendar digest'). Omit to auto-generate from message."
      },
      "schedule": {
        "type": "string",
        "description": "Optional. 'once' or omit = run once at run_at. For recurring, use 5-field cron (min hr dom mon dow, UTC): e.g. '0 * * * *' (every hour), '0 9 * * *' (daily 9am), '0 9 * * 1' (Monday 9am)."
      }
    },
    "required": ["run_at", "message"]
  }
}
```

**Behaviour:**
- Resolve `run_at` to a Unix timestamp (relative or absolute) → store in `run_at_utc_secs`.
- **Once:** Omit `schedule` or pass `"once"` → store `schedule` = null.
- **Recurring:** Pass cron string (e.g. `"0 9 * * *"`) → store in `schedule`. Optionally compute first next run from cron and set `run_at_utc_secs` to that (or keep user’s `run_at` as first run).
- **In-conversation:** Use current `conversation_id` and profile; store `message` as task text. `new_conversation` false or omitted.
- **Fresh conversation:** Set `conversation_id` = null; store `message` as full prompt, optional `title`; require `profile_name`. `new_conversation` = true.
- Insert into `scheduled_jobs`; return human-readable confirmation.

**Where to branch:** In the agent loop, when executing a tool call, if `tool_call.name == "schedule_task"` then call an internal handler (e.g. `schedule_task_handler(conversation_id, profile_name, params)`) instead of `registry.call_tool(...)`. Tool definitions for the LLM = MCP tools + internal tools (merge the two lists before calling the LLM).

---

## Execution path: `run_scheduled_task`

Reuse the same agent path as `handle_send_message`, but without a live WebSocket sender. Branch on **conversation_id**:

**A. In-conversation (conversation_id set):**
1. Load conversation and build LLM messages (same as `build_llm_messages` + context/summarization rules).
2. Append one **user** message:  
   `Scheduled task (due now): {job.message}. Please carry out this task.`
3. Optionally append a short system hint: “This is a scheduled reminder; the user is not necessarily online.”
4. Spawn agent task; broadcast to `conversation_id` subscribers.

**B. Fresh conversation (conversation_id null):**
1. Create a new conversation: `storage.create_conversation_with_profile(title, profile_name)`. Title = `job.title` or truncated `job.message` (e.g. “Scheduled run”).
2. Add `job.message` as the **first user message** (no synthetic wrapper; it *is* the user prompt).
3. Build LLM messages: system prompt + that one user message (no prior history). Use `job.profile_name` for profile (required).
4. Spawn agent task; broadcast to the **new** `conversation_id` subscribers. Any client that later loads this conversation will see the full thread (user prompt + agent response).
5. **Recurring + fresh:** Each run creates a **new** conversation, so “every day at 9am with prompt X” yields one new thread per day. Alternative (same conversation for recurring fresh prompt) is possible but “new conversation per run” keeps each run’s context isolated and is easier to reason about.

**Common:**
5. No need to “subscribe” a connection: broadcast is enough. If no one is viewing, events are just dropped.
6. **After execution:**
   - **Once** (`schedule` null or "once"): Set job `status` to `completed` (or `failed` if execution failed).
   - **Recurring** (`schedule` = cron): Compute **next** run from cron (e.g. `cron_next` crate: next occurrence after `now`), then `UPDATE scheduled_jobs SET run_at_utc_secs = ?, status = 'pending', updated_at_utc_secs = ? WHERE id = ?`. On failure you can still re-queue (retry next tick) or mark failed — recommend re-queue for transient errors.

**Critical:** Extract the “build messages + spawn agent task” logic into a shared function used by both `handle_send_message` and `run_scheduled_task`, so you don’t duplicate context building, summarization, and token logic.

**Next-run (recurring):** Use a cron library (e.g. Rust `cron` or `cron_next`) to compute the next occurrence after `now` from the 5-field expression; set `run_at_utc_secs` to that. No custom date math needed.

---

## Scheduler loop

- **Pattern:** Same as `spawn_title_generation_thread`: one `tokio::spawn(async move { loop { sleep(interval).await; ... } })`.
- **Interval:** 30–60 seconds is a good default (balance between latency and DB load).
- **Query:** `SELECT * FROM scheduled_jobs WHERE status = 'pending' AND run_at_utc_secs <= ? ORDER BY run_at_utc_secs ASC LIMIT N`.
- **Locking:** Before running, `UPDATE scheduled_jobs SET status = 'running', updated_at_utc_secs = ? WHERE id = ? AND status = 'pending'`. Only run if the update affected a row (avoids double execution after a long-running job).
- **After run (recurring):** If `schedule` is cron, compute next run from cron (cron library); then `UPDATE … SET run_at_utc_secs = ?, status = 'pending', … WHERE id = ?`. Same row = next occurrence.
- **Concurrency:** Run jobs one-by-one in the loop for v1 (or process a small batch sequentially). No need for a thread pool until you have high volume.

---

## Component checklist

| Component | Location | Notes |
|-----------|----------|--------|
| **scheduled_jobs table** | Storage (SQLite migration or init) | Add in `sqlite_storage_simple` or storage wrapper |
| **Scheduler loop** | `server/mod.rs` (or a small `server/scheduler.rs`) | Spawn at startup like title generation |
| **run_scheduled_task** | `handlers.rs` or `server/scheduler.rs` | Shared “run agent for conversation with this message” helper |
| **Internal tool: schedule_task** | Agent loop + a small module (e.g. `services/schedule_service.rs`) | Register tool def; in loop, if name == `schedule_task` call handler instead of MCP |
| **Tool list merge** | Where you build `available_tools` for the LLM | MCP tools + internal tools (e.g. `[schedule_task]`) |
| **run_at parsing** | Schedule service or a small util | Relative (“in 1 hour”) and absolute (ISO / timestamp) |
| **Cron next-run** | Schedule service | Use `cron` / `cron_next` crate to compute next run from 5-field expression |

---

## Edge cases and limits

- **Conversation deleted (in-conversation only):** Before running, check that the conversation still exists. If not, mark job `cancelled` or `failed`. Fresh-conversation jobs don’t reference an existing conversation so no check needed.
- **Profile missing:** Use conversation’s profile or default; if none, fail the job and set `error_message`.
- **Very long execution:** Scheduler loop is blocked for that job. For v1, acceptable; later you can run each job in a separate spawn and only update DB on completion.
- **Timezone:** Store and compare in UTC; show users “local” time in UI if needed.
- **Recurring:** Supported via cron string in `schedule`; after each run, compute next run from cron and update `run_at_utc_secs`, set `status` = `pending`. Same row = next occurrence.
- **Cancelling a recurring job:** User could say “cancel the daily digest”; agent would need a tool like `cancel_scheduled_task(job_id)` or “cancel by conversation_id + message match” — store job id in a way the agent can reference, or add a list endpoint so the agent can cancel by id.

---

## Summary

| Idea | Implementation |
|------|-----------------|
| “Do X in one hour” | Agent calls `schedule_task(run_at, message)` → one-shot row (`schedule` = null). |
| “Every hour / every day / every Monday” | Agent calls `schedule_task(run_at, message, schedule: "0 * * * *"` or `"0 9 * * *"` or `"0 9 * * 1"`) → row with cron in `schedule`. After each run, cron library gives next run; update `run_at_utc_secs`, `status` = `pending`. |
| “Every morning start a fresh conversation with prompt X” | Agent calls `schedule_task(run_at, message, new_conversation: true, schedule: "0 8 * * *")` → row with `conversation_id` = null, cron in `schedule`. At run time: create conversation, add prompt, run agent; each run = new conversation. |
| “In one hour / at 9am / every day this is executed” | Scheduler loop finds due rows, calls `run_scheduled_task` → in-conversation or fresh; if cron, compute next run and re-queue same row. |
| Where it runs | Same agent loop, same conversation, same broadcast mechanism; only trigger is time-based. |
| Persistence | SQLite `scheduled_jobs`; scheduler loop in-process, no extra daemon. |

This keeps scheduling (one-shot and recurring) as a thin layer on top of your existing agent and broadcast design, with minimal new surface area and one clear execution path.
