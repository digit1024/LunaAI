//! Embedded `shell_execute` tool.
//!
//! Behaves like the upstream `mcp-shell-server`: argv-only execution (never a
//! shell string), a basename whitelist with optional regex patterns, an
//! optional directory confinement, a timeout and an output cap. Pipelines using
//! `|` are supported; input/output redirection is not.

use super::validator::CommandValidator;
use super::SHELL_TOOL_NAME;
use crate::config::ShellToolConfig;
use crate::llm::{ToolCall, ToolDefinition, ToolResult};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};

const READ_CHUNK: usize = 8192;

pub struct ShellTool {
    config: ShellToolConfig,
    validator: CommandValidator,
}

impl ShellTool {
    pub fn new(config: ShellToolConfig) -> Self {
        let validator = CommandValidator::new(&config.allow_commands, &config.allow_patterns);
        Self { config, validator }
    }

    pub fn definition(&self) -> ToolDefinition {
        let allowed = self.validator.allowed_commands().join(", ");
        let patterns = self.validator.allowed_patterns().join(", ");
        ToolDefinition {
            name: SHELL_TOOL_NAME.to_string(),
            description: format!(
                "Execute a shell command.\nAllowed commands: {}\nAllowed patterns: {}\n\
                 Default timeout: {}s; maximum timeout: {}s; output cap: {} bytes",
                allowed,
                patterns,
                self.config.default_timeout_secs,
                self.config.max_timeout_secs,
                self.config.output_limit_bytes
            ),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Command and its arguments as array"
                    },
                    "stdin": {
                        "type": "string",
                        "description": "Input to be passed to the command via stdin"
                    },
                    "directory": {
                        "type": "string",
                        "description": "Optional working directory. Omit to use the configured default or the server process CWD."
                    },
                    "timeout": {
                        "type": "integer",
                        "description": "Maximum execution time in seconds; clamped to the configured maximum",
                        "minimum": 1
                    }
                },
                "required": ["command"]
            }),
        }
    }

    /// Parse the argv, stdin, directory and timeout from a tool call.
    fn parse_args(
        &self,
        call: &ToolCall,
    ) -> Result<(Vec<String>, Option<String>, Option<String>, Option<u64>), String> {
        let params = &call.parameters;

        let command_val = params.get("command").ok_or("No command provided")?;
        let arr = command_val
            .as_array()
            .ok_or("'command' must be an array")?;
        let mut argv = Vec::with_capacity(arr.len());
        for v in arr {
            let s = v.as_str().ok_or("'command' must be an array of strings")?;
            argv.push(s.to_string());
        }
        if argv.is_empty() {
            return Err("No command provided".to_string());
        }

        let stdin = params.get("stdin").and_then(|v| v.as_str()).map(str::to_string);
        let directory = params
            .get("directory")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let timeout = params.get("timeout").and_then(|v| v.as_u64());

        Ok((argv, stdin, directory, timeout))
    }

    fn resolve_directory(&self, requested: Option<&str>) -> Result<PathBuf, String> {
        let dir = match requested {
            Some(d) if !d.trim().is_empty() => PathBuf::from(d),
            Some(_) => return Err("Directory cannot be empty".to_string()),
            None => match &self.config.default_directory {
                Some(d) if !d.trim().is_empty() => PathBuf::from(d),
                _ => std::env::current_dir()
                    .map_err(|e| format!("Cannot resolve current directory: {}", e))?,
            },
        };

        if !dir.is_absolute() {
            return Err(format!("Directory must be an absolute path: {}", dir.display()));
        }
        let meta = std::fs::metadata(&dir)
            .map_err(|_| format!("Directory does not exist: {}", dir.display()))?;
        if !meta.is_dir() {
            return Err(format!("Not a directory: {}", dir.display()));
        }

        if !self.config.allowed_directories.is_empty() {
            let canon = dir
                .canonicalize()
                .map_err(|e| format!("Directory is not accessible: {} ({})", dir.display(), e))?;
            let allowed = self.config.allowed_directories.iter().any(|a| {
                PathBuf::from(a)
                    .canonicalize()
                    .map(|base| canon.starts_with(&base))
                    .unwrap_or(false)
            });
            if !allowed {
                return Err(format!("Directory not allowed by policy: {}", dir.display()));
            }
        }

        Ok(dir)
    }

    fn effective_timeout(&self, requested: Option<u64>) -> Result<Duration, String> {
        let max = self.config.max_timeout_secs;
        let secs = match requested {
            Some(0) => return Err("'timeout' must be greater than 0".to_string()),
            Some(t) => t.min(max),
            None => self.config.default_timeout_secs.min(max),
        };
        Ok(Duration::from_secs(secs.max(1)))
    }

    async fn run(&self, call: &ToolCall) -> Result<String, String> {
        let (argv, stdin, directory, timeout) = self.parse_args(call)?;
        let dir = self.resolve_directory(directory.as_deref())?;
        let timeout = self.effective_timeout(timeout)?;

        let cleaned = preprocess(argv);
        if cleaned.iter().any(|t| t == "|") {
            let cmds = split_pipeline(&cleaned);
            if cmds.is_empty() {
                return Err("Empty command before pipe operator".to_string());
            }
            for cmd in &cmds {
                self.validator.validate_command(cmd)?;
            }
            return self.run_pipeline(cmds, &dir, stdin.as_deref(), timeout).await;
        }

        for token in &cleaned {
            CommandValidator::validate_no_shell_operators(token)?;
        }
        self.validator.validate_command(&cleaned)?;
        self.run_single(&cleaned, &dir, stdin.as_deref(), timeout).await
    }

    async fn run_single(
        &self,
        argv: &[String],
        dir: &Path,
        stdin: Option<&str>,
        timeout: Duration,
    ) -> Result<String, String> {
        let mut cmd = Command::new(&argv[0]);
        cmd.args(&argv[1..])
            .current_dir(dir)
            .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to execute '{}': {}", argv[0], e))?;

        if let Some(input) = stdin {
            if let Some(mut si) = child.stdin.take() {
                let bytes = input.as_bytes().to_vec();
                tokio::spawn(async move {
                    let _ = si.write_all(&bytes).await;
                });
            }
        }

        let cap = self.config.output_limit_bytes;
        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
        let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

        let work = async {
            let ((so, sto), (se, ste)) =
                tokio::join!(read_capped(stdout, cap), read_capped(stderr, cap));
            let _ = child.wait().await;
            (so, se, sto || ste)
        };

        match tokio::time::timeout(timeout, work).await {
            Err(_) => {
                let _ = child.start_kill();
                Err(format!("Command timed out after {} seconds", timeout.as_secs()))
            }
            Ok((so, se, truncated)) => Ok(assemble_output(&so, &se, truncated, cap)),
        }
    }

    async fn run_pipeline(
        &self,
        cmds: Vec<Vec<String>>,
        dir: &Path,
        stdin: Option<&str>,
        timeout: Duration,
    ) -> Result<String, String> {
        let mut children: Vec<Child> = Vec::with_capacity(cmds.len());
        for (i, argv) in cmds.iter().enumerate() {
            let mut c = Command::new(&argv[0]);
            c.args(&argv[1..])
                .current_dir(dir)
                .kill_on_drop(true)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            c.stdin(if i == 0 {
                if stdin.is_some() {
                    Stdio::piped()
                } else {
                    Stdio::null()
                }
            } else {
                Stdio::piped()
            });
            let child = c
                .spawn()
                .map_err(|e| format!("Failed to execute '{}': {}", argv[0], e))?;
            children.push(child);
        }

        if let Some(input) = stdin {
            if let Some(mut si) = children[0].stdin.take() {
                let bytes = input.as_bytes().to_vec();
                tokio::spawn(async move {
                    let _ = si.write_all(&bytes).await;
                });
            }
        }

        let last = children.len() - 1;
        for i in 0..last {
            let mut out = match children[i].stdout.take() {
                Some(o) => o,
                None => continue,
            };
            let mut inp = match children[i + 1].stdin.take() {
                Some(s) => s,
                None => continue,
            };
            tokio::spawn(async move {
                let _ = tokio::io::copy(&mut out, &mut inp).await;
                let _ = inp.shutdown().await;
            });
        }

        for child in children.iter_mut().take(last) {
            if let Some(se) = child.stderr.take() {
                tokio::spawn(async move {
                    let _ = read_capped(se, 64 * 1024).await;
                });
            }
        }

        let cap = self.config.output_limit_bytes;
        let last_stdout = children[last].stdout.take().ok_or("Failed to capture stdout")?;
        let last_stderr = children[last].stderr.take().ok_or("Failed to capture stderr")?;

        let work = async {
            let ((so, sto), (se, ste)) =
                tokio::join!(read_capped(last_stdout, cap), read_capped(last_stderr, cap));
            for child in children.iter_mut() {
                let _ = child.wait().await;
            }
            (so, se, sto || ste)
        };

        match tokio::time::timeout(timeout, work).await {
            Err(_) => {
                for child in children.iter_mut() {
                    let _ = child.start_kill();
                }
                Err(format!("Command timed out after {} seconds", timeout.as_secs()))
            }
            Ok((so, se, truncated)) => Ok(assemble_output(&so, &se, truncated, cap)),
        }
    }
}

