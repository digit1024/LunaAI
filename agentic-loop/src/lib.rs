//! Agentic Loop Library
//!
//! A library for implementing agentic loop execution patterns.

pub mod error;
pub mod mcp_config;
pub mod mcp_connection;
pub mod mcp_servers_registry;

/// Module prelude for common imports
pub mod prelude {
    pub use crate::error::{AgenticLoopError, Result};
    pub use crate::mcp_config;
}



