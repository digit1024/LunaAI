//! TTS Client - DBus client for Text-to-Speech service
//!
//! Connects to the ttsandsttp DBus service and provides methods to speak text
//! and subscribe to status changes.

use anyhow::{Context, Result};
use async_stream::stream;
use futures::Stream;
use std::sync::Arc;
use zbus::{connection, Connection};

/// TTS Client for communicating with ttsandsttp DBus service
#[derive(Clone)]
pub struct TtsClient {
    connection: Arc<Connection>,
}

impl std::fmt::Debug for TtsClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TtsClient").finish()
    }
}

impl TtsClient {
    /// Create a new TTS client and connect to the DBus service
    pub async fn new() -> Result<Self> {
        let connection = connection::Builder::session()
            .context("Failed to create DBus session connection")?
            .build()
            .await
            .context("Failed to build DBus connection")?;

        // Verify service is available by trying to call a method
        // We'll use a simple approach: try to get the service name owner
        let dbus_proxy = zbus::fdo::DBusProxy::new(&connection)
            .await
            .context("Failed to create DBus proxy")?;

        // Check if service is available (this will fail if service doesn't exist)
        let service_name = zbus::names::BusName::try_from("com.github.digit1024.ttsstt")
            .context("Invalid service name")?;
        match dbus_proxy.get_name_owner(service_name).await {
            Ok(owner) if !owner.as_str().is_empty() => {
                tracing::info!("TTS client connected to service: {}", owner.as_str());
            }
            Ok(_) => {
                anyhow::bail!("TTS service name has no owner (service not running)");
            }
            Err(e) => {
                anyhow::bail!("TTS service not available on DBus: {}", e);
            }
        }

        Ok(Self {
            connection: Arc::new(connection),
        })
    }

    /// Speak text using TTS
    ///
    /// # Arguments
    /// * `text` - The text to speak (should be plain text, no markdown)
    /// * `language` - Language code (e.g., "en-US")
    pub async fn speak(&self, text: String, language: String) -> Result<()> {
        tracing::debug!("TTS speak: language={}, text_length={}", language, text.len());

        // Use zbus proxy to call the method
        let proxy = zbus::Proxy::new(
            &self.connection,
            "com.github.digit1024.ttsstt",
            "/com/github/digit1024/ttsstt",
            "com.github.digit1024.ttsstt.Service",
        )
        .await
        .context("Failed to create TTS proxy")?;

        proxy
            .call_method("Tts", &(text, language))
            .await
            .context("Failed to call TTS method")?;

        tracing::debug!("TTS speak request sent successfully");
        Ok(())
    }

    /// Stop current TTS playback
    pub async fn stop(&self) -> Result<String> {
        tracing::debug!("TTS stop requested");

        // Use zbus proxy to call the method
        let proxy = zbus::Proxy::new(
            &self.connection,
            "com.github.digit1024.ttsstt",
            "/com/github/digit1024/ttsstt",
            "com.github.digit1024.ttsstt.Service",
        )
        .await
        .context("Failed to create TTS proxy")?;

        // Call Stop method - according to DBus introspection, it has no return value
        // The Rust implementation may return a String internally, but DBus doesn't expose it
        proxy
            .call_method("Stop", &())
            .await
            .context("Failed to call TTS Stop method")?;

        tracing::debug!("TTS stop completed");
        // Return empty string since DBus interface doesn't expose return value
        // (The service may return recognized text internally, but it's not exposed via DBus)
        Ok(String::new())
    }

    /// Subscribe to StatusChanged signals from the TTS service
    ///
    /// Returns a stream that yields status strings: "idle", "speaking", "listening", "processing"
    pub async fn subscribe_status(&self) -> Result<impl Stream<Item = Result<String>> + Send + 'static> {
        use futures::StreamExt;
        
        // Create proxy for signal subscription
        let proxy = zbus::Proxy::new(
            &self.connection,
            "com.github.digit1024.ttsstt",
            "/com/github/digit1024/ttsstt",
            "com.github.digit1024.ttsstt.Service",
        )
        .await
        .context("Failed to create TTS proxy for signal subscription")?;

        // Receive StatusChanged signals
        let mut signal_stream = proxy.receive_signal("StatusChanged")
            .await
            .context("Failed to subscribe to StatusChanged signal")?;

        Ok(stream! {
            loop {
                match signal_stream.next().await {
                    Some(signal) => {
                        // Deserialize signal body
                        match signal.body().deserialize::<(String,)>() {
                            Ok((status,)) => {
                                tracing::debug!("TTS status changed: {}", status);
                                yield Ok(status);
                            }
                            Err(e) => {
                                tracing::warn!("Failed to deserialize StatusChanged signal: {}", e);
                            }
                        }
                    }
                    None => {
                        tracing::warn!("StatusChanged signal stream ended");
                        break;
                    }
                }
            }
        })
    }
}

