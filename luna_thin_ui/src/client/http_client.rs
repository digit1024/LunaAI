use crate::client::config::ServerConfig;
use reqwest::multipart;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

/// Response from POST /api/attach-file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadResult {
    pub uid: String,
    pub original_name: String,
    /// Full path on server (config_dir/uploads/{uid}.{ext})
    pub stored_path: String,
}

#[derive(Clone)]
pub struct FileClient {
    config: ServerConfig,
    client: reqwest::Client,
    rest_base: String,
}

impl FileClient {
    pub fn new(config: ServerConfig) -> Self {
        let rest_base = config
            .http_rest_base_uris()
            .into_iter()
            .next()
            .unwrap_or_else(|| config.http_uri_secure());
        Self::with_rest_base(config, rest_base)
    }

    pub fn with_rest_base(config: ServerConfig, rest_base: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            config,
            client,
            rest_base,
        }
    }

    fn request_bases(&self) -> Vec<String> {
        let mut bases = vec![self.rest_base.clone()];
        for base in self.config.http_rest_base_uris() {
            if !bases.iter().any(|b| b == &base) {
                bases.push(base);
            }
        }
        bases
    }

    fn headers(&self) -> reqwest::header::HeaderMap {
        headers_for(&self.config)
    }

    pub async fn upload_file<P: AsRef<Path>>(
        &self,
        file_path: P,
        conversation_id: Option<&str>,
    ) -> Result<UploadResult, Box<dyn std::error::Error + Send + Sync>> {
        let path = file_path.as_ref();
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();

        let file_content = tokio::fs::read(path).await?;
        let mime_type = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();

        let mut last_err: Option<Box<dyn std::error::Error + Send + Sync>> = None;
        for base in self.request_bases() {
            let file_part = multipart::Part::bytes(file_content.clone())
                .file_name(file_name.clone())
                .mime_str(&mime_type)?;

            let mut form = multipart::Form::new().part("file", file_part);
            if let Some(cid) = conversation_id {
                if !cid.is_empty() {
                    form = form.text("conversation_id", cid.to_string());
                }
            }

            let url = format!("{}/api/attach-file", base.trim_end_matches('/'));
            tracing::debug!("Uploading file to: {}", url);

            let response = match self
                .client
                .post(&url)
                .headers(self.headers())
                .multipart(form)
                .send()
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    last_err = Some(e.into());
                    continue;
                }
            };

            if response.status().is_success() {
                let result: UploadResult = response.json().await?;
                return Ok(result);
            }

            let status = response.status();
            let body = response.text().await?;
            return Err(format!("File upload failed: {} - {}", status, body).into());
        }

        Err(last_err
            .unwrap_or_else(|| "File upload failed: no reachable REST base".into()))
    }

    pub async fn remove_file(&self, uid: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut last_err: Option<Box<dyn std::error::Error + Send + Sync>> = None;
        for base in self.request_bases() {
            let url = format!("{}/api/attach-file/{}", base.trim_end_matches('/'), uid);
            tracing::debug!("Removing file: {}", url);

            let response = match self
                .client
                .delete(&url)
                .headers(self.headers())
                .send()
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    last_err = Some(e.into());
                    continue;
                }
            };

            if response.status().is_success() {
                return Ok(());
            }

            let status = response.status();
            let body = response.text().await?;
            return Err(format!("File removal failed: {} - {}", status, body).into());
        }

        Err(last_err
            .unwrap_or_else(|| "File removal failed: no reachable REST base".into()))
    }

    pub async fn list_mcp_servers(
        &self,
    ) -> Result<crate::server::dto::MCPServersResponse, Box<dyn std::error::Error + Send + Sync>>
    {
        let mut last_err: Option<Box<dyn std::error::Error + Send + Sync>> = None;
        for base in self.request_bases() {
            let url = format!("{}/api/mcp-servers", base.trim_end_matches('/'));
            tracing::debug!("Fetching MCP servers from: {}", url);

            let response = match self
                .client
                .get(&url)
                .headers(self.headers())
                .send()
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    tracing::warn!(url = %url, error = %e, "MCP servers request transport error");
                    last_err = Some(e.into());
                    continue;
                }
            };

            if response.status().is_success() {
                let servers: crate::server::dto::MCPServersResponse = response.json().await?;
                return Ok(servers);
            }

            let status = response.status();
            let body = response.text().await?;
            return Err(format!("Failed to fetch MCP servers: {} - {}", status, body).into());
        }

        Err(last_err.unwrap_or_else(|| {
            "Failed to fetch MCP servers: no reachable REST base".into()
        }))
    }
}

fn headers_for(config: &ServerConfig) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    if let Ok(val) = reqwest::header::HeaderValue::from_str(&config.api_key) {
        headers.insert("x-api-key", val);
    }
    if let Ok(val) =
        reqwest::header::HeaderValue::from_str(&format!("Bearer {}", config.api_key))
    {
        headers.insert("authorization", val);
    }
    headers
}

pub fn merge_connection_warnings(
    ws_warning: Option<String>,
    http_warning: Option<String>,
) -> Option<String> {
    match (ws_warning, http_warning) {
        (None, None) => None,
        (Some(w), None) => Some(w),
        (None, Some(h)) => Some(h),
        (Some(w), Some(h)) => Some(format!("{w}\n\n{h}")),
    }
}
