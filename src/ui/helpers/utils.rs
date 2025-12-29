//! Utility helper functions
//!
//! Helper functions for JSON formatting, value coercion, markdown stripping, etc.

use serde_json::Value;

/// Format a JSON string for display
pub fn format_json_string(raw: &str) -> String {
    match serde_json::from_str::<Value>(raw) {
        Ok(value) => serde_json::to_string_pretty(&value).unwrap_or_else(|_| raw.to_string()),
        Err(_) => raw.to_string(),
    }
}

/// Coerce a string to a JSON Value
pub fn coerce_value(raw: &str) -> Value {
    serde_json::from_str::<Value>(raw).unwrap_or(Value::String(raw.to_string()))
}

/// Strip markdown formatting from a single line of text.
/// Removes markdown characters (#, *, `, _, ~, |) and extracts text from links [text](url).
fn strip_markdown_line(line: &str) -> String {
    let mut result = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    
    while let Some(ch) = chars.next() {
        match ch {
            // Skip markdown formatting characters
            '#' | '*' | '`' | '_' | '~' | '|' => continue,
            // Handle brackets - extract text from [text](url) or [text]
            '[' => {
                // Collect text inside brackets
                let mut bracket_text = String::new();
                let mut depth = 1;
                while let Some(&next) = chars.peek() {
                    chars.next();
                    match next {
                        '[' => {
                            depth += 1;
                            bracket_text.push(next);
                        }
                        ']' => {
                            depth -= 1;
                            if depth == 0 {
                                // We have the text, add it to result
                                result.push_str(&bracket_text);
                                // Skip the (url) part if present
                                if chars.peek() == Some(&'(') {
                                    chars.next(); // skip (
                                    while let Some(&next) = chars.peek() {
                                        chars.next();
                                        if next == ')' {
                                            break;
                                        }
                                    }
                                }
                                break;
                            } else {
                                bracket_text.push(next);
                            }
                        }
                        _ => bracket_text.push(next),
                    }
                }
            }
            ']' => continue, // Skip standalone closing brackets
            // Keep regular characters
            _ => result.push(ch),
        }
    }
    
    result.trim().to_string()
}

/// Strip markdown formatting from text content for TTS.
/// Removes markdown characters and extracts readable text from markdown syntax.
pub fn strip_markdown_for_tts(content: &str) -> String {
    content
        .lines()
        .map(|line| strip_markdown_line(line.trim()))
        .filter(|line| !line.is_empty()) // Only filter out completely empty lines
        .collect::<Vec<_>>()
        .join(" ")
}

