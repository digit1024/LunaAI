# Context Management Implementation Guide (Option 3: Hybrid Approach)

## 🎯 Overview

This document outlines the implementation of **Option 3: Hybrid Approach** for context overflow management, including:
1. **Model-specific token counting** (handles different tokenization schemes)
2. **Importance scoring** for messages and tool calls
3. **Smart context selection** that preserves critical information

---

## 📊 Part 1: Determining Importance

### Importance Scoring System

We need to score each message/tool call based on multiple factors:

#### **Message Importance Score (0-100)**

```rust
pub struct MessageImportance {
    pub base_score: f32,           // Role-based (System=100, User=80, Assistant=40, Tool=60)
    pub recency_bonus: f32,        // More recent = higher score (decay function)
    pub tool_chain_bonus: f32,     // Part of active tool chain = +30
    pub user_question_bonus: f32,  // User asking question = +20
    pub content_length_penalty: f32, // Very long messages = -10
    pub attachment_bonus: f32,     // Has attachments = +15
}
```

#### **Scoring Rules:**

1. **System Messages (Always Keep)**
   - System messages are added automatically to the conversation so we don't have to manage them. 

2. **Recent Messages**
   - Formula: `base_score + (100 - position_from_end) * 0.5`
   - Last message: +50 points
   - 2nd to last: +49 points
   - etc.
   - Decay: `recency_bonus = max(0, 50 - (distance_from_end * 0.5))`

3. **Tool Call Chains (Critical)**
   - **Assistant message with tool_calls**: +30 points
   - **Tool result message**: +30 points (linked by `tool_call_id`)
   - **Rule**: If keeping tool call, MUST keep corresponding result
   - **Rule**: If keeping tool result, SHOULD keep corresponding call

4. **User Questions**
   - Pattern detection: Ends with "?", contains "what/how/why/when/where"
   - Bonus: +20 points
   - These are usually followed by assistant responses we want to keep

5. **Content Length**
   - Very short (< 50 chars): No penalty
   - Long (> 1000 chars): -10 points (might be verbose)
   - But long user messages might be important (instructions)

6. **Attachments**
   - Has attachments: +15 points
   - Attachments are expensive (tokens) but often important

7. **Reasoning Content** (DeepSeek)
   - Has reasoning_content: +10 points
   - Indicates complex thinking we might want to preserve

#### **Implementation:**

```rust
// src/server/context_manager.rs

pub struct MessageWithImportance {
    pub message: LlmMessage,
    pub importance_score: f32,
    pub token_count: usize,
    pub index: usize,
    pub is_system: bool,
    pub has_tool_calls: bool,
    pub has_tool_result: bool,
    pub tool_call_id: Option<String>,
    pub linked_tool_call_ids: Vec<String>, // For tool results, which tool_call_ids they respond to
}

impl MessageWithImportance {
    fn calculate_importance(
        msg: &LlmMessage,
        index: usize,
        total_messages: usize,
        tool_call_map: &HashMap<String, usize>, // tool_call_id -> message_index
    ) -> f32 {
        let mut score = match msg.role {
            Role::System => 100.0,  // Always keep
            Role::User => 80.0,
            Role::Assistant => 40.0,
            Role::Tool => 60.0,
        };

        // Recency bonus
        let distance_from_end = total_messages - index - 1;
        score += (50.0 - (distance_from_end as f32 * 0.5)).max(0.0);

        // Tool chain bonus
        if let Some(tool_calls) = &msg.tool_calls {
            if !tool_calls.is_empty() {
                score += 30.0; // This message triggers tool calls
            }
        }
        
        if let Some(tool_call_id) = &msg.tool_call_id {
            score += 30.0; // This is a tool result
        }

        // User question bonus
        if msg.role == Role::User {
            let content_lower = msg.content.to_lowercase();
            if msg.content.ends_with('?') || 
               content_lower.contains("what") || 
               content_lower.contains("how") ||
               content_lower.contains("why") ||
               content_lower.contains("when") ||
               content_lower.contains("where") {
                score += 20.0;
            }
        }

        // Content length penalty (but not for user messages - those are important)
        if msg.role != Role::User && msg.content.len() > 1000 {
            score -= 10.0;
        }

        // Attachment bonus
        if msg.attachments.is_some() && !msg.attachments.as_ref().unwrap().is_empty() {
            score += 15.0;
        }

        // Reasoning content bonus
        if msg.reasoning_content.is_some() {
            score += 10.0;
        }

        score
    }
}
```

---

## 🔢 Part 2: Model-Specific Token Counting

### The Challenge

