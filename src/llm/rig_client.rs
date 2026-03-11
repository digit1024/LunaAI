//! Rig-backed LlmClient adapter.
//!
//! Implements LlmClient using rig-core providers (openai, anthropic, ollama, gemini).


use super::{ChatResponse, LlmClient, LlmError, Message, Role, ToolDefinition};
use crate::config::ModelPreset;
use crate::rig_core::luna_messages_to_rig_history;
use async_trait::async_trait;
use futures::Stream;
use rig::agent::MultiTurnStreamItem;
use rig::completion::Chat;
use rig::prelude::*;
use rig::streaming::{StreamedAssistantContent, StreamingChat};
use std::pin::Pin;

/// Derive base URL for Rig from Luna endpoint.
fn endpoint_to_base_url(endpoint: &str) -> String {
    const SUFFIXES: &[&str] = &[
        "/chat/completions",
        "/v1/chat/completions",
        "/v1/messages",
        "/api/chat",
        "/api/generate",
    ];
    let mut s = endpoint.trim_end_matches('/').to_string();
    for suffix in SUFFIXES {
        if s.ends_with(suffix) {
            s = s.strip_suffix(suffix).unwrap_or(&s).trim_end_matches('/').to_string();
            break;
        }
    }
    s
}

pub struct RigLlmClient {
    preset: ModelPreset,
}

impl RigLlmClient {
    pub fn new(preset: ModelPreset) -> Self {
        Self { preset }
    }

    /// Extract preamble (system messages) and split last user message from history.
    fn split_messages(&self, messages: Vec<Message>) -> (String, String, Vec<rig::message::Message>) {
        let mut preamble_parts = Vec::new();
        let mut rest = Vec::new();
        for msg in &messages {
            match msg.role {
                Role::System => preamble_parts.push(msg.content.clone()),
                _ => rest.push(msg.clone()),
            }
        }
        let preamble = preamble_parts.join("\n\n");
        let (prompt, history) = if let Some((last, prev)) = rest.split_last() {
            if matches!(last.role, Role::User) {
                (last.content.clone(), prev.to_vec())
            } else {
                (String::new(), rest)
            }
        } else {
            (String::new(), vec![])
        };
        let rig_history = luna_messages_to_rig_history(&history);
        (preamble, prompt, rig_history)
    }
}

