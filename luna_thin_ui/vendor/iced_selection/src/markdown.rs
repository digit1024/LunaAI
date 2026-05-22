//! A custom markdown viewer and its corresponding functions.
use iced_widget::graphics::text::Paragraph;
use iced_widget::markdown::{
    Bullet, Catalog, HeadingLevel, Item, Settings, Text, Viewer, view_with,
};
pub use iced_widget::markdown::{Content, Uri, parse};
use iced_widget::{checkbox, column, container, row, scrollable};

use crate::core::Font;
use crate::core::alignment;
use crate::core::padding;
use crate::core::{self, Element, Length, Pixels};
use crate::{rich_text, text};

fn bullet_items(bullet: &Bullet) -> &[Item] {
    match bullet {
        Bullet::Point { items } | Bullet::Task { items, .. } => items,
    }
}

/// Display a bunch of markdown items.
pub fn view<'a, Theme, Renderer>(
    items: impl IntoIterator<Item = &'a Item>,
    settings: impl Into<Settings>,
) -> Element<'a, Uri, Theme, Renderer>
where
    Theme: Catalog + text::Catalog + 'a,
    Renderer: core::text::Renderer<Paragraph = Paragraph, Font = Font> + 'a,
{
    view_with(items, settings, &SelectableViewer)
}
/// Displays a heading using the default look.
pub fn heading<'a, Message, Theme, Renderer>(
    settings: Settings,
    level: &'a HeadingLevel,
    text: &'a Text,
    index: usize,
    on_link_click: impl Fn(Uri) -> Message + 'a,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: Catalog + text::Catalog + 'a,
    Renderer: core::text::Renderer<Paragraph = Paragraph, Font = Font> + 'a,
{
    let Settings {
        h1_size,
        h2_size,
        h3_size,
        h4_size,
        h5_size,
        h6_size,
        text_size,
        ..
    } = settings;

    container(
        rich_text(text.spans(settings.style))
            .on_link_click(on_link_click)
            .size(match level {
                HeadingLevel::H1 => h1_size,
                HeadingLevel::H2 => h2_size,
                HeadingLevel::H3 => h3_size,
                HeadingLevel::H4 => h4_size,
                HeadingLevel::H5 => h5_size,
                HeadingLevel::H6 => h6_size,
            }),
    )
    .padding(padding::top(if index > 0 {
        text_size / 2.0
    } else {
        Pixels::ZERO
    }))
    .into()
}

/// Displays a paragraph using the default look.
pub fn paragraph<'a, Message, Theme, Renderer>(
    settings: Settings,
    text: &Text,
    on_link_click: impl Fn(Uri) -> Message + 'a,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: Catalog + text::Catalog + 'a,
    Renderer: core::text::Renderer<Paragraph = Paragraph, Font = Font> + 'a,
{
    rich_text(text.spans(settings.style))
        .size(settings.text_size)
        .on_link_click(on_link_click)
        .into()
}

/// Displays an unordered list using the default look and
/// calling the [`Viewer`] for each bullet point item.
///
/// [`Viewer`]: https://docs.iced.rs/iced/widget/markdown/trait.Viewer.html
pub fn unordered_list<'a, Message, Theme, Renderer>(
    viewer: &impl Viewer<'a, Message, Theme, Renderer>,
    settings: Settings,
    bullets: &'a [Bullet],
) -> Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: Catalog + text::Catalog + 'a,
    Renderer: core::text::Renderer<Paragraph = Paragraph, Font = Font> + 'a,
{
    column(bullets.iter().map(|bullet| {
        row![
            match bullet {
                Bullet::Point { .. } => {
                    text("•").size(settings.text_size).into()
                }
                Bullet::Task { done, .. } => {
                    Element::from(
                        container(checkbox(*done).size(settings.text_size))
                            .center_y(
                                iced_widget::text::LineHeight::default()
                                    .to_absolute(settings.text_size),
                            ),
                    )
                }
            },
            view_with(
                bullet_items(bullet),
                Settings {
                    spacing: settings.spacing * 0.6,
                    ..settings
                },
                viewer,
            )
        ]
        .spacing(settings.spacing)
        .into()
    }))
    .spacing(settings.spacing * 0.75)
    .padding([0.0, settings.spacing.0])
    .into()
}

