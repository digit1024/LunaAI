# Importance Scoring - Visual Guide

## 📊 How Messages Are Scored

```
┌─────────────────────────────────────────────────────────────┐
│                    IMPORTANCE SCORING                        │
└─────────────────────────────────────────────────────────────┘

System Message
├─ Base Score: 100 (always kept)
└─ Final: 100 ⭐ ALWAYS KEEP

User Message (recent)
├─ Base Score: 80
├─ Recency: +50 (last message)
├─ Question Bonus: +20 (if contains "?")
└─ Final: 150 ⭐ HIGH PRIORITY

Assistant with Tool Calls
├─ Base Score: 40
├─ Recency: +45 (2nd from end)
├─ Tool Chain Bonus: +30 (triggers tools)
└─ Final: 115 ⭐ HIGH PRIORITY

Tool Result (linked to above)
├─ Base Score: 60
├─ Recency: +40 (3rd from end)
├─ Tool Chain Bonus: +30 (part of chain)
└─ Final: 130 ⭐ HIGH PRIORITY (must keep if tool call kept)

Old User Message (20 messages ago)
├─ Base Score: 80
├─ Recency: +30 (distance = 20, decay applied)
└─ Final: 110 ⭐ MEDIUM PRIORITY

Old Assistant Message (no tools, 30 messages ago)
├─ Base Score: 40
├─ Recency: +20 (distance = 30, decay applied)
└─ Final: 60 ⭐ LOW-MEDIUM PRIORITY

Very Old Assistant (50+ messages ago)
├─ Base Score: 40
├─ Recency: 0 (too old, no bonus)
└─ Final: 40 ⭐ LOW PRIORITY (can drop)
```

## 🔗 Tool Chain Preservation

```
Message Flow:
┌─────────────────┐
│ User: "Do X"    │ ← Importance: 150
└────────┬────────┘
         │
         ▼
┌───────────────────────────────┐
│ Assistant: [tool_calls]       │ ← Importance: 115
│ - call_id: "abc"              │   MUST KEEP IF KEEPING TOOL RESULT
└────────┬──────────────────────┘
         │
         ▼
┌───────────────────────────────┐
│ Tool: [tool_call_id: "abc"]   │ ← Importance: 130
│ Result: {...}                 │   MUST KEEP IF KEEPING TOOL CALL
└────────┬──────────────────────┘
         │
         ▼
┌───────────────────────────────┐
│ Assistant: "Based on tool..." │ ← Importance: 100
└───────────────────────────────┘

Rule: If any part of chain is kept, entire chain should be kept
```

## 🎯 Context Selection Algorithm

```
Step 1: Calculate all scores
┌─────────────────────────────────┐
│ For each message:               │
│ 1. Base score (role)            │
│ 2. Recency bonus                │
│ 3. Tool chain bonus             │
│ 4. Question bonus               │
│ 5. Attachment bonus             │
│ 6. Penalties                    │
│ → Final importance score        │
└─────────────────────────────────┘

Step 2: Identify tool chains
┌─────────────────────────────────┐
│ Group messages by tool_call_id  │
│ Mark which messages are linked  │
└─────────────────────────────────┘

Step 3: Select messages
┌─────────────────────────────────┐
│ 1. ALWAYS include System msgs   │
│ 2. Sort by importance (high→low)│
│ 3. Add messages until token     │
│    budget exhausted             │
│ 4. If dropping tool result,     │
│    also drop corresponding call │
│ 5. If keeping tool call,        │
│    ensure result is included    │
└─────────────────────────────────┘
```

## 📈 Recency Decay Function

```
Score Bonus
  ↑
 50│●───────────────────────────
   │ ●
 40│   ●
   │     ●
 30│       ●
   │         ●
 20│           ●
   │             ●
 10│               ●
   │                 ●
  0└────────────────────────────► Distance from end
   0  10  20  30  40  50  60  70

Formula: bonus = max(0, 50 - (distance * 0.5))
Last 10 messages get significant boost
After ~50 messages, no recency bonus
```

## ⚖️ Token Budget Allocation

```
Total Context Limit: 100,000 tokens (e.g., Claude 3.5)
  │
  ├─ Reserve for response: 20,000 tokens (20%)
  │
  └─ Available for context: 80,000 tokens
     │
     ├─ System prompts: ~2,000 tokens
     │
     └─ Conversation history: ~78,000 tokens
        │
        ├─ Recent messages (last 10): ~10,000 tokens
        │
        ├─ Tool chains: ~30,000 tokens
        │
        └─ Other messages: ~38,000 tokens
           (selected by importance score)
```

## 🎯 Decision Flow

```
Is message System?
  YES → Keep (score = 100)
  NO  ↓
      │
Is message in recent N (e.g., 10)?
  YES → High recency bonus (+40-50)
  NO  ↓
      │
Does message have tool_calls?
  YES → Tool chain bonus (+30)
  NO  ↓
      │
Is message Tool result?
  YES → Tool chain bonus (+30)
        Check if linked tool_call is kept
  NO  ↓
      │
Is message User question?
  YES → Question bonus (+20)
  NO  ↓
      │
Calculate final score → Sort → Select by token budget
```