#[async_trait]
impl super::EmbeddedTool for ShellTool {
    fn definitions(&self) -> Vec<ToolDefinition> {
        vec![self.definition()]
    }

    fn names(&self) -> Vec<&'static str> {
        vec![SHELL_TOOL_NAME]
    }

    async fn execute(&self, call: &ToolCall) -> ToolResult {
        match self.run(call).await {
            Ok(content) => ToolResult {
                content: if content.is_empty() {
                    "(no output)".to_string()
                } else {
                    content
                },
                is_error: false,
            },
            Err(message) => ToolResult {
                content: message,
                is_error: true,
            },
        }
    }
}

/// Expand tokens that embed pipe characters, mirroring upstream preprocessing.
fn preprocess(tokens: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for token in tokens {
        if token == "||" || token == "&&" || token == ";" {
            out.push(token);
        } else if token.contains('|') && token != "|" {
            for part in token.split('|') {
                let p = part.trim();
                if !p.is_empty() {
                    out.push(p.to_string());
                }
            }
            out.push("|".to_string());
        } else if !token.is_empty() {
            out.push(token);
        }
    }
    out
}

fn split_pipeline(tokens: &[String]) -> Vec<Vec<String>> {
    let mut commands: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for token in tokens {
        if token == "|" {
            if !current.is_empty() {
                commands.push(std::mem::take(&mut current));
            }
        } else {
            current.push(token.clone());
        }
    }
    if !current.is_empty() {
        commands.push(current);
    }
    commands
}