#[async_trait]
impl LlmClient for RigLlmClient {
    async fn send_message_stream(
        &self,
        messages: Vec<Message>,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LlmError>> + Send>>, LlmError> {
        let (preamble, prompt, history) = self.split_messages(messages);
        if prompt.is_empty() {
            return Err(LlmError::Config("No user message in messages".into()));
        }

        let preset = self.preset.clone();
        let temp = temperature.unwrap_or(preset.temperature.unwrap_or(0.7));
        let max_tok = max_tokens.or(preset.max_tokens).unwrap_or(4096) as u64;

        let stream = async move {
            match preset.backend.as_str() {
                "openai" => {
                    let base_url = endpoint_to_base_url(&preset.endpoint);
                    let client = rig::providers::openai::CompletionsClient::builder()
                        .api_key(&preset.api_key)
                        .base_url(&base_url)
                        .build()
                        .map_err(|e| LlmError::Config(format!("OpenAI client: {}", e)))?;
                    let agent = client
                        .agent(&preset.model)
                        .preamble(if preamble.is_empty() {
                            "You are a helpful assistant."
                        } else {
                            &preamble
                        })
                        .temperature(temp as f64)
                        .max_tokens(max_tok)
                        .build();
                    let mut stream = agent.stream_chat(prompt, history).await;
                    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                    tokio::spawn(async move {
                        use futures::StreamExt;
                        while let Some(chunk) = stream.next().await {
                            match chunk {
                                Ok(MultiTurnStreamItem::StreamAssistantItem(
                                    StreamedAssistantContent::Text(text),
                                )) => {
                                    let _ = tx.send(Ok(text.text));
                                }
                                Ok(MultiTurnStreamItem::FinalResponse(res)) => {
                                    let s = res.response().to_string();
                                    if !s.is_empty() {
                                        let _ = tx.send(Ok(s));
                                    }
                                    break;
                                }
                                Err(e) => {
                                    let _ = tx.send(Err(LlmError::Api(format!("{:?}", e))));
                                    break;
                                }
                                _ => {}
                            }
                        }
                    });
                    Ok::<_, LlmError>(tokio_stream::wrappers::UnboundedReceiverStream::new(rx))
                }
                "anthropic" => {
                    let base_url = endpoint_to_base_url(&preset.endpoint);
                let client = rig::providers::anthropic::Client::builder()
                    .api_key(preset.api_key.clone())
                    .base_url(&base_url)
                    .build()
                    .map_err(|e| LlmError::Config(format!("Anthropic client: {}", e)))?;
                let agent = client
                        .agent(&preset.model)
                        .preamble(if preamble.is_empty() {
                            "You are a helpful assistant."
                        } else {
                            &preamble
                        })
                        .temperature(temp as f64)
                        .max_tokens(max_tok)
                        .build();
                    let mut stream = agent.stream_chat(prompt, history).await;
                    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                    tokio::spawn(async move {
                        use futures::StreamExt;
                        while let Some(chunk) = stream.next().await {
                            match chunk {
                                Ok(MultiTurnStreamItem::StreamAssistantItem(
                                    StreamedAssistantContent::Text(text),
                                )) => {
                                    let _ = tx.send(Ok(text.text));
                                }
                                Ok(MultiTurnStreamItem::FinalResponse(res)) => {
                                    let s = res.response().to_string();
                                    if !s.is_empty() {
                                        let _ = tx.send(Ok(s));
                                    }
                                    break;
                                }
                                Err(e) => {
                                    let _ = tx.send(Err(LlmError::Api(format!("{:?}", e))));
                                    break;
                                }
                                _ => {}
                            }
                        }
                    });
                    Ok::<_, LlmError>(tokio_stream::wrappers::UnboundedReceiverStream::new(rx))
                }
                "ollama" => {
                    let base_url = endpoint_to_base_url(&preset.endpoint);
                    let client = rig::providers::ollama::Client::builder()
                        .api_key(rig::client::Nothing)
                        .base_url(&base_url)
                        .build()
                        .map_err(|e| LlmError::Config(format!("Ollama client: {}", e)))?;
                    let agent = client
                        .agent(&preset.model)
                        .preamble(if preamble.is_empty() {
                            "You are a helpful assistant."
                        } else {
                            &preamble
                        })
                        .temperature(temp as f64)
                        .max_tokens(max_tok)
                        .build();
                    let mut stream = agent.stream_chat(prompt, history).await;
                    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                    tokio::spawn(async move {
                        use futures::StreamExt;
                        while let Some(chunk) = stream.next().await {
                            match chunk {
                                Ok(MultiTurnStreamItem::StreamAssistantItem(
                                    StreamedAssistantContent::Text(text),
                                )) => {
                                    let _ = tx.send(Ok(text.text));
                                }
                                Ok(MultiTurnStreamItem::FinalResponse(res)) => {
                                    let s = res.response().to_string();
                                    if !s.is_empty() {
                                        let _ = tx.send(Ok(s));
                                    }
                                    break;
                                }
                                Err(e) => {
                                    let _ = tx.send(Err(LlmError::Api(format!("{:?}", e))));
                                    break;
                                }
                                _ => {}
                            }
                        }
                    });
                    Ok::<_, LlmError>(tokio_stream::wrappers::UnboundedReceiverStream::new(rx))
                }
                "gemini" => {
                    let base_url = endpoint_to_base_url(&preset.endpoint);
                    let client = rig::providers::gemini::Client::builder()
                        .api_key(&preset.api_key)
                        .base_url(&base_url)
                        .build()
                        .map_err(|e| LlmError::Config(format!("Gemini client: {}", e)))?;
                    let agent = client
                        .agent(&preset.model)
                        .preamble(if preamble.is_empty() {
                            "You are a helpful assistant."
                        } else {
                            &preamble
                        })
                        .temperature(temp as f64)
                        .max_tokens(max_tok)
                        .build();
                    let mut stream = agent.stream_chat(prompt, history).await;
                    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                    tokio::spawn(async move {
                        use futures::StreamExt;
                        while let Some(chunk) = stream.next().await {
                            match chunk {
                                Ok(MultiTurnStreamItem::StreamAssistantItem(
                                    StreamedAssistantContent::Text(text),
                                )) => {
                                    let _ = tx.send(Ok(text.text));
                                }
                                Ok(MultiTurnStreamItem::FinalResponse(res)) => {
                                    let s = res.response().to_string();
                                    if !s.is_empty() {
                                        let _ = tx.send(Ok(s));
                                    }
                                    break;
                                }
                                Err(e) => {
                                    let _ = tx.send(Err(LlmError::Api(format!("{:?}", e))));
                                    break;
                                }
                                _ => {}
                            }
                        }
                    });
                    Ok::<_, LlmError>(tokio_stream::wrappers::UnboundedReceiverStream::new(rx))
                }
                _ => {
                    // Fallback to OpenAI-compatible
                    let base_url = endpoint_to_base_url(&preset.endpoint);
                    let client = rig::providers::openai::CompletionsClient::builder()
                        .api_key(&preset.api_key)
                        .base_url(&base_url)
                        .build()
                        .map_err(|e| LlmError::Config(format!("OpenAI client: {}", e)))?;
                    let agent = client
                        .agent(&preset.model)
                        .preamble(if preamble.is_empty() {
                            "You are a helpful assistant."
                        } else {
                            &preamble
                        })
                        .temperature(temp as f64)
                        .max_tokens(max_tok)
                        .build();
                    let mut stream = agent.stream_chat(prompt, history).await;
                    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                    tokio::spawn(async move {
                        use futures::StreamExt;
                        while let Some(chunk) = stream.next().await {
                            match chunk {
                                Ok(MultiTurnStreamItem::StreamAssistantItem(
                                    StreamedAssistantContent::Text(text),
                                )) => {
                                    let _ = tx.send(Ok(text.text));
                                }
                                Ok(MultiTurnStreamItem::FinalResponse(res)) => {
                                    let s = res.response().to_string();
                                    if !s.is_empty() {
                                        let _ = tx.send(Ok(s));
                                    }
                                    break;
                                }
                                Err(e) => {
                                    let _ = tx.send(Err(LlmError::Api(format!("{:?}", e))));
                                    break;
                                }
                                _ => {}
                            }
                        }
                    });
                    Ok::<_, LlmError>(tokio_stream::wrappers::UnboundedReceiverStream::new(rx))
                }
            }
        }
        .await?;

        Ok(Box::pin(stream))
    }

    async fn send_message_with_tools(
        &self,
        messages: Vec<Message>,
        _available_tools: Vec<ToolDefinition>,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
    ) -> Result<ChatResponse, LlmError> {
        let (preamble, prompt, history) = self.split_messages(messages);
        if prompt.is_empty() {
            return Err(LlmError::Config("No user message in messages".into()));
        }

        let preset = &self.preset;
        let temp = temperature.unwrap_or(preset.temperature.unwrap_or(0.7));
        let max_tok = max_tokens.or(preset.max_tokens).unwrap_or(4096) as u64;

        let content = match preset.backend.as_str() {
            "openai" => {
                let base_url = endpoint_to_base_url(&preset.endpoint);
                let client = rig::providers::openai::CompletionsClient::builder()
                    .api_key(&preset.api_key)
                    .base_url(&base_url)
                    .build()
                    .map_err(|e| LlmError::Config(format!("OpenAI client: {}", e)))?;
                let agent = client
                    .agent(&preset.model)
                    .preamble(if preamble.is_empty() {
                        "You are a helpful assistant."
                    } else {
                        &preamble
                    })
                    .temperature(temp as f64)
                    .max_tokens(max_tok)
                    .build();
                agent.chat(prompt, history).await.map_err(|e| LlmError::Api(format!("{:?}", e)))?
            }
            "anthropic" => {
                let base_url = endpoint_to_base_url(&preset.endpoint);
                let client = rig::providers::anthropic::Client::builder()
                    .api_key(preset.api_key.clone())
                    .base_url(&base_url)
                    .build()
                    .map_err(|e| LlmError::Config(format!("Anthropic client: {}", e)))?;
                let agent = client
                    .agent(&preset.model)
                    .preamble(if preamble.is_empty() {
                        "You are a helpful assistant."
                    } else {
                        &preamble
                    })
                    .temperature(temp as f64)
                    .max_tokens(max_tok)
                    .build();
                agent.chat(prompt, history).await.map_err(|e| LlmError::Api(format!("{:?}", e)))?
            }
            "ollama" => {
                let base_url = endpoint_to_base_url(&preset.endpoint);
                let client = rig::providers::ollama::Client::builder()
                    .api_key(rig::client::Nothing)
                    .base_url(&base_url)
                    .build()
                    .map_err(|e| LlmError::Config(format!("Ollama client: {}", e)))?;
                let agent = client
                    .agent(&preset.model)
                    .preamble(if preamble.is_empty() {
                        "You are a helpful assistant."
                    } else {
                        &preamble
                    })
                    .temperature(temp as f64)
                    .max_tokens(max_tok)
                    .build();
                agent.chat(prompt, history).await.map_err(|e| LlmError::Api(format!("{:?}", e)))?
            }
            "gemini" => {
                let base_url = endpoint_to_base_url(&preset.endpoint);
                let client = rig::providers::gemini::Client::builder()
                    .api_key(&preset.api_key)
                    .base_url(&base_url)
                    .build()
                    .map_err(|e| LlmError::Config(format!("Gemini client: {}", e)))?;
                let agent = client
                    .agent(&preset.model)
                    .preamble(if preamble.is_empty() {
                        "You are a helpful assistant."
                    } else {
                        &preamble
                    })
                    .temperature(temp as f64)
                    .max_tokens(max_tok)
                    .build();
                agent.chat(prompt, history).await.map_err(|e| LlmError::Api(format!("{:?}", e)))?
            }
            _ => {
                let base_url = endpoint_to_base_url(&preset.endpoint);
                let client = rig::providers::openai::CompletionsClient::builder()
                    .api_key(&preset.api_key)
                    .base_url(&base_url)
                    .build()
                    .map_err(|e| LlmError::Config(format!("OpenAI client: {}", e)))?;
                let agent = client
                    .agent(&preset.model)
                    .preamble(if preamble.is_empty() {
                        "You are a helpful assistant."
                    } else {
                        &preamble
                    })
                    .temperature(temp as f64)
                    .max_tokens(max_tok)
                    .build();
                agent.chat(prompt, history).await.map_err(|e| LlmError::Api(format!("{:?}", e)))?
            }
        };

        Ok(ChatResponse {
            content,
            tool_calls: vec![],
            reasoning_content: None,
        })
    }
}