Each model family uses different tokenization:
- **OpenAI/DeepSeek**: `cl100k_base` (tiktoken)
- **Anthropic Claude**: SentencePiece (similar to `cl100k_base` but not identical)
- **Gemini**: Uses SentencePiece but different vocabulary
- **Ollama**: Depends on model (Llama, Mistral, etc. - each different)

### Solution: Multi-Tokenizer Strategy

```rust
// src/llm/tokenizer.rs

pub enum TokenizerType {
    /// OpenAI/DeepSeek models (cl100k_base)
    Cl100kBase,
    /// Anthropic Claude (approximate - similar to cl100k_base)
    Anthropic,
    /// Google Gemini (approximate)
    Gemini,
    /// Ollama - depends on model name
    Ollama { model_name: String },
    /// Fallback: character-based estimation
    Estimation,
}

pub struct TokenCounter {
    tokenizer_type: TokenizerType,
    cl100k_encoder: Option<tiktoken_rs::CoreBPE>, // Cached encoder
}

impl TokenCounter {
    pub fn new(profile: &LlmProfile) -> Self {
        let tokenizer_type = Self::detect_tokenizer(profile);
        let mut counter = Self {
            tokenizer_type,
            cl100k_encoder: None,
        };
        
        // Initialize encoder if needed
        if matches!(counter.tokenizer_type, TokenizerType::Cl100kBase) {
            counter.cl100k_encoder = tiktoken_rs::cl100k_base().ok();
        }
        
        counter
    }

    fn detect_tokenizer(profile: &LlmProfile) -> TokenizerType {
        match profile.backend.as_str() {
            "openai" | "deepseek" => {
                // Check model name for specific tokenizer
                let model_lower = profile.model.to_lowercase();
                if model_lower.contains("gpt-4") || 
                   model_lower.contains("gpt-3.5") ||
                   model_lower.contains("gpt-4o") ||
                   model_lower.contains("o1") {
                    TokenizerType::Cl100kBase
                } else {
                    TokenizerType::Cl100kBase // Default for OpenAI
                }
            }
            "anthropic" => TokenizerType::Anthropic,
            "gemini" => TokenizerType::Gemini,
            "ollama" => TokenizerType::Ollama { 
                model_name: profile.model.clone() 
            },
            _ => TokenizerType::Estimation,
        }
    }

    pub fn count_tokens(&self, text: &str) -> usize {
        match &self.tokenizer_type {
            TokenizerType::Cl100kBase => {
                if let Some(encoder) = &self.cl100k_encoder {
                    encoder.encode_with_special_tokens(text).len()
                } else {
                    // Fallback to estimation
                    Self::estimate_tokens(text)
                }
            }
            TokenizerType::Anthropic => {
                // Anthropic uses similar tokenization to cl100k_base
                // But not exactly the same. Use cl100k as approximation
                if let Some(encoder) = &self.cl100k_encoder {
                    // Add 5% buffer for differences
                    (encoder.encode_with_special_tokens(text).len() as f32 * 1.05) as usize
                } else {
                    Self::estimate_tokens(text)
                }
            }
            TokenizerType::Gemini => {
                // Gemini tokenization is different, use estimation with adjustment
                Self::estimate_tokens_gemini(text)
            }
            TokenizerType::Ollama { model_name } => {
                // Ollama models vary - try to detect
                Self::estimate_tokens_ollama(text, model_name)
            }
            TokenizerType::Estimation => {
                Self::estimate_tokens(text)
            }
        }
    }

    pub fn count_message_tokens(&self, msg: &LlmMessage) -> usize {
        let mut tokens = 0;
        
        // Role + formatting tokens (~4-10 tokens depending on API)
        tokens += 5;
        
        // Content
        tokens += self.count_tokens(&msg.content);
        
        // Reasoning content (if present)
        if let Some(ref reasoning) = msg.reasoning_content {
            tokens += self.count_tokens(reasoning);
        }
        
        // Tool calls (if present)
        if let Some(ref tool_calls) = msg.tool_calls {
            for tool_call in tool_calls {
                // Tool call structure overhead
                tokens += 10;
                tokens += self.count_tokens(&tool_call.name);
                tokens += self.count_tokens(&tool_call.parameters.to_string());
            }
        }
        
        // Attachments
        if let Some(ref attachments) = msg.attachments {
            for attachment in attachments {
                // Image encoding overhead + base64 content estimation
                // For text files, count content
                if let Some(ref content) = attachment.content {
                    tokens += self.count_tokens(content);
                } else {
                    // Binary/image file - estimate from size
                    // Rough estimate: ~1 token per 4 bytes of base64
                    tokens += attachment.file_size as usize / 4;
                }
                // File metadata
                tokens += self.count_tokens(&attachment.file_name);
                tokens += 5;
            }
        }
        
        tokens
    }

    // Fallback estimation: ~4 characters per token (conservative)
    fn estimate_tokens(text: &str) -> usize {
        // Conservative: English text is ~4 chars/token, but code/markdown can be different
        // Use a heuristic that accounts for whitespace
        let char_count = text.chars().count();
        let word_count = text.split_whitespace().count();
        
        // Average: take the higher estimate
        let char_based = char_count / 4;
        let word_based = word_count * 1.3 as usize; // ~1.3 tokens per word
        
        char_based.max(word_based)
    }

    fn estimate_tokens_gemini(text: &str) -> usize {
        // Gemini tends to tokenize more aggressively (smaller tokens)
        // Estimate ~3.5 chars per token
        (text.chars().count() as f32 / 3.5) as usize
    }

    fn estimate_tokens_ollama(text: &str, model_name: &str) -> usize {
        // Ollama models vary:
        // - Llama: SentencePiece, ~3-4 chars/token
        // - Mistral: Similar
        // - Code models: Can be very different
        
        let model_lower = model_name.to_lowercase();
        if model_lower.contains("code") {
            // Code models tokenize differently
            text.chars().count() / 5 // More tokens for code
        } else {
            // Default for most Ollama models
            text.chars().count() / 3
        }
    }
}
```

