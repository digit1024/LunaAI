//! Conversation-scoped event broadcast: all clients "watching" a conversation
//! receive streaming/tool/completion events for that conversation.
//! Enables multi-client same-conversation and reconnect-to-live-stream.

use crate::server::dto::ServerEvent;
use std::collections::HashMap;
use tokio::sync::{mpsc::UnboundedSender, RwLock};
use uuid::Uuid;

/// Identifies a WebSocket connection (not a conversation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionId(pub Uuid);

impl ConnectionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

/// Tracks which connection is viewing which conversation and broadcasts
/// conversation-scoped events to all subscribers of that conversation.
pub struct ConversationSubscriptions {
    /// connection_id -> conversation_id they are currently viewing
    viewing: RwLock<HashMap<ConnectionId, Uuid>>,
    /// conversation_id -> list of (connection_id, sender) subscribed to that conversation
    subscribers: RwLock<HashMap<Uuid, Vec<(ConnectionId, UnboundedSender<ServerEvent>)>>>,
}

impl ConversationSubscriptions {
    pub fn new() -> Self {
        Self {
            viewing: RwLock::new(HashMap::new()),
            subscribers: RwLock::new(HashMap::new()),
        }
    }

    /// Set which conversation this connection is viewing. The connection will receive
    /// all broadcast events for that conversation until they switch or disconnect.
    /// Pass the connection's outbound sender so we can add it to subscribers.
    pub async fn set_viewing(
        &self,
        connection_id: ConnectionId,
        conversation_id: Option<Uuid>,
        sender: UnboundedSender<ServerEvent>,
    ) {
        let mut viewing = self.viewing.write().await;
        let mut subscribers = self.subscribers.write().await;

        // Remove from previous conversation's subscribers
        if let Some(old_conv) = viewing.remove(&connection_id) {
            if let Some(list) = subscribers.get_mut(&old_conv) {
                list.retain(|(c, _)| *c != connection_id);
                if list.is_empty() {
                    subscribers.remove(&old_conv);
                }
            }
        }

        // Add to new conversation's subscribers
        if let Some(conv_id) = conversation_id {
            viewing.insert(connection_id, conv_id);
            subscribers
                .entry(conv_id)
                .or_default()
                .push((connection_id, sender));
        }
    }

    /// Send an event to all connections currently viewing this conversation.
    /// Removes subscribers whose send fails (e.g. connection closed).
    pub async fn broadcast(&self, conversation_id: Uuid, event: ServerEvent) {
        let mut subscribers = self.subscribers.write().await;
        let list = match subscribers.get_mut(&conversation_id) {
            Some(l) => l,
            None => return,
        };
        let mut dead = Vec::new();
        for (conn_id, sender) in list.iter() {
            if sender.send(event.clone()).is_err() {
                dead.push(*conn_id);
            }
        }
        for conn_id in dead {
            list.retain(|(c, _)| *c != conn_id);
        }
        if list.is_empty() {
            subscribers.remove(&conversation_id);
        }
    }

    /// Call when a connection closes so we stop sending to it and remove it from subscribers.
    pub async fn on_connection_closed(&self, connection_id: ConnectionId) {
        let mut viewing = self.viewing.write().await;
        let mut subscribers = self.subscribers.write().await;

        if let Some(conv_id) = viewing.remove(&connection_id) {
            if let Some(list) = subscribers.get_mut(&conv_id) {
                list.retain(|(c, _)| *c != connection_id);
                if list.is_empty() {
                    subscribers.remove(&conv_id);
                }
            }
        }
    }
}

impl Default for ConversationSubscriptions {
    fn default() -> Self {
        Self::new()
    }
}
