//! Rig-based conversation engine.
//!
//! LLM orchestration via [rig-core](https://crates.io/crates/rig-core). Rig is the default
//! and only engine. Supports OpenAI backend, streaming, and MCP tools via `rig_tools`.
//! Wire protocol (ServerEvent) is preserved for thin clients and mobile app.

mod adapters;
mod pipeline;

pub use adapters::luna_messages_to_rig_history;
pub use pipeline::{run_turn_streaming, RigConversationContext, StreamChunk};
