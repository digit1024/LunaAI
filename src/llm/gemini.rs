use super::*;
use crate::config::LlmProfile;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

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
#[serde(untagged)]
enum GeminiPart {
    Text {
        text: String,
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
    candidates: Vec<GeminiCandidate>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: GeminiContent,
}

pub struct GeminiClient {
    client: Client,
    profile: LlmProfile,
}

impl GeminiClient {
    pub fn new(profile: LlmProfile) -> Self {
        Self {
            client: Client::new(),
            profile,
        }
    }

    /// Build the Gemini API endpoint URL for a given method
    /// Handles endpoints that already include the path or just the base URL
    fn build_endpoint(&self, method: &str) -> String {
        let base = self.profile.endpoint.trim_end_matches('/');
        let model = &self.profile.model;

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
            println!(
                "🔍 DEBUG: Converting message to Gemini: role={:?}, content={}, attachments={:?}",
                msg.role, msg.content, msg.attachments
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
                // Regular text message with potential attachments
                let mut text_content = msg.content;

                // Handle attachments
                if let Some(attachments) = msg.attachments {
                    for attachment in attachments {
                        match attachment.mime_type.as_str() {
                            mime if mime.starts_with("image/") => {
                                text_content.push_str(&format!(
                                    "\n[Image: {} - {} bytes]",
                                    attachment.file_name, attachment.file_size
                                ));
                            }
                            mime if mime.starts_with("text/") => {
                                if let Some(file_content) = &attachment.content {
                                    text_content.push_str(&format!(
                                        "\n\nFile: {}\nContent:\n{}",
                                        attachment.file_name, file_content
                                    ));
                                }
                            }
                            _ => {
                                text_content.push_str(&format!(
                                    "\nFile attached: {} ({} bytes)",
                                    attachment.file_name, attachment.file_size
                                ));
                            }
                        }
                    }
                }

                current_parts.push(GeminiPart::Text { text: text_content });
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
        let contents = self.convert_messages_to_gemini(messages);

        let generation_config = GeminiGenerationConfig {
            temperature: temperature.or(self.profile.temperature),
            max_output_tokens: max_tokens.or(self.profile.max_tokens),
        };

        let request = GeminiRequest {
            contents,
            generation_config: Some(generation_config),
            tools: None,
        };

        // Build endpoint with model
        let endpoint = self.build_endpoint("streamGenerateContent");

        let response = self
            .client
            .post(&endpoint)
            .header("Content-Type", "application/json")
            .header("x-goog-api-key", &self.profile.api_key)
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            if error_text.is_empty() {
                return Err(LlmError::Api(format!(
                    "Gemini API error: HTTP {} {}",
                    status.as_u16(),
                    status.canonical_reason().unwrap_or("Unknown")
                )));
            }
            return Err(LlmError::Api(format!(
                "Gemini API error (HTTP {}): {}",
                status.as_u16(),
                error_text
            )));
        }

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
        let contents = self.convert_messages_to_gemini(messages);

        let generation_config = GeminiGenerationConfig {
            temperature: temperature.or(self.profile.temperature),
            max_output_tokens: max_tokens.or(self.profile.max_tokens),
        };

        let tools = if available_tools.is_empty() {
            None
        } else {
            Some(vec![GeminiTool {
                function_declarations: available_tools
                    .into_iter()
                    .map(|tool| {
                        let sanitized_params = self.sanitize_schema(tool.parameters);
                        log::debug!("🔧 Gemini tool: {} (sanitized schema)", tool.name);
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

        log::debug!(
            "📤 Sending Gemini request with {} tools",
            request.tools.as_ref().map(|t| t.len()).unwrap_or(0)
        );

        // Build endpoint with model
        let endpoint = self.build_endpoint("generateContent");

        let response = self
            .client
            .post(&endpoint)
            .header("Content-Type", "application/json")
            .header("x-goog-api-key", &self.profile.api_key)
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            if error_text.is_empty() {
                return Err(LlmError::Api(format!(
                    "Gemini API error: HTTP {} {}",
                    status.as_u16(),
                    status.canonical_reason().unwrap_or("Unknown")
                )));
            }
            return Err(LlmError::Api(format!(
                "Gemini API error (HTTP {}): {}",
                status.as_u16(),
                error_text
            )));
        }

        let response_data: GeminiResponse = response.json().await?;

        let candidate = response_data
            .candidates
            .first()
            .ok_or_else(|| LlmError::Api("No response from Gemini".to_string()))?;

        let mut content = String::new();
        let mut tool_calls = Vec::new();

        for part in &candidate.content.parts {
            match part {
                GeminiPart::Text { text } => {
                    content.push_str(text);
                }
                GeminiPart::FunctionCall { function_call } => {
                    tool_calls.push(ToolCall {
                        id: uuid::Uuid::new_v4().to_string(),
                        name: function_call.name.clone(),
                        parameters: function_call.args.clone(),
                    });
                }
                _ => {}
            }
        }

        Ok(ChatResponse {
            content,
            tool_calls,
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
