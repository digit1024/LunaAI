//! Top panel - title, new chat button, profile dropdown

use cosmic::{
    iced::Length,
    widget::{self, button, container, dropdown, text, Column, Row, Space},
    Element,
};

use crate::ui::app::{LunaThinApp, Message, ConnectionStatus};

pub fn top_panel(app: &LunaThinApp) -> Element<'static, Message> {
    // Get conversation title
    let title = if app.current_conversation_id.is_some() {
        // Find title from conversations list
        app.conversations
            .iter()
            .find(|c| Some(&c.id) == app.current_conversation_id.as_ref())
            .map(|c| c.title.clone())
            .unwrap_or_else(|| "Conversation".to_string())
    } else {
        "New Chat".to_string()
    };

    // Connection indicator
    let conn_icon = match app.connection_status {
        ConnectionStatus::Connected => "network-wireless-symbolic",
        ConnectionStatus::Connecting => "network-wireless-acquiring-symbolic",
        ConnectionStatus::Disconnected => "network-wireless-offline-symbolic",
        ConnectionStatus::Error => "network-error-symbolic",
    };

    // Profile dropdown - need two copies: one for display, one for closure
    let profiles_display: Vec<String> = app.profiles.clone();
    let profiles_closure: Vec<String> = app.profiles.clone();
    let current_idx = profiles_display.iter().position(|p| p == &app.current_profile);

    container(
        Column::new()
            .push(
                // First row: Connection + Title <-> New chat button
                Row::new()
                    .push(
                        widget::icon::from_name(conn_icon).size(16)
                    )
                    .push(Space::new().width(8))
                    .push(text(title).size(18))
                    .push(Space::new().width(Length::Fill))
                    .push(
                        button::icon(crate::ui::icons::get_handle("plus-circle-filled-symbolic", 16))
                            .on_press(Message::NewConversation)
                            .class(widget::button::ButtonClass::Suggested),
                    )
                    .spacing(8)
                    .align_y(cosmic::iced::Alignment::Center),
            )
            .push(
                // Divider
                container(Space::new().height(Length::Fixed(1.0)))
                    .width(Length::Fill)
                    .style(|_theme| cosmic::widget::container::Style {
                        background: Some(cosmic::iced::Background::Color(
                            cosmic::iced::Color::from_rgb(0.3, 0.3, 0.3),
                        )),
                        ..Default::default()
                    }),
            )
            .push(
                // Second row: Profile dropdown
                Row::new()
                    .push(text("Profile").size(12))
                    .push(Space::new().width(8))
                    .push(
                        dropdown(profiles_display, current_idx, move |idx| {
                            if let Some(profile) = profiles_closure.get(idx) {
                                Message::ChangeProfile(profile.clone())
                            } else {
                                Message::ChangeProfile(String::new())
                            }
                        })
                    )
                    .push(Space::new().width(Length::Fill))
                    .spacing(8)
                    .align_y(cosmic::iced::Alignment::Center),
            )
            .spacing(8),
    )
    .padding(12)
    .width(Length::Fill)
    .class(cosmic::style::Container::Card)
    .into()
}

