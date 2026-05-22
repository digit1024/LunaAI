//! Selectable message text via iced_selection (vendored for libcosmic iced).

use cosmic::{
    iced::{self, Length},
    Element, Renderer, Theme,
};

/// Accent used for text selection and markdown table chrome (matches theme).
pub fn accent_highlight(theme: &Theme) -> iced::Color {
    let cosmic = theme.cosmic();
    let accent: iced::Color = cosmic.accent.base.into();
    iced::Color {
        a: 0.55,
        ..accent
    }
}

/// Selection highlight color (see `iced_selection` cosmic catalog).
pub fn bubble_text_style(theme: &Theme) -> iced_selection::text::Style {
    let cosmic = theme.cosmic();
    iced_selection::text::Style {
        color: Some(cosmic.on_bg_color().into()),
        selection: accent_highlight(theme),
    }
}

/// Horizontal/vertical rule style for table borders and header underlines.
pub fn accent_rule_style(theme: &Theme) -> iced::widget::rule::Style {
    iced::widget::rule::Style {
        color: accent_highlight(theme),
        radius: 0.0.into(),
        fill_mode: iced::widget::rule::FillMode::Full,
        snap: true,
    }
}

/// Selectable plain-text widget for chat bubbles (14px, wrapped, themed).
pub fn bubble_text<'a>(content: String) -> Element<'a, crate::ui::app::Message> {
    iced_selection::text::<Theme, Renderer>(content)
        .size(14)
        .width(Length::Fill)
        .wrapping(iced::widget::text::Wrapping::Word)
        .style(bubble_text_style)
        .into()
}
