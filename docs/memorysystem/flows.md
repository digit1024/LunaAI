# Memory System Flows

Request and background flows with sequence and flowchart diagrams.

---

## 1. Memory RAG injection (per user message)

On every `SendMessage`, after building LLM messages and injecting system prompts, the server retrieves relevant memories and injects them as one system message. Recalled IDs are persisted so the same memory is not re-injected in the same conversation (and dedup survives restarts).

### Sequence diagram

```mermaid
sequenceDiagram
    participant User
    participant Handler as handle_send_message
    participant Storage as SQLite
    participant RAG as memory_rag
    participant Dedup as memory_dedup map

    User->>Handler: Send message (content, conversation_id)
    Handler->>Handler: build_llm_messages, inject_prompts
    Handler->>Storage: get_recalled_memory_ids(conversation_id)
    Storage-->>Handler: Vec of previously recalled memory IDs
    Handler->>Dedup: Seed used_ids if empty
    Handler->>RAG: retrieve_memory_context(storage, content, used_ids)
    RAG->>RAG: extract_keywords(content), len >= 3
    RAG->>Storage: search_memory(keywords, 10)
    Storage->>Storage: memory_fts MATCH, BM25
    Storage-->>RAG: Vec MemoryEntry
    RAG->>RAG: Filter out used_ids, format message
    RAG->>Dedup: Add new IDs to used_ids
    RAG-->>Handler: Some((system_message, new_ids))
    Handler->>Handler: Insert system message into llm_messages
    Handler->>Storage: record_memory_recalls(conversation_id, new_ids)
    Handler->>Handler: Context selection, spawn agent
    Handler-->>User: Stream response
```

### Flowchart (RAG internals)

```mermaid
flowchart TD
    Start[User message] --> Extract[extract_keywords: split, len >= 3, dedup]
    Extract --> Empty{Keywords empty?}
    Empty -- Yes --> ReturnNone[Return None]
    Empty -- No --> FTS[storage.search_memory OR query, limit 10]
    FTS --> Filter[Filter entries not in used_ids]
    Filter --> NoNew{Any new entries?}
    NoNew -- No --> ReturnNone
    NoNew -- Yes --> Format[Format system message with id, content, importance]
    Format --> UpdateUsed[Add new IDs to used_ids]
    UpdateUsed --> Return[Return Some message, new_ids]
```

---

## 2. Recording and loading recalls

Recalls are written when Memory RAG injects memories; they are read when the dedup set for that conversation is empty (e.g. first message after server restart).

### Write path

```mermaid
sequenceDiagram
    participant Handler
    participant Storage

    Handler->>Handler: retrieve_memory_context returns new_ids
    Handler->>Storage: record_memory_recalls(conversation_id, new_ids)
    Storage->>Storage: INSERT OR IGNORE conversation_memory_recalls per id
```

### Read path (seed dedup)

```mermaid
sequenceDiagram
    participant Handler
    participant Storage
    participant Dedup

    Handler->>Dedup: entry(conversation_uuid).or_default()
    Handler->>Handler: used_ids.is_empty?
    alt empty
        Handler->>Storage: get_recalled_memory_ids(conversation_id)
        Storage->>Storage: SELECT memory_id FROM conversation_memory_recalls
        Storage-->>Handler: Vec i64
        Handler->>Dedup: used_ids.extend(recalled)
    end
```

---

## 3. Deep Sleep cycle (background)

A single cycle processes all unprocessed conversations in batches. Each batch runs Step 1 → Step 2 → Step 3, then the watermark is advanced. The cycle ends when Step 1 returns an empty digest (no conversations with messages after the watermark).

### High-level cycle loop

```mermaid
flowchart TD
    Start[run_deep_sleep_cycle] --> LoadWatermark[Read last_processed_message_id from state]
    LoadWatermark --> Loop[Batch loop]
    Loop --> Step1[Step 1: Summarize conversations]
    Step1 --> Empty{Digest empty?}
    Empty -- Yes --> Done[Save last_run_at, exit]
    Empty -- No --> Step2[Step 2: Evaluate all memories vs digest]
    Step2 --> Step3[Step 3: Extract new memories from digest]
    Step3 --> Persist[Set last_processed_message_id = max_msg_id]
    Persist --> Loop
```