### Model Context Limits

We also need to track context limits per model:

```rust
impl TokenCounter {
    pub fn get_context_limit(&self, profile: &LlmProfile) -> usize {
        // First, check if context_window_size is configured in profile
        if let Some(configured_size) = profile.context_window_size {
            return configured_size;
        }
        
        // Otherwise, auto-detect based on model name
        let model_lower = profile.model.to_lowercase();
        
        match profile.backend.as_str() {
            "openai" | "deepseek" => {
                if model_lower.contains("gpt-4o") {
                    128_000 // GPT-4o
                } else if model_lower.contains("gpt-4-turbo") || model_lower.contains("gpt-4-1106") {
                    128_000 // GPT-4 Turbo
                } else if model_lower.contains("gpt-4") {
                    8_192 // GPT-4 base
                } else if model_lower.contains("o1") {
                    200_000 // o1 models
                } else if model_lower.contains("gpt-3.5-turbo") {
                    16_385 // GPT-3.5 Turbo
                } else {
                    4_096 // Default/safe
                }
            }
            "anthropic" => {
                if model_lower.contains("claude-3-5-sonnet") || model_lower.contains("claude-3-opus") {
                    200_000
                } else if model_lower.contains("claude-3") {
                    200_000
                } else {
                    100_000 // Older Claude
                }
            }
            "gemini" => {
                if model_lower.contains("1.5-pro") || model_lower.contains("1.5-flash") {
                    1_000_000 // Gemini 1.5
                } else if model_lower.contains("2.0") {
                    1_000_000 // Gemini 2.0
                } else {
                    32_768 // Gemini 1.0
                }
            }
            "ollama" => {
                // Ollama models vary - check model name
                if model_lower.contains("llama3.1") || model_lower.contains("llama3") {
                    128_000 // Llama 3.1
                } else if model_lower.contains("mistral") || model_lower.contains("mixtral") {
                    32_768 // Mistral/Mixtral
                } else {
                    4_096 // Conservative default
                }
            }
            _ => 4_096, // Safe default
        }
    }

    pub fn get_summarize_threshold(&self, profile: &LlmProfile) -> f32 {
        // Get summarize_threshold from profile, default to 0.7 (70%)
        profile.summarize_threshold.unwrap_or(0.7)
    }

    pub fn get_summarize_threshold_tokens(&self, profile: &LlmProfile) -> usize {
        // Calculate the token count at which summarization should trigger
        let context_limit = self.get_context_limit(profile);
        let threshold = self.get_summarize_threshold(profile);
        (context_limit as f32 * threshold) as usize
    }
}
```

---

## 🚀 Part 3: Implementation Steps (Correct Order)

### **Phase 1: Token Counting Foundation** ⚡ (CRITICAL - Do First)

**Why first?** Without accurate token counting, we can't make smart decisions.

1. **Add dependencies** to `Cargo.toml`:
   ```toml
   tiktoken-rs = "0.5"
   ```

2. **Create `src/llm/tokenizer.rs`**:
   - Implement `TokenCounter` struct
   - Implement model detection
   - Implement token counting methods
   - Implement context limit detection

