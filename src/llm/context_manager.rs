//! Conversation summarization for context overflow.
//!
//! Message *selection* on overflow is now contiguous boundary compaction in
//! [`crate::llm::compaction::engine`]; this module owns the LLM summarizer that
//! produces the durable compaction handoff.

use crate::config::ModelPreset;
use crate::llm::{LlmClient, LlmError, Message, Role};

/// Owning type for the compaction summarizer.
pub struct SmartContextManager;

impl SmartContextManager {
    /// Summarize a list of messages using the LLM, producing a structured,
    /// update-in-place compaction summary as an `assistant` message.
    ///
    /// When a prior summary is present (identified by
    /// [`crate::llm::compaction::SUMMARY_MARKER`]) it is passed back to the model
    /// with instructions to *update* it rather than re-summarize from scratch,
    /// which prevents lossy drift across cycles. On LLM failure a deterministic
    /// fallback summary is committed unless `abort_on_summary_failure` is set.
    pub async fn summarize_messages(
        messages_to_summarize: Vec<Message>,
        _preset: &ModelPreset,
        compact_config: &crate::config::ConversationCompactConfig,
        llm_client: &dyn LlmClient,
    ) -> Result<Message, LlmError> {
        if messages_to_summarize.is_empty() {
            return Err(LlmError::Config("Cannot summarize empty message list".into()));
        }

        // Max length for tool result content in the summarization input (save tokens)
        const TOOL_CONTENT_MAX_CHARS: usize = 800;

        // Split off a prior summary so it can be *updated*, not re-summarized.
        let mut prior_summary: Option<String> = None;
        let mut convo: Vec<&Message> = Vec::new();
        for msg in &messages_to_summarize {
            if crate::llm::compaction::is_summary_content(&msg.content) {
                prior_summary = Some(strip_summary_marker(&msg.content));
            } else {
                convo.push(msg);
            }
        }

        let structured = compact_config.structured_summary;

        // Build transcript; collect anchors and verbatim user turns.
        let mut transcript = String::new();
        let mut anchors: Vec<String> = Vec::new();
        let mut user_turns: Vec<String> = Vec::new();
        for msg in &convo {
            let role_str = match msg.role {
                Role::User => "User",
                Role::Assistant => "Assistant",
                Role::System => "System",
                Role::Tool => "Tool",
            };
            let content = if msg.role == Role::Tool && msg.content.chars().count() > TOOL_CONTENT_MAX_CHARS {
                let truncated: String = msg.content.chars().take(TOOL_CONTENT_MAX_CHARS).collect();
                format!("{truncated}... [truncated]")
            } else {
                msg.content.clone()
            };
            transcript.push_str(&format!("[{}]: {}\n\n", role_str, content));

            // Include tool calls if present (assistant requested tools)
            if let Some(tool_calls) = &msg.tool_calls {
                for tool_call in tool_calls {
                    transcript.push_str(&format!(
                        "  [Tool Call: {}] {}\n",
                        tool_call.name,
                        serde_json::to_string(&tool_call.parameters).unwrap_or_default()
                    ));
                }
            }

            if structured {
                if msg.role == Role::User && !content.trim().is_empty() {
                    user_turns.push(content.clone());
                }
                collect_anchors(&content, &mut anchors);
            }
        }

        // Newest-first, so the most relevant user turn is first under budget.
        user_turns.reverse();

        // Assemble the summarizer prompt: system prompt is configurable in [conversation_compact].
        let prompt = if structured {
            let mut p = compact_config.system_prompt.clone();
            if let Some(prior) = &prior_summary {
                p.push_str(
                    "\n\n[Prior summary] Update the summary below with the new turns: move \
finished items to Done, add new progress, and remove obsolete details. Keep it a single summary.\n",
                );
                p.push_str(prior);
                p.push('\n');
            }
            p.push_str("\n\nConversation:\n");
            p.push_str(&transcript);

            if !user_turns.is_empty() {
                p.push_str("\n[Real user messages, newest first — quote verbatim when relevant]\n");
                push_within_budget(&mut p, &user_turns, compact_config.verbatim_user_budget_tokens, true);
            }
            if !anchors.is_empty() {
                p.push_str("\n[Anchor index — reproduce these verbatim under Critical Context]\n");
                push_within_budget(&mut p, &anchors, compact_config.anchor_budget_tokens, false);
            }
            p
        } else {
            format!("{}\n\nConversation:\n{}", compact_config.system_prompt, transcript)
        };

        // Use the same model for summarization (could be optimized to use cheaper model)
        let summary_response = match llm_client
            .send_message_with_tools(
                vec![Message::new(Role::User, prompt)],
                vec![], // No tools for summarization
                Some(0.3), // Lower temperature for more focused summaries
                Some(compact_config.max_tokens),
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                if compact_config.abort_on_summary_failure {
                    return Err(e);
                }
                tracing::warn!(
                    error = %e,
                    "Summarization failed; committing deterministic fallback summary"
                );
                return Ok(deterministic_summary(
                    prior_summary.as_deref(),
                    &user_turns,
                    &anchors,
                    convo.len(),
                ));
            }
        };

