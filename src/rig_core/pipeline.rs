//! Rig-based conversation pipeline.
//!
//! Runs a single turn: accepts history + user message + preamble (system prompt), returns assistant text.

use crate::config::ModelPreset;
use crate::llm::Message;
use crate::rig_core::luna_messages_to_rig_history;
use anyhow::Result;
use futures::Stream;
use rig::prelude::*;
use rig::providers::openai;
use rig::streaming::{StreamedAssistantContent, StreamingChat};
use std::pin::Pin;

/// Chunk from the Rig stream. Delta = incremental text; Final = full accumulated response.
/// Some providers (e.g. DeepSeek) only stream turn 1 and put turn 2 in FinalResponse.
/// We use Final to fill any gap without duplicating already-streamed content.
#[derive(Debug, Clone)]
pub enum StreamChunk {
    Delta(String),
    Final(String),
}

/// Input context for a Rig turn.
pub struct RigConversationContext {
    /// Chat history (Luna messages; System messages excluded, use preamble instead).
    pub messages: Vec<Message>,
    /// User message for this turn.
    pub user_message: String,
    /// Model preset (backend, model, endpoint, api_key).
    pub preset: ModelPreset,
    /// System prompt / preamble (from ContextService::inject_prompts). Default: "You are a helpful assistant."
    pub preamble: String,
    /// Pre-built MCP + internal tool wrappers (built by engine when tools are enabled). Empty = no tools.
    pub mcp_tools: Vec<Box<dyn rig::tool::ToolDyn + 'static>>,
}

impl std::fmt::Debug for RigConversationContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RigConversationContext")
            .field("messages", &self.messages.len())
            .field("user_message", &self.user_message)
            .field("preset", &self.preset)
            .field("preamble_len", &self.preamble.len())
            .field("tool_count", &self.mcp_tools.len())
            .finish()
    }
}

/// Derive base URL for rig from Luna endpoint (full URL to chat completions).
/// Rig expects base_url + "/chat/completions"; Luna stores the full URL.
fn endpoint_to_base_url(endpoint: &str) -> String {
    const CHAT_PATH: &str = "/chat/completions";
    endpoint
        .trim_end_matches('/')
        .strip_suffix(CHAT_PATH)
        .unwrap_or(endpoint)
        .trim_end_matches('/')
        .to_string()
}

/// Run a single turn with streaming (ConversationEngine integration).
#[tracing::instrument(skip(context), fields(model = %context.preset.model, history_len = context.messages.len()))]
pub async fn run_turn_streaming(
    context: RigConversationContext,
) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, anyhow::Error>> + Send>>> {
    let preset = &context.preset;

    if preset.backend != "openai" {
        anyhow::bail!(
            "Rig supports only OpenAI-compatible backend for now; got: {}",
            preset.backend
        );
    }

    let base_url = endpoint_to_base_url(&preset.endpoint);
    let openai_client = openai::CompletionsClient::builder()
        .api_key(&preset.api_key)
        .base_url(&base_url)
        .build()
        .map_err(|e| anyhow::anyhow!("OpenAI client init: {}", e))?;

    let preamble = context.preamble.trim();
    let preamble = if preamble.is_empty() {
        "You are a helpful assistant."
    } else {
        preamble
    };

    let agent = if context.mcp_tools.is_empty() {
        openai_client
            .agent(&preset.model)
            .preamble(preamble)
            .temperature(preset.temperature.unwrap_or(0.7) as f64)
            .max_tokens(preset.max_tokens.unwrap_or(4096) as u64)
            .build()
    } else {
        openai_client
            .agent(&preset.model)
            .preamble(preamble)
            .temperature(preset.temperature.unwrap_or(0.7) as f64)
            .max_tokens(preset.max_tokens.unwrap_or(4096) as u64)
            .tools(context.mcp_tools)
            .build()
    };

    let rig_history = luna_messages_to_rig_history(&context.messages);

    let stream_req = agent
        .stream_chat(context.user_message.clone(), rig_history)
        .multi_turn(10);
    let mut rig_stream = stream_req.await;

    use futures::StreamExt;
    use rig::agent::MultiTurnStreamItem;

    let stream = async_stream::stream! {
        while let Some(chunk) = rig_stream.next().await {
            match chunk {
                Ok(MultiTurnStreamItem::StreamAssistantItem(
                    StreamedAssistantContent::Text(text),
                )) => yield Ok(StreamChunk::Delta(text.text)),
                Ok(MultiTurnStreamItem::FinalResponse(final_resp)) => {
                    // Some providers (e.g. DeepSeek) only stream turn 1; turn 2 is in FinalResponse.
                    // Engine will append only the suffix not already received to avoid duplication.
                    let s = final_resp.response().to_string();
                    if !s.is_empty() {
                        yield Ok(StreamChunk::Final(s));
                    }
                    break;
                }
                Ok(_) => {}
                Err(e) => {
                    yield Err(anyhow::anyhow!("{:?}", e));
                    break;
                }
            }
        }
    };

    Ok(Box::pin(stream))
}