/// Displays an ordered list using the default look and
/// calling the [`Viewer`] for each numbered item.
///
/// [`Viewer`]: https://docs.iced.rs/iced/widget/markdown/trait.Viewer.html
pub fn ordered_list<'a, Message, Theme, Renderer>(
    viewer: &impl Viewer<'a, Message, Theme, Renderer>,
    settings: Settings,
    start: u64,
    bullets: &'a [Bullet],
) -> Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: Catalog + text::Catalog + 'a,
    Renderer: core::text::Renderer<Paragraph = Paragraph, Font = Font> + 'a,
{
    let digits = ((start + bullets.len() as u64).max(1) as f32)
        .log10()
        .ceil();

    column(bullets.iter().enumerate().map(|(i, bullet)| {
        row![
            text!("{}.", i as u64 + start)
                .size(settings.text_size)
                .align_x(alignment::Horizontal::Right)
                .width(settings.text_size * ((digits / 2.0).ceil() + 1.0)),
            view_with(
                bullet_items(bullet),
                Settings {
                    spacing: settings.spacing * 0.6,
                    ..settings
                },
                viewer,
            )
        ]
        .spacing(settings.spacing)
        .into()
    }))
    .spacing(settings.spacing * 0.75)
    .into()
}

/// Displays a code block using the default look.
pub fn code_block<'a, Message, Theme, Renderer>(
    settings: Settings,
    lines: &'a [Text],
    on_link_click: impl Fn(Uri) -> Message + Clone + 'a,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: Catalog + text::Catalog + 'a,
    Renderer: core::text::Renderer<Paragraph = Paragraph, Font = Font> + 'a,
{
    container(
        scrollable(
            container(column(lines.iter().map(|line| {
                rich_text(line.spans(settings.style))
                    .on_link_click(on_link_click.clone())
                    .font(Font::MONOSPACE)
                    .size(settings.code_size)
                    .into()
            })))
            .padding(settings.code_size),
        )
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::default()
                .width(settings.code_size / 2)
                .scroller_width(settings.code_size / 2),
        )),
    )
    .width(Length::Fill)
    .padding(settings.code_size / 4)
    .class(Theme::code_block())
    .into()
}

#[derive(Debug, Clone, Copy)]
struct SelectableViewer;

impl<'a, Theme, Renderer> Viewer<'a, Uri, Theme, Renderer> for SelectableViewer
where
    Theme: Catalog + text::Catalog + 'a,
    Renderer: core::text::Renderer<Paragraph = Paragraph, Font = Font> + 'a,
{
    fn on_link_click(url: Uri) -> Uri {
        url
    }

    fn heading(
        &self,
        settings: Settings,
        level: &'a HeadingLevel,
        text: &'a Text,
        index: usize,
    ) -> Element<'a, Uri, Theme, Renderer> {
        heading::<'a, Uri, Theme, Renderer>(
            settings,
            level,
            text,
            index,
            |url| url,
        )
    }

    fn paragraph(
        &self,
        settings: Settings,
        text: &Text,
    ) -> Element<'a, Uri, Theme, Renderer> {
        paragraph(settings, text, |url| url)
    }

    fn unordered_list(
        &self,
        settings: Settings,
        items: &'a [Bullet],
    ) -> Element<'a, Uri, Theme, Renderer> {
        unordered_list(self, settings, items)
    }

    fn ordered_list(
        &self,
        settings: Settings,
        start: u64,
        items: &'a [Bullet],
    ) -> Element<'a, Uri, Theme, Renderer> {
        ordered_list(self, settings, start, items)
    }

    fn code_block(
        &self,
        settings: Settings,
        _language: Option<&'a str>,
        _code: &'a str,
        lines: &'a [Text],
    ) -> Element<'a, Uri, Theme, Renderer> {
        code_block(settings, lines, |url| url)
    }
}
