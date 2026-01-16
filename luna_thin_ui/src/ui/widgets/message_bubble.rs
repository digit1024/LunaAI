//! Message bubble widget - styled chat bubbles with smart corners

use cosmic::{
    iced::{Length, Padding},
    widget::{self, button, column, container, markdown, row, text, Space},
    Element,
};
use crate::ui::icons;

/// Context for rendering message bubbles with smart corners
#[derive(Debug, Clone, Copy)]
pub struct BubbleContext {
    pub is_prev_user: bool,
    pub is_prev_assistant: bool,
    pub is_next_assistant: bool,
    pub has_next: bool,
}

impl Default for BubbleContext {
    fn default() -> Self {
        Self {
            is_prev_user: false,
            is_prev_assistant: false,
            is_next_assistant: false,
            has_next: false,
        }
    }
}

/// Message bubble for user messages (right-aligned, primary color)
pub fn user_bubble<M: Clone + 'static>(
    content: &str,
    on_copy: M,
) -> Element<'static, M> {
    let content_owned = content.to_string();
    
    let content_widget = container(
        text(content_owned)
            .size(14)
            .class(cosmic::style::Text::Color(
                cosmic::theme::active().cosmic().on_bg_color().into(),
            )),
    )
    .width(Length::Fill);

    // Copy button row
    let buttons = row()
        .push(Space::with_width(Length::Fill))
        .push(
            button::icon(icons::get_handle("copy-symbolic", 16))
                .on_press(on_copy)
                .class(cosmic::style::Button::Text)
                .padding(4),
        )
        .spacing(4);

    let message_widget = container(
        column()
            .push(content_widget)
            .push(buttons)
            .spacing(4),
    )
    .padding(Padding::from([12, 16]))
    .style(|theme| {
        cosmic::widget::container::Style {
            background: Some(cosmic::iced::Background::Color(
                theme.cosmic().primary.component.hover.into(),
            )),
            border: cosmic::iced::Border {
                width: 0.0,
                color: cosmic::iced::Color::TRANSPARENT,
                radius: cosmic::iced::border::Radius {
                    top_left: 20.0,
                    top_right: 20.0,
                    bottom_left: 20.0,
                    bottom_right: 0.0, // Not rounded on right bottom
                },
            },
            ..Default::default()
        }
    })
    .width(Length::FillPortion(7));

    // Right-aligned row
    row()
        .push(Space::with_width(Length::FillPortion(3)))
        .push(
            container(message_widget)
                .width(Length::FillPortion(7))
                .padding(Padding::from([12, 0])),
        )
        .into()
}

/// Message bubble for assistant messages (left-aligned, smart corners)
pub fn assistant_bubble<M: Clone + 'static>(
    content: &str,
    reasoning: Option<&str>,
    is_reasoning_expanded: bool,
    ctx: BubbleContext,
    on_copy: M,
    on_toggle_reasoning: Option<M>,
    on_regenerate: Option<M>,
    is_current_tts_message: bool, // true if THIS message is being spoken
    on_start_tts: Option<M>, // Start TTS for this message
    on_stop_tts: Option<M>, // Stop TTS (only shown when is_current_tts_message == true)
) -> Element<'static, M> {
    let content_owned = content.to_string();
    let reasoning_owned = reasoning.map(|s| s.to_string());
    
    // Calculate smart corners
    let top_radius = if ctx.is_prev_assistant { 0.0 } else { 20.0 };
    let bottom_left_radius = 0.0; // Always 0 for AI
    let bottom_right_radius = if !ctx.has_next || !ctx.is_next_assistant {
        20.0
    } else {
        0.0
    };
    let top_margin = if ctx.is_prev_assistant { 0.0 } else { 12.0 };
    let bottom_margin = if !ctx.has_next || !ctx.is_next_assistant {
        12.0
    } else {
        0.0
    };

    // Main content with markdown
    let content_widget: Element<'static, M> = if content_owned.is_empty() {
        Space::with_height(Length::Fixed(1.0)).into()
    } else {
        let items: Vec<_> = markdown::parse(&content_owned).collect();
        let settings = widget::markdown::Settings::with_text_size(14.0);
        let style = widget::markdown::Style {
            inline_code_padding: cosmic::iced::Padding::from([1, 2]),
            inline_code_highlight: widget::markdown::Highlight {
                background: cosmic::iced::Background::Color(cosmic::iced::Color::from_rgb(
                    0.1, 0.1, 0.1,
                )),
                border: cosmic::iced::Border::default().rounded(2),
            },
            inline_code_color: cosmic::iced::Color::WHITE,
            link_color: cosmic::iced::Color::from_rgb(0.3, 0.6, 1.0),
        };
        let on_copy_clone = on_copy.clone();
        container(
            widget::markdown(&items, settings, style).map(move |_| on_copy_clone.clone()),
        )
        .width(Length::Fill)
        .into()
    };

    let mut col = column().push(content_widget).spacing(8);

    // Reasoning toggle
    if let Some(reasoning_content) = reasoning_owned {
        if !reasoning_content.is_empty() {
            let toggle_text = format!(
                "{} 💭 Thinking",
                if is_reasoning_expanded { "▼" } else { "▶" }
            );

            if let Some(on_toggle) = on_toggle_reasoning {
                col = col.push(
                    button::text(toggle_text)
                        .on_press(on_toggle)
                        .class(cosmic::style::Button::Text)
                        .width(Length::Fill),
                );

                if is_reasoning_expanded {
                    col = col.push(
                        container(
                            text(reasoning_content)
                                .size(12)
                                .font(cosmic::font::Font::MONOSPACE)
                                .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(
                                    0.6, 0.7, 0.9,
                                )))
                                .width(Length::Fill),
                        )
                        .padding(Padding::from([8, 12]))
                        .class(cosmic::style::Container::Card)
                        .width(Length::Fill),
                    );
                }
            }
        }
    }

    // Button row: Regenerate, TTS (Play/Stop), Copy (order: Regenerate, TTS, Copy)
    let mut buttons = row()
        .push(Space::with_width(Length::Fill));
    
    // Regenerate button (agent only)
    if let Some(on_regenerate) = on_regenerate {
        buttons = buttons.push(
            button::icon(icons::get_handle("arrow-circular-bottom-right-symbolic", 16))
                .on_press(on_regenerate)
                .class(cosmic::style::Button::Text)
                .padding(4),
        );
    }
    
    // TTS button: Play or Stop based on current state
    if is_current_tts_message {
        // This message is being spoken - show STOP button
        if let Some(on_stop) = on_stop_tts {
            buttons = buttons.push(
                button::icon(icons::get_handle("stop-symbolic", 16))
                    .on_press(on_stop)
                    .class(cosmic::style::Button::Text)
                    .padding(4),
            );
        }
    } else {
        // This message is NOT being spoken - show PLAY button
        if let Some(on_start) = on_start_tts {
            buttons = buttons.push(
                button::icon(icons::get_handle("playback-symbolic", 16))
                    .on_press(on_start)
                    .class(cosmic::style::Button::Text)
                    .padding(4),
            );
        }
    }
    
    // Copy button (always present)
    buttons = buttons.push(
        button::icon(icons::get_handle("copy-symbolic", 16))
            .on_press(on_copy)
            .class(cosmic::style::Button::Text)
            .padding(4),
    );
    
    buttons = buttons.spacing(4);
    col = col.push(buttons);

    let message_widget = container(col)
        .padding(Padding::from([12, 16]))
        .style(move |theme| cosmic::widget::container::Style {
            background: Some(cosmic::iced::Background::Color(
                theme.cosmic().background.component.hover.into(),
            )),
            border: cosmic::iced::Border {
                width: 0.0,
                color: cosmic::iced::Color::TRANSPARENT,
                radius: cosmic::iced::border::Radius {
                    top_left: top_radius,
                    top_right: top_radius,
                    bottom_left: bottom_left_radius,
                    bottom_right: bottom_right_radius,
                },
            },
            ..Default::default()
        })
        .width(Length::FillPortion(7));

    // Left-aligned row with margins
    row()
        .push(
            container(message_widget)
                .width(Length::FillPortion(7))
                .padding(cosmic::iced::Padding {
                    top: top_margin,
                    bottom: bottom_margin,
                    left: 0.0,
                    right: 0.0,
                }),
        )
        .push(Space::with_width(Length::FillPortion(3)))
        .into()
}

