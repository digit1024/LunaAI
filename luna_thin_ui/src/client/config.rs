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

    /// Returns secure WebSocket URI (wss://) – WebSocket route is /ws
    pub fn websocket_uri_secure(&self) -> String {
        if self.port == 443 {
            format!("wss://{}/ws", self.host)
        } else {
            format!("wss://{}:{}/ws", self.host, self.port)
        }
    }

    /// Returns insecure WebSocket URI (ws://) – WebSocket route is /ws
    pub fn websocket_uri_insecure(&self) -> String {
        format!("ws://{}:{}/ws", self.host, self.port)
    }

    /// Returns secure URI by default (for backward compatibility)
    pub fn websocket_uri(&self) -> String {
        self.websocket_uri_secure()
    }

    /// HTTP API (attach-file, mcp-servers) – same port as WebSocket
    pub fn http_uri(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
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

