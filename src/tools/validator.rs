//! Argv command validation and default-deny policy.
//!
//! Faithful Rust port of the upstream `mcp-shell-server` `command_validator.py`:
//! commands are argv arrays (never a shell string), matched against a basename
//! whitelist / optional regex patterns, with a built-in default-deny list.

use regex::Regex;
use std::collections::HashSet;
use std::path::Path;

/// Characters that may never appear in a command name (shell metacharacters).
const SHELL_METACHAR: &[char] = &[
    ' ', '\t', ';', '&', '|', '<', '>', '`', '\n', '\r',
];

/// Interpreters and indirection commands rejected regardless of whitelist.
pub const DANGEROUS_COMMANDS: &[&str] = &[
    "sh", "bash", "zsh", "fish", "csh", "ksh", "python", "python3", "perl", "ruby", "node",
    "php", "lua", "env", "xargs",
];

pub struct CommandValidator {
    allowed: HashSet<String>,
    patterns: Vec<Regex>,
}

impl CommandValidator {
    /// Build a validator from the configured whitelist. Invalid patterns are
    /// logged and skipped rather than disabling the tool entirely.
    pub fn new(allow_commands: &[String], allow_patterns: &[String]) -> Self {
        let allowed = allow_commands
            .iter()
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty())
            .collect();

        let mut patterns = Vec::new();
        for raw in allow_patterns {
            let p = raw.trim();
            if p.is_empty() {
                continue;
            }
            if p.chars().any(|c| SHELL_METACHAR.contains(&c)) {
                tracing::warn!(pattern = %p, "Unsafe shell allow_pattern (contains shell metacharacter); skipped");
                continue;
            }
            match Regex::new(p) {
                Ok(re) => patterns.push(re),
                Err(e) => tracing::warn!(pattern = %p, error = %e, "Invalid shell allow_pattern; skipped"),
            }
        }

