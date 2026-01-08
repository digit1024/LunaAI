use dirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub api_key: String,
}

impl ServerConfig {
    pub fn new(host: String, port: u16, api_key: String) -> Self {
        Self { host, port, api_key }
    }

    pub fn websocket_uri(&self) -> String {
        // No path - server listens on root (same as mobile app)
        format!("ws://{}:{}/", self.host, self.port)
    }

    pub fn http_uri(&self) -> String {
        // HTTP server runs on port + 1
        format!("http://{}:{}", self.host, self.port + 1)
    }

    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("luna_thin_ui")
            .join("server_config.toml")
    }

    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let path = Self::config_path();
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            let config: ServerConfig = toml::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self::new("localhost".to_string(), 8080, String::new())
    }
}

