use crate::config::ModelPreset;
use crate::llm::{build_llm_client, Message as LlmMessage, Role};
use crate::storage::sqlite_storage_simple::Message as SqliteMessage;
use anyhow::{Context, Result};

const MAX_TITLE_LENGTH: usize = 100;

/// Generate a title from messages (internal function that doesn't need storage)
pub async fn generate_title_from_messages(
    messages: Vec<SqliteMessage>,
    preset: &ModelPreset,
    summary_chars: u32,
    system_prompt: &str,
) -> Result<String> {
    // Get first 5 messages excluding "tool" role
    let filtered_messages: Vec<_> = messages
        .into_iter()
        .filter(|msg| msg.role != "tool")
        .take(5)
        .collect();

    if filtered_messages.is_empty() {
        return Ok("Untitled Conversation".to_string());
    }

    // Format transcript as "User: CONTENT\nAssistant: CONTENT\n..."
    let mut transcript = String::new();
    let mut char_count = 0;

    for msg in &filtered_messages {
        use crate::llm::Role;
        let role = Role::from(msg.role.as_str());
        let role_label = match role {
            Role::User => "User",
            Role::Assistant => "Assistant",
            Role::System => "System",
            Role::Tool => continue, // Skip tool messages in title generation
        };

        let content = msg.content.trim();
        if content.is_empty() {
            continue;
        }

        let line = format!("{}: {}\n", role_label, content);
        let line_len = line.chars().count();

        if char_count + line_len > summary_chars as usize {
            // Truncate the last line if needed
            let remaining = summary_chars as usize - char_count;
            if remaining > 0 {
                let truncated_content: String = content
                    .chars()
                    .take(remaining.saturating_sub(role_label.len() + 2))
                    .collect();
                transcript.push_str(&format!("{}: {}\n", role_label, truncated_content));
            }
            break;
        }

        transcript.push_str(&line);
        char_count += line_len;
    }

    // Create LLM client
    let llm_client = build_llm_client(preset);

    // Build messages: System prompt + User message (transcript)
    let mut llm_messages = Vec::new();
    llm_messages.push(LlmMessage {
        role: Role::System,
        content: system_prompt.to_string(),
        timestamp: None,
        is_prompt: false,
        tool_call_id: None,
        tool_calls: None,
        attachments: None,
        reasoning_content: None,
    });
    llm_messages.push(LlmMessage {
        role: Role::User,
        content: transcript,
        timestamp: None,
        is_prompt: false,
        tool_call_id: None,
        tool_calls: None,
        attachments: None,
        reasoning_content: None,
    });

    // Call LLM with empty tools vector
    let response = llm_client
        .send_message_with_tools(llm_messages, Vec::new(), None, None)
        .await
        .context("LLM call failed for title generation")?;

    // Truncate response to MAX_TITLE_LENGTH chars
    let title = response.content.trim();
    let truncated_title: String = title.chars().take(MAX_TITLE_LENGTH).collect();

    Ok(truncated_title)
}

// Function removed - not used anywhere

