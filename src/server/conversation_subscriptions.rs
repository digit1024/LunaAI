//! Conversation-scoped event broadcast: all clients "watching" a conversation
//! receive streaming/tool/completion events for that conversation.
//! Enables multi-client same-conversation and reconnect-to-live-stream.

use crate::server::dto::ServerEvent;
use std::collections::HashMap;
use tokio::sync::{
    mpsc::{error::TrySendError, Sender},
    RwLock,
};
use uuid::Uuid;

/// Identifies a WebSocket connection (not a conversation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionId(pub Uuid);

impl ConnectionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

type ConversationSubscribers = Vec<(ConnectionId, Sender<ServerEvent>)>;

/// Tracks which connection is viewing which conversation and broadcasts
/// conversation-scoped events to all subscribers of that conversation.
pub struct ConversationSubscriptions {
    /// connection_id -> conversation_id they are currently viewing
    viewing: RwLock<HashMap<ConnectionId, Uuid>>,
    /// conversation_id -> list of (connection_id, sender) subscribed to that conversation
    subscribers: RwLock<HashMap<Uuid, ConversationSubscribers>>,
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
        sender: Sender<ServerEvent>,
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
    /// Removes subscribers whose channel closed; events are dropped for
    /// subscribers whose bounded queue is full (client too slow).
    pub async fn broadcast(&self, conversation_id: Uuid, event: ServerEvent) {
        self.broadcast_inner(conversation_id, event, None).await;
    }

    /// Like `broadcast`, but skips one connection (used when the caller already
    /// sent the event to itself directly and must not receive it twice).
    pub async fn broadcast_excluding(
        &self,
        conversation_id: Uuid,
        event: ServerEvent,
        exclude: ConnectionId,
    ) {
        self.broadcast_inner(conversation_id, event, Some(exclude))
            .await;
    }

    async fn broadcast_inner(
        &self,
        conversation_id: Uuid,
        event: ServerEvent,
        exclude: Option<ConnectionId>,
    ) {
        let mut subscribers = self.subscribers.write().await;
        let list = match subscribers.get_mut(&conversation_id) {
            Some(l) => l,
            None => return,
        };
        let mut dead = Vec::new();
        for (conn_id, sender) in list.iter() {
            if exclude == Some(*conn_id) {
                continue;
            }
            match sender.try_send(event.clone()) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {
                    tracing::debug!(
                        conversation_id = %conversation_id,
                        connection_id = %conn_id.0,
                        "Dropping broadcast event: subscriber outbound queue full"
                    );
                }
                Err(TrySendError::Closed(_)) => dead.push(*conn_id),
            }
        }
        for conn_id in dead {
            list.retain(|(c, _)| *c != conn_id);
        }
        if list.is_empty() {
            subscribers.remove(&conversation_id);
        }
    }

    /// Drop every subscriber of a conversation (e.g. after it was deleted).
    pub async fn clear_conversation(&self, conversation_id: Uuid) {
        let mut viewing = self.viewing.write().await;
        let mut subscribers = self.subscribers.write().await;
        if subscribers.remove(&conversation_id).is_some() {
            viewing.retain(|_, conv| *conv != conversation_id);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn broadcast_prunes_dead_subscribers() {
        let subs = ConversationSubscriptions::new();
        let conn_id = ConnectionId::new();
        let conv_id = Uuid::new_v4();
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        subs.set_viewing(conn_id, Some(conv_id), tx).await;
        drop(rx);

        subs.broadcast(
            conv_id,
            ServerEvent::ConversationComplete {
                conversation_id: conv_id.to_string(),
            },
        )
        .await;

        subs.broadcast(
            conv_id,
            ServerEvent::Info {
                message: "should not panic".into(),
            },
        )
        .await;
    }

    #[tokio::test]
    async fn broadcast_excluding_and_clear_conversation() {
        let subs = ConversationSubscriptions::new();
        let conv_id = Uuid::new_v4();
        let a = ConnectionId::new();
        let b = ConnectionId::new();
        let (tx_a, mut rx_a) = tokio::sync::mpsc::channel(8);
        let (tx_b, mut rx_b) = tokio::sync::mpsc::channel(8);
        subs.set_viewing(a, Some(conv_id), tx_a).await;
        subs.set_viewing(b, Some(conv_id), tx_b).await;

        // Excluded connection receives nothing; the other one does.
        subs.broadcast_excluding(
            conv_id,
            ServerEvent::Info {
                message: "hi".into(),
            },
            a,
        )
        .await;
        assert!(rx_a.try_recv().is_err());
        assert!(rx_b.try_recv().is_ok());

        // After clearing, broadcasts reach nobody.
        subs.clear_conversation(conv_id).await;
        subs.broadcast(
            conv_id,
            ServerEvent::Info {
                message: "gone".into(),
            },
        )
        .await;
        assert!(rx_a.try_recv().is_err());
        assert!(rx_b.try_recv().is_err());
    }

    #[tokio::test]
    async fn full_subscriber_queue_drops_instead_of_blocking() {
        let subs = ConversationSubscriptions::new();
        let conv_id = Uuid::new_v4();
        let conn = ConnectionId::new();
        // Capacity-1 channel, receiver never drains.
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        subs.set_viewing(conn, Some(conv_id), tx).await;

        for i in 0..10 {
            subs.broadcast(
                conv_id,
                ServerEvent::Info {
                    message: format!("m{i}"),
                },
            )
            .await;
        }
        // Broadcast must not block or prune the (full but alive) subscriber.
        let still_subscribed = {
            let subscribers = subs.subscribers.read().await;
            subscribers.get(&conv_id).map(|l| l.len()).unwrap_or(0)
        };
        assert_eq!(still_subscribed, 1);
    }
}