### Step 1: Summarize conversations

```mermaid
flowchart TD
    S1Start[step1_summarize_conversations] --> Query[get_conversations_with_messages_after watermark, limit]
    Query --> NoConvos{Empty?}
    NoConvos -- Yes --> ReturnEmpty[Return empty digest, same watermark]
    NoConvos -- No --> ForEach[For each conversation]
    ForEach --> LoadMsgs[load_conversation_messages]
    LoadMsgs --> BuildText[build_conversation_text: user/assistant/summary, truncate 500]
    BuildText --> LLM[LLM: summarize 2-5 bullet points]
    LLM --> Append[Append to digest parts]
    Append --> Delay[Sleep inter_call_delay_secs]
    Delay --> ForEach
    ForEach -- Done --> Join[Join parts, max_msg_id]
    Join --> Return[Return digest, max_msg_id]
```

### Step 2: Evaluate memories

```mermaid
flowchart TD
    S2Start[step2_evaluate_memories] --> List[list_memory 1000]
    List --> Chunk[Chunk by memory_batch_size]
    Chunk --> EvalLoop[For each chunk]
    EvalLoop --> EvalLLM[LLM: digest + memory list, return JSON actions]
    EvalLLM --> Apply[Apply KEEP / UPDATE / DELETE to storage]
    Apply --> Delay[Sleep inter_call_delay_secs]
    Delay --> EvalLoop
    EvalLoop -- Done --> Log[Log updated, deleted, kept]
```

### Step 3: Extract new memories

```mermaid
flowchart TD
    S3Start[step3_extract_new_memories] --> List[list_memory 1000]
    List --> ExtractLLM[LLM: digest + current memories, return new facts JSON]
    ExtractLLM --> ForEach[For each proposed memory]
    ForEach --> Dedup[FTS5 search + 70% word overlap check]
    Dedup --> Skip{Duplicate?}
    Skip -- Yes --> Next[Skip]
    Skip -- No --> Store[store_memory]
    Store --> Next
    ForEach -- Done --> Delay[Sleep inter_call_delay_secs]
```

### Scheduler (when does it run?)

```mermaid
flowchart TD
    Boot[Server start] --> Spawn[spawn_deep_sleep_loop if enabled and profile set]
    Spawn --> Tick[Every 5 min tick]
    Tick --> IsDue[is_due: last_run_at + interval_hours <= now?]
    IsDue -- No --> Tick
    IsDue -- Yes --> BuildClient[Build LLM client from profile]
    BuildClient --> Run[run_deep_sleep_cycle]
    Run --> Tick
```

---

## 4. Manual Deep Sleep run

```mermaid
sequenceDiagram
    participant User
    participant CLI as cosmic_llm --deep-sleep
    participant Config
    participant Storage
    participant Service as deep_sleep_service

    User->>CLI: --deep-sleep
    CLI->>Config: Load config, resolve deep_sleep.profile
    CLI->>Storage: Open DB (same path as server)
    CLI->>Service: run_deep_sleep_cycle(storage, config, llm_client)
    Service->>Service: Full cycle until backlog done
    Service-->>CLI: Ok or Err
    CLI-->>User: Exit 0 or 1
```

---

## 5. Data flow summary

```mermaid
flowchart LR
    subgraph Inputs
        Conversations[Conversations and messages]
        MemoryStore[memory table]
    end

    subgraph RAG
        RAGRetrieve[Keyword search]
        RAGInject[Inject as system message]
        RAGRecalls[conversation_memory_recalls]
    end

    subgraph DeepSleep
        Digest[Session digest]
        Eval[Evaluate memories]
        Extract[Extract new]
    end

    Conversations --> RAGRetrieve
    MemoryStore --> RAGRetrieve
    RAGRetrieve --> RAGInject
    RAGInject --> RAGRecalls

    Conversations --> Digest
    Digest --> Eval
    MemoryStore --> Eval
    Eval --> MemoryStore
    Digest --> Extract
    MemoryStore --> Extract
    Extract --> MemoryStore
```
