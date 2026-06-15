

use crate::llm::{ToolCall, ToolDefinition, ToolResult};
use rust_mcp_sdk::schema::{CallToolRequestParams, CallToolResult, Tool};


impl From<&Tool> for ToolDefinition {
    fn from(tool: &Tool) -> Self {
        // Convert ToolInputSchema to serde_json::Value by serializing
        let parameters = serde_json::to_value(&tool.input_schema)
            .unwrap_or_else(|_| serde_json::json!({}));

        ToolDefinition {
            name: tool.name.clone(),
            description: tool.description.clone().unwrap_or_default(),
            parameters,
        }
    }
}

impl From<Tool> for ToolDefinition {
    fn from(tool: Tool) -> Self {
        Self::from(&tool)
    }
}

impl From<&ToolDefinition> for Tool {
    fn from(def: &ToolDefinition) -> Self {
        // Convert serde_json::Value to ToolInputSchema by deserializing
        // If deserialization fails, create a minimal schema with empty object
        let input_schema = serde_json::from_value(def.parameters.clone())
            .unwrap_or_else(|_| {
                // Create a minimal ToolInputSchema if conversion fails
                // Use a default JSON schema structure
                serde_json::from_value(serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }))
                .unwrap_or_else(|_| {
                    // Last resort: try to deserialize an empty object
                    serde_json::from_value(serde_json::json!({}))
                        .expect("Failed to create minimal ToolInputSchema")
                })
            });

        Tool {
            name: def.name.clone(),
            description: Some(def.description.clone()),
            input_schema,
            annotations: None,
            execution: None,
            icons: Vec::new(),
            meta: None,
            output_schema: None,
            title: None,
        }
    }
}

impl From<ToolDefinition> for Tool {
    fn from(def: ToolDefinition) -> Self {
        Self::from(&def)
    }
}



impl From<&CallToolResult> for ToolResult {
    fn from(result: &CallToolResult) -> Self {
        let is_error = result.is_error.unwrap_or(false);
        
        // Extract text content from Vec<ContentBlock>
        let content = result.content
            .iter()
            .find_map(|block| {
                // Try to extract text from ContentBlock
                block.as_text_content().ok().map(|text_content| text_content.text.clone())
            })
            .unwrap_or_else(|| {
                // Fallback: if no text content found, collect all content as JSON
                // This handles cases with images, audio, etc.
                if result.content.is_empty() {
                    String::new()
                } else {
                    // For now, just indicate non-text content
                    format!("[{} content blocks]", result.content.len())
                }
            });

        ToolResult {
            content,
            is_error,
        }
    }
}

impl From<CallToolResult> for ToolResult {
    fn from(result: CallToolResult) -> Self {
        Self::from(&result)
    }
}



/// Convert app ToolCall to SDK CallToolRequestParams
/// 
/// Note: ToolCall.parameters is serde_json::Value, but CallToolRequestParams
/// expects Option<Map<String, Value>>. This helper handles the conversion.
pub fn tool_call_to_params(tool_call: &ToolCall) -> CallToolRequestParams {
    let arguments: Option<serde_json::Map<String, serde_json::Value>> = if tool_call.parameters.is_null() {
        None
    } else if let Some(obj) = tool_call.parameters.as_object() {
        Some(obj.clone())
    } else {
        // If parameters is not an object, wrap it
        let mut map = serde_json::Map::new();
        map.insert("value".to_string(), tool_call.parameters.clone());
        Some(map)
    };

    CallToolRequestParams {
        name: tool_call.name.clone(),
        arguments,
        meta: None,
        task: None,
    }
}



/// Convert a slice of SDK Tools to app ToolDefinitions
pub fn tools_to_definitions(tools: &[Tool]) -> Vec<ToolDefinition> {
    tools.iter().map(From::from).collect()
}
