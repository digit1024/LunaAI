---
name: scheduling
description: Schedule tasks to run later, once or repeatedly. Use the schedule_task tool when the user asks for reminders, "do X in N", "every day at X", or "start a fresh conversation with prompt X at time Y".
allowed-tools:
  - schedule_task
  - cancel_scheduled_task
license: MIT
---

# Scheduling System

Schedule tasks to run at a later time, once or on a recurring basis. At run time the agent executes in the same conversation (reminder/task) or in a new conversation (fresh prompt).

## When to Use This Skill

**Activate when:**

1. **Explicit reminder/task:** User says "remind me in 1 hour to…", "in 30 minutes do…", "at 5pm send…"
2. **Recurring task:** User says "every day at 9am…", "every Monday…", "every hour…"
3. **Fresh conversation on a schedule:** User says "every morning start a new chat with: What's on my calendar?", "daily at 8am run with prompt: summarize my emails"
4. **Deferred action:** User clearly wants something to happen later, not now

**DO NOT use for:**

- Things the user wants done **right now**
- One-off answers or immediate follow-ups
- Vague "maybe later" without a clear time or recurrence
- When the user is only asking what time something is (no request to schedule)

## Tools

### `schedule_task`

Internal tool (sent to the API with other MCP tools). Call it with structured parameters; invalid `run_at` or `schedule` returns a tool error — report that clearly to the user. **The tool returns the job id** (e.g. "Scheduled for ... (id: abc-123)") — the user needs this id to cancel later.

### Parameters

| Parameter          | Required | Description |
|--------------------|----------|-------------|
| `run_at`           | Yes      | When to run (first time): relative e.g. `"in 30 minutes"`, `"in 2 hours"`, or absolute ISO 8601 e.g. `"2025-02-01T09:00:00Z"`. |
| `message`          | Yes      | **In-conversation:** short task/reminder (e.g. "Call John"). **New conversation:** full initial user prompt (e.g. "What's on my calendar today? Summarize and send to my email."). |
| `schedule`         | No       | Omit or `"once"` = run once at `run_at`. **Recurring:** 5-field cron (min hr dom mon dow) in **UTC**, e.g. `"0 9 * * *"` (daily 9am UTC), `"0 * * * *"` (every hour). |
| `new_conversation` | No       | `true` = at run time create a **new** conversation and use `message` as the first user prompt (no prior context). `false` or omit = inject into **current** conversation as a reminder. |
| `title`            | No       | For `new_conversation: true` only: conversation title (e.g. "Daily calendar digest"). Omit to auto-generate from `message`. |

### Schedule format (cron)

- **5 fields:** `minute hour day-of-month month day-of-week` (UTC).
- Minute 0–59, hour 0–23, day-of-month 1–31, month 1–12, day-of-week 0–6 (0 = Sunday).
- Use `*` for “every”. Examples:
  - Every hour: `0 * * * *`
  - Every day at 9am UTC: `0 9 * * *`
  - Every Monday at 9am UTC: `0 9 * * 1`
  - 1st of month at 9am: `0 9 1 * *`
- Invalid cron returns a tool error — tell the user the format (5-field, UTC).

### `cancel_scheduled_task`

Cancel (delete) a scheduled task by its id. Use when the user says "cancel that reminder", "remove the daily digest", "stop the scheduled task".

| Parameter | Required | Description |
|-----------|----------|-------------|
| `job_id`  | Yes      | UUID of the scheduled job, as returned when the task was scheduled (e.g. from "Scheduled for ... (id: abc-123)"). |

- If the job exists and was deleted: confirm to the user.
- If no job found (already run, already cancelled, or wrong id): tell the user clearly (tool returns a message; relay it).

## Examples

**One-shot reminder (current conversation):**
```json
{
  "run_at": "in 1 hour",
  "message": "Call John about the project"
}
```

**Daily at 9am UTC (current conversation):**
```json
{
  "run_at": "2025-02-02T09:00:00Z",
  "message": "Send me a digest of today's priorities",
  "schedule": "0 9 * * *"
}
```