3. **Test token counting**:
   - Unit tests for each tokenizer type
   - Verify against known token counts
   - Test edge cases (empty, very long, code, markdown)

4. **Integrate into existing code**:
   - Add token counting to message building
   - Log token counts (for debugging)
   - Add token usage tracking

**File structure:**
```
src/llm/
  ├── tokenizer.rs (NEW)
  ├── mod.rs (add pub mod tokenizer)
```

---

### **Phase 2: Importance Scoring** 📊

1. **Create `src/server/context_manager.rs`**:
   - Implement `MessageWithImportance` struct
   - Implement importance calculation
   - Implement tool chain detection

2. **Test importance scoring**:
   - Test with different message types
   - Test tool chain linking
   - Test recency decay

**File structure:**
```
src/server/
  ├── context_manager.rs (NEW)
  ├── mod.rs (add pub mod context_manager)
```

---

### **Phase 3: Context Selection Algorithm** 🎯

1. **Implement `SmartContextManager`**:
   - Build importance-scored message list
   - Sort by importance
   - Apply token budget
   - Preserve tool chains

2. **Key Algorithm:**
   ```rust
   pub fn select_context(
       &self,
       messages: Vec<LlmMessage>,
       token_counter: &TokenCounter,
       profile: &LlmProfile,
   ) -> Vec<LlmMessage> {
       // 1. Calculate importance for each message
       // 2. Calculate token counts
       // 3. Get context limit (with headroom)
       // 4. Always include system messages
       // 5. Group tool chains
       // 6. Select by importance until token budget exhausted
       // 7. Ensure tool chains stay together
   }
   ```

---

### **Phase 3.5: Summarization Trigger & Process** 📝

**WHEN Summarization Happens:**

Summarization occurs **before sending messages to the LLM**, during the context preparation phase (in `build_llm_messages` or `select_context`). Here's the flow:

1. **Token Counting Phase**:
   - Count total tokens for all conversation messages
   - Get the `summarize_threshold_tokens` = `context_window_size * summarize_threshold`

2. **Threshold Check**:
   - If `total_tokens > summarize_threshold_tokens`, summarization is triggered
   - Example: If `context_window_size = 128000` and `summarize_threshold = 0.7`, summarization triggers when usage exceeds **89,600 tokens** (70%)

3. **Summarization Process**:
   ```
   User sends message → build_llm_messages() called
     ↓
   Count tokens of all messages
     ↓
   If tokens > summarize_threshold_tokens:
     ↓
   [SUMMARIZATION STEP]
     - Select oldest messages (excluding recent N messages)
     - Send them to LLM with summarization prompt
     - Receive summary from LLM
     - Replace original messages with single summary message
     - Store summary in database (linked to conversation)
     ↓
   Continue with context selection/truncation if needed
     ↓
   Send final message list to LLM for response
   ```

4. **Which Messages Get Summarized**:
   - **Always keep**: Most recent messages (last 5-10 messages, configurable)
   - **Summarize**: Older messages (everything before the "keep recent" threshold)
   - **Tool chains**: If a tool chain spans the boundary, either keep it whole or exclude it from summarization

5. **Summarization Implementation**:
   ```rust
   pub async fn summarize_messages(
       &self,
       messages_to_summarize: Vec<LlmMessage>,
       profile: &LlmProfile,
       llm_client: &dyn LlmClient,
   ) -> Result<LlmMessage> {
       // Build summarization prompt
       let prompt = format!(
           "Summarize the following conversation history concisely. \
            Preserve key facts, decisions, and important context. \
            Keep tool call results if they contain important data.\n\n{}",
           // Serialize messages_to_summarize
       );
       
       // Use cheaper/same model for summarization
       // Could use a separate "summarization profile" if configured
       let summary_response = llm_client.send_message(
           vec![LlmMessage {
               role: Role::User,
               content: prompt,
               ..Default::default()
           }],
           vec![], // No tools for summarization
       ).await?;
       
       // Return summary as a system message (or special summary message type)
       Ok(LlmMessage {
           role: Role::System, // or Role::Assistant with special flag
           content: format!("[Previous conversation summary]: {}", summary_response),
           ..Default::default()
       })
   }
   ```

6. **Storage**:
   - Store the summary message in the database
   - Link it to the conversation
   - Mark original messages as "summarized" (optional: keep them archived)
   - On next request, use the summary instead of original messages

**Key Points:**
- ✅ Summarization happens **before** the main LLM call
- ✅ It's a **separate LLM API call** (uses tokens but reduces future context)
- ✅ The summary replaces old messages, reducing token count
- ✅ Process is **transparent** - user can see that summarization occurred
- ✅ Only triggers when threshold is exceeded (not on every message)

