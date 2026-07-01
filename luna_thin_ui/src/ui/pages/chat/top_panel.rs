//! Top panel - title, new chat button, profile dropdown, chat actions menu

use cosmic::{
    iced::Length,
    widget::{self, button, container, dropdown, popover, text, Column, Row, Space},
    Element,
};

use crate::ui::app::{ConnectionStatus, LunaThinApp, Message};
use crate::ui::icons;

pub fn top_panel(app: &LunaThinApp) -> Element<'static, Message> {
    let title = if app.current_conversation_id.is_some() {
        app.conversations
            .iter()
            .find(|c| Some(&c.id) == app.current_conversation_id.as_ref())
            .map(|c| c.title.clone())
            .unwrap_or_else(|| "Conversation".to_string())
    } else {
        "New Chat".to_string()
    };

    let conn_icon = match app.connection_status {
        ConnectionStatus::Connected => "network-wireless-symbolic",
        ConnectionStatus::Connecting => "network-wireless-acquiring-symbolic",
        ConnectionStatus::Disconnected => "network-wireless-offline-symbolic",
        ConnectionStatus::Error => "network-error-symbolic",
    };

    let profiles_display: Vec<String> = app.profiles.clone();
    let profiles_closure: Vec<String> = app.profiles.clone();
    let current_idx = profiles_display.iter().position(|p| p == &app.current_profile);

    let chat_actions_enabled = app.connection_status == ConnectionStatus::Connected
        && app.current_conversation_id.is_some();
    let resume_enabled = chat_actions_enabled && !app.is_current_streaming();
    let has_conversation = app.current_conversation_id.is_some();

    let menu_trigger = button::icon(icons::get_handle("open-menu-symbolic", 16))
        .on_press(Message::ToggleChatMenu)
        .class(widget::button::ButtonClass::Standard);

    let mut popover_widget = popover::popover(menu_trigger)
        .position(popover::Position::Bottom)
        .on_close(Message::CloseChatMenu);

    if app.chat_menu_open {
        popover_widget = popover_widget.popup(chat_actions_menu(
            chat_actions_enabled,
            resume_enabled,
            has_conversation,
            app.current_conversation_internal,
            app.current_conversation_id.clone().unwrap_or_default(),
        ));
    }

    container(
        Column::new()
            .push(
                Row::new()
                    .push(widget::icon::from_name(conn_icon).size(16))
                    .push(Space::new().width(8))
                    .push(text(title).size(18))
                    .push(Space::new().width(Length::Fill))
                    .push(
                        button::icon(icons::get_handle("plus-circle-filled-symbolic", 16))
                            .on_press(Message::NewConversation)
                            .class(widget::button::ButtonClass::Suggested),
                    )
                    .spacing(8)
                    .align_y(cosmic::iced::Alignment::Center),
            )
            .push(
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
                Row::new()
                    .push(text("Profile").size(12))
                    .push(Space::new().width(8))
                    .push(dropdown(profiles_display, current_idx, move |idx| {
                        if let Some(profile) = profiles_closure.get(idx) {
                            Message::ChangeProfile(profile.clone())
                        } else {
                            Message::ChangeProfile(String::new())
                        }
                    }))
                    .push(Space::new().width(Length::Fill))
                    .push(popover_widget)
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

fn chat_actions_menu(
    chat_actions_enabled: bool,
    resume_enabled: bool,
    has_conversation: bool,
    is_internal: bool,
    conversation_id: String,
) -> Element<'static, Message> {
    let internal_label = if is_internal {
        "Mark as regular"
    } else {
        "Mark as internal"
    };

    let compact = menu_item(
        "compress-symbolic",
        "Compact",
        chat_actions_enabled.then(|| Message::SummarizeConversation),
    );
    let resume = menu_item(
        "media-playback-start-symbolic",
        "Resume agent",
        resume_enabled.then(|| Message::ResumeAgent),
    );

    let mut items = Column::new()
        .push(
            container(text("Current chat").size(12))
                .padding([0, 4])
                .width(Length::Fill),
        )
        .push(compact)
        .push(resume);

    if has_conversation {
        items = items.push(menu_item(
            if is_internal {
                "view-reveal-symbolic"
            } else {
                "view-conceal-symbolic"
            },
            internal_label,
            chat_actions_enabled.then(|| Message::SetConversationInternal {
                conversation_id,
                internal: !is_internal,
            }),
        ));
    }

    container(items.padding(4).spacing(2).width(Length::Fixed(220.0)))
        .padding(4)
        .class(cosmic::style::Container::Card)
        .into()
}

fn menu_item(
    icon_name: &str,
    label: &str,
    message: Option<Message>,
) -> Element<'static, Message> {
    let label = label.to_string();
    let row = Row::new()
        .push(widget::icon::icon(icons::get_handle(icon_name, 16)).size(16))
        .push(Space::new().width(8))
        .push(text(label).size(14))
        .spacing(8)
        .align_y(cosmic::iced::Alignment::Center)
        .width(Length::Fill);

    let btn = if let Some(msg) = message {
        button::custom(row)
            .on_press(msg)
            .width(Length::Fill)
            .class(widget::button::ButtonClass::Text)
    } else {
        button::custom(row)
            .width(Length::Fill)
            .class(widget::button::ButtonClass::Text)
    };

    btn.into()
}
