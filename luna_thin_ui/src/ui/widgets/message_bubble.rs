//! Message bubble widget - styled chat bubbles with smart corners

use std::collections::HashMap;

use cosmic::{
    iced::{Length, Padding},
    widget::{self, button, container, markdown, text, Column, Row, Space},
    Element,
};

use crate::ui::app::{ImageState, Message};
use crate::ui::icons;
use crate::ui::widgets::markdown_viewer::ImageViewer;

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

fn bubble_markdown_style() -> widget::markdown::Style {
    let iced_theme = if cosmic::theme::is_dark() {
        cosmic::iced::theme::Theme::Dark
    } else {
        cosmic::iced::theme::Theme::Light
    };
    let mut style = widget::markdown::Style::from(iced_theme);
    style.inline_code_padding = cosmic::iced::Padding::from([1, 2]);
    style.inline_code_highlight = widget::markdown::Highlight {
        background: cosmic::iced::Background::Color(cosmic::iced::Color::from_rgb(
            0.1, 0.1, 0.1,
        )),
        border: cosmic::iced::Border::default().rounded(2),
    };
    style.inline_code_color = cosmic::iced::Color::WHITE;
    style.link_color = cosmic::iced::Color::from_rgb(0.3, 0.6, 1.0);
    style
}

// ============================================================================
// User bubble
// ============================================================================

/// Message bubble for user messages (right-aligned, primary color)
pub fn user_bubble<'a>(
    content: &str,
    on_copy: Message,
    on_retry: Option<Message>,
) -> Element<'a, Message> {
    let content_owned = content.to_string();

    let content_widget = container(
        text(content_owned)
            .size(14)
            .class(cosmic::style::Text::Color(
                cosmic::theme::active().cosmic().on_bg_color().into(),
            )),
    )
    .width(Length::Fill);

    let mut buttons = Row::new().push(Space::new().width(Length::Fill));

    if let Some(on_retry) = on_retry {
        buttons = buttons.push(
            button::icon(icons::get_handle("arrow-circular-bottom-right-symbolic", 16))
                .on_press(on_retry)
                .class(cosmic::style::Button::Text)
                .padding(4),
        );
    }

    buttons = buttons
        .push(
            button::icon(icons::get_handle("copy-symbolic", 16))
                .on_press(on_copy)
                .class(cosmic::style::Button::Text)
                .padding(4),
        )
        .spacing(4);

    let message_widget = container(
        Column::new()
            .push(content_widget)
            .push(buttons)
            .spacing(4),
    )
    .padding(Padding::from([12, 16]))
    .style(|theme| cosmic::widget::container::Style {
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
                bottom_right: 0.0,
            },
        },
        ..Default::default()
    })
    .width(Length::FillPortion(7));

    Row::new()
        .push(Space::new().width(Length::FillPortion(3)))
        .push(
            container(message_widget)
                .width(Length::FillPortion(7))
                .padding(Padding::from([12, 0])),
        )
        .into()
}

// ============================================================================
// Assistant bubble
// ============================================================================

