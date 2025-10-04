use crate::llm::{ToolDefinition, ToolCall, ToolResult};
use anyhow::Result;

#[async_trait::async_trait]
pub trait MCPTransport: Send + Sync {
    async fn connect(&mut self) -> Result<()>;
    async fn disconnect(&mut self) -> Result<()>;
    async fn discover_tools(&mut self) -> Result<Vec<ToolDefinition>>;
    async fn call_tool(&mut self, tool_call: ToolCall) -> Result<ToolResult>;
}

