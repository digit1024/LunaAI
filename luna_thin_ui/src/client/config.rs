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

    /// Plain HTTP API base (insecure fallback for local dev or when HTTPS fails).
    pub fn http_uri(&self) -> String {
        if self.port == 80 {
            format!("http://{}", self.host)
        } else {
            format!("http://{}:{}", self.host, self.port)
        }
    }

    /// Insecure HTTP API base – alias for [Self::http_uri].
    pub fn http_uri_insecure(&self) -> String {
        self.http_uri()
    }

    /// HTTPS API base – same host/port semantics as [Self::websocket_uri_secure].
    pub fn http_uri_secure(&self) -> String {
        if self.port == 443 {
            format!("https://{}", self.host)
        } else {
            format!("https://{}:{}", self.host, self.port)
        }
    }

    /// REST base aligned with how the WebSocket connection was established.
    pub fn rest_base_for_ws_secure(&self, ws_secure: bool) -> String {
        if ws_secure {
            self.http_uri_secure()
        } else {
            self.http_uri_insecure()
        }
    }

    /// True for loopback / emulator hosts that typically speak plain HTTP.
    pub fn is_local_rest_host(&self) -> bool {
        let h = self.host.to_lowercase();
        h == "127.0.0.1" || h == "localhost"
    }

    /// REST bases to try for static files and uploads (HTTPS first on remote hosts).
    pub fn http_rest_base_uris(&self) -> Vec<String> {
        if self.is_local_rest_host() {
            vec![self.http_uri(), self.http_uri_secure()]
        } else if self.port == 443 {
            // Standard HTTPS port — plain HTTP on :443 will not work behind reverse proxies.
            vec![self.http_uri_secure()]
        } else if self.port == 80 {
            vec![self.http_uri_insecure()]
        } else {
            vec![self.http_uri_secure(), self.http_uri_insecure()]
        }
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

