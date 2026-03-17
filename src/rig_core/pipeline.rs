//! Rig-based conversation pipeline.
//!
//! Runs a single turn: accepts history + user message + preamble (system prompt), returns assistant text.

use crate::config::ModelPreset;
use crate::llm::Message;
use crate::rig_core::luna_messages_to_rig_history;
use crate::types::PlannedToolView;
use anyhow::Result;
use futures::Stream;
use rig::prelude::*;
use rig::providers::openai;
use rig::streaming::{StreamedAssistantContent, StreamedUserContent, StreamingChat};
use std::collections::HashMap;
use std::pin::Pin;

/// Chunk from the Rig stream. Delta = incremental text; Final = full accumulated response.
/// ToolPlanned/ToolStarted/ToolResult for MCP tool events (from Rig stream).
#[derive(Debug, Clone)]
pub enum StreamChunk {
    Delta(String),
    Final(String),
    ToolPlanned {
        tools: Vec<PlannedToolView>,
    },
    ToolStarted {
        tool_call_id: String,
        name: String,
        params_json: serde_json::Value,
    },
    ToolResult {
        tool_call_id: String,
        name: String,
        result_json: serde_json::Value,
        is_error: bool,
    },
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
    /// MCP servers: (tools, peer) per server for rmcp_tools.
    pub mcp_servers: Vec<(Vec<rmcp::model::Tool>, rmcp::service::ServerSink)>,
    /// Internal tools (schedule_task, memory).
    pub internal_tools: Vec<Box<dyn rig::tool::ToolDyn + 'static>>,
}

impl std::fmt::Debug for RigConversationContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RigConversationContext")
            .field("messages", &self.messages.len())
            .field("user_message", &self.user_message)
            .field("preset", &self.preset)
            .field("preamble_len", &self.preamble.len())
            .field("mcp_server_count", &self.mcp_servers.len())
            .field("internal_tool_count", &self.internal_tools.len())
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

    let has_tools = !context.mcp_servers.is_empty() || !context.internal_tools.is_empty();
    let agent = if !has_tools {
        openai_client
            .agent(&preset.model)
            .preamble(preamble)
            .temperature(preset.temperature.unwrap_or(0.7) as f64)
            .max_tokens(preset.max_tokens.unwrap_or(4096) as u64)
            .build()
    } else {
        let base = openai_client
            .agent(&preset.model)
            .preamble(preamble)
            .temperature(preset.temperature.unwrap_or(0.7) as f64)
            .max_tokens(preset.max_tokens.unwrap_or(4096) as u64);
        let builder = match context.mcp_servers.split_first() {
            None => base.tools(context.internal_tools),
            Some(((tools, peer), mcp_rest)) => {
                let mut b = base.rmcp_tools(tools.clone(), peer.clone());
                for (t, p) in mcp_rest {
                    b = b.rmcp_tools(t.clone(), p.clone());
                }
                if !context.internal_tools.is_empty() {
                    b = b.tools(context.internal_tools);
                }
                b
            }
        };
        builder.build()
    };

    let rig_history = luna_messages_to_rig_history(&context.messages);

    let stream_req = agent
        .stream_chat(context.user_message.clone(), rig_history)
        .multi_turn(64);
    let mut rig_stream = stream_req.await;

    use futures::StreamExt;
    use rig::agent::MultiTurnStreamItem;

    let mut tool_call_id_to_name: HashMap<String, String> = HashMap::new();

    let stream = async_stream::stream! {
        while let Some(chunk) = rig_stream.next().await {
            match chunk {
                Ok(MultiTurnStreamItem::StreamAssistantItem(
                    StreamedAssistantContent::Text(text),
                )) => yield Ok(StreamChunk::Delta(text.text)),
                Ok(MultiTurnStreamItem::StreamAssistantItem(
                    StreamedAssistantContent::ToolCall { tool_call, .. },
                )) => {
                    let name = tool_call.function.name.clone();
                    tool_call_id_to_name.insert(tool_call.id.clone(), name.clone());
                    let planned = PlannedToolView {
                        id: tool_call.id.clone(),
                        name: name.clone(),
                        params_json: tool_call.function.arguments.clone(),
                    };
                    yield Ok(StreamChunk::ToolPlanned { tools: vec![planned] });
                    yield Ok(StreamChunk::ToolStarted {
                        tool_call_id: tool_call.id.clone(),
                        name,
                        params_json: tool_call.function.arguments,
                    });
                }
                Ok(MultiTurnStreamItem::StreamUserItem(
                    StreamedUserContent::ToolResult { tool_result, .. },
                )) => {
                    use rig::completion::message::ToolResultContent;
                    let name = tool_call_id_to_name
                        .get(&tool_result.id)
                        .cloned()
                        .unwrap_or_else(|| tool_result.id.clone());
                    let raw: String = tool_result
                        .content
                        .iter()
                        .filter_map(|c| {
                            if let ToolResultContent::Text(t) = c {
                                Some(t.text.as_str())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    let config = crate::safety::SafetyConfig::default();
                    let sanitized = crate::safety::sanitize_tool_output(&name, &raw, &config);
                    let content = sanitized.content;
                    let result_json = serde_json::json!({ "content": content, "is_error": false });
                    yield Ok(StreamChunk::ToolResult {
                        tool_call_id: tool_result.id,
                        name,
                        result_json,
                        is_error: false,
                    });
                }
                Ok(MultiTurnStreamItem::FinalResponse(final_resp)) => {
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

