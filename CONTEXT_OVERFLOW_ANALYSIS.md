# Context Overflow Management: 5ire vs LunaAI

## 🔍 Current State Analysis

### LunaAI (Current Implementation)

**Status: ❌ No Context Overflow Management**

Looking at `src/server/handlers.rs`:

```rust
fn conversation_to_llm(conversation: StoredConversation) -> Vec<LlmMessage> {
    conversation
        .messages
        .into_iter()
        .filter_map(|msg| {
            // ... converts all messages without truncation
        })
        .collect()
}
```

**Issues:**
1. **No token counting** - All messages are sent regardless of length
2. **No truncation** - Conversations can grow indefinitely
3. **No summarization** - Old messages are never condensed
4. **No window management** - No sliding window or smart selection
5. **Relies on API errors** - Will fail when context limit is exceeded

**What happens when overflow occurs:**
- API returns error (e.g., "context_length_exceeded")
- User sees error message
- Conversation becomes unusable
- No graceful degradation

### 5ire (Likely Implementation)

Based on industry best practices and the web search results, 5ire likely implements:

1. **Token Counting & Monitoring**
   - Tracks token usage per message
   - Monitors total context size
   - Warns before approaching limits

2. **Smart Truncation Strategies**
   - **Sliding Window**: Keep most recent N messages
   - **Summarization**: Condense old messages into summaries
   - **Priority-based**: Keep important messages (user questions, key responses)
   - **Tool-aware**: Preserve tool call/result pairs together

3. **Proactive Management**
   - Automatically truncate before sending
   - Preserve system prompts
   - Maintain conversation coherence

4. **User Control**
   - Option to manually summarize conversation
   - Clear indication when truncation occurs
   - Ability to see full history even if truncated

---

## 🎯 Recommended Implementation for LunaAI

### Strategy 1: Sliding Window (Simplest)

**Implementation:**

```rust
// src/server/handlers.rs

use crate::llm::tokenizer::estimate_tokens; // Need to implement

fn conversation_to_llm_with_window(
    conversation: StoredConversation,
    max_tokens: usize,
    profile: &LlmProfile,
) -> Vec<LlmMessage> {
    let mut messages: Vec<LlmMessage> = conversation
        .messages
        .into_iter()
        .filter_map(|msg| {
            // ... existing conversion logic
        })
        .collect();
    
    // Always keep system prompts
    let system_messages: Vec<LlmMessage> = messages
        .iter()
        .filter(|m| matches!(m.role, Role::System))
        .cloned()
        .collect();
    
    let mut non_system: Vec<LlmMessage> = messages
        .into_iter()
        .filter(|m| !matches!(m.role, Role::System))
        .collect();
    
    // Calculate tokens
    let mut total_tokens = system_messages.iter()
        .map(|m| estimate_tokens(&m.content))
        .sum::<usize>();
    
    // Keep most recent messages that fit
    let mut selected = Vec::new();
    for msg in non_system.into_iter().rev() {
        let msg_tokens = estimate_tokens(&msg.content);
        if total_tokens + msg_tokens <= max_tokens {
            selected.push(msg);
            total_tokens += msg_tokens;
        } else {
            break;
        }
    }
    
    selected.reverse(); // Restore chronological order
    
    // Combine: system messages + selected messages
    let mut result = system_messages;
    result.append(&mut selected);
    result
}
```

**Pros:**
- Simple to implement
- Fast
- Preserves recent context

**Cons:**
- Loses older context completely
- May break tool call chains
- No semantic understanding

---

### Strategy 2: Summarization (Better)

**Implementation:**

