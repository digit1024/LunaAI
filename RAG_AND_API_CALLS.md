# How RAG Connects with API Calls

## 🎯 Key Point: RAG Doesn't Make API Calls

**RAG is a local retrieval system that enhances the context sent TO the LLM API.**

---

## 📊 The Flow: RAG → API Call

### Current Flow (Without RAG)

```
User Query
    ↓
Build Messages (conversation history)
    ↓
Send to LLM API
    ↓
Get Response
```

### Flow with RAG

```
User Query
    ↓
RAG Retrieval (LOCAL - no API call)
    ↓
Build Messages (conversation history + RAG context)
    ↓
Send to LLM API (with enhanced context)
    ↓
Get Response
```

---

## 🔍 Detailed Breakdown

### Step 1: RAG Retrieval (Local, No API Call)

```rust
// This happens BEFORE the API call
async fn process_message(user_query: &str) {
    // RAG retrieval - all local, no API call
    let rag_context = rag_retriever.retrieve_context(user_query, 3)?;
    // Returns: Vec<String> with relevant document chunks
    
    // Example result:
    // [
    //   "From project_notes.md: Error handling in Rust uses Result types...",
    //   "From meeting_notes.md: We discussed async patterns with futures...",
    //   "From docs.md: Performance optimization requires careful profiling..."
    // ]
}
```

**What happens:**
1. Query converted to embedding vector (local model)
2. Vector similarity search in local database
3. Top-k relevant chunks retrieved
4. **No API calls** - all local computation

**Time:** ~10-50ms (local search)
**Cost:** $0 (local computation)

---

### Step 2: Context Injection (Before API Call)

```rust
// In src/server/handlers.rs (modified)
async fn handle_send_message(&mut self, content: String) -> Result<()> {
    // 1. RAG retrieval (local)
    let rag_context = self.rag_retriever.retrieve_context(&content, 3).await?;
    
    // 2. Build messages with RAG context
    let mut llm_messages = self.build_llm_messages(conversation_uuid).await?;
    
    // 3. Inject RAG context as system message or prepend to user message
    if !rag_context.is_empty() {
        let context_text = format!(
            "Relevant context from knowledge base:\n{}\n",
            rag_context.join("\n\n")
        );
        
        // Option A: Add as system message
        llm_messages.insert(0, LlmMessage::new(
            Role::System,
            context_text
        ));
        
        // OR Option B: Prepend to user message
        // llm_messages.last_mut().unwrap().content = format!(
        //     "{}\n\n{}", context_text, content
        // );
    }
    
    // 4. Add current user message
    llm_messages.push(LlmMessage::new(Role::User, content));
    
    // 5. NOW send to LLM API (with RAG context included)
    let response = llm_client.send_message_stream(llm_messages).await?;
}
```

**What gets sent to API:**

```json
{
  "messages": [
    {
      "role": "system",
      "content": "Relevant context from knowledge base:\n\nFrom project_notes.md: Error handling in Rust uses Result types...\n\nFrom meeting_notes.md: We discussed async patterns..."
    },
    {
      "role": "user",
      "content": "What did I learn about error handling?"
    }
  ]
}
```

---

### Step 3: API Call (With Enhanced Context)

```rust
// In src/llm/openai.rs (or anthropic.rs, etc.)
async fn send_message_stream(
    &self,
    messages: Vec<Message>,  // Includes RAG context
) -> Result<impl Stream<Item = Result<String>>> {
    // Build API request
    let request = OpenAIRequest {
        model: self.profile.model.clone(),
        messages: messages,  // RAG context is already in here
        temperature: self.profile.temperature,
        max_tokens: self.profile.max_tokens,
        stream: true,
    };
    
    // Make API call
    let response = self.client
        .post(&format!("{}/chat/completions", self.profile.endpoint))
        .json(&request)
        .send()
        .await?;
    
    // Stream response
    // ...
}
```

**What happens:**
1. Messages (with RAG context) sent to OpenAI/Anthropic/etc. API
2. LLM processes the enhanced context
3. Response includes knowledge from your documents
4. **One API call** - RAG context is part of the request

---

## 💰 Cost Implications

### Without RAG

```
User Query: "What did I learn about async?" (10 tokens)
+ Conversation History: 500 tokens
= Total: 510 tokens sent to API
```

### With RAG

```
User Query: "What did I learn about async?" (10 tokens)
+ Conversation History: 500 tokens
+ RAG Context: 300 tokens (retrieved documents)
= Total: 810 tokens sent to API
```

**Impact:**
- ✅ Better responses (LLM has relevant context)
- ⚠️ More tokens = higher cost
- ⚠️ More tokens = longer context window needed

**Cost Example (GPT-4):**
- Without RAG: 510 tokens × $0.03/1k = $0.015
- With RAG: 810 tokens × $0.03/1k = $0.024
- **Difference: +$0.009 per request** (but better quality)

---

## 🔄 Comparison: RAG vs MCP Tool Calls

### RAG Flow (Context Enhancement)

```
User Query
    ↓
RAG Retrieval (LOCAL, ~10ms, $0)
    ↓
Inject Context into Messages
    ↓
Send to LLM API (ONE call, includes context)
    ↓
Get Response
```

**Characteristics:**
- Happens BEFORE API call
- Local computation (no API cost for retrieval)
- Context included in single API request
- Increases token count

---

### MCP Tool Call Flow (Action Execution)

```
User Query
    ↓
Send to LLM API (FIRST call)
    ↓
LLM decides to call tool
    ↓
MCP Tool Call (external API/operation)
    ↓
Tool Result
    ↓
Send to LLM API (SECOND call, with tool result)
    ↓
Get Final Response
```