---

### **Database Schema Changes & Post-Summarization Flow** 🗄️

#### **Database Schema Changes**

Add the following fields to the `messages` table:

```sql
-- Add columns to messages table for summarization support
ALTER TABLE messages ADD COLUMN is_summary INTEGER NOT NULL DEFAULT 0;
ALTER TABLE messages ADD COLUMN summarized_message_ids TEXT;  -- JSON array of message IDs that were summarized
ALTER TABLE messages ADD COLUMN summarized_count INTEGER;     -- Number of messages this summary replaces
```

**Schema Explanation:**

- **`is_summary`** (INTEGER, 0 or 1):
  - `1` = This message is a summary of previous messages
  - `0` = Normal message
  - Used to identify summary messages when loading conversation

- **`summarized_message_ids`** (TEXT, JSON array):
  - Optional: Stores the IDs of messages that were summarized
  - Format: `"[1, 2, 3, 4, 5]"`
  - Useful for debugging, auditing, and potential future "expand summary" feature
  - Can be `NULL` if we don't need to track this

- **`summarized_count`** (INTEGER):
  - Number of messages this summary replaces
  - Useful for UI display ("Summarized 15 previous messages")
  - Can be `NULL` if `summarized_message_ids` is present (we can count those)

**Updated Message struct:**

```rust
// src/storage/sqlite_storage_simple.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: i64,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub created_at: i64,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub tool_status: Option<String>,
    pub tool_params_json: Option<Value>,
    pub tool_result_json: Option<Value>,
    pub reasoning_content: Option<String>,
    // NEW FIELDS FOR SUMMARIZATION:
    #[serde(default)]
    pub is_summary: bool,
    pub summarized_message_ids: Option<Vec<i64>>,  // IDs of messages that were summarized
    pub summarized_count: Option<usize>,            // Count of messages summarized
}
```

#### **What Happens During Summarization (Database Operations)**

```rust
pub async fn perform_summarization(
    storage: &SqliteStorage,
    conversation_id: &str,
    messages_to_summarize: Vec<Message>,  // Oldest messages to summarize
    summary_content: String,
) -> Result<()> {
    let transaction = storage.conn.transaction()?;
    
    // 1. Collect IDs of messages to be deleted
    let message_ids: Vec<i64> = messages_to_summarize.iter().map(|m| m.id).collect();
    let summarized_count = message_ids.len();
    
    // 2. Get the earliest timestamp from messages being summarized
    // (to maintain chronological order - summary should appear where old messages were)
    let earliest_timestamp = messages_to_summarize
        .iter()
        .map(|m| m.created_at)
        .min()
        .unwrap_or(chrono::Utc::now().timestamp());
    
    // 3. Delete the old messages from messages table
    // (CASCADE will also delete from messages_fts via trigger)
    let placeholders = message_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    transaction.execute(
        &format!("DELETE FROM messages WHERE id IN ({})", placeholders),
        rusqlite::params_from_iter(message_ids.iter()),
    )?;
    
    // 4. Insert the summary message
    // Use earliest timestamp to maintain chronological order
    let summary_message_ids_json = serde_json::to_string(&message_ids)?;
    transaction.execute(
        "INSERT INTO messages (
            conversation_id, role, content, created_at, 
            is_summary, summarized_message_ids, summarized_count
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            conversation_id,
            "system",  // or "assistant" - summary role
            summary_content,
            earliest_timestamp,
            1,  // is_summary = true
            summary_message_ids_json,
            summarized_count
        ],
    )?;
    
    transaction.commit()?;
    Ok(())
}
```

#### **What Happens on the NEXT Message (After Summarization)**

When the next user message arrives after summarization:

```
1. Load conversation from DB:
   └─> SELECT * FROM messages WHERE conversation_id = ? ORDER BY created_at ASC
       Returns: [summary_message, message_N, message_N+1, ..., message_N+M]
       
2. Convert DB messages to LlmMessage format:
   └─> Summary message becomes LlmMessage with role=System and special flag
   └─> All other messages converted normally
   
3. Count tokens:
   └─> summary_message tokens + all subsequent messages tokens + new_user_message tokens
   
4. Check threshold:
   └─> If still > summarize_threshold_tokens:
       → Summarize again (but now we'd summarize: summary + some newer messages)
   └─> If below threshold:
       → Continue normally
       
5. Apply context selection/truncation if needed:
   └─> Even after summarization, if we're still over context_limit, 
       apply smart truncation to fit within limit
       
6. Send to LLM:
   └─> Final message list: [system_prompt, summary_message, ...recent_messages, new_user_message]
```

