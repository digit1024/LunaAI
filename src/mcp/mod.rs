//! MCP (Model Context Protocol) client and registry using rmcp.

pub mod client;
pub mod model;
pub mod registry;

pub use model::ServerStatus;
pub use registry::McpRegistry;