        Self { allowed, patterns }
    }

    /// True when no command can ever be allowed.
    pub fn is_empty(&self) -> bool {
        self.allowed.is_empty() && self.patterns.is_empty()
    }

    pub fn allowed_commands(&self) -> Vec<String> {
        let mut v: Vec<String> = self.allowed.iter().cloned().collect();
        v.sort();
        v
    }

    pub fn allowed_patterns(&self) -> Vec<String> {
        self.patterns.iter().map(|p| p.as_str().to_string()).collect()
    }

    fn validate_command_name_form(command: &str) -> Result<String, String> {
        let cmd = command.trim();
        if cmd.is_empty() {
            return Err("Empty command".to_string());
        }
        if cmd.chars().any(|c| SHELL_METACHAR.contains(&c)) {
            return Err(format!("Unsafe command name: {}", cmd));
        }
        Ok(cmd.to_string())
    }

    fn is_command_allowed(&self, command: &str) -> bool {
        if self.allowed.contains(command) {
            return true;
        }
        self.patterns.iter().any(|p| {
            p.find(command)
                .map(|m| m.start() == 0 && m.end() == command.len())
                .unwrap_or(false)
        })
    }

    /// Reject a token that is, or contains, a shell control operator.
    pub fn validate_no_shell_operators(cmd: &str) -> Result<(), String> {
        if cmd == ";" || cmd == "&&" || cmd == "||" || cmd == "|" {
            return Err(format!("Unexpected shell operator: {}", cmd));
        }
        if cmd.contains(';') || cmd.contains("&&") || cmd.contains("||") || cmd.contains('|')
            || cmd.contains('`') || cmd.contains('\n') || cmd.contains('\r')
        {
            return Err(format!("Unexpected shell operator: {}", cmd));
        }
        Ok(())
    }

    fn git_config_values(args: &[String]) -> Vec<String> {
        let mut values = Vec::new();
        let mut index = 0;
        while index < args.len() {
            let arg = &args[index];
            if arg == "-c" {
                if index + 1 < args.len() {
                    values.push(args[index + 1].clone());
                }
                index += 2;
                continue;
            }
            if arg.starts_with("-c") && arg != "-c" {
                values.push(arg[2..].to_string());
            }
            index += 1;
        }
        values
    }

    fn is_git_dangerous_config(config_value: &str) -> bool {
        let compact: String = config_value.chars().filter(|c| !c.is_whitespace()).collect();
        let alias_re = Regex::new(r"(?i)(?:^|[.\s])alias\.[^=\s]+\s*=\s*!").unwrap();
        let core_re = Regex::new(r"(?i)^core\.(pager|sshcommand)=").unwrap();
        alias_re.is_match(config_value) || core_re.is_match(&compact)
    }

    fn has_option_value<F: Fn(&str) -> bool>(args: &[String], option: &str, pred: F) -> bool {
        for (index, arg) in args.iter().enumerate() {
            if arg == option && index + 1 < args.len() && pred(&args[index + 1]) {
                return true;
            }
            if let Some(v) = arg.strip_prefix(&format!("{}=", option)) {
                if pred(v) {
                    return true;
                }
            }
        }
        false
    }

    fn has_any_option(args: &[String], options: &[&str]) -> bool {
        args.iter().any(|arg| {
            options.iter().any(|o| arg == o || arg.starts_with(&format!("{}=", o)))
        })
    }

    fn has_short_option_prefix(args: &[String], option: &str) -> bool {
        args.iter().any(|arg| arg == option || arg.starts_with(option))
    }

    fn validate_default_argument_policy(command: &[String]) -> Result<(), String> {
        let cmd = Path::new(&command[0])
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| command[0].clone());
        let args = &command[1..];

        if DANGEROUS_COMMANDS.contains(&cmd.as_str()) {
            return Err(format!("Command rejected by default security policy: {}", cmd));
        }

        if cmd == "find" && args.iter().any(|a| a == "-exec" || a == "-execdir") {
            return Err("Command rejected by default security policy: find -exec".to_string());
        }

        if cmd == "awk"
            && args.iter().any(|a| {
                let compact: String = a.chars().filter(|c| !c.is_whitespace()).collect();
                compact.contains("system(") || compact.contains('|')
            })
        {
            return Err(
                "Command rejected by default security policy: awk command execution".to_string(),
            );
        }

        if cmd == "tar"
            && (Self::has_option_value(args, "--checkpoint-action", |v| v.starts_with("exec="))
                || Self::has_any_option(
                    args,
                    &["--to-command", "--use-compress-program", "--rsh-command"],
                )
                || Self::has_short_option_prefix(args, "-I"))
        {
            return Err(
                "Command rejected by default security policy: tar command execution option"
                    .to_string(),
            );
        }

        if cmd == "git"
            && Self::git_config_values(args)
                .iter()
                .any(|v| Self::is_git_dangerous_config(v))
        {
            return Err(
                "Command rejected by default security policy: git command execution config"
                    .to_string(),
            );
        }

        Ok(())
    }

    /// Validate a full argv against policy.
    pub fn validate_command(&self, command: &[String]) -> Result<(), String> {
        if command.is_empty() {
            return Err("Empty command".to_string());
        }
        if self.is_empty() {
            return Err(
                "No commands are allowed. Set allow_commands in [embedded_tools.shell]."
                    .to_string(),
            );
        }

        let cleaned = Self::validate_command_name_form(&command[0])?;
        let mut full = vec![cleaned.clone()];
        full.extend_from_slice(&command[1..]);
        Self::validate_default_argument_policy(&full)?;

        if !self.is_command_allowed(&cleaned) {
            return Err(format!("Command not allowed: {}", cleaned));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validator(cmds: &[&str]) -> CommandValidator {
        let owned: Vec<String> = cmds.iter().map(|s| s.to_string()).collect();
        CommandValidator::new(&owned, &[])
    }

    #[test]
    fn allows_whitelisted_command() {
        let v = validator(&["ls", "cat"]);
        assert!(v.validate_command(&["ls".into(), "-la".into()]).is_ok());
    }

    #[test]
    fn rejects_non_whitelisted_command() {
        let v = validator(&["ls"]);
        assert!(v.validate_command(&["rm".into(), "-rf".into(), "/".into()]).is_err());
    }

    #[test]
    fn empty_whitelist_denies_all() {
        let v = validator(&[]);
        assert!(v.is_empty());
        assert!(v.validate_command(&["ls".into()]).is_err());
    }

    #[test]
    fn rejects_dangerous_interpreter() {
        let v = validator(&["python3"]);
        let err = v.validate_command(&["python3".into(), "-c".into()]).unwrap_err();
        assert!(err.contains("default security policy"));
    }

    #[test]
    fn rejects_shell_operators() {
        assert!(CommandValidator::validate_no_shell_operators(";").is_err());
        assert!(CommandValidator::validate_no_shell_operators("a&&b").is_err());
        assert!(CommandValidator::validate_no_shell_operators("a|b").is_err());
        assert!(CommandValidator::validate_no_shell_operators("ok").is_ok());
    }

    #[test]
    fn rejects_find_exec() {
        let v = validator(&["find"]);
        let err = v
            .validate_command(&["find".into(), ".".into(), "-exec".into(), "rm".into()])
            .unwrap_err();
        assert!(err.contains("find -exec"));
    }

    #[test]
    fn rejects_metachars_in_command_name() {
        let v = validator(&["ls"]);
        assert!(v.validate_command(&["ls;rm".into()]).is_err());
    }

    #[test]
    fn pattern_fullmatch() {
        let v = CommandValidator::new(&[], &["ls.*".to_string()]);
        assert!(v.validate_command(&["ls".into()]).is_ok());
        assert!(v.validate_command(&["lsblk".into()]).is_ok());
        // fullmatch: substring must not satisfy
        assert!(v.validate_command(&["xls".into()]).is_err());
    }

    #[test]
    fn rejects_dangerous_git_config() {
        let v = validator(&["git"]);
        let err = v
            .validate_command(&[
                "git".into(),
                "-c".into(),
                "alias.x=!sh".into(),
                "x".into(),
            ])
            .unwrap_err();
        assert!(err.contains("git command execution config"));
    }
}
