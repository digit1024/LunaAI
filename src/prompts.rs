use anyhow::Result;
use log::{debug, warn};
use serde::{Deserialize, Serialize};

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
            system_prompt_file: Some(data_dir.join("system_prompt.md").to_string_lossy().to_string()),
            user_prompt_file: Some(data_dir.join("user_prompt.md").to_string_lossy().to_string()),
        }
    }
}

#[derive(Clone)]
pub struct PromptManager {
    system_prompt: Option<String>,
}

impl PromptManager {
    pub fn load_from_config(config: &PromptConfig) -> Result<Self> {
        let system_prompt = if let Some(path) = &config.system_prompt_file {
            match std::fs::read_to_string(path) {
                Ok(content) => {
                    debug!("✅ Loaded system prompt from: {}", path);
                    Some(content.trim().to_string())
                },
                Err(e) => {
                    warn!("⚠️ Failed to load system prompt from {}: {}", path, e);
                    None
                }
            }
        } else {
            debug!("No system prompt file configured");
            None
        };

        Ok(Self {
            system_prompt,
        })
    }

    pub fn get_system_prompt(&self) -> Option<&str> {
        self.system_prompt.as_deref()
    }

}
