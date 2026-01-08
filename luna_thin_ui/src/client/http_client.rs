use crate::client::config::ServerConfig;
use reqwest::multipart;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAttachment {
    pub file_id: String,
    pub file_name: String,
    pub mime_type: String,
    pub file_size: u64,
}

#[derive(Clone)]
pub struct FileClient {
    config: ServerConfig,
    client: reqwest::Client,
}

impl FileClient {
    pub fn new(config: ServerConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    fn base_url(&self) -> String {
        self.config.http_uri()
    }

    fn headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        if let Ok(val) = reqwest::header::HeaderValue::from_str(&self.config.api_key) {
            headers.insert("x-api-key", val);
        }
        if let Ok(val) = reqwest::header::HeaderValue::from_str(&format!("Bearer {}", self.config.api_key)) {
            headers.insert("authorization", val);
        }
        headers
    }

    pub async fn upload_file<P: AsRef<Path>>(
        &self,
        file_path: P,
    ) -> Result<FileAttachment, Box<dyn std::error::Error + Send + Sync>> {
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

        let file_part = multipart::Part::bytes(file_content)
            .file_name(file_name.clone())
            .mime_str(&mime_type)?;

        let form = multipart::Form::new().part("file", file_part);
        let url = format!("{}/api/attach-file", self.base_url());

        tracing::debug!("Uploading file to: {}", url);

        let response = self
            .client
            .post(&url)
            .headers(self.headers())
            .multipart(form)
            .send()
            .await?;

        if response.status().is_success() {
            let attachment: FileAttachment = response.json().await?;
            Ok(attachment)
        } else {
            let status = response.status();
            let body = response.text().await?;
            Err(format!("File upload failed: {} - {}", status, body).into())
        }
    }

    pub async fn remove_file(&self, file_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/api/attach-file/{}", self.base_url(), file_id);
        tracing::debug!("Removing file: {}", url);

        let response = self
            .client
            .delete(&url)
            .headers(self.headers())
            .send()
            .await?;

        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let body = response.text().await?;
            Err(format!("File removal failed: {} - {}", status, body).into())
        }
    }
}






