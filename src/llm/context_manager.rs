//! Context management for handling context overflow
//! 
//! This module provides smart context selection that:
//! - Scores messages by importance
//! - Preserves tool call chains
//! - Selects messages to fit within token budget
//! - Handles summarization triggers

use crate::config::ModelPreset;
use crate::llm::{Message, Role, LlmClient, LlmError};
use crate::llm::tokenizer::TokenCounter;
use anyhow::Result;
use std::collections::HashMap;

/// Message with calculated importance score and metadata
#[derive(Debug, Clone)]
pub struct MessageWithImportance {
    pub message: Message,
    pub importance_score: f32,
    pub token_count: usize,
    pub index: usize,
    pub is_system: bool,
    pub has_tool_calls: bool,
    pub has_tool_result: bool,
    pub tool_call_id: Option<String>,
    #[allow(dead_code)]
    pub linked_tool_call_ids: Vec<String>, // For tool results, which tool_call_ids they respond to
}

impl MessageWithImportance {
    /// Calculate importance score for a message
    fn calculate_importance(
        msg: &Message,
        index: usize,
        total_messages: usize,
        _tool_call_map: &HashMap<String, usize>, // tool_call_id -> message_index
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
        
        if msg.tool_call_id.is_some() {
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
        if let Some(attachments) = msg.attachments.as_ref() {
            if !attachments.is_empty() {
            score += 15.0;
            }
        }

        // Reasoning content bonus
        if msg.reasoning_content.is_some() {
            score += 10.0;
        }

        score
    }

    /// Create a MessageWithImportance from a Message
    pub fn new(
        msg: Message,
        index: usize,
        total_messages: usize,
        token_counter: &TokenCounter,
        tool_call_map: &HashMap<String, usize>,
    ) -> Self {
        let is_system = msg.role == Role::System;
        let has_tool_calls = msg.tool_calls.as_ref().map_or(false, |calls| !calls.is_empty());
        let has_tool_result = msg.tool_call_id.is_some();
        let _tool_call_id = msg.tool_call_id.clone();
        
        // Build linked_tool_call_ids for tool results
        let _linked_tool_call_ids = if has_tool_result {
            _tool_call_id.clone().map(|id| vec![id]).unwrap_or_default()
        } else {
            Vec::new()
        };

        let importance_score = Self::calculate_importance(&msg, index, total_messages, tool_call_map);
        let token_count = token_counter.count_message_tokens(&msg);

        Self {
            message: msg,
            importance_score,
            token_count,
            index,
            is_system,
            has_tool_calls,
            has_tool_result,
            tool_call_id: _tool_call_id,
            linked_tool_call_ids: _linked_tool_call_ids,
        }
    }
}

/// Smart context manager that selects messages based on importance
pub struct SmartContextManager;

impl SmartContextManager {
    /// Select messages to fit within token budget, preserving important messages and tool chains
    pub fn select_context(
        messages: Vec<Message>,
        token_counter: &TokenCounter,
        preset: &ModelPreset,
    ) -> Vec<Message> {
        if messages.is_empty() {
            return messages;
        }

        // Build tool call map: tool_call_id -> message_index
        let mut tool_call_map: HashMap<String, usize> = HashMap::new();
        for (idx, msg) in messages.iter().enumerate() {
            if let Some(tool_calls) = &msg.tool_calls {
                for tool_call in tool_calls {
                    tool_call_map.insert(tool_call.id.clone(), idx);
                }
            }
        }

        // Calculate importance and token counts for all messages
        let total_messages = messages.len();
        let scored_messages: Vec<MessageWithImportance> = messages
            .into_iter()
            .enumerate()
            .map(|(idx, msg)| {
                MessageWithImportance::new(msg, idx, total_messages, token_counter, &tool_call_map)
            })
            .collect();

        // Get context limit (with headroom)
        let context_limit = token_counter.get_safe_context_limit(preset);

        // Separate system messages (always keep) and regular messages
        let mut system_messages: Vec<MessageWithImportance> = Vec::new();
        let mut regular_messages: Vec<MessageWithImportance> = Vec::new();

        for scored in scored_messages {
            if scored.is_system {
                system_messages.push(scored);
            } else {
                regular_messages.push(scored);
            }
        }

        // Count system message tokens
        let system_tokens: usize = system_messages.iter().map(|m| m.token_count).sum();
        let available_tokens = context_limit.saturating_sub(system_tokens);

        if available_tokens == 0 {
            // Only system messages fit - return them
            return system_messages.into_iter().map(|m| m.message).collect();
        }

        // Sort regular messages by importance (highest first)
        regular_messages.sort_by(|a, b| {
            b.importance_score
                .partial_cmp(&a.importance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Select messages to fit within token budget
        let mut selected: Vec<MessageWithImportance> = Vec::new();
        let mut selected_indices: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let mut used_tokens = 0;

        // First pass: Select by importance
        for scored in &regular_messages {
            if used_tokens + scored.token_count <= available_tokens {
                selected.push(scored.clone());
                selected_indices.insert(scored.index);
                used_tokens += scored.token_count;
            }
        }

        // Second pass: Ensure tool chains are preserved
        // If we selected a tool call, ensure its result is included
        // If we selected a tool result, ensure its call is included
        let mut additional_messages: Vec<MessageWithImportance> = Vec::new();
        let mut additional_tokens = 0;

        for scored in &regular_messages {
            if selected_indices.contains(&scored.index) {
                continue; // Already selected
            }

            // Check if this message is linked to a selected message
            let mut should_include = false;

            if scored.has_tool_calls {
                // This is a tool call - check if any of its results are selected
                if let Some(tool_calls) = &scored.message.tool_calls {
                    for tool_call in tool_calls {
                        // Find the tool result message for this tool_call_id
                        for selected_msg in &selected {
                            if selected_msg.tool_call_id.as_ref() == Some(&tool_call.id) {
                                should_include = true;
                                break;
                            }
                        }
                        if should_include {
                            break;
                        }
                    }
                }
            }

            if scored.has_tool_result {
                // This is a tool result - check if its call is selected
                if let Some(tool_call_id) = &scored.tool_call_id {
                    for selected_msg in &selected {
                        if let Some(selected_tool_calls) = &selected_msg.message.tool_calls {
                            for tool_call in selected_tool_calls {
                                if &tool_call.id == tool_call_id {
                                    should_include = true;
                                    break;
                                }
                            }
                        }
                        if should_include {
                            break;
                        }
                    }
                }
            }

            if should_include && used_tokens + additional_tokens + scored.token_count <= available_tokens {
                additional_messages.push(scored.clone());
                additional_tokens += scored.token_count;
            }
        }

        // Combine selected messages
        selected.extend(additional_messages);

        // Third pass: Remove orphaned tool results (tool result whose assistant wasn't added due to budget).
        // Otherwise we'd send a tool result with tool_call_id that has no preceding assistant tool_call -> API "tool_call_id not found".
        let selected_tool_call_ids: std::collections::HashSet<String> = selected
            .iter()
            .flat_map(|m| m.message.tool_calls.as_deref().unwrap_or(&[]).iter())
            .map(|tc| tc.id.clone())
            .collect();
        selected.retain(|m| {
            if let Some(ref tid) = m.tool_call_id {
                if !selected_tool_call_ids.contains(tid) {
                    return false; // drop orphaned tool result
                }
            }
            true
        });

        // Sort selected messages back to original order
        selected.sort_by_key(|m| m.index);

        // Combine with system messages (system messages first)
        let mut result: Vec<Message> = system_messages.into_iter().map(|m| m.message).collect();
        result.extend(selected.into_iter().map(|m| m.message));

        result
    }

    /// Summarize a list of messages using the LLM
    /// Returns a summary message that can replace the original messages
    pub async fn summarize_messages(
        messages_to_summarize: Vec<Message>,
        _preset: &ModelPreset,
        llm_client: &dyn LlmClient,
    ) -> Result<Message, LlmError> {
        if messages_to_summarize.is_empty() {
            return Err(LlmError::Config("Cannot summarize empty message list".into()));
        }

        // Max length for tool result content in the summarization input (save tokens)
        const TOOL_CONTENT_MAX_CHARS: usize = 800;

        // Build a formatted representation of messages for summarization (include tool calls/results)
        let mut conversation_text = String::new();
        for msg in &messages_to_summarize {
            let role_str = match msg.role {
                Role::User => "User",
                Role::Assistant => "Assistant",
                Role::System => "System",
                Role::Tool => "Tool",
            };
            let content = if msg.role == Role::Tool && msg.content.len() > TOOL_CONTENT_MAX_CHARS {
                format!("{}... [truncated]", &msg.content[..TOOL_CONTENT_MAX_CHARS])
            } else {
                msg.content.clone()
            };
            conversation_text.push_str(&format!("[{}]: {}\n\n", role_str, content));

            // Include tool calls if present (assistant requested tools)
            if let Some(tool_calls) = &msg.tool_calls {
                for tool_call in tool_calls {
                    conversation_text.push_str(&format!(
                        "  [Tool Call: {}] {}\n",
                        tool_call.name,
                        serde_json::to_string(&tool_call.parameters).unwrap_or_default()
                    ));
                }
            }
        }

        // Build summarization prompt: include tool usage briefly, save tokens
        let prompt = format!(
            "Summarize the following conversation history in a compact way to save tokens. \
            Include: key facts, decisions, and what matters for future turns. \
            For tool calls and results: mention which tools were used and the outcome in one short line each (e.g. \"Used search: found X\"; \"Read file Y: key point Z\"). \
            Do not copy long tool output verbatim; compress to the essential result. \
            Goal: preserve continuity and context in as few tokens as possible.\n\n\
            Conversation:\n{}",
            conversation_text
        );

        // Use the same model for summarization (could be optimized to use cheaper model)
        let summary_response = llm_client
            .send_message_with_tools(
                vec![Message::new(Role::User, prompt)],
                vec![], // No tools for summarization
                Some(0.3), // Lower temperature for more focused summaries
                Some(2000), // Limit summary length
            )
            .await?;

        // Return summary as a system message
        Ok(Message {
            role: Role::System,
            content: format!("[Previous conversation summary]: {}", summary_response.content),
            timestamp: None,
            is_prompt: false,
            tool_call_id: None,
            tool_calls: None,
            attachments: None,
            reasoning_content: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ModelPreset;
    use crate::llm::Message;

    #[test]
    fn test_message_importance_scoring() {
        let preset = ModelPreset::default();
        let token_counter = TokenCounter::new(&preset);
        let tool_call_map = HashMap::new();

        // System message should have high score (base 100 + recency bonus when index 0 of 10)
        let system_msg = Message::new(Role::System, "System prompt".to_string());
        let score = MessageWithImportance::calculate_importance(&system_msg, 0, 10, &tool_call_map);
        assert!(score >= 100.0);

        // User message should have good score
        let user_msg = Message::new(Role::User, "Hello".to_string());
        let score = MessageWithImportance::calculate_importance(&user_msg, 9, 10, &tool_call_map);
        assert!(score > 80.0); // Base 80 + recency bonus
    }
}