        let mut body = summary_response.content;
        if structured {
            // Keep the mechanically-extracted anchors even if the model omitted
            // them, then point at the recovery surface.
            let missing: Vec<&String> = anchors
                .iter()
                .filter(|anchor| !body.contains(anchor.as_str()))
                .collect();
            if !missing.is_empty() {
                body.push_str("\n\n### Critical Context (anchors)\n");
                for anchor in missing {
                    body.push_str(&format!("- {anchor}\n"));
                }
            }
            body.push_str(
                "\n\n---\n_Recovery: earlier turns are archived and remain searchable. \
Use the search_history tool to retrieve specifics._",
            );
        }

        // Return summary as an assistant turn carrying the compaction marker.
        // Not `system`: Anthropic keeps only the first system message, so a
        // system summary is silently dropped; selection would also hoist it.
        Ok(Message {
            role: Role::Assistant,
            content: format!("{}\n\n{}", crate::llm::compaction::SUMMARY_MARKER, body),
            timestamp: None,
            is_prompt: false,
            tool_call_id: None,
            tool_calls: None,
            attachments: None,
            reasoning_content: None,
        })
    }
}

/// Strip the compaction marker (and the blank line after it) from stored text.
fn strip_summary_marker(content: &str) -> String {
    content
        .trim_start()
        .strip_prefix(crate::llm::compaction::SUMMARY_MARKER)
        .map(|rest| rest.trim_start().to_string())
        .unwrap_or_else(|| content.to_string())
}

/// Append `items` to `out` until `budget_tokens` worth of characters is used.
/// `numbered` renders `1. item`; otherwise `- item`.
fn push_within_budget(out: &mut String, items: &[String], budget_tokens: u32, numbered: bool) {
    let budget_chars = (budget_tokens as usize).saturating_mul(4);
    let mut used = 0usize;
    for (idx, item) in items.iter().enumerate() {
        if used >= budget_chars {
            break;
        }
        if numbered {
            out.push_str(&format!("{}. {}\n", idx + 1, item));
        } else {
            out.push_str(&format!("- {}\n", item));
        }
        used += item.chars().count();
    }
}