**Key Points:**
- ✅ Summary message is loaded like any other message
- ✅ Chronological order is preserved (summary has `created_at` = earliest original message)
- ✅ Token counting includes the summary message
- ✅ Multiple summarizations can occur (summary can be re-summarized later)
- ✅ Summary appears in conversation UI with special indicator

#### **What Happens When Loading Conversation from DB**

```rust
// In load_conversation() - already exists, no changes needed!
pub fn load_conversation(&self, conversation_id: &str) -> SqliteResult<Vec<Message>> {
    let mut stmt = self.conn.prepare(
        "SELECT id, conversation_id, role, content, embedding, created_at, 
                tool_calls, tool_call_id, tool_name, tool_status, 
                tool_params_json, tool_result_json, reasoning_content,
                is_summary, summarized_message_ids, summarized_count
         FROM messages 
         WHERE conversation_id = ?1 
         ORDER BY created_at ASC"  // Chronological order preserved
    )?;
    
    // ... existing deserialization code ...
    // Add deserialization for new fields:
    // is_summary: row.get(13)?
    // summarized_message_ids: parse JSON from row.get(14)?
    // summarized_count: row.get(15)?
    
    Ok(messages)  // Returns messages in order: [summary, msg1, msg2, ...]
}
```

**Loading Flow:**
1. All messages are loaded in chronological order (`ORDER BY created_at ASC`)
2. Summary messages appear where the original messages were (using earliest timestamp)
3. Summary messages are identified by `is_summary = 1`
4. UI can display summaries differently (collapsible, special icon, etc.)
5. When building LLM context, summary messages are treated as system messages

#### **UI Display Requirements for Summary Messages** 🎨

Summary messages must be displayed with special styling in the conversation UI:

**Display Requirements:**
- ✅ **100% width bubble** - Unlike regular messages (70% width), summary messages span the full width
- ✅ **Collapsed by default** - Summary content is hidden initially, showing only a header/toggle
- ✅ **Expandable on click** - User can click to expand and view the summary content
- ✅ **Visual indicator** - Should have a distinct visual style (icon, color, or styling) to indicate it's a summary

**UI Implementation:**

```rust
// In src/ui/pages/chat/message_list.rs

// When rendering messages, check if message is a summary:
let is_summary = msg.is_summary; // From database Message struct

let message_widget = if is_summary {
    // Summary message: 100% width, collapsed by default
    let expanded = app.expanded_summaries.contains(&i);
    let summary_count = msg.summarized_count.unwrap_or(0);
    
    let toggle_button = cosmic::widget::button::text(format!(
        "{} 📄 Summary ({} messages)",
        if expanded { "▼" } else { "▶" },
        summary_count
    ))
    .on_press(Message::ToggleSummary(i))
    .class(cosmic::style::Button::Text)
    .width(Length::Fill);
    
    let mut summary_column = cosmic::widget::column()
        .push(toggle_button);
    
    if expanded {
        summary_column = summary_column.push(
            cosmic::widget::container(
                cosmic::widget::text(&msg.content)
                    .size(14)
                    .width(Length::Fill)
            )
            .padding(Padding::from([12, 16]))
            .width(Length::Fill)
        );
    }
    
    cosmic::widget::container(summary_column)
        .padding(Padding::from([12, 16]))
        .class(cosmic::style::Container::Card)
        .width(Length::Fill)  // 100% width
        .into()
} else {
    // Regular message (existing code - 70% width)
    // ... existing message rendering code ...
};
```

**UI State Management:**

Add to `CosmicLlmApp` struct:
```rust
// Track which summaries are expanded (by message index)
pub expanded_summaries: HashSet<usize>,
```

Add message variant:
```rust
// In Message enum
ToggleSummary(usize),  // Toggle summary expansion at index
```

**Visual Design:**
- Use a distinct container style (e.g., lighter background, border)
- Include an icon (📄 or 🔽) to indicate summary
- Show count of summarized messages in the header: "📄 Summary (15 messages)"
- When collapsed, show only the toggle button
- When expanded, show the full summary content in a readable format

#### **Example: Conversation Evolution**