/// Message bubble for assistant messages (left-aligned, smart corners)
pub fn assistant_bubble<'a>(
    markdown_items: &'a [markdown::Item],
    image_cache: &'a HashMap<String, ImageState>,
    _content: &str,
    reasoning: Option<&str>,
    is_reasoning_expanded: bool,
    ctx: BubbleContext,
    on_copy: Message,
    on_toggle_reasoning: Option<Message>,
    _on_regenerate: Option<Message>,
    is_current_tts_message: bool,
    on_start_tts: Option<Message>,
    on_stop_tts: Option<Message>,
) -> Element<'a, Message> {
    let reasoning_owned = reasoning.map(|s| s.to_string());

    let top_radius = if ctx.is_prev_assistant { 0.0 } else { 20.0 };
    let bottom_left_radius = 0.0;
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

    let content_widget: Element<'a, Message> = if markdown_items.is_empty() {
        Space::new().height(Length::Fixed(1.0)).into()
    } else {
        let settings =
            markdown::Settings::with_text_size(14.0f32, bubble_markdown_style());
        let viewer = ImageViewer { image_cache };
        container(markdown::view_with(markdown_items, settings, &viewer))
            .width(Length::Fill)
            .into()
    };

    let mut col = Column::new().push(content_widget).spacing(8);

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
                                .class(cosmic::style::Text::Color(
                                    cosmic::iced::Color::from_rgb(0.6, 0.7, 0.9),
                                ))
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

    let mut buttons = Row::new().push(Space::new().width(Length::Fill));

    if is_current_tts_message {
        if let Some(on_stop) = on_stop_tts {
            buttons = buttons.push(
                button::icon(icons::get_handle("stop-symbolic", 16))
                    .on_press(on_stop)
                    .class(cosmic::style::Button::Text)
                    .padding(4),
            );
        }
    } else if let Some(on_start) = on_start_tts {
        buttons = buttons.push(
            button::icon(icons::get_handle("playback-symbolic", 16))
                .on_press(on_start)
                .class(cosmic::style::Button::Text)
                .padding(4),
        );
    }

    buttons = buttons
        .push(
            button::icon(icons::get_handle("copy-symbolic", 16))
                .on_press(on_copy)
                .class(cosmic::style::Button::Text)
                .padding(4),
        )
        .spacing(4);
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

    Row::new()
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
        .push(Space::new().width(Length::FillPortion(3)))
        .into()
}

// ============================================================================
// Summary bubble
// ============================================================================

/// Summary bubble (centered, full width, collapsible)
pub fn summary_bubble<'a>(
    markdown_items: &'a [markdown::Item],
    image_cache: &'a HashMap<String, ImageState>,
    count: usize,
    is_expanded: bool,
    on_toggle: Message,
) -> Element<'a, Message> {
    let toggle_text = format!(
        "{} 📄 Summary ({count} messages)",
        if is_expanded { "▼" } else { "▶" },
    );

    let toggle = button::text(toggle_text)
        .on_press(on_toggle.clone())
        .class(cosmic::style::Button::Text)
        .width(Length::Fill);

    let mut col = Column::new().push(toggle);

    if is_expanded {
        let settings =
            markdown::Settings::with_text_size(14.0f32, bubble_markdown_style());
        let viewer = ImageViewer { image_cache };
        col = col.push(
            container(markdown::view_with(markdown_items, settings, &viewer))
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

// ============================================================================
// Public entry point
// ============================================================================

/// Routes to the appropriate bubble type.
#[allow(clippy::too_many_arguments)]
pub fn message_bubble<'a>(
    markdown_items: &'a [markdown::Item],
    image_cache: &'a HashMap<String, ImageState>,
    content: &str,
    is_user: bool,
    is_summary: bool,
    summarized_count: Option<usize>,
    reasoning: Option<&str>,
    is_reasoning_expanded: bool,
    is_summary_expanded: bool,
    ctx: BubbleContext,
    on_copy: Message,
    on_toggle_reasoning: Option<Message>,
    on_toggle_summary: Option<Message>,
    on_regenerate: Option<Message>,
    is_current_tts_message: bool,
    on_start_tts: Option<Message>,
    on_stop_tts: Option<Message>,
    on_retry: Option<Message>,
) -> Element<'a, Message> {
    if is_summary {
        summary_bubble(
            markdown_items,
            image_cache,
            summarized_count.unwrap_or(0),
            is_summary_expanded,
            on_toggle_summary.unwrap_or_else(|| on_copy.clone()),
        )
    } else if is_user {
        user_bubble(content, on_copy, on_retry)
    } else {
        assistant_bubble(
            markdown_items,
            image_cache,
            content,
            reasoning,
            is_reasoning_expanded,
            ctx,
            on_copy,
            on_toggle_reasoning,
            on_regenerate,
            is_current_tts_message,
            on_start_tts,
            on_stop_tts,
        )
    }
}
