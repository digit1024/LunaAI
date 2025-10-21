use super::{Message, Attachment};

/// Simple token estimation using character count
/// Uses 4 characters ≈ 1 token as a rough approximation
pub fn estimate_tokens(text: &str) -> u32 {
    // Basic estimation: 4 characters per token
    // This is a rough approximation - actual tokenization varies by model
    (text.len() as f32 / 4.0).ceil() as u32
}

/// Estimate tokens for a single message
pub fn estimate_tokens_for_message(message: &Message) -> u32 {
    let mut total = 0u32;
    
    // Count content
    total += estimate_tokens(&message.content);
    
    // Count tool calls if present
    if let Some(tool_calls) = &message.tool_calls {
        for tool_call in tool_calls {
            total += estimate_tokens(&tool_call.id);
            total += estimate_tokens(&tool_call.name);
            total += estimate_tokens(&tool_call.parameters.to_string());
        }
    }
    
    // Count attachments if present
    if let Some(attachments) = &message.attachments {
        total += estimate_tokens_for_attachments(attachments);
    }
    
    total
}

/// Estimate tokens for multiple messages
pub fn estimate_tokens_for_messages(messages: &[Message]) -> u32 {
    messages.iter()
        .map(estimate_tokens_for_message)
        .sum()
}

/// Estimate tokens for attachments
pub fn estimate_tokens_for_attachments(attachments: &[Attachment]) -> u32 {
    attachments.iter()
        .map(|attachment| {
            let mut total = 0u32;
            
            // Count file path and name
            total += estimate_tokens(&attachment.file_path);
            total += estimate_tokens(&attachment.file_name);
            total += estimate_tokens(&attachment.mime_type);
            
            // Count content if present (for text files)
            if let Some(content) = &attachment.content {
                total += estimate_tokens(content);
            }
            
            total
        })
        .sum()
}

/// Estimate tokens for a system prompt
#[allow(dead_code)]
pub fn estimate_tokens_for_system_prompt(system_prompt: &str) -> u32 {
    estimate_tokens(system_prompt)
}

/// Estimate total context size including system prompt, messages, and attachments
#[allow(dead_code)]
pub fn estimate_total_context_tokens(
    system_prompt: Option<&str>,
    messages: &[Message],
) -> u32 {
    let mut total = 0u32;
    
    // Add system prompt tokens
    if let Some(prompt) = system_prompt {
        total += estimate_tokens_for_system_prompt(prompt);
    }
    
    // Add message tokens
    total += estimate_tokens_for_messages(messages);
    
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::Role;

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens("Hello world"), 3); // 11 chars / 4 = 2.75 -> 3
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("a"), 1);
    }

    #[test]
    fn test_estimate_tokens_for_message() {
        let message = Message::new(Role::User, "Hello world".to_string());
        assert_eq!(estimate_tokens_for_message(&message), 3);
    }

    #[test]
    fn test_estimate_tokens_for_messages() {
        let messages = vec![
            Message::new(Role::User, "Hello".to_string()),
            Message::new(Role::Assistant, "Hi there".to_string()),
        ];
        assert_eq!(estimate_tokens_for_messages(&messages), 4); // 5/4 + 8/4 = 2 + 2 = 4
    }
}
