use thiserror::Error;

/// Error types for the agentic loop library
#[derive(Error, Debug)]
pub enum AgenticLoopError {
    #[error("Generic error: {0}")]
    Generic(String),
    
    /// MCP configuration errors
    #[error("MCP config file not found: {0}")]
    MCPConfigNotFound(String),
    
    #[error("Failed to read MCP config file: {0}")]
    MCPConfigReadError(#[from] std::io::Error),
    
    #[error("Failed to parse MCP config JSON: {0}")]
    MCPConfigParseError(#[from] serde_json::Error),

    #[error("Failed to connect to MCP server: {0}")]
    MCPConnectionError(String),
}




/// Result type alias for agentic loop operations
pub type Result<T> = std::result::Result<T, AgenticLoopError>;

