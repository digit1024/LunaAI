use anyhow::Result;
use tracing::{debug, warn};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PromptConfig {
    pub system_prompt_file: Option<String>,
    pub user_prompt_file: Option<String>,
}

impl Default for PromptConfig {
    fn default() -> Self {
        // Default to data directory alongside config and database
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("cosmic_llm");

        Self {
            system_prompt_file: Some(
                data_dir
                    .join("system_prompt.md")
                    .to_string_lossy()
                    .to_string(),
            ),
            user_prompt_file: Some(
                data_dir
                    .join("user_prompt.md")
                    .to_string_lossy()
                    .to_string(),
            ),
        }
    }
}

#[derive(Clone)]
pub struct PromptManager {
    pub(crate) system_prompt: Option<String>,
}

impl PromptManager {
    pub fn load_from_config(config: &PromptConfig) -> Result<Self> {
        let system_prompt = if let Some(path) = &config.system_prompt_file {
            match std::fs::read_to_string(path) {
                Ok(content) => {
                    debug!(path = %path, "Loaded system prompt");
                    Some(content.trim().to_string())
                }
                Err(e) => {
                    warn!(path = %path, error = %e, "Failed to load system prompt");
                    None
                }
            }
        } else {
            debug!("No system prompt file configured");
            None
        };

        Ok(Self { system_prompt })
    }

    pub fn get_system_prompt(&self) -> Option<&str> {
        self.system_prompt.as_deref()
    }

    pub fn load_profile_prompt(&self, path: &str) -> Result<String, ProfilePromptError> {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                debug!(path = %path, "Loaded profile prompt");
                Ok(content.trim().to_string())
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    warn!(path = %path, "Profile prompt not found");
                    Err(ProfilePromptError::NotFound(path.to_string()))
                } else {
                    warn!(path = %path, error = %e, "Failed to load profile prompt");
                    Err(ProfilePromptError::IoError {
                        path: path.to_string(),
                        error: e.to_string(),
                    })
                }
            }
        }
    }
}

#[derive(Debug)]
pub enum ProfilePromptError {
    NotFound(String),
    IoError { path: String, error: String },
}

impl ProfilePromptError {
    pub fn path(&self) -> &str {
        match self {
            ProfilePromptError::NotFound(path) => path,
            ProfilePromptError::IoError { path, .. } => path,
        }
    }
}

impl fmt::Display for ProfilePromptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProfilePromptError::NotFound(path) => write!(f, "profile prompt not found: {}", path),
            ProfilePromptError::IoError { path, error } => {
                write!(f, "failed to load profile prompt from {}: {}", path, error)
            }
        }
    }
}

impl std::error::Error for ProfilePromptError {}
