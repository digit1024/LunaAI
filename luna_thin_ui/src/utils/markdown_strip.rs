//! Markdown Stripping Utility
//!
//! Strips all markdown syntax from text, leaving only plain text content.
//! Used before sending text to TTS to ensure natural speech.

use pulldown_cmark::{Event, Parser, Tag, TagEnd};

/// Strip all markdown syntax from text, preserving only plain text content
///
/// Removes:
/// - Code blocks (``` and ```language)
/// - Inline code (`code`)
/// - Bold (**text**, __text__)
/// - Italic (*text*, _text_)
/// - Headers (#, ##, ###, etc.)
/// - Links ([text](url)) → text
/// - Images (![alt](url)) → alt
/// - Lists (-, *, 1.)
/// - Blockquotes (>)
/// - Horizontal rules (---, ***)
/// - Strikethrough (~~text~~)
///
/// Preserves:
/// - Plain text content
/// - Sentence structure
/// - Basic whitespace (normalized)
pub fn strip_markdown(text: &str) -> String {
    let parser = Parser::new(text);
    let mut result = String::new();

    for event in parser {
        match event {
            Event::Start(Tag::CodeBlock(_)) => {
                // Code block started - ignore content
            }
            Event::End(TagEnd::CodeBlock) => {
                // Add space after code block
                if !result.ends_with(' ') {
                    result.push(' ');
                }
            }
            Event::Text(text) => {
                result.push_str(&text);
            }
            Event::Code(text) => {
                // Inline code - include the text
                result.push_str(&text);
            }
            Event::SoftBreak | Event::HardBreak => {
                // Convert breaks to spaces
                if !result.ends_with(' ') {
                    result.push(' ');
                }
            }
            Event::Start(Tag::Link { .. }) => {
                // Links: we'll get the text from Event::Text, ignore URL
                // Just continue to collect text
            }
            Event::End(TagEnd::Link) => {
                // Link ended, add space if needed
                if !result.ends_with(' ') {
                    result.push(' ');
                }
            }
            Event::Start(Tag::Image { .. }) => {
                // Images: we'll get alt text from Event::Text if present
                // Just continue to collect text
            }
            Event::End(TagEnd::Image) => {
                // Image ended, add space if needed
                if !result.ends_with(' ') {
                    result.push(' ');
                }
            }
            Event::Start(Tag::List(_)) => {
                // List started
            }
            Event::End(TagEnd::List(_)) => {
                // Add space after list
                if !result.ends_with(' ') {
                    result.push(' ');
                }
            }
            Event::Start(Tag::Item) => {
                // List item - add space if needed
                if !result.ends_with(' ') && !result.is_empty() {
                    result.push(' ');
                }
            }
            Event::End(TagEnd::Item) => {
                // List item ended
            }
            Event::Start(Tag::Paragraph) => {
                // Paragraph start - add space if needed
                if !result.ends_with(' ') && !result.is_empty() {
                    result.push(' ');
                }
            }
            Event::End(TagEnd::Paragraph) => {
                // Paragraph ended - add space
                if !result.ends_with(' ') {
                    result.push(' ');
                }
            }
            Event::Start(Tag::Heading { .. }) => {
                // Heading start - add space if needed
                if !result.ends_with(' ') && !result.is_empty() {
                    result.push(' ');
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                // Heading ended - add space
                if !result.ends_with(' ') {
                    result.push(' ');
                }
            }
            Event::Start(Tag::BlockQuote(_)) => {
                // Blockquote start
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                // Blockquote ended - add space
                if !result.ends_with(' ') {
                    result.push(' ');
                }
            }
            Event::Start(Tag::Emphasis) | Event::End(TagEnd::Emphasis) => {
                // Italic - ignore tags, text is already captured
            }
            Event::Start(Tag::Strong) | Event::End(TagEnd::Strong) => {
                // Bold - ignore tags, text is already captured
            }
            Event::Start(Tag::Strikethrough) | Event::End(TagEnd::Strikethrough) => {
                // Strikethrough - ignore tags, text is already captured
            }
            // Note: Tag::Rule doesn't exist in pulldown_cmark, horizontal rules are handled differently
            // They appear as Event::Start(Tag::Paragraph) followed by Event::End(Tag::Paragraph)
            _ => {
                // Ignore other events
            }
        }
    }

    // Clean up extra whitespace
    result
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_bold() {
        assert_eq!(strip_markdown("**bold** text"), "bold text");
        assert_eq!(strip_markdown("__bold__ text"), "bold text");
    }

    #[test]
    fn test_strip_italic() {
        assert_eq!(strip_markdown("*italic* text"), "italic text");
        assert_eq!(strip_markdown("_italic_ text"), "italic text");
    }

    #[test]
    fn test_strip_code() {
        assert_eq!(strip_markdown("`code` text"), "code text");
        assert_eq!(
            strip_markdown("```\ncode block\n```"),
            "code block"
        );
    }

    #[test]
    fn test_strip_headers() {
        assert_eq!(strip_markdown("# Header"), "Header");
        assert_eq!(strip_markdown("## Header"), "Header");
    }

    #[test]
    fn test_strip_links() {
        assert_eq!(
            strip_markdown("[text](https://example.com)"),
            "text"
        );
    }

    #[test]
    fn test_preserve_text() {
        assert_eq!(
            strip_markdown("This is plain text."),
            "This is plain text."
        );
    }

    #[test]
    fn test_complex_markdown() {
        let input = "# Title\n\nThis is **bold** and *italic* text with `code`.";
        let output = strip_markdown(input);
        assert!(output.contains("Title"));
        assert!(output.contains("bold"));
        assert!(output.contains("italic"));
        assert!(output.contains("code"));
        assert!(!output.contains("*"));
        assert!(!output.contains("`"));
    }
}