/// Mechanically extract high-signal identifiers (paths, SHAs, ticket IDs, error
/// strings) that must survive compaction verbatim rather than being paraphrased.
fn collect_anchors(text: &str, out: &mut Vec<String>) {
    if text.is_empty() {
        return;
    }
    static ANCHOR_RES: std::sync::OnceLock<Vec<regex::Regex>> = std::sync::OnceLock::new();
    let res = ANCHOR_RES.get_or_init(|| {
        [
            r"\b[\w./-]+\.(?:rs|py|ts|tsx|js|jsx|json|toml|yaml|yml|md|sh|sql|html|css|go|java|c|cc|cpp|h|hpp)\b",
            r"\b[0-9a-fA-F]{7,40}\b",
            r"\b[A-Z][A-Z0-9]+-\d+\b",
            r#"(?i)\berror(?:\[[^\]]*\])?[: ][^\n]{0,120}"#,
        ]
        .iter()
        .filter_map(|pattern| regex::Regex::new(pattern).ok())
        .collect()
    });
    for re in res {
        for m in re.find_iter(text) {
            let found = m.as_str().trim().to_string();
            // Hex-ish matches must contain a digit to avoid matching words like
            // "defaced"; dedupe keeps the index compact.
            if found.len() < 4 {
                continue;
            }
            if re.as_str().contains("0-9a-fA-F") && !found.chars().any(|c| c.is_ascii_digit()) {
                continue;
            }
            if !out.iter().any(|existing| existing == &found) {
                out.push(found);
            }
        }
    }
}

