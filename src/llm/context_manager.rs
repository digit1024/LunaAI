use super::{Message, Role, LlmClient, LlmError};
use crate::llm::token_counter;
use anyhow::Result;

/// Manages context window and summarization
pub struct ContextManager {
    /// Number of recent message pairs to keep when summarizing
    /// (e.g., 5 means keep last 5 user-assistant exchanges = 10 messages)
    pub keep_recent_pairs: usize,
}

impl Default for ContextManager {
    fn default() -> Self {
        Self {
            keep_recent_pairs: 5, // Keep last 5 exchanges by default
        }
    }
}

impl ContextManager {
    #[allow(dead_code)]
    pub fn new(keep_recent_pairs: usize) -> Self {
        Self { keep_recent_pairs }
    }

    /// Check if summarization is needed based on current token count
    pub fn should_summarize(&self, current_tokens: u32, window_size: u32, threshold: f32) -> bool {
        if window_size == 0 {
            return false;
        }
        
        let usage_ratio = current_tokens as f32 / window_size as f32;
        usage_ratio >= threshold
    }

    /// Build messages that need to be summarized (excluding system prompt and recent messages)
    pub fn build_summarization_messages(&self, messages: &[Message]) -> Vec<Message> {
        // Find the cutoff point: keep last N message pairs
        let keep_count = self.keep_recent_pairs * 2; // 2 messages per pair
        
        if messages.len() <= keep_count {
            return Vec::new(); // Nothing to summarize
        }
        
        // Skip system prompt (first message if it's a system message)
        let start_idx = if let Some(first) = messages.first() {
            if matches!(first.role, Role::System) { 1 } else { 0 }
        } else {
            0
        };
        
        // Take messages from start to (total - keep_count)
        let end_idx = messages.len().saturating_sub(keep_count);
        
        if start_idx >= end_idx {
            return Vec::new(); // Nothing to summarize
        }
        
        messages[start_idx..end_idx].to_vec()
    }

    /// Get messages to keep (system prompt + recent messages)
    pub fn get_messages_to_keep(&self, messages: &[Message]) -> Vec<Message> {
        let keep_count = self.keep_recent_pairs * 2;
        
        if messages.len() <= keep_count {
            return messages.to_vec();
        }
        
        // Always keep system prompt if present
        let mut result = Vec::new();
        if let Some(first) = messages.first() {
            if matches!(first.role, Role::System) {
                result.push(first.clone());
            }
        }
        
        // Add recent messages
        let recent_start = messages.len().saturating_sub(keep_count);
        result.extend_from_slice(&messages[recent_start..]);
        
        result
    }

    /// Summarize old messages using the LLM
    pub async fn summarize_messages(
        &self,
        llm_client: &dyn LlmClient,
        messages_to_summarize: &[Message],
    ) -> Result<String, LlmError> {
        if messages_to_summarize.is_empty() {
            return Ok(String::new());
        }

        // Build a prompt for summarization
        let mut conversation_text = String::new();
        for msg in messages_to_summarize {
            let role = match msg.role {
                Role::User => "User",
                Role::Assistant => "Assistant",
                Role::System => "System",
                Role::Tool => "Tool",
            };
            conversation_text.push_str(&format!("{}: {}\n\n", role, msg.content));
        }

        let summary_prompt = format!(
            "Please provide a concise summary of the following conversation history. \
             Focus on key topics, decisions, and important information. \
             Keep it under 200 words:\n\n{}",
            conversation_text
        );

        // Create a simple message for summarization
        let summary_messages = vec![
            Message::new(Role::System, "You are a helpful assistant that summarizes conversations concisely.".to_string()),
            Message::new(Role::User, summary_prompt),
        ];

        // Call LLM for summarization
        let response = llm_client.send_message_with_tools(
            summary_messages,
            vec![], // No tools needed for summarization
            Some(0.3), // Lower temperature for more consistent summaries
            Some(200), // Limit summary length
        ).await?;

        Ok(response.content)
    }