```
Initial State (before summarization):
┌─────────────────────────────────────┐
│ Message 1  (oldest)                 │
│ Message 2                           │
│ ...                                 │
│ Message 15                          │
│ Message 16  (recent - kept)         │
│ Message 17  (recent - kept)         │
│ Message 18  (recent - kept)         │
└─────────────────────────────────────┘
Total: 18 messages, 95,000 tokens (> 89,600 threshold)

After Summarization:
┌─────────────────────────────────────┐
│ [SUMMARY] Messages 1-15 (15 msgs)   │  ← New summary message
│ Message 16  (recent - kept)         │
│ Message 17  (recent - kept)         │
│ Message 18  (recent - kept)         │
└─────────────────────────────────────┘
Total: 4 messages, 12,000 tokens

Next User Message Arrives:
┌─────────────────────────────────────┐
│ [SUMMARY] Messages 1-15 (15 msgs)   │
│ Message 16                          │
│ Message 17                          │
│ Message 18                          │
│ Message 19  (NEW USER MESSAGE)      │  ← New message
└─────────────────────────────────────┘
Total: 5 messages, 14,500 tokens (still below threshold ✓)

Later, if conversation grows:
┌─────────────────────────────────────┐
│ [SUMMARY 1] Messages 1-15 (15 msgs) │
│ Message 16                          │
│ ...                                 │
│ Message 30                          │
│ (New messages accumulated...)       │
└─────────────────────────────────────┘
Total: 16 messages, 92,000 tokens (> threshold again)

After Second Summarization:
┌─────────────────────────────────────┐
│ [SUMMARY 2] Summary1+Msgs16-28      │  ← New summary (includes old summary!)
│ Message 29  (recent - kept)         │
│ Message 30  (recent - kept)         │
└─────────────────────────────────────┘
Total: 3 messages, 8,000 tokens
```

**Important:** Summary messages can be re-summarized! The summary itself becomes part of the conversation history and can be included in future summaries.

---

### **Phase 4: Integration** 🔌

1. **Modify `build_llm_messages` in `handlers.rs`**:
   - Add token counting
   - Check if summarization threshold is exceeded
   - Trigger summarization if needed (before context selection)
   - Apply context selection/truncation after summarization
   - Log when summarization or truncation occurs

2. **Implement summarization**:
   - Add `summarize_messages()` method to `SmartContextManager`
   - Integrate summarization call into message building flow
   - Store summaries in database (link to conversation)
   - Handle summarization errors gracefully (fallback to truncation)

3. **Add configuration**:
   - Add `context_window_size` and `summarize_threshold` fields to `LlmProfile` struct
   - Allow users to configure per-profile context management behavior

4. **Add user notifications**:
   - Send event when summarization occurs
   - Send event when context is truncated
   - Show token usage in UI
   - Show when messages were summarized

5. **UI Implementation for Summary Messages**:
   - Update `message_list.rs` to detect and render summary messages
   - Implement 100% width collapsed summary bubble
   - Add `expanded_summaries` state to track which summaries are expanded
   - Add `ToggleSummary` message variant for expand/collapse interaction
   - Style summary messages with distinct visual indicator (icon, styling)
   - Display summary count in collapsed header ("📄 Summary (15 messages)")

---

## 📋 Step-by-Step Implementation Checklist

### ✅ Step 1: Token Counting (Week 1)

- [ ] Add `tiktoken-rs` dependency
- [ ] Create `src/llm/tokenizer.rs`
- [ ] Implement `TokenCounter::new()` with model detection
- [ ] Implement `count_tokens()` for each tokenizer type
- [ ] Implement `count_message_tokens()` (handles all message parts)
- [ ] Implement `get_context_limit()` for all backends
- [ ] Implement `get_safe_context_limit()` (80% headroom)
- [ ] Write unit tests
- [ ] Test with real API responses (compare counted vs actual)

### ✅ Step 2: Message Importance (Week 1-2)

- [ ] Create `src/server/context_manager.rs`
- [ ] Implement `MessageWithImportance` struct
- [ ] Implement `calculate_importance()` method
- [ ] Implement tool chain detection/linking
- [ ] Write tests for importance scoring

### ✅ Step 3: Context Selection (Week 2)

- [ ] Implement `SmartContextManager` struct
- [ ] Implement `select_context()` method
- [ ] Implement tool chain preservation logic
- [ ] Test with various conversation lengths
- [ ] Test tool chain preservation

### ✅ Step 3.5: Summarization (Week 2-3)

