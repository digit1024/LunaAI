//! Settings page - server connection configuration

use cosmic::{
    iced::Length,
    widget::{self, button, column, container, row, text, text_input, Space},
    Element,
};

use crate::ui::app::{LunaThinApp, Message, ConnectionStatus};

pub fn settings_page(app: &LunaThinApp) -> Element<Message> {
    let mut content = column().spacing(16).padding(16);

    // Header
    content = content.push(text("⚙️ Server Connection").size(24));

    // Connection status card
    let (status_icon, status_text, status_desc) = match app.connection_status {
        ConnectionStatus::Disconnected => (
            "⚪",
            "Disconnected",
            "Enter server details and connect",
        ),
        ConnectionStatus::Connecting => ("🔄", "Connecting...", "Establishing connection"),
        ConnectionStatus::Connected => ("🟢", "Connected", "Ready to chat"),
        ConnectionStatus::Error => ("🔴", "Error", "Connection failed - check settings"),
    };

    content = content.push(
        container(
            row()
                .push(text(status_icon).size(24))
                .push(
                    column()
                        .push(text(status_text).size(16))
                        .push(
                            text(status_desc)
                                .size(12)
                                .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(
                                    0.6, 0.6, 0.6,
                                ))),
                        )
                        .spacing(4),
                )
                .spacing(12)
                .align_y(cosmic::iced::Alignment::Center),
        )
        .padding(16)
        .width(Length::Fill)
        .class(cosmic::style::Container::Card),
    );

    // Server configuration
    content = content.push(text("Server Details").size(18));

    // Host input
    content = content.push(
        column()
            .push(text("Host").size(14))
            .push(
                text_input("e.g. 192.168.1.100 or localhost", &app.settings_host)
                    .on_input(Message::HostChanged)
                    .width(Length::Fill),
            )
            .spacing(4),
    );

    // Port input
    content = content.push(
        column()
            .push(text("Port").size(14))
            .push(
                text_input("e.g. 8080", &app.settings_port)
                    .on_input(Message::PortChanged)
                    .width(Length::Fixed(120.0)),
            )
            .spacing(4),
    );

    // API Key input
    content = content.push(
        column()
            .push(text("API Key").size(14))
            .push(
                text_input("Your server API key", &app.settings_api_key)
                    .on_input(Message::ApiKeyChanged)
                    .password()
                    .width(Length::Fill),
            )
            .spacing(4),
    );

    // Connect/Disconnect button
    content = content.push(Space::with_height(8));

    let connection_button: Element<Message> = match app.connection_status {
        ConnectionStatus::Connected => button::text("Disconnect")
            .on_press(Message::Disconnect)
            .class(cosmic::style::Button::Destructive)
            .into(),
        ConnectionStatus::Connecting => button::text("Connecting...")
            .class(cosmic::style::Button::Standard)
            .into(),
        _ => button::text("Connect")
            .on_press(Message::Connect)
            .class(cosmic::style::Button::Suggested)
            .into(),
    };

    content = content.push(connection_button);

    // Profile selection (only when connected)
    if app.connection_status == ConnectionStatus::Connected && !app.profiles.is_empty() {
        content = content.push(Space::with_height(16));
        content = content.push(text("🎭 Active Profile").size(18));

        let mut profile_row = row().spacing(8);
        for profile in &app.profiles {
            let is_active = profile == &app.current_profile;
            let btn: Element<Message> = if is_active {
                button::text(format!("✓ {}", profile))
                    .class(cosmic::style::Button::Suggested)
                    .into()
            } else {
                button::text(profile)
                    .on_press(Message::ChangeProfile(profile.clone()))
                    .class(cosmic::style::Button::Standard)
                    .into()
            };
            profile_row = profile_row.push(btn);
        }
        content = content.push(profile_row);
    }

    // Info section
    content = content.push(Space::with_height(24));
    content = content.push(
        container(
            column()
                .push(text("ℹ️ About ThinUI").size(14))
                .push(
                    text("This is a thin client that connects to a Luna AI server. All processing happens on the server - this app only provides the interface.")
                        .size(12)
                        .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(
                            0.6, 0.6, 0.6,
                        ))),
                )
                .spacing(4),
        )
        .padding(12)
        .width(Length::Fill)
        .style(|_theme| cosmic::widget::container::Style {
            background: Some(cosmic::iced::Background::Color(
                cosmic::iced::Color::from_rgba(0.3, 0.3, 0.5, 0.2),
            )),
            border: cosmic::iced::Border {
                width: 1.0,
                color: cosmic::iced::Color::from_rgba(0.3, 0.3, 0.5, 0.4),
                radius: 8.0.into(),
            },
            ..Default::default()
        }),
    );

    widget::scrollable(content).into()
}

