```rust
// src/storage/context_manager.rs

pub struct ContextManager {
    storage: Arc<Mutex<Storage>>,
    llm_client: Arc<dyn LlmClient>,
    max_context_tokens: usize,
    summary_threshold: usize, // Start summarizing at 80% of max
}

impl ContextManager {
    pub async fn prepare_messages(
        &self,
        conversation_id: Uuid,
        profile: &LlmProfile,
    ) -> Result<Vec<LlmMessage>> {
        let storage = self.storage.lock().await;
        let conversation = storage.get_conversation(&conversation_id)?;
        
        let all_messages = conversation_to_llm(conversation);
        let total_tokens = estimate_total_tokens(&all_messages);
        
        if total_tokens <= self.max_context_tokens {
            return Ok(all_messages);
        }
        
        // Need to summarize
        if total_tokens > self.summary_threshold {
            self.summarize_old_messages(conversation_id, profile).await?;
        }
        
        // Rebuild with summaries
        let conversation = storage.get_conversation(&conversation_id)?;
        let messages = conversation_to_llm(conversation);
        
        // Apply sliding window as fallback
        Ok(self.apply_sliding_window(messages, self.max_context_tokens))
    }
    
    async fn summarize_old_messages(
        &self,
        conversation_id: Uuid,
        profile: &LlmProfile,
    ) -> Result<()> {
        let storage = self.storage.lock().await;
        let conversation = storage.get_conversation(&conversation_id)?;
        
        // Get messages to summarize (everything except last N)
        let keep_recent = 10; // Keep last 10 messages
        let to_summarize: Vec<_> = conversation
            .messages
            .iter()
            .take(conversation.messages.len().saturating_sub(keep_recent))
            .collect();
        
        if to_summarize.is_empty() {
            return Ok(());
        }
        
        // Build summary prompt
        let conversation_text = to_summarize
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");
        
        let summary_prompt = format!(
            "Summarize the following conversation history, preserving key information, \
             decisions, and context that would be important for continuing the conversation:\n\n{}",
            conversation_text
        );
        
        // Call LLM for summary (use cheaper/faster model if available)
        let summary = self.llm_client
            .send_message(
                vec![LlmMessage::new(Role::User, summary_prompt)],
                vec![],
                None,
                None,
            )
            .await?;
        
        // Replace old messages with summary
        let storage = self.storage.lock().await;
        // Implementation: Delete old messages, insert summary message
        // This requires storage API extension
        
        Ok(())
    }
}
```

**Pros:**
- Preserves important context
- Maintains conversation coherence
- Better user experience

**Cons:**
- Requires LLM call (cost/latency)
- More complex implementation
- Need to handle summary storage

---

### Strategy 3: Hybrid Approach (Best)

**Combines:**
1. **Token counting** - Always track usage
2. **Smart windowing** - Keep recent messages + important ones
3. **Summarization** - When window isn't enough
4. **Tool-aware** - Preserve tool call chains

**Implementation:**

```rust
// src/server/context_manager.rs

pub struct SmartContextManager {
    max_context_tokens: usize,
    keep_recent_messages: usize, // Always keep last N
    preserve_tool_chains: bool,
}

impl SmartContextManager {
    pub fn prepare_context(
        &self,
        messages: Vec<LlmMessage>,
    ) -> Vec<LlmMessage> {
        let total_tokens = estimate_total_tokens(&messages);
        
        if total_tokens <= self.max_context_tokens {
            return messages;
        }
        
        // Strategy:
        // 1. Always keep system messages
        // 2. Always keep recent N messages
        // 3. Preserve tool call chains (tool_use + tool_result pairs)
        // 4. Summarize or truncate the rest
        
        let (system_msgs, other_msgs) = self.split_system_messages(messages);
        let (recent_msgs, old_msgs) = self.split_recent(other_msgs);
        let (tool_chains, regular_msgs) = self.extract_tool_chains(old_msgs);
        
        // Calculate what fits
        let mut result = system_msgs;
        let mut available_tokens = self.max_context_tokens 
            - estimate_total_tokens(&result);
        
        // Add tool chains (they're important)
        for chain in tool_chains {
            let chain_tokens = estimate_total_tokens(&chain);
            if chain_tokens <= available_tokens {
                result.extend(chain);
                available_tokens -= chain_tokens;
            }
        }
        
        // Add recent messages
        for msg in recent_msgs.into_iter().rev() {
            let msg_tokens = estimate_tokens(&msg.content);
            if msg_tokens <= available_tokens {
                result.push(msg);
                available_tokens -= msg_tokens;
            } else {
                break;
            }
        }
        
        // If still need space, summarize regular_msgs
        if available_tokens < self.max_context_tokens * 20 / 100 {
            // Add summary placeholder
            result.insert(
                system_msgs.len(),
                LlmMessage::new(
                    Role::System,
                    format!(
                        "[Previous conversation summarized: {} messages omitted]",
                        regular_msgs.len()
                    )
                )
            );
        }
        
        result
    }
    
    fn extract_tool_chains(
        &self,
        messages: Vec<LlmMessage>,
    ) -> (Vec<Vec<LlmMessage>>, Vec<LlmMessage>) {
        // Group tool_use and corresponding tool_result messages
        // This is complex - need to track tool_call_id
        // Simplified version:
        (vec![], messages)
    }
}
```

