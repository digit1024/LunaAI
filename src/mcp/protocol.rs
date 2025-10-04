use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct MCPRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MCPResponse {
    pub jsonrpc: String,
    pub id: u64,
    pub result: Option<serde_json::Value>,
    pub error: Option<MCPError>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MCPError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

impl MCPRequest {
    pub fn new(id: u64, method: String, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method,
            params,
        }
    }
    
    pub fn tools_list(id: u64) -> Self {
        Self::new(id, "tools/list".to_string(), Some(serde_json::json!({})))
    }
    
    pub fn tools_call(id: u64, name: String, arguments: serde_json::Value) -> Self {
        Self::new(
            id,
            "tools/call".to_string(),
            Some(serde_json::json!({
                "name": name,
                "arguments": arguments
            }))
        )
    }
    
    pub fn initialize(id: u64) -> Self {
        Self::new(
            id,
            "initialize".to_string(),
            Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "clientInfo": {
                    "name": "cosmic_llm",
                    "version": "1.0.0"
                }
            }))
        )
    }
}