- [ ] Implement `summarize_messages()` method
- [ ] Implement summarization threshold checking
- [ ] Integrate summarization into context building flow
- [ ] Add database storage for summaries
- [ ] Update database schema (add `is_summary`, `summarized_message_ids`, `summarized_count`)
- [ ] Update `load_conversation()` to include new fields
- [ ] Test summarization trigger (when threshold exceeded)
- [ ] Test summarization output quality
- [ ] Handle summarization errors gracefully
- [ ] **UI: Add summary message detection in message rendering**
- [ ] **UI: Implement 100% width collapsed summary bubble**
- [ ] **UI: Add `expanded_summaries` state tracking**
- [ ] **UI: Add `ToggleSummary` message handler**
- [ ] **UI: Style summary messages with distinct visual indicator**

### ✅ Step 4: Integration (Week 2-3)

- [ ] Modify `build_llm_messages()` in `handlers.rs`
- [ ] Add summarization trigger check
- [ ] Integrate summarization call
- [ ] Add context management configuration to `LlmProfile`
- [ ] Add summarization logging
- [ ] Add truncation logging
- [ ] Add user notifications (summarization + truncation events)
- [ ] Test end-to-end with summarization

---

## 🔧 Configuration

Add per-profile context management settings to each profile in `config.toml`:

```toml
[profiles.openai]
backend = "openai"
api_key = "sk-..."
model = "gpt-4o"
endpoint = "https://api.openai.com/v1"
temperature = 0.7
max_tokens = 4000

# Context management settings (per-profile)
# Maximum context window size in tokens
# If not set, will be auto-detected based on model
context_window_size = 128000

# Summarization threshold (0.0 - 1.0)
# When context usage reaches this percentage of context_window_size,
# summarization of older messages will be triggered
# Default: 0.7 (70% of context window)
summarize_threshold = 0.7

[profiles.anthropic]
backend = "anthropic"
api_key = "sk-ant-..."
model = "claude-3-5-sonnet-20241022"
endpoint = "https://api.anthropic.com"
temperature = 0.7
max_tokens = 4000

# Context management settings
context_window_size = 200000
summarize_threshold = 0.7
```

**Configuration Fields:**

- **`context_window_size`** (optional, integer): 
  - Maximum context window size in tokens for this profile
  - If not specified, will be auto-detected based on the model name
  - Should match the actual model's context limit

- **`summarize_threshold`** (optional, float, default: 0.7):
  - Threshold (0.0 - 1.0) indicating when to trigger summarization
  - When context usage reaches `context_window_size * summarize_threshold`, older messages will be summarized
  - Example: `summarize_threshold = 0.7` means summarization triggers at 70% of context window

---

## 🎨 Code Structure Preview

```
src/
├── llm/
│   ├── tokenizer.rs          # NEW: Token counting
│   └── mod.rs
├── server/
│   ├── context_manager.rs    # NEW: Context selection logic
│   ├── handlers.rs           # MODIFY: Use context manager
│   └── mod.rs
└── config/
    └── mod.rs                # MODIFY: Add context_window_size and summarize_threshold to LlmProfile
```

---

## ⚠️ Important Considerations

1. **Token Counting Accuracy**:
   - OpenAI: Use `tiktoken-rs` for accuracy
   - Anthropic: Approximate with cl100k_base + 5% buffer
   - Gemini: Use estimation (API doesn't expose tokenizer)
   - Ollama: Model-dependent, use estimation

2. **Tool Chain Preservation**:
   - MUST keep tool_use and corresponding tool_result together
   - If tool_use is important, tool_result is important
   - Break tool chains only as last resort

3. **System Messages**:
   - Always keep (score = 100)
   - Count against token budget
   - Include in initial token calculation

4. **Performance**:
   - Cache tokenizers (don't recreate for each call)
   - Cache importance scores during selection
   - Consider async if tokenization is slow

5. **User Experience**:
   - Log when truncation occurs
   - Show how many messages were dropped
   - Show token usage (for transparency)

---

## 🧪 Testing Strategy

1. **Unit Tests**:
   - Token counting accuracy
   - Importance scoring correctness
   - Context selection logic

2. **Integration Tests**:
   - End-to-end context management
   - Tool chain preservation
   - Error handling (context too large even after truncation)

3. **Manual Testing**:
   - Long conversations (>10k tokens)
   - Conversations with many tool calls
   - Conversations with attachments
   - Different model backends

---

## 📚 Next Steps

After Phase 1-4 complete, consider:

1. **Summarization Enhancements** (Future improvements):
   - Use cheaper model specifically for summarization
   - Incremental summarization (summarize in chunks)
   - Better summarization prompts for preserving tool call context
   - Allow users to configure which messages to summarize

2. **Embedding-based Selection** (Advanced):
   - Use embeddings to find semantically relevant messages
   - Keep messages related to current user query

3. **Adaptive Thresholds**:
   - Adjust importance thresholds based on conversation type
   - Learn from user feedback