/// Read an async stream, stopping once `cap` bytes have been captured.
async fn read_capped<R: AsyncRead + Unpin>(mut r: R, cap: usize) -> (Vec<u8>, bool) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; READ_CHUNK];
    let mut truncated = false;
    loop {
        match r.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => {
                if buf.len() >= cap {
                    truncated = true;
                    break;
                }
                let take = n.min(cap - buf.len());
                buf.extend_from_slice(&chunk[..take]);
                if take < n {
                    truncated = true;
                    break;
                }
            }
            Err(_) => break,
        }
    }
    (buf, truncated)
}

fn assemble_output(stdout: &[u8], stderr: &[u8], truncated: bool, cap: usize) -> String {
    let mut out = String::new();
    if !stdout.is_empty() {
        out.push_str(&String::from_utf8_lossy(stdout));
    }
    if !stderr.is_empty() {
        let st = String::from_utf8_lossy(stderr);
        if !st.contains("cannot set terminal process group") {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&st);
        }
    }
    if truncated {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&format!("[output truncated at {} bytes]", cap));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::EmbeddedTool;

    fn tool(allow: &[&str]) -> ShellTool {
        ShellTool::new(ShellToolConfig {
            enabled: true,
            allow_commands: allow.iter().map(|s| s.to_string()).collect(),
            ..ShellToolConfig::default()
        })
    }

    fn call(name: &str, params: serde_json::Value) -> ToolCall {
        ToolCall {
            id: "t".to_string(),
            name: name.to_string(),
            parameters: params,
        }
    }

    #[tokio::test]
    async fn executes_whitelisted_command() {
        let t = tool(&["echo"]);
        let r = t
            .execute(&call(SHELL_TOOL_NAME, serde_json::json!({"command": ["echo", "hello"]})))
            .await;
        assert!(!r.is_error, "{}", r.content);
        assert!(r.content.contains("hello"));
    }

    #[tokio::test]
    async fn rejects_non_whitelisted_command() {
        let t = tool(&["echo"]);
        let r = t
            .execute(&call(SHELL_TOOL_NAME, serde_json::json!({"command": ["ls"]})))
            .await;
        assert!(r.is_error);
        assert!(r.content.contains("not allowed"));
    }

    #[tokio::test]
    async fn rejects_empty_whitelist() {
        let t = ShellTool::new(ShellToolConfig::default());
        let r = t
            .execute(&call(SHELL_TOOL_NAME, serde_json::json!({"command": ["echo", "hi"]})))
            .await;
        assert!(r.is_error);
        assert!(r.content.contains("No commands are allowed"));
    }

    #[tokio::test]
    async fn caps_output() {
        let mut cfg = ShellToolConfig {
            enabled: true,
            allow_commands: vec!["printf".to_string()],
            ..ShellToolConfig::default()
        };
        cfg.output_limit_bytes = 10;
        let t = ShellTool::new(cfg);
        let r = t
            .execute(&call(
                SHELL_TOOL_NAME,
                serde_json::json!({"command": ["printf", "0123456789ABCDEFGHIJ"]}),
            ))
            .await;
        assert!(!r.is_error, "{}", r.content);
        assert!(r.content.contains("truncated"));
    }

    #[tokio::test]
    async fn times_out() {
        let t = tool(&["sleep"]);
        let r = t
            .execute(&call(
                SHELL_TOOL_NAME,
                serde_json::json!({"command": ["sleep", "5"], "timeout": 1}),
            ))
            .await;
        assert!(r.is_error);
        assert!(r.content.contains("timed out"));
    }

    #[tokio::test]
    async fn rejects_relative_directory() {
        let t = tool(&["echo"]);
        let r = t
            .execute(&call(
                SHELL_TOOL_NAME,
                serde_json::json!({"command": ["echo", "hi"], "directory": "relative/path"}),
            ))
            .await;
        assert!(r.is_error);
        assert!(r.content.contains("absolute path"));
    }

    #[tokio::test]
    async fn pipeline_connects_commands() {
        let t = tool(&["echo", "grep"]);
        let r = t
            .execute(&call(
                SHELL_TOOL_NAME,
                serde_json::json!({"command": ["echo", "hello world", "|", "grep", "hello"]}),
            ))
            .await;
        assert!(!r.is_error, "{}", r.content);
        assert!(r.content.contains("hello"));
    }
}
