//! Workaround for rust-mcp-sdk v0.8.x busy-loop bug in stderr reader.
//!
//! The SDK's `start_standalone()` spawns an `err_task` that polls `is_shut_down()` in a
//! `tokio::select!` loop. Because `is_shut_down()` resolves near-instantly (mutex read → false),
//! it always wins the race against `reader.next_line()`, creating a busy spin that consumes
//! ~100% of one CPU core **per connected MCP server**.
//!
//! This wrapper hides the stderr stream from the SDK so its `err_task` sees `None` and exits
//! immediately. We then spawn our own well-behaved stderr reader that simply awaits lines
//! without polling `is_shut_down()`.

use async_trait::async_trait;
use rust_mcp_sdk::schema::schema_utils::{
    ClientMessage, ClientMessages, MessageFromClient, ServerMessage, ServerMessages,
};
use rust_mcp_sdk::schema::RequestId;
use rust_mcp_sdk::{
    IoStream, McpDispatch, MessageDispatcher, StdioTransport, Transport, TransportDispatcher,
    TransportOptions, TransportResult,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::oneshot::Sender;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::ReceiverStream;

/// A wrapper around `StdioTransport` that prevents the SDK's buggy stderr
/// reader from spinning. The real stderr is read by our own properly-awaiting task.
pub struct NoStderrTransport {
    inner: StdioTransport<ServerMessage>,
    /// Always `None` — returned to the SDK so its `err_task` skips the busy loop.
    empty_error_stream: RwLock<Option<IoStream>>,
}

impl NoStderrTransport {
    /// Create the transport the same way as `StdioTransport::create_with_server_launch`,
    /// but with the stderr-suppression wrapper applied.
    pub fn create_with_server_launch(
        command: &str,
        args: Vec<String>,
        env: Option<HashMap<String, String>>,
        options: TransportOptions,
    ) -> TransportResult<Self> {
        let inner = StdioTransport::create_with_server_launch(command, args, env, options)?;
        Ok(Self {
            inner,
            empty_error_stream: RwLock::new(None),
        })
    }
}

// ---------------------------------------------------------------------------
// Transport trait — the SDK calls these during `start_standalone()`
// ---------------------------------------------------------------------------

#[async_trait]
impl
    Transport<
        ServerMessages,
        MessageFromClient,
        ServerMessage,
        ClientMessages,
        ClientMessage,
    > for NoStderrTransport
{
    async fn start(&self) -> TransportResult<ReceiverStream<ServerMessages>>
    where
        MessageDispatcher<ServerMessage>:
            McpDispatch<ServerMessages, ClientMessages, ServerMessage, ClientMessage>,
    {
        // Let the inner transport spawn the subprocess and set up streams normally.
        let stream = <_ as Transport<ServerMessages, MessageFromClient, ServerMessage, ClientMessages, ClientMessage>>::start(&self.inner).await?;

        // Steal the real stderr from the inner transport before the SDK can grab it.
        // The inner transport's `error_stream()` now holds `None`.
        let inner_err_stream = <_ as Transport<ServerMessages, MessageFromClient, ServerMessage, ClientMessages, ClientMessage>>::error_stream(&self.inner);
        let mut real_err_guard = inner_err_stream.write().await;
        if let Some(IoStream::Readable(input)) = real_err_guard.take() {
            // Spawn a proper reader — just awaits lines, no busy loop.
            tokio::spawn(async move {
                let mut reader = BufReader::new(input).lines();
                loop {
                    match reader.next_line().await {
                        Ok(Some(line)) => {
                            tracing::warn!(target: "mcp_stderr", "{}", line);
                        }
                        Ok(None) => break,   // EOF — process exited
                        Err(e) => {
                            tracing::debug!(target: "mcp_stderr", "stderr read error (process likely exited): {e}");
                            break;
                        }
                    }
                }
            });
        }

        Ok(stream)
    }

    /// Return our always-empty stream. The SDK's `err_task` checks
    /// `if let Some(IoStream::Readable(..))` — with `None` it skips the loop entirely.
    fn error_stream(&self) -> &RwLock<Option<IoStream>> {
        &self.empty_error_stream
    }

    // --- Everything below delegates straight to the inner transport ---
    // Uses fully-qualified syntax because StdioTransport<ServerMessage> implements
    // Transport for multiple type-parameter sets and the compiler can't infer which one.

    fn message_sender(&self) -> Arc<RwLock<Option<MessageDispatcher<ServerMessage>>>> {
        <_ as Transport<ServerMessages, MessageFromClient, ServerMessage, ClientMessages, ClientMessage>>::message_sender(&self.inner)
    }

    async fn shut_down(&self) -> TransportResult<()> {
        <_ as Transport<ServerMessages, MessageFromClient, ServerMessage, ClientMessages, ClientMessage>>::shut_down(&self.inner).await
    }

    async fn is_shut_down(&self) -> bool {
        <_ as Transport<ServerMessages, MessageFromClient, ServerMessage, ClientMessages, ClientMessage>>::is_shut_down(&self.inner).await
    }

    async fn consume_string_payload(&self, payload: &str) -> TransportResult<()> {
        <_ as Transport<ServerMessages, MessageFromClient, ServerMessage, ClientMessages, ClientMessage>>::consume_string_payload(&self.inner, payload).await
    }

    async fn pending_request_tx(&self, request_id: &RequestId) -> Option<Sender<ServerMessage>> {
        <_ as Transport<ServerMessages, MessageFromClient, ServerMessage, ClientMessages, ClientMessage>>::pending_request_tx(&self.inner, request_id).await
    }

    async fn keep_alive(
        &self,
        interval: Duration,
        disconnect_tx: Sender<()>,
    ) -> TransportResult<JoinHandle<()>> {
        <_ as Transport<ServerMessages, MessageFromClient, ServerMessage, ClientMessages, ClientMessage>>::keep_alive(&self.inner, interval, disconnect_tx).await
    }
}

// ---------------------------------------------------------------------------
// McpDispatch — message sending, delegates entirely to inner
// ---------------------------------------------------------------------------

#[async_trait]
impl McpDispatch<ServerMessages, ClientMessages, ServerMessage, ClientMessage>
    for NoStderrTransport
{
    async fn send_message(
        &self,
        message: ClientMessages,
        timeout: Option<Duration>,
    ) -> TransportResult<Option<ServerMessages>> {
        self.inner.send_message(message, timeout).await
    }

    async fn send(
        &self,
        message: ClientMessage,
        timeout: Option<Duration>,
    ) -> TransportResult<Option<ServerMessage>> {
        self.inner.send(message, timeout).await
    }

    async fn send_batch(
        &self,
        message: Vec<ClientMessage>,
        timeout: Option<Duration>,
    ) -> TransportResult<Option<Vec<ServerMessage>>> {
        self.inner.send_batch(message, timeout).await
    }

    async fn write_str(&self, payload: &str, skip_store: bool) -> TransportResult<()> {
        self.inner.write_str(payload, skip_store).await
    }
}

// ---------------------------------------------------------------------------
// TransportDispatcher — marker trait combining Transport + McpDispatch
// ---------------------------------------------------------------------------

impl
    TransportDispatcher<
        ServerMessages,
        MessageFromClient,
        ServerMessage,
        ClientMessages,
        ClientMessage,
    > for NoStderrTransport
{
}