/// Last-resort summary used when the summarizer LLM fails and abort is off.
/// Carries forward the prior summary, anchors, and the newest user message so
/// no information is silently dropped.
fn deterministic_summary(
    prior_summary: Option<&str>,
    user_turns_newest_first: &[String],
    anchors: &[String],
    summarized_count: usize,
) -> Message {
    let mut body = String::new();
    body.push_str("## Progress\n### Done\n");
    body.push_str(&format!(
        "- {summarized_count} earlier turn(s) were compacted without an LLM summary.\n"
    ));
    if let Some(prior) = prior_summary {
        body.push_str("\n## Prior summary (retained)\n");
        body.push_str(prior);
        body.push('\n');
    }
    if let Some(last_user) = user_turns_newest_first.first() {
        body.push_str("\n## Last user message (verbatim)\n");
        body.push_str(last_user);
        body.push('\n');
    }
    if !anchors.is_empty() {
        body.push_str("\n## Critical Context\n");
        for anchor in anchors.iter().take(50) {
            body.push_str(&format!("- {anchor}\n"));
        }
    }
    body.push_str(
        "\n---\n_Recovery: earlier turns are archived and remain searchable. \
Use the search_history tool to retrieve specifics._",
    );
    Message {
        role: Role::Assistant,
        content: format!("{}\n\n{}", crate::llm::compaction::SUMMARY_MARKER, body),
        timestamp: None,
        is_prompt: false,
        tool_call_id: None,
        tool_calls: None,
        attachments: None,
        reasoning_content: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConversationCompactConfig;
    use crate::llm::{ChatResponse, LlmError, Message, ToolDefinition};
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct CapturingClient {
        last_prompt: Mutex<String>,
        reply: String,
    }

    impl CapturingClient {
        fn new(reply: &str) -> Self {
            Self { last_prompt: Mutex::new(String::new()), reply: reply.to_string() }
        }
        fn prompt(&self) -> String {
            self.last_prompt.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl LlmClient for CapturingClient {
        async fn send_message_with_tools(
            &self,
            messages: Vec<Message>,
            _available_tools: Vec<ToolDefinition>,
            _temperature: Option<f32>,
            _max_tokens: Option<u32>,
        ) -> Result<ChatResponse, LlmError> {
            *self.last_prompt.lock().unwrap() =
                messages.last().map(|m| m.content.clone()).unwrap_or_default();
            Ok(ChatResponse {
                content: self.reply.clone(),
                tool_calls: vec![],
                reasoning_content: None,
            })
        }
    }

    struct FailingClient;

    #[async_trait]
    impl LlmClient for FailingClient {
        async fn send_message_with_tools(
            &self,
            _messages: Vec<Message>,
            _available_tools: Vec<ToolDefinition>,
            _temperature: Option<f32>,
            _max_tokens: Option<u32>,
        ) -> Result<ChatResponse, LlmError> {
            Err(LlmError::Api("summary backend down".into()))
        }
    }

    #[test]
    fn collect_anchors_keeps_paths_ids_and_errors() {
        let mut anchors = Vec::new();
        collect_anchors(
            "edited src/server/handler.rs and commit deadbeef12; ticket PROJ-4821; error: disk full at /var/log/app.log",
            &mut anchors,
        );
        assert!(anchors.iter().any(|a| a == "src/server/handler.rs"), "{anchors:?}");
        assert!(anchors.iter().any(|a| a.contains("deadbeef12")), "{anchors:?}");
        assert!(anchors.iter().any(|a| a == "PROJ-4821"), "{anchors:?}");
        assert!(anchors.iter().any(|a| a.to_lowercase().contains("error")), "{anchors:?}");
    }

    #[test]
    fn collect_anchors_skips_hex_without_digits() {
        let mut anchors = Vec::new();
        collect_anchors("the defaced facade was added", &mut anchors);
        assert!(!anchors.iter().any(|a| a == "defaced"), "{anchors:?}");
    }

    #[test]
    fn strip_summary_marker_removes_header() {
        let raw = format!("{}\n\n## Goal\nship it", crate::llm::compaction::SUMMARY_MARKER);
        assert_eq!(strip_summary_marker(&raw), "## Goal\nship it");
        assert_eq!(strip_summary_marker("plain"), "plain");
    }

    #[test]
    fn deterministic_summary_carries_prior_anchor_and_user() {
        let msg = deterministic_summary(
            Some("## Goal\nprior goal"),
            &["latest question".to_string()],
            &["src/main.rs".to_string()],
            7,
        );
        assert_eq!(msg.role, Role::Assistant);
        assert!(crate::llm::compaction::is_summary_content(&msg.content));
        assert!(msg.content.contains("prior goal"));
        assert!(msg.content.contains("latest question"));
        assert!(msg.content.contains("src/main.rs"));
        assert!(msg.content.contains("7 earlier turn(s)"));
        assert!(msg.content.contains("search_history"));
    }

    #[tokio::test]
    async fn structured_summary_updates_prior_and_marks_assistant() {
        let messages = vec![
            Message {
                role: Role::Assistant,
                content: format!("{}\n\n## Goal\nold goal", crate::llm::compaction::SUMMARY_MARKER),
                timestamp: None,
                is_prompt: false,
                tool_call_id: None,
                tool_calls: None,
                attachments: None,
                reasoning_content: None,
            },
            Message::new(Role::User, "now do the next thing".to_string()),
            Message::new(Role::Assistant, "working on it, touched src/lib.rs".to_string()),
        ];
        let client = CapturingClient::new("## Goal\nupdated goal");
        let config = ConversationCompactConfig::default();

        let summary = SmartContextManager::summarize_messages(
            messages,
            &ModelPreset::default(),
            &config,
            &client,
        )
        .await
        .unwrap();

        assert_eq!(summary.role, Role::Assistant);
        assert!(crate::llm::compaction::is_summary_content(&summary.content));
        assert!(summary.content.contains("updated goal"));
        assert!(summary.content.contains("search_history"));

        let prompt = client.prompt();
        assert!(prompt.contains("[Prior summary]"), "{prompt}");
        assert!(prompt.contains("old goal"), "{prompt}");
        assert!(prompt.contains("now do the next thing"), "{prompt}");
        assert!(prompt.contains("Conversation:"), "{prompt}");
    }

    #[tokio::test]
    async fn failed_summary_commits_deterministic_fallback() {
        let messages = vec![Message::new(Role::User, "keep this question".to_string())];
        let config = ConversationCompactConfig::default();

        let summary = SmartContextManager::summarize_messages(
            messages.clone(),
            &ModelPreset::default(),
            &config,
            &FailingClient,
        )
        .await
        .unwrap();

        assert!(crate::llm::compaction::is_summary_content(&summary.content));
        assert!(summary.content.contains("without an LLM summary"));
        assert!(summary.content.contains("keep this question"));

        // With abort enabled the error propagates instead.
        let aborting = ConversationCompactConfig {
            abort_on_summary_failure: true,
            ..Default::default()
        };
        let err = SmartContextManager::summarize_messages(
            messages,
            &ModelPreset::default(),
            &aborting,
            &FailingClient,
        )
        .await;
        assert!(err.is_err());
    }
}

