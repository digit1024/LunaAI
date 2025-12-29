//! D-Bus subscription helper
//!
//! Extracted from app.rs subscription method for better modularity.

use crate::ui::app::Message;
use cosmic::iced_futures::Subscription;
use std::sync::Arc;

/// Create a D-Bus status signal subscription
///
/// This listens for StatusChanged signals from the D-Bus TTS/STT service
/// and forwards them as DbusStatusChanged messages.
#[cfg(feature = "ttsandstt")]
pub fn create_dbus_status_subscription(
    client: Arc<crate::dbus::DbusTtsSttClient>,
) -> Subscription<Message> {
    use cosmic::iced_futures::stream;
    use cosmic::iced_futures::futures::SinkExt;
    use futures::StreamExt;
    use zbus::{MessageStream, message::Type};
    
    // Use a stable UUID so cosmic doesn't recreate the subscription on every state change
    // Constant UUID for D-Bus subscription (hardcoded, safe to unwrap)
    // Using const to ensure it's always valid at compile time
    const DBUS_SUB_UUID_STR: &str = "550e8400-e29b-41d4-a716-446655440000";
    let dbus_sub_id = uuid::Uuid::parse_str(DBUS_SUB_UUID_STR)
        .unwrap_or_else(|_| {
            // This should never happen with a valid hardcoded UUID, but handle gracefully
            tracing::error!("Failed to parse hardcoded D-Bus subscription UUID, using fallback");
            uuid::Uuid::new_v4()
        });
    
    Subscription::run_with_id(
        dbus_sub_id,
        stream::channel(100, move |mut output| async move {
            tracing::debug!("Starting D-Bus signal subscription");
            
            // Get connection
            let conn = match client.get_connection_for_signals().await {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!(error = %e, "Failed to get D-Bus connection for signals");
                    return;
                }
            };
            
            // Add match rule using org.freedesktop.DBus.AddMatch (like Go's AddMatchSignal)
            // Call the AddMatch method directly on the connection
            let match_rule_str = format!(
                "type='signal',path='{}',interface='{}',member='StatusChanged'",
                crate::dbus::SERVICE_PATH, crate::dbus::SERVICE_INTERFACE
            );
            
            use zbus::names::BusName;
            use zbus::zvariant::ObjectPath;
            let dbus_name: BusName = match "org.freedesktop.DBus".try_into() {
                Ok(name) => name,
                Err(e) => {
                    tracing::error!(error = %e, "Failed to parse D-Bus bus name");
                    return;
                }
            };
            let dbus_path: ObjectPath = match "/org/freedesktop/DBus".try_into() {
                Ok(path) => path,
                Err(e) => {
                    tracing::error!(error = %e, "Failed to parse D-Bus object path");
                    return;
                }
            };
            
            // Call AddMatch method
            match conn.call_method(
                Some(&dbus_name),
                &dbus_path,
                Some("org.freedesktop.DBus"),
                "AddMatch",
                &(match_rule_str.clone()),
            ).await {
                Ok(_) => tracing::debug!("Added match rule for StatusChanged signals"),
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to add match rule (will filter manually)");
                    // Continue anyway, we'll filter manually
                }
            }
            
            // Create message stream (like Go's conn.Signal(signalChan))
            let mut stream = MessageStream::from(conn);
            tracing::debug!("MessageStream created, listening for StatusChanged signals");
            
            // Listen for signals
            while let Some(msg_result) = stream.next().await {
                match msg_result {
                    Ok(message) => {
                        let header = message.header();
                        
                        // Verify it's our signal
                        if header.message_type() == Type::Signal {
                            if let Some(interface) = header.interface() {
                                if interface.as_str() == crate::dbus::SERVICE_INTERFACE {
                                    if let Some(member) = header.member() {
                                        if member.as_str() == "StatusChanged" {
                                            // Deserialize status
                                            if let Ok((status,)) = message.body().deserialize::<(String,)>() {
                                                tracing::debug!(status = %status, "Received StatusChanged signal");
                                                let _ = output.send(Message::DbusStatusChanged(status)).await;
                                            } else {
                                                tracing::error!("Failed to deserialize StatusChanged signal");
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Error receiving D-Bus message");
                    }
                }
            }
            
            tracing::warn!("D-Bus signal stream ended");
        })
    )
}

/// Create a D-Bus status signal subscription (no-op when feature disabled)
#[cfg(not(feature = "ttsandstt"))]
pub fn create_dbus_status_subscription(
    _client: Arc<crate::dbus::DbusTtsSttClient>,
) -> Subscription<Message> {
    Subscription::none()
}