---

## 📊 Comparison Table

| Feature | LunaAI (Current) | 5ire (Likely) | Recommended for LunaAI |
|---------|------------------|---------------|------------------------|
| **Token Counting** | ❌ None | ✅ Yes | ✅ **Implement** |
| **Truncation** | ❌ None | ✅ Sliding window | ✅ **Implement** |
| **Summarization** | ❌ None | ✅ Yes | ⚠️ **Future** |
| **Tool Chain Preservation** | ❌ None | ✅ Yes | ✅ **Implement** |
| **User Notification** | ❌ None | ✅ Yes | ✅ **Implement** |
| **Proactive Management** | ❌ None | ✅ Yes | ✅ **Implement** |
| **Graceful Degradation** | ❌ Fails | ✅ Works | ✅ **Implement** |

---

## 🚀 Implementation Priority

### Phase 1: Basic Protection (Week 1)
1. **Add token estimation**
   - Simple character-based or use `tiktoken` crate
   - Track tokens per message
   
2. **Implement sliding window**
   - Keep last N messages that fit
   - Always preserve system prompts
   
3. **Add error handling**
   - Catch context_length_exceeded errors
   - Retry with truncated context

### Phase 2: Smart Management (Week 2-3)
4. **Tool chain awareness**
   - Preserve tool_use + tool_result pairs
   - Don't break tool call sequences
   
5. **User notification**
   - Show when truncation occurs
   - Display token usage

### Phase 3: Advanced (Future)
6. **Summarization**
   - Background summarization of old messages
   - Store summaries in database
   
7. **Intelligent selection**
   - Keep important messages (user questions, key info)
   - Use embeddings to find relevant context

---

## 🔧 Quick Implementation: Token Estimation

```rust
// src/llm/tokenizer.rs

pub fn estimate_tokens(text: &str) -> usize {
    // Simple estimation: ~4 characters per token
    // For better accuracy, use tiktoken-rs crate
    text.chars().count() / 4
}

// Or use tiktoken-rs:
use tiktoken_rs::cl100k_base;

pub fn estimate_tokens_accurate(text: &str) -> usize {
    let bpe = cl100k_base().unwrap();
    bpe.encode_with_special_tokens(text).len()
}
```

Add to `Cargo.toml`:
```toml
tiktoken-rs = "0.5"  # For accurate token counting
```

---

## 📝 Configuration

Add to `config.toml`:

```toml
[context_management]
# Maximum context tokens (leave room for response)
max_context_tokens = 3000  # For 4k context model, leave 1k for response

# Always keep last N messages
keep_recent_messages = 10

# Preserve tool call chains
preserve_tool_chains = true

# Enable summarization (future)
enable_summarization = false
```

---

## ⚠️ Important Considerations

1. **Model-specific limits:**
   - GPT-3.5: 4,096 tokens
   - GPT-4: 8,192 tokens  
   - Claude 3: 200,000 tokens
   - Gemini 1.5: 1,000,000+ tokens
   
   Need to configure per profile!

2. **Tool calls:**
   - Tool definitions add tokens
   - Tool results can be large
   - Need to account for these

3. **Attachments:**
   - Images add significant tokens
   - Documents converted to text add many tokens
   - Need special handling

4. **System prompts:**
   - Can be long
   - Should always be preserved
   - Count against limit

---

## 🎯 Conclusion

**5ire likely handles context overflow well** with:
- Proactive token management
- Smart truncation/summarization
- Tool-aware preservation
- User-friendly notifications

**LunaAI currently has no protection** and will fail when limits are exceeded.

**Recommended action:** Implement Phase 1 (basic sliding window + token counting) immediately, as this is a critical feature for production use.








