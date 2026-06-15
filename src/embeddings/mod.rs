//! Embedding provider for long-term memory vector search.
//!
//! Supports OpenAI-compatible embeddings APIs.

use crate::config::EmbeddingConfig;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use std::sync::Arc;

/// Provider that embeds text into vectors for semantic search.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Embed a single text string. Returns a vector of dimension matching the configured model.
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
}

/// OpenAI-compatible embeddings API client.
pub struct OpenAiEmbeddingProvider {
    client: reqwest::Client,
    endpoint: String,
    model: String,
    dimensions: usize,
    api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

impl OpenAiEmbeddingProvider {
    pub fn new(config: &EmbeddingConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client for embeddings")?;

        let api_key = config
            .api_key
            .clone()
            .or_else(|| std::env::var("OPENAI_API_KEY").ok());

        Ok(Self {
            client,
            endpoint: config.endpoint.clone(),
            model: config.model.clone(),
            dimensions: config.dimensions,
            api_key,
        })
    }

    pub fn from_config(config: &EmbeddingConfig) -> Result<Arc<dyn EmbeddingProvider>> {
        Ok(Arc::new(Self::new(config)?))
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAiEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut body = serde_json::json!({
            "input": text,
            "model": self.model,
            "encoding_format": "float",
        });
        // text-embedding-3-* models support dimensions; others (e.g. ada-002) do not
        if self.model.contains("embedding-3") {
            body["dimensions"] = serde_json::json!(self.dimensions);
        }
        let mut request = self.client.post(&self.endpoint).json(&body);

        if let Some(ref key) = self.api_key {
            request = request.header("Authorization", format!("Bearer {}", key));
        }

        let response = request
            .send()
            .await
            .context("Embedding API request failed")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Embedding API error {}: {}", status, body);
        }

        let parsed: EmbeddingResponse = response
            .json()
            .await
            .context("Failed to parse embedding response")?;

        let embedding = parsed
            .data
            .into_iter()
            .next()
            .context("No embedding in response")?
            .embedding;

        if embedding.len() != self.dimensions {
            anyhow::bail!(
                "Embedding dimension mismatch: expected {}, got {}",
                self.dimensions,
                embedding.len()
            );
        }

        Ok(embedding)
    }
}
