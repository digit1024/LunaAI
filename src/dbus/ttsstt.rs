//! TTS/STT D-Bus client implementation
//!
//! This module provides a high-level client for the TTS/STT D-Bus service.
//! It wraps the generated proxy and provides convenience methods.

use std::sync::Arc;
use tokio::sync::Mutex;

// Always import Connection so it can be used in struct definition
// zbus is always available (not conditionally compiled)
use zbus::Connection;
#[cfg(feature = "ttsandstt")]
use zbus::Result as ZbusResult;
#[cfg(feature = "ttsandstt")]
use crate::dbus::generated::ServiceProxy;

/// D-Bus service constants
// Always defined (not conditionally compiled) so they can be re-exported
#[cfg(feature = "ttsandstt")]
pub const SERVICE_NAME: &str = "com.github.digit1024.ttsstt";
#[cfg(not(feature = "ttsandstt"))]
pub const SERVICE_NAME: &str = "";

#[cfg(feature = "ttsandstt")]
pub const SERVICE_PATH: &str = "/com/github/digit1024/ttsstt";
#[cfg(not(feature = "ttsandstt"))]
pub const SERVICE_PATH: &str = "";

#[cfg(feature = "ttsandstt")]
pub const SERVICE_INTERFACE: &str = "com.github.digit1024.ttsstt.Service";
#[cfg(not(feature = "ttsandstt"))]
pub const SERVICE_INTERFACE: &str = "";

/// High-level client for TTS/STT D-Bus service
// Always defined with the same structure so it can be re-exported
pub struct DbusTtsSttClient {
    // Always use Connection type (zbus is always available)
    // When feature is off, these will just be None and unused
    inner: Option<Arc<Mutex<Option<Connection>>>>,
    signal_inner: Option<Arc<Mutex<Option<Connection>>>>,
}

impl DbusTtsSttClient {
    /// Create a new D-Bus client
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "ttsandstt")]
            inner: Some(Arc::new(Mutex::new(None))),
            #[cfg(not(feature = "ttsandstt"))]
            inner: None,
            #[cfg(feature = "ttsandstt")]
            signal_inner: Some(Arc::new(Mutex::new(None))),
            #[cfg(not(feature = "ttsandstt"))]
            signal_inner: None,
        }
    }

    /// Get or create a connection for method calls
    #[cfg(feature = "ttsandstt")]
    async fn get_connection(&self) -> ZbusResult<Connection> {
        if let Some(ref inner) = self.inner {
            let mut conn_guard = inner.lock().await;
            if let Some(ref conn) = *conn_guard {
                return Ok(conn.clone());
            }

            let conn = Connection::session().await?;
            let conn_clone = conn.clone();
            *conn_guard = Some(conn);
            Ok(conn_clone)
        } else {
            unreachable!()
        }
    }

    /// Get or create a connection for signal listening
    #[cfg(feature = "ttsandstt")]
    pub async fn get_connection_for_signals(&self) -> ZbusResult<Connection> {
        if let Some(ref inner) = self.signal_inner {
            let mut conn_guard = inner.lock().await;
            if let Some(ref conn) = *conn_guard {
                return Ok(conn.clone());
            }

            let conn = Connection::session().await?;
            let conn_clone = conn.clone();
            *conn_guard = Some(conn);
            Ok(conn_clone)
        } else {
            unreachable!()
        }
    }

    #[cfg(not(feature = "ttsandstt"))]
    pub async fn get_connection_for_signals(&self) -> Result<(), ()> {
        Err(())
    }

    /// Check if the service is available
    #[cfg(feature = "ttsandstt")]
    pub async fn check_availability(&self) -> bool {
        match self.get_connection().await {
            Ok(conn) => {
                match ServiceProxy::new(&conn).await {
                    Ok(_) => true,
                    Err(_) => false,
                }
            }
            Err(_) => false,
        }
    }

    #[cfg(not(feature = "ttsandstt"))]
    pub async fn check_availability(&self) -> bool {
        false
    }

    /// Call STT (Speech-to-Text)
    #[cfg(feature = "ttsandstt")]
    pub async fn call_stt(&self, language: &str, pause_duration: f64) -> ZbusResult<String> {
        let conn = self.get_connection().await?;
        let proxy = ServiceProxy::new(&conn).await?;
        proxy.stt(language, pause_duration).await
    }

    #[cfg(not(feature = "ttsandstt"))]
    pub async fn call_stt(&self, _language: &str, _pause_duration: f64) -> Result<String, ()> {
        Err(())
    }

    /// Stop any ongoing operation (TTS or STT)
    #[cfg(feature = "ttsandstt")]
    pub async fn stop(&self) -> ZbusResult<()> {
        let conn = self.get_connection().await?;
        let proxy = ServiceProxy::new(&conn).await?;
        proxy.stop().await
    }

    #[cfg(not(feature = "ttsandstt"))]
    pub async fn stop(&self) -> Result<(), ()> {
        Err(())
    }

    /// Call TTS (Text-to-Speech)
    #[cfg(feature = "ttsandstt")]
    pub async fn call_tts(&self, text: &str, language: &str) -> ZbusResult<()> {
        let conn = self.get_connection().await?;
        let proxy = ServiceProxy::new(&conn).await?;
        proxy.tts(text, language).await
    }

    #[cfg(not(feature = "ttsandstt"))]
    pub async fn call_tts(&self, _text: &str, _language: &str) -> Result<(), ()> {
        Err(())
    }
}
