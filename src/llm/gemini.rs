use super::*;
use crate::config::ModelPreset;
use crate::llm::observability::{classify_reqwest_error, CallOutcome, LlmCallSpan};
use crate::llm::tokenizer::TokenCounter;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing;

#[derive(Debug, Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<GeminiTool>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiInlineBlob {
    #[serde(rename = "mimeType")]
    mime_type: String,
    data: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum GeminiPart {
    Text {
        text: String,
    },
    InlineData {
        #[serde(rename = "inlineData")]
        inline_data: GeminiInlineBlob,
    },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: GeminiFunctionCall,
    },
    FunctionResponse {
        #[serde(rename = "functionResponse")]
        function_response: GeminiFunctionResponse,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionCall {
    name: String,
    args: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionResponse {
    name: String,
    response: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
}

#[derive(Debug, Serialize)]
struct GeminiTool {
    #[serde(rename = "functionDeclarations")]
    function_declarations: Vec<GeminiFunctionDeclaration>,
}

#[derive(Debug, Serialize)]
struct GeminiFunctionDeclaration {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct GeminiResponse {
    #[serde(default)]
    candidates: Vec<GeminiCandidate>,
    #[serde(rename = "usageMetadata", default)]
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: GeminiContent,
    #[serde(rename = "finishReason", default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct GeminiUsageMetadata {
    #[serde(rename = "promptTokenCount", default)]
    prompt_token_count: Option<u32>,
    #[serde(rename = "candidatesTokenCount", default)]
    candidates_token_count: Option<u32>,
    #[serde(rename = "totalTokenCount", default)]
    total_token_count: Option<u32>,
}

pub struct GeminiClient {
    client: Client,
    preset: ModelPreset,
}

impl GeminiClient {
    pub fn new(preset: ModelPreset) -> Self {
        Self {
            client: super::observability::shared_http_client(),
            preset,
        }
    }

    fn estimate_context_tokens(messages: &[Message]) -> Option<usize> {
        let counter = TokenCounter::cl100k();
        let mut total = 0usize;
        for m in messages {
            total = total.saturating_add(counter.count_message_tokens(m));
        }
        Some(total)
    }

    fn extract_request_id(resp: &reqwest::Response) -> Option<String> {
        resp.headers()
            .get("x-request-id")
            .or_else(|| resp.headers().get("x-goog-request-id"))
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    }

    /// Build the Gemini API endpoint URL for a given method
    /// Handles endpoints that already include the path or just the base URL
    fn build_endpoint(&self, method: &str) -> String {
        let base = self.preset.endpoint.trim_end_matches('/');
        let model = &self.preset.model;

        // Check if endpoint already includes the models path
        if base.ends_with("/v1beta/models") || base.ends_with("/v1beta/models/") {
            format!("{}/{}:{}", base.trim_end_matches('/'), model, method)
        } else {
            format!("{}/v1beta/models/{}:{}", base, model, method)
        }
    }

    /// Sanitize JSON Schema to only include fields supported by Gemini API
    /// Gemini only supports: type, nullable, required, format, description, properties, items, enum
    fn sanitize_schema(&self, schema: serde_json::Value) -> serde_json::Value {
        match schema {
            serde_json::Value::Object(mut map) => {
                // Remove unsupported fields
                map.remove("additionalProperties");
                map.remove("$schema"); // JSON Schema draft identifier - not supported by Gemini
                map.remove("$ref");
                map.remove("$defs");
                map.remove("default");
                map.remove("optional");
                map.remove("maximum");
                map.remove("minimum");
                map.remove("exclusiveMaximum");
                map.remove("exclusiveMinimum");
                map.remove("oneOf");
                map.remove("anyOf");
                map.remove("allOf");
                map.remove("not");
                map.remove("pattern");
                map.remove("minLength");
                map.remove("maxLength");
                map.remove("minItems");
                map.remove("maxItems");

                // Recursively sanitize "properties" (object with schema values)
                if let Some(serde_json::Value::Object(properties)) = map.get_mut("properties") {
                    for (_key, value) in properties.iter_mut() {
                        *value = self.sanitize_schema(value.clone());
                    }
                }

                // Recursively sanitize "items" (array element schema)
                if let Some(items) = map.get_mut("items") {
                    *items = self.sanitize_schema(items.clone());
                }

                serde_json::Value::Object(map)
            }
            serde_json::Value::Array(arr) => {
                serde_json::Value::Array(arr.into_iter().map(|v| self.sanitize_schema(v)).collect())
            }
            other => other,
        }
    }

    fn convert_messages_to_gemini(&self, messages: Vec<Message>) -> Vec<GeminiContent> {
        let mut gemini_contents = Vec::new();
        let mut current_role: Option<String> = None;
        let mut current_parts: Vec<GeminiPart> = Vec::new();

        for msg in messages {
            tracing::debug!(
                role = ?msg.role,
                content_length = msg.content.len(),
                attachment_count = msg.attachments.as_ref().map(|a| a.len()).unwrap_or(0),
                "Converting message to Gemini format"
            );

            let role = match msg.role {
                Role::User => "user",
                Role::Assistant => "model",
                Role::System => "user", // Gemini doesn't have system role, treat as user
                Role::Tool => "function", // Tool results
            };

            // If role changes, push accumulated content
            if let Some(prev_role) = &current_role {
                if prev_role != role {
                    if !current_parts.is_empty() {
                        gemini_contents.push(GeminiContent {
                            role: prev_role.clone(),
                            parts: std::mem::take(&mut current_parts),
                        });
                    }
                }
            }

            // Add message parts
            if msg.role == Role::Tool {
                // Tool result
                if let Some(tool_call_id) = msg.tool_call_id {
                    current_parts.push(GeminiPart::FunctionResponse {
                        function_response: GeminiFunctionResponse {
                            name: tool_call_id,
                            response: serde_json::json!({ "result": msg.content }),
                        },
                    });
                }
            } else if let Some(tool_calls) = msg.tool_calls {
                // Tool calls from assistant
                for tc in tool_calls {
                    current_parts.push(GeminiPart::FunctionCall {
                        function_call: GeminiFunctionCall {
                            name: tc.name,
                            args: tc.parameters,
                        },
                    });
                }
            } else {
                // Regular text message with potential attachments (text first, then image inlineData)
                let mut text_content = msg.content;
                let mut image_parts: Vec<GeminiPart> = Vec::new();

                if let Some(attachments) = msg.attachments {
                    for attachment in attachments {
                        if attachment.mime_type.starts_with("image/") {
                            if let Some(data) = attachment.content {
                                image_parts.push(GeminiPart::InlineData {
                                    inline_data: GeminiInlineBlob {
                                        mime_type: attachment.mime_type.clone(),
                                        data,
                                    },
                                });
                            }
                        } else if attachment.mime_type.starts_with("text/") {
                            if let Some(file_content) = &attachment.content {
                                text_content.push_str(&format!(
                                    "\n\nFile: {}\nContent:\n{}",
                                    attachment.file_name, file_content
                                ));
                            }
                        } else {
                            text_content.push_str(&format!(
                                "\nFile attached: {} ({} bytes)",
                                attachment.file_name, attachment.file_size
                            ));
                        }
                    }
                }

                if !text_content.is_empty() {
                    current_parts.push(GeminiPart::Text { text: text_content });
                }
                current_parts.extend(image_parts);
            }

            current_role = Some(role.to_string());
        }

        // Push remaining content
        if !current_parts.is_empty() {
            if let Some(role) = current_role {
                gemini_contents.push(GeminiContent {
                    role,
                    parts: current_parts,
                });
            }
        }

        gemini_contents
    }
}

#[async_trait]
impl LlmClient for GeminiClient {
    async fn send_message_stream(
        &self,
        messages: Vec<Message>,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LlmError>> + Send>>, LlmError> {
        let mut span = LlmCallSpan::start("gemini", self.preset.model.clone(), true);
        let context_tokens = Self::estimate_context_tokens(&messages);
        span.set_input_shape(messages.len(), 0, context_tokens);

        let contents = self.convert_messages_to_gemini(messages);

        let generation_config = GeminiGenerationConfig {
            temperature: temperature.or(self.preset.temperature),
            max_output_tokens: max_tokens.or(self.preset.max_tokens),
        };

        let request = GeminiRequest {
            contents,
            generation_config: Some(generation_config),
            tools: None,
        };

        // Build endpoint with model
        let endpoint = self.build_endpoint("streamGenerateContent");

        let response = match self
            .client
            .post(&endpoint)
            .header("Content-Type", "application/json")
            .header("x-goog-api-key", &self.preset.api_key)
            .json(&request)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                let (outcome, kind) = classify_reqwest_error(&e);
                span.finish_error(outcome, kind, e.to_string());
                return Err(LlmError::Http(e));
            }
        };

        let status = response.status();
        let request_id = Self::extract_request_id(&response);
        span.set_response_headers(status.as_u16(), request_id);

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            let msg = if error_text.is_empty() {
                format!(
                    "Gemini API error: HTTP {} {}",
                    status.as_u16(),
                    status.canonical_reason().unwrap_or("Unknown")
                )
            } else {
                format!("Gemini API error (HTTP {}): {}", status.as_u16(), error_text)
            };
            span.finish_error(CallOutcome::HttpError, format!("http_{}", status.as_u16()), msg.clone());
            return Err(LlmError::Api(msg));
        }

        // Stream succeeded to first byte. Gemini's streaming chunk parser
        // below uses the high-level Stream API, so we cannot easily detect
        // a truncated stream here without restructuring. Mark span as
        // successful at the headers stage; transport-level keepalive will
        // turn silent drops into reqwest errors that subscribers see.
        span.finish_success();

        let stream = response.bytes_stream();
        let stream = futures::StreamExt::map(stream, |chunk_result| {
            chunk_result
                .map_err(|e| LlmError::Http(e))
                .and_then(|chunk| {
                    let chunk_str = String::from_utf8(chunk.to_vec())
                        .map_err(|e| LlmError::Api(format!("Invalid UTF-8: {}", e)))?;

                    let mut content = String::new();

                    // Gemini streaming returns JSON objects separated by newlines
                    for line in chunk_str.lines() {
                        if line.trim().is_empty() {
                            continue;
                        }

                        if let Ok(response) = serde_json::from_str::<GeminiResponse>(line) {
                            if let Some(candidate) = response.candidates.first() {
                                for part in &candidate.content.parts {
                                    if let GeminiPart::Text { text } = part {
                                        content.push_str(text);
                                    }
                                }
                            }
                        }
                    }

                    if content.is_empty() {
                        Ok(None)
                    } else {
                        Ok(Some(content))
                    }
                })
        });

        let stream = futures::StreamExt::filter_map(stream, |result| async move {
            match result {
                Ok(Some(content)) => Some(Ok(content)),
                Ok(None) => None,
                Err(e) => Some(Err(e)),
            }
        });

        Ok(Box::pin(stream))
    }

    async fn send_message_with_tools(
        &self,
        messages: Vec<Message>,
        available_tools: Vec<ToolDefinition>,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
    ) -> Result<ChatResponse, LlmError> {
        let mut span = LlmCallSpan::start("gemini", self.preset.model.clone(), false);
        let context_tokens = Self::estimate_context_tokens(&messages);
        span.set_input_shape(messages.len(), available_tools.len(), context_tokens);

        let contents = self.convert_messages_to_gemini(messages);

        let generation_config = GeminiGenerationConfig {
            temperature: temperature.or(self.preset.temperature),
            max_output_tokens: max_tokens.or(self.preset.max_tokens),
        };

        let tools = if available_tools.is_empty() {
            None
        } else {
            Some(vec![GeminiTool {
                function_declarations: available_tools
                    .into_iter()
                    .map(|tool| {
                        let sanitized_params = self.sanitize_schema(tool.parameters);
                        tracing::debug!(tool_name = %tool.name, "Gemini tool sanitized schema");
                        GeminiFunctionDeclaration {
                            name: tool.name,
                            description: tool.description,
                            parameters: sanitized_params,
                        }
                    })
                    .collect(),
            }])
        };

        let request = GeminiRequest {
            contents,
            generation_config: Some(generation_config),
            tools,
        };

        tracing::debug!(
            "📤 Sending Gemini request with {} tools",
            request.tools.as_ref().map(|t| t.len()).unwrap_or(0)
        );

        // Build endpoint with model
        let endpoint = self.build_endpoint("generateContent");

        let response = match self
            .client
            .post(&endpoint)
            .header("Content-Type", "application/json")
            .header("x-goog-api-key", &self.preset.api_key)
            .json(&request)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                let (outcome, kind) = classify_reqwest_error(&e);
                span.finish_error(outcome, kind, e.to_string());
                return Err(LlmError::Http(e));
            }
        };

        let status = response.status();
        let request_id = Self::extract_request_id(&response);
        span.set_response_headers(status.as_u16(), request_id);

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            let msg = if error_text.is_empty() {
                format!(
                    "Gemini API error: HTTP {} {}",
                    status.as_u16(),
                    status.canonical_reason().unwrap_or("Unknown")
                )
            } else {
                format!("Gemini API error (HTTP {}): {}", status.as_u16(), error_text)
            };
            span.finish_error(CallOutcome::HttpError, format!("http_{}", status.as_u16()), msg.clone());
            return Err(LlmError::Api(msg));
        }

        let response_data: GeminiResponse = match response.json().await {
            Ok(r) => r,
            Err(e) => {
                let (outcome, kind) = classify_reqwest_error(&e);
                span.finish_error(outcome, kind, e.to_string());
                return Err(LlmError::Http(e));
            }
        };

        if let Some(u) = response_data.usage_metadata.as_ref() {
            span.set_usage(u.prompt_token_count, u.candidates_token_count, u.total_token_count);
        }

        let candidate = match response_data.candidates.first() {
            Some(c) => c,
            None => {
                let msg = "No response from Gemini".to_string();
                span.finish_error(CallOutcome::Parse, "empty_candidates", msg.clone());
                return Err(LlmError::Api(msg));
            }
        };

        if let Some(reason) = candidate.finish_reason.clone() {
            span.set_finish_reason(reason);
        }

        let mut content = String::new();
        let mut tool_calls = Vec::new();

        for part in &candidate.content.parts {
            match part {
                GeminiPart::Text { text } => {
                    content.push_str(text);
                }
                GeminiPart::FunctionCall { function_call } => {
                    span.observe_tool_call();
                    tool_calls.push(ToolCall {
                        id: uuid::Uuid::new_v4().to_string(),
                        name: function_call.name.clone(),
                        parameters: function_call.args.clone(),
                    });
                }
                _ => {}
            }
        }

        span.finish_success();
        Ok(ChatResponse {
            content,
            tool_calls,
            reasoning_content: None,
        })
    }

    async fn send_message_stream_with_tools(
        &self,
        messages: Vec<Message>,
        available_tools: Vec<ToolDefinition>,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<ChatStreamEvent, LlmError>> + Send>>, LlmError>
    {
        let response = self
            .send_message_with_tools(messages, available_tools, temperature, max_tokens)
            .await?;

        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            if !response.content.is_empty() {
                let _ = tx.send(Ok(ChatStreamEvent::ContentDelta(response.content)));
            }
            for tool_call in response.tool_calls {
                let _ = tx.send(Ok(ChatStreamEvent::ToolCallDelta(tool_call)));
            }
        });

        Ok(Box::pin(UnboundedReceiverStream::new(rx)))
    }
}
