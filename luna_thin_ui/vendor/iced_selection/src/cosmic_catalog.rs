//! [`cosmic::Theme`](cosmic::Theme) catalog for selectable text in COSMIC apps.

use cosmic::Theme;
use cosmic::iced::Color;

use crate::text::{Catalog, Style, StyleFn};

impl Catalog for Theme {
    type Class<'a> = StyleFn<'a, Self>;

    fn default<'a>() -> Self::Class<'a> {
        Box::new(default_style)
    }

    fn style(&self, class: &Self::Class<'_>) -> Style {
        class(self)
    }
}

fn default_style(theme: &Theme) -> Style {
    let cosmic = theme.cosmic();
    let accent: Color = cosmic.accent.base.into();
    Style {
        color: Some(cosmic.on_bg_color().into()),
        selection: Color {
            a: 0.55,
            ..accent
        },
    }
}
