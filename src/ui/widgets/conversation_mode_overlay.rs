//! Conversation mode overlay widget
//!
//! Provides visual feedback for conversation mode states: listening, processing, speaking

use crate::ui::app::{ConversationModeState, Message};
use cosmic::{Element, iced::{Alignment, Background, Color, Length}, widget};

/// Create the conversation mode overlay
pub fn conversation_mode_overlay(
    state: ConversationModeState,
) -> Element<'static, Message> {
    let content = widget::column::with_capacity(3)
        .push(
            // Close button in top-right
            widget::row::with_capacity(1)
                .push(widget::Space::with_width(Length::Fill))
                .push(
                    widget::button::icon(
                        crate::ui::icons::get_handle("window-close-symbolic", 24)
                    )
                    .on_press(Message::StopConversationMode)
                )
                .width(Length::Fill)
                .align_y(Alignment::Center),
        )
        .push(
            // Central icon with state-specific animation
            _build_state_icon(state)
        )
        .push(
            // Hint text
            widget::text(_get_hint_text(state))
                .size(16)
                .class(cosmic::style::Text::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.7)))
        )
        .spacing(24);

    widget::container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_theme| cosmic::widget::container::Style {
            background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.85))),
            ..Default::default()
        })
        .padding(48)
        .into()
}

fn _build_state_icon(state: ConversationModeState) -> Element<'static, Message> {
    match state {
        ConversationModeState::Listening => {
            // Mic icon with red circle
            widget::container(
                crate::ui::icons::get_icon("audio-input-microphone-symbolic", 80)
            )
            .width(Length::Fixed(160.0))
            .height(Length::Fixed(160.0))
            .style(|_theme| cosmic::widget::container::Style {
                background: Some(Background::Color(Color::from_rgba(1.0, 0.2, 0.2, 0.15))),
                border: cosmic::iced::Border {
                    width: 3.0,
                    radius: 80.0.into(),
                    color: Color::from_rgba(1.0, 0.2, 0.2, 0.6),
                },
                ..Default::default()
            })
            .into()
        }
        ConversationModeState::Processing => {
            // Three dots (simplified - cosmic doesn't have easy animation support)
            widget::container(
                widget::row::with_capacity(3)
                    .push(_dot(Color::from_rgb(0.2, 0.4, 1.0)))
                    .push(_dot(Color::from_rgb(0.2, 0.4, 1.0)))
                    .push(_dot(Color::from_rgb(0.2, 0.4, 1.0)))
                    .spacing(12)
                    .align_y(Alignment::Center)
            )
            .width(Length::Fixed(160.0))
            .height(Length::Fixed(160.0))
            .style(|_theme| cosmic::widget::container::Style {
                background: Some(Background::Color(Color::from_rgba(0.2, 0.4, 1.0, 0.15))),
                border: cosmic::iced::Border {
                    width: 3.0,
                    radius: 80.0.into(),
                    color: Color::from_rgba(0.2, 0.4, 1.0, 0.6),
                },
                ..Default::default()
            })
            .into()
        }
        ConversationModeState::Speaking => {
            // Speaker icon with green circle (tap to stop)
            widget::mouse_area(
                widget::container(
                    crate::ui::icons::get_icon("audio-volume-high-symbolic", 80)
                )
                .width(Length::Fixed(160.0))
                .height(Length::Fixed(160.0))
                .style(|_theme| cosmic::widget::container::Style {
                    background: Some(Background::Color(Color::from_rgba(0.2, 1.0, 0.2, 0.15))),
                    border: cosmic::iced::Border {
                        width: 3.0,
                        radius: 80.0.into(),
                        color: Color::from_rgba(0.2, 1.0, 0.2, 0.6),
                    },
                    ..Default::default()
                })
            )
            .on_press(Message::StopMessageTts)
            .into()
        }
    }
}

fn _dot(color: Color) -> Element<'static, Message> {
    widget::container(widget::Space::with_width(Length::Fixed(18.0)))
        .width(Length::Fixed(18.0))
        .height(Length::Fixed(18.0))
        .style(move |_theme| cosmic::widget::container::Style {
            background: Some(Background::Color(color)),
            border: cosmic::iced::Border {
                width: 0.0,
                radius: 9.0.into(),
                color: Color::TRANSPARENT,
            },
            ..Default::default()
        })
        .into()
}

fn _get_hint_text(state: ConversationModeState) -> &'static str {
    match state {
        ConversationModeState::Listening => "Listening...",
        ConversationModeState::Processing => "Thinking...",
        ConversationModeState::Speaking => "Tap to stop",
    }
}