**Characteristics:**
- Happens AFTER first API call
- May involve external API calls (costs money)
- Multiple API calls (LLM → tool → LLM)
- Tool calls add latency

---

## 📋 Real-World Example

### Scenario: "What did I write about error handling in my notes?"

#### Without RAG

```rust
// 1. Build messages (just conversation history)
let messages = vec![
    system_prompt,
    ...conversation_history...,
    user_query: "What did I write about error handling?"
];

// 2. Send to API
let response = llm_client.send_message(messages).await?;
// LLM responds: "I don't have access to your notes. You could use a file search tool..."
```

**Result:** LLM doesn't know about your notes

---

#### With RAG

```rust
// 1. RAG retrieval (local, no API)
let rag_context = rag_retriever.retrieve_context(
    "error handling", 
    3
).await?;
// Returns: [
//   "From notes.md: Error handling in Rust uses Result<T, E> types...",
//   "From docs.md: Exception management patterns...",
//   "From meeting.md: We discussed error recovery strategies..."
// ]

// 2. Build messages with RAG context
let messages = vec![
    system_prompt,
    LlmMessage::new(Role::System, format!(
        "Relevant context:\n{}", rag_context.join("\n\n")
    )),
    ...conversation_history...,
    user_query: "What did I write about error handling?"
];

// 3. Send to API (with context)
let response = llm_client.send_message(messages).await?;
// LLM responds: "Based on your notes, you wrote about Result types in Rust..."
```

**Result:** LLM has access to your notes and can answer

---

#### With MCP Tool (Alternative)

```rust
// 1. Send query to LLM
let messages = vec![
    system_prompt,
    user_query: "What did I write about error handling?"
];

let response = llm_client.send_message_with_tools(messages, tools).await?;
// LLM decides: "I should search the user's notes"
// LLM calls: search_files_tool("error handling")

// 2. MCP tool executes (may call external API or local search)
let tool_result = mcp_registry.call_tool(tool_call).await?;
// Returns: File search results

// 3. Send tool result back to LLM
let messages = vec![
    system_prompt,
    user_query,
    tool_result_message
];

let final_response = llm_client.send_message(messages).await?;
```

**Result:** Works, but requires:
- LLM to decide to call tool
- Multiple API calls
- Tool call overhead

---

## 🎯 Key Differences

| Aspect | RAG | MCP Tool Call |
|--------|-----|---------------|
| **When** | Before API call | After first API call |
| **Location** | Local (no API) | May be external API |
| **Cost** | $0 (local) | May cost money |
| **API Calls** | 1 (with context) | 2+ (query → tool → response) |
| **Latency** | +10-50ms (local) | +100-500ms (tool call) |
| **Automatic** | ✅ Yes | ❌ LLM must decide |
| **Context** | Included in request | Added in follow-up |

---

## 💡 Implementation in LunaAI

### Current Code Flow

```rust
// src/server/handlers.rs
async fn handle_send_message(&mut self, content: String) -> Result<()> {
    // 1. Build messages from conversation history
    let mut llm_messages = self.build_llm_messages(conversation_uuid).await?;
    
    // 2. Add current user message
    llm_messages.push(LlmMessage::new(Role::User, content));
    
    // 3. Inject prompts
    let agent_messages = inject_prompts(llm_messages, &prompt_manager, &profile)?;
    
    // 4. Send to LLM API
    loop_engine.process_message(agent_messages, ...).await?;
}
```

### With RAG Added

```rust
// src/server/handlers.rs (modified)
async fn handle_send_message(&mut self, content: String) -> Result<()> {
    // 1. RAG retrieval (NEW - local, no API)
    let rag_context = if self.ctx.rag_enabled {
        self.ctx.rag_retriever.retrieve_context(&content, 3).await?
    } else {
        Vec::new()
    };
    
    // 2. Build messages from conversation history
    let mut llm_messages = self.build_llm_messages(conversation_uuid).await?;
    
    // 3. Inject RAG context (NEW)
    if !rag_context.is_empty() {
        let context_msg = LlmMessage::new(
            Role::System,
            format!("Relevant context from knowledge base:\n\n{}", 
                rag_context.join("\n\n"))
        );
        // Insert after system prompts, before conversation history
        llm_messages.insert(
            system_prompt_count,
            context_msg
        );
    }
    
    // 4. Add current user message
    llm_messages.push(LlmMessage::new(Role::User, content));
    
    // 5. Inject prompts
    let agent_messages = inject_prompts(llm_messages, &prompt_manager, &profile)?;
    
    // 6. Send to LLM API (with RAG context included)
    loop_engine.process_message(agent_messages, ...).await?;
}
```

---

## 📊 Token Usage Example

### Query: "What did I learn about async programming?"

**Without RAG:**
```
System Prompt: 200 tokens
Conversation History: 500 tokens
User Query: 10 tokens
─────────────────────────
Total: 710 tokens
Cost: $0.021 (GPT-4)
```

**With RAG (3 chunks, ~100 tokens each):**
```
System Prompt: 200 tokens
RAG Context: 300 tokens  ← Added
Conversation History: 500 tokens
User Query: 10 tokens
─────────────────────────
Total: 1,010 tokens
Cost: $0.030 (GPT-4)
Increase: +$0.009 per request
```

**But:** Response quality is much better because LLM has relevant context!

---

## 🎯 Summary

**RAG and API Calls:**
1. **RAG doesn't make API calls** - it's local retrieval
2. **RAG enhances context** sent TO the LLM API
3. **RAG happens BEFORE** the API call
4. **One API call** includes RAG context
5. **More tokens** = higher cost, but better responses

**Key Insight:**
- RAG = **Enhance the input** (before API call)
- MCP = **Execute actions** (after API call, may involve more API calls)

**They work together:**
- RAG provides knowledge context
- MCP performs actions based on that knowledge