/// Summary bubble (centered, full width, collapsible)
pub fn summary_bubble<M: Clone + 'static>(
    content: &str,
    count: usize,
    is_expanded: bool,
    on_toggle: M,
) -> Element<'static, M> {
    let content_owned = content.to_string();
    
    let toggle_text = format!(
        "{} 📄 Summary ({} messages)",
        if is_expanded { "▼" } else { "▶" },
        count
    );

    let on_toggle_clone = on_toggle.clone();
    let toggle = button::text(toggle_text)
        .on_press(on_toggle)
        .class(cosmic::style::Button::Text)
        .width(Length::Fill);

    let mut col = column().push(toggle);

    if is_expanded {
        let items: Vec<_> = markdown::parse(&content_owned).collect();
        let settings = widget::markdown::Settings::with_text_size(14.0);
        let style = widget::markdown::Style {
            inline_code_padding: cosmic::iced::Padding::from([1, 2]),
            inline_code_highlight: widget::markdown::Highlight {
                background: cosmic::iced::Background::Color(cosmic::iced::Color::from_rgb(
                    0.1, 0.1, 0.1,
                )),
                border: cosmic::iced::Border::default().rounded(2),
            },
            inline_code_color: cosmic::iced::Color::WHITE,
            link_color: cosmic::iced::Color::from_rgb(0.3, 0.6, 1.0),
        };
        col = col.push(
            container(widget::markdown(&items, settings, style).map(move |_| on_toggle_clone.clone()))
                .width(Length::Fill)
                .padding(Padding::from([8, 12])),
        );
    }

    container(col)
        .padding(Padding::from([12, 16]))
        .class(cosmic::style::Container::Card)
        .width(Length::Fill)
        .into()
}

/// Main entry point - routes to appropriate bubble type
pub fn message_bubble<M: Clone + 'static>(
    content: &str,
    is_user: bool,
    is_summary: bool,
    summarized_count: Option<usize>,
    reasoning: Option<&str>,
    is_reasoning_expanded: bool,
    is_summary_expanded: bool,
    ctx: BubbleContext,
    on_copy: M,
    on_toggle_reasoning: Option<M>,
    on_toggle_summary: Option<M>,
    on_regenerate: Option<M>,
    is_current_tts_message: bool, // true if THIS message is being spoken
    on_start_tts: Option<M>, // Start TTS for this message
    on_stop_tts: Option<M>, // Stop TTS (only shown when is_current_tts_message == true)
) -> Element<'static, M> {
    if is_summary {
        summary_bubble(content, summarized_count.unwrap_or(0), is_summary_expanded, on_toggle_summary.unwrap_or_else(|| on_copy.clone()))
    } else if is_user {
        user_bubble(content, on_copy)
    } else {
        assistant_bubble(content, reasoning, is_reasoning_expanded, ctx, on_copy, on_toggle_reasoning, on_regenerate, is_current_tts_message, on_start_tts, on_stop_tts)
    }
}