**Every Monday at 9am UTC:**
```json
{
  "run_at": "next Monday 09:00",
  "message": "Run the weekly report",
  "schedule": "0 9 * * 1"
}
```

**Fresh conversation every morning (new thread each day):**
```json
{
  "run_at": "2025-02-02T08:00:00Z",
  "message": "What's on my calendar today? Summarize and send to my email.",
  "new_conversation": true,
  "title": "Daily calendar digest",
  "schedule": "0 8 * * *"
}
```

## Workflow

### Step 1: Detect scheduling intent
- User gives a **time** (relative or absolute) and/or **recurrence** (every day, every hour, etc.).
- User wants an **action** at that time (reminder, task, or “run this prompt in a new chat”).

### Step 2: Choose target
- **Current conversation:** reminder or follow-up in this chat → `new_conversation: false` or omit.
- **New conversation:** “every morning start a **new** conversation with prompt X” → `new_conversation: true`, and `message` = full prompt.

### Step 3: Set run time and recurrence
- **Once:** `run_at` only; omit `schedule` or use `"once"`.
- **Recurring:** set `run_at` (first run) and `schedule` (5-field cron, UTC). Validate cron; if the tool returns an error, show it to the user.

### Step 4: Call `schedule_task`
- Pass `run_at`, `message`, and optionally `schedule`, `new_conversation`, `title`.
- On success, confirm to the user (e.g. “Scheduled for …”, “Recurring every …”). On tool error (invalid schedule, etc.), report the error in plain language.

## Message guidelines

**Good `message` (in-conversation):**
- ✅ "Call John about the contract"
- ✅ "Send daily digest to my email"
- ✅ "Check build status and report"

**Good `message` (new conversation):**
- ✅ "What's on my calendar today? Summarize and email me."
- ✅ "Summarize my unread emails and list action items."

**Bad:**
- ❌ "" (empty — tool will error)
- ❌ Too vague: "Do the thing we discussed"
- ❌ Mixing reminder text with full prompt when `new_conversation: true` — use the **full prompt** the user wants in the new chat

## Critical rules

1. **Always use the tool** for “later” or “every X” — do not promise to “remember” without calling `schedule_task`.
2. **Validate in your head:** `run_at` must be parseable (relative or ISO/Unix); recurring `schedule` must be valid 5-field cron (UTC).
3. **One-shot vs recurring:** Omit `schedule` (or `"once"`) for once; use cron for recurring. Wrong format causes a tool error.
4. **new_conversation:** Only set `true` when the user explicitly wants a **new** conversation per run (e.g. “every day start a new chat with this prompt”). Otherwise the reminder runs in the **current** conversation.
5. **Confirm to the user** what was scheduled and when (and if recurring, how often). **Include the job id** in your reply when relevant so the user can cancel later (e.g. “Scheduled for 14:00 UTC (id: abc-123). You can cancel it anytime by saying ‘cancel that reminder’.”).
6. **Cancelling:** When the user asks to cancel a scheduled task, call `cancel_scheduled_task` with the `job_id` they were given (or that you have from the last schedule confirmation). If they don’t have the id, say you need it or that they can describe which task (e.g. “the daily digest”) and you’ll cancel it if you have the id from earlier in the conversation.

## Notes

- All times are **UTC** for cron. Tell the user “9am UTC” (or equivalent) if relevant.
- Recurring + `new_conversation: true` creates a **new** conversation each run (e.g. one thread per day).
- The tool returns a confirmation string (e.g. “Scheduled for 2025-02-01 14:00 UTC (id: …)”). Use it to confirm to the user.
- **Cancelling:** `cancel_scheduled_task(job_id)` deletes the job from the DB. The job id is in the confirmation when the user scheduled the task; if the user says “cancel the one I just scheduled” use that id from your last tool result.
- Standard cron does not support “every two weeks”; use two cron-based jobs (e.g. 1st and 3rd Monday) or document the limitation if the user asks.
