//! Embedded (compiled-in) tools that used to be external MCP servers.
//!
//! Each tool implements [`EmbeddedTool`]; [`EmbeddedToolRegistry`] is built from
//! `[embedded_tools]` config and wired into the agentic loop. Names are kept
//! identical to the MCP servers they replace (`shell_execute`, `get_current_time`,
//! `convert_time`) so existing prompts/skills keep working.

pub mod shell;
pub mod time;
pub mod time_format;
pub mod validator;

use crate::config::EmbeddedToolsConfig;
use crate::llm::{ToolCall, ToolDefinition, ToolResult};
use async_trait::async_trait;

/// Tool name exposed by the embedded shell tool.
pub const SHELL_TOOL_NAME: &str = "shell_execute";
/// Tool name exposed by the embedded time tool (current time).
pub const TIME_GET_TOOL_NAME: &str = "get_current_time";
/// Tool name exposed by the embedded time tool (timezone conversion).
pub const TIME_CONVERT_TOOL_NAME: &str = "convert_time";

/// A compiled-in tool. Implementations own their config and produce their own
/// tool definitions and execution results.
#[async_trait]
pub trait EmbeddedTool: Send + Sync {
    /// Definitions advertised to the model (only called for enabled tools).
    fn definitions(&self) -> Vec<ToolDefinition>;
    /// All tool names this implementation can execute.
    fn names(&self) -> Vec<&'static str>;
    /// Execute a tool call. Only called for names returned by [`Self::names`].
    async fn execute(&self, call: &ToolCall) -> ToolResult;
}

/// Registry of enabled embedded tools, built from `[embedded_tools]`.
pub struct EmbeddedToolRegistry {
    tools: Vec<Box<dyn EmbeddedTool>>,
}

impl EmbeddedToolRegistry {
    /// Build the registry, including only tools whose `enabled` flag is true.
    pub fn from_config(cfg: &EmbeddedToolsConfig) -> Self {
        let mut tools: Vec<Box<dyn EmbeddedTool>> = Vec::new();
        if cfg.shell.enabled {
            tools.push(Box::new(shell::ShellTool::new(cfg.shell.clone())));
        }
        if cfg.time.enabled {
            tools.push(Box::new(time::TimeTool::new(cfg.time.clone())));
        }
        Self { tools }
    }

    /// Definitions for all enabled embedded tools.
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.iter().flat_map(|t| t.definitions()).collect()
    }

    /// Names of all enabled embedded tools (policy candidate set).
    pub fn names(&self) -> Vec<String> {
        self.tools
            .iter()
            .flat_map(|t| t.names().into_iter().map(str::to_string))
            .collect()
    }

    /// Dispatch a call to the owning embedded tool, or `None` if not ours.
    pub async fn dispatch(&self, call: &ToolCall) -> Option<ToolResult> {
        for tool in &self.tools {
            if tool.names().contains(&call.name.as_str()) {
                return Some(tool.execute(call).await);
            }
        }
        None
    }
}

/// Convenience: names of enabled embedded tools for a config (used by policy).
pub fn enabled_tool_names(cfg: &EmbeddedToolsConfig) -> Vec<String> {
    EmbeddedToolRegistry::from_config(cfg).names()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::llm::ToolCall;

    fn call(name: &str, params: serde_json::Value) -> ToolCall {
        ToolCall {
            id: "t".to_string(),
            name: name.to_string(),
            parameters: params,
        }
    }

    #[test]
    fn parses_embedded_tools_from_toml() {
        let dir = std::env::temp_dir().join(format!("luna_embedded_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            r#"
default = "p"
[profiles.p]
model_preset = "m"
tools_policy = "default"
[model_presets.m]
backend = "openai"
model = "x"
endpoint = "http://localhost"
api_key = "k"
[embedded_tools.shell]
enabled = true
allow_commands = ["echo"]
[embedded_tools.time]
enabled = false
"#,
        )
        .unwrap();

        let cfg = AppConfig::load_from_path(Some(&path)).unwrap();
        assert!(cfg.embedded_tools.shell.enabled);
        assert_eq!(cfg.embedded_tools.shell.allow_commands, vec!["echo".to_string()]);
        assert_eq!(cfg.embedded_tools.shell.max_timeout_secs, 300); // default fills in
        assert!(!cfg.embedded_tools.time.enabled);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn registry_dispatches_shell_and_time() {
        let mut cfg = EmbeddedToolsConfig::default();
        cfg.shell.allow_commands = vec!["echo".to_string()];

        let registry = EmbeddedToolRegistry::from_config(&cfg);
        assert!(registry.names().contains(&SHELL_TOOL_NAME.to_string()));
        assert!(registry.names().contains(&TIME_GET_TOOL_NAME.to_string()));

        let shell = registry
            .dispatch(&call(SHELL_TOOL_NAME, serde_json::json!({"command": ["echo", "hi"]})))
            .await
            .expect("shell tool dispatched");
        assert!(!shell.is_error, "{}", shell.content);
        assert!(shell.content.contains("hi"));

        let time = registry
            .dispatch(&call(
                TIME_GET_TOOL_NAME,
                serde_json::json!({"timezone": "UTC"}),
            ))
            .await
            .expect("time tool dispatched");
        assert!(!time.is_error, "{}", time.content);

        assert!(registry.dispatch(&call("nope", serde_json::json!({}))).await.is_none());
    }
}