    /// Truncate context using sliding window approach
    #[allow(dead_code)]
    pub fn truncate_context(
        &self,
        messages: Vec<Message>,
        max_tokens: u32,
        system_prompt_tokens: u32,
    ) -> Vec<Message> {
        let available_tokens = max_tokens.saturating_sub(system_prompt_tokens);
        
        // Start with system prompt + recent messages
        let mut result = self.get_messages_to_keep(&messages);
        
        // Calculate current token usage
        let mut current_tokens = token_counter::estimate_tokens_for_messages(&result);
        
        // If we're still over the limit, remove oldest messages (except system)
        if current_tokens > available_tokens {
            // Remove system prompt from calculation for truncation
            let system_msg = if result.first().map(|m| matches!(m.role, Role::System)).unwrap_or(false) {
                result.remove(0)
            } else {
                return result; // No system message to remove
            };
            
            // Keep removing oldest messages until we fit
            while current_tokens > available_tokens && result.len() > 1 {
                result.remove(0);
                current_tokens = token_counter::estimate_tokens_for_messages(&result);
            }
            
            // Put system message back at the front
            result.insert(0, system_msg);
        }
        
        result
    }

    /// Get statistics about context usage
    #[allow(dead_code)]
    pub fn get_context_stats(&self, messages: &[Message], window_size: u32) -> ContextStats {
        let total_tokens = token_counter::estimate_tokens_for_messages(messages);
        let usage_ratio = if window_size > 0 {
            total_tokens as f32 / window_size as f32
        } else {
            0.0
        };
        
        ContextStats {
            total_tokens,
            window_size,
            usage_ratio,
            message_count: messages.len(),
        }
    }
}

/// Statistics about context usage
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ContextStats {
    pub total_tokens: u32,
    pub window_size: u32,
    pub usage_ratio: f32,
    pub message_count: usize,
}

impl ContextStats {
    /// Get a color class for UI display based on usage
    #[allow(dead_code)]
    pub fn get_color_class(&self) -> &'static str {
        if self.usage_ratio < 0.5 {
            "cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.2, 0.8, 0.2))" // Green
        } else if self.usage_ratio < 0.7 {
            "cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.8, 0.8, 0.2))" // Yellow
        } else if self.usage_ratio < 0.9 {
            "cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.8, 0.5, 0.0))" // Orange
        } else {
            "cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.8, 0.2, 0.2))" // Red
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::Role;

    #[test]
    fn test_should_summarize() {
        let manager = ContextManager::default();
        
        // Should summarize at 70% threshold
        assert!(manager.should_summarize(70000, 100000, 0.7));
        assert!(!manager.should_summarize(60000, 100000, 0.7));
        
        // Edge cases
        assert!(!manager.should_summarize(0, 100000, 0.7));
        assert!(manager.should_summarize(100000, 100000, 0.7));
    }

    #[test]
    fn test_build_summarization_messages() {
        let manager = ContextManager::new(2); // Keep last 2 pairs
        
        let messages = vec![
            Message::new(Role::System, "System prompt".to_string()),
            Message::new(Role::User, "Message 1".to_string()),
            Message::new(Role::Assistant, "Response 1".to_string()),
            Message::new(Role::User, "Message 2".to_string()),
            Message::new(Role::Assistant, "Response 2".to_string()),
            Message::new(Role::User, "Message 3".to_string()),
            Message::new(Role::Assistant, "Response 3".to_string()),
        ];
        
        let to_summarize = manager.build_summarization_messages(&messages);
        
        // Should summarize first 3 messages (excluding system prompt and last 2 pairs)
        assert_eq!(to_summarize.len(), 3);
        assert_eq!(to_summarize[0].content, "Message 1");
        assert_eq!(to_summarize[1].content, "Response 1");
        assert_eq!(to_summarize[2].content, "Message 2");
    }

    #[test]
    fn test_get_messages_to_keep() {
        let manager = ContextManager::new(2);
        
        let messages = vec![
            Message::new(Role::System, "System prompt".to_string()),
            Message::new(Role::User, "Message 1".to_string()),
            Message::new(Role::Assistant, "Response 1".to_string()),
            Message::new(Role::User, "Message 2".to_string()),
            Message::new(Role::Assistant, "Response 2".to_string()),
        ];
        
        let to_keep = manager.get_messages_to_keep(&messages);
        
        // Should keep system prompt + last 2 pairs
        assert_eq!(to_keep.len(), 5);
        assert_eq!(to_keep[0].content, "System prompt");
        assert_eq!(to_keep[1].content, "Message 1");
        assert_eq!(to_keep[4].content, "Response 2");
    }
}
