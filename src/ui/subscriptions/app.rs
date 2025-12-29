//! App-level subscription helpers
//!
//! Helper functions for creating app-level subscriptions, extracted from app.rs.

use cosmic::iced::Subscription;

use crate::ui::app::{CosmicLlmApp, Message};

/// Create all app-level subscriptions
pub fn create_app_subscriptions(app: &CosmicLlmApp) -> Subscription<Message> {
    use cosmic::iced::time;
    use cosmic::iced_futures::Subscription;
    
    // Create a subscription for streaming LLM responses
    let streaming_sub = if app.is_streaming {
        app.create_streaming_subscription(app.current_streaming_id)
    } else {
        Subscription::none()
    };
    
    // Create a timer subscription for typing indicator animation
    let animation_sub = if app.is_streaming {
        time::every(time::Duration::from_millis(50))
            .map(|instant| Message::TypingIndicatorTick(instant))
    } else {
        Subscription::none()
    };
    
    // Create a periodic subscription to refresh conversation list every 15 seconds
    let conversation_refresh_sub = time::every(time::Duration::from_secs(15))
        .map(|_| Message::RefreshConversationList);
    
    // Periodically check D-Bus service availability (every 5 seconds) - only if feature enabled
    #[cfg(feature = "ttsandstt")]
    let dbus_check_sub = time::every(time::Duration::from_secs(5))
        .map(|_| Message::CheckDbusService);
    #[cfg(not(feature = "ttsandstt"))]
    let dbus_check_sub = Subscription::none();
    
    // D-Bus status signal subscription
    #[cfg(feature = "ttsandstt")]
    let dbus_status_sub = if app.dbus_ttsstt_available {
        crate::ui::subscriptions::dbus::create_dbus_status_subscription(
            app.dbus_ttsstt_client.clone()
        )
    } else {
        Subscription::none()
    };
    #[cfg(not(feature = "ttsandstt"))]
    let dbus_status_sub = Subscription::none();
    
    Subscription::batch(vec![streaming_sub, animation_sub, conversation_refresh_sub, dbus_check_sub, dbus_status_sub])
}

