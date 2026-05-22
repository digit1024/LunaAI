//! Custom markdown viewers with image support.
//!
//! `ImageViewer` — cosmic markdown (summary bubbles).
//! `SelectableImageViewer` — selectable text via iced_selection (assistant bubbles).

use std::collections::HashMap;

use cosmic::{
    iced::{
        alignment,
        widget::{column, row, rule, scrollable, text},
        Border, ContentFit, Length,
    },
    theme,
    widget::{self, container, markdown},
    Element, Renderer, Theme,
};
use iced_selection::markdown::{
    code_block, heading, ordered_list, paragraph, unordered_list,
};

use crate::ui::app::{ImageState, Message};
use crate::ui::widgets::selectable_text;

const TABLE_CELL_PADDING: f32 = 8.0;
const TABLE_LINE: f32 = 1.0;

/// Viewer that renders images from the app's image cache (non-selectable, summary).
pub struct ImageViewer<'a> {
    pub image_cache: &'a HashMap<String, ImageState>,
}

impl<'a> markdown::Viewer<'a, Message, cosmic::Theme, Renderer> for ImageViewer<'a> {
    fn on_link_click(url: markdown::Uri) -> Message {
        Message::OpenUrl(url)
    }

    fn image(
        &self,
        settings: markdown::Settings,
        url: &'a markdown::Uri,
        title: &'a str,
        _alt: &markdown::Text,
    ) -> Element<'a, Message> {
        render_image(self.image_cache, settings, url, title)
    }

    fn table(
        &self,
        settings: markdown::Settings,
        columns: &'a [markdown::Column],
        rows: &'a [markdown::Row],
    ) -> Element<'a, Message> {
        styled_table(self, settings, columns, rows)
    }
}

/// Selectable markdown with image cache (assistant bubbles).
pub struct SelectableImageViewer<'a> {
    pub image_cache: &'a HashMap<String, ImageState>,
}

impl<'a> markdown::Viewer<'a, Message, cosmic::Theme, Renderer> for SelectableImageViewer<'a> {
    fn on_link_click(url: markdown::Uri) -> Message {
        Message::OpenUrl(url)
    }

    fn heading(
        &self,
        settings: markdown::Settings,
        level: &'a markdown::HeadingLevel,
        text: &'a markdown::Text,
        index: usize,
    ) -> Element<'a, Message> {
        heading(settings, level, text, index, Self::on_link_click)
    }

    fn paragraph(
        &self,
        settings: markdown::Settings,
        text: &markdown::Text,
    ) -> Element<'a, Message> {
        paragraph(settings, text, Self::on_link_click)
    }

    fn unordered_list(
        &self,
        settings: markdown::Settings,
        items: &'a [markdown::Bullet],
    ) -> Element<'a, Message> {
        unordered_list(self, settings, items)
    }

    fn ordered_list(
        &self,
        settings: markdown::Settings,
        start: u64,
        items: &'a [markdown::Bullet],
    ) -> Element<'a, Message> {
        ordered_list(self, settings, start, items)
    }

    fn code_block(
        &self,
        settings: markdown::Settings,
        _language: Option<&'a str>,
        _code: &'a str,
        lines: &'a [markdown::Text],
    ) -> Element<'a, Message> {
        code_block(settings, lines, Self::on_link_click)
    }

    fn image(
        &self,
        settings: markdown::Settings,
        url: &'a markdown::Uri,
        title: &'a str,
        _alt: &markdown::Text,
    ) -> Element<'a, Message> {
        render_image(self.image_cache, settings, url, title)
    }

    fn table(
        &self,
        settings: markdown::Settings,
        columns: &'a [markdown::Column],
        rows: &'a [markdown::Row],
    ) -> Element<'a, Message> {
        styled_table(self, settings, columns, rows)
    }
}

/// Full-width table with thin accent borders and underlined headers.
fn styled_table<'a, V>(
    viewer: &V,
    settings: markdown::Settings,
    columns: &'a [markdown::Column],
    rows: &'a [markdown::Row],
) -> Element<'a, Message>
where
    V: markdown::Viewer<'a, Message, cosmic::Theme, Renderer>,
{
    let _ = settings.spacing;
    let mut grid = column![].spacing(0).width(Length::Fill);

    grid = grid.push(table_header_row(viewer, settings, columns));

    for (row_index, row) in rows.iter().enumerate() {
        grid = grid.push(table_data_row(viewer, settings, columns, row));
        if row_index + 1 < rows.len() {
            grid = grid.push(accent_h_rule());
        }
    }

    let table_body = scrollable(grid)
        .width(Length::Fill)
        .height(Length::Shrink);

    container(table_body)
        .width(Length::Fill)
        .style(table_frame_style)
        .into()
}

fn table_header_row<'a, V>(
    viewer: &V,
    settings: markdown::Settings,
    columns: &'a [markdown::Column],
) -> Element<'a, Message>
where
    V: markdown::Viewer<'a, Message, cosmic::Theme, Renderer>,
{
    let mut header = row![].spacing(0).width(Length::Fill);

    for (index, column) in columns.iter().enumerate() {
        if index > 0 {
            header = header.push(accent_v_rule());
        }
        header = header.push(
            container(markdown::items(viewer, settings, &column.header))
                .width(Length::Fill)
                .padding(TABLE_CELL_PADDING)
                .align_x(column_alignment(column.alignment))
                .align_y(alignment::Vertical::Top),
        );
    }

    column![header, accent_h_rule()]
        .spacing(0)
        .width(Length::Fill)
        .into()
}

fn table_data_row<'a, V>(
    viewer: &V,
    settings: markdown::Settings,
    columns: &'a [markdown::Column],
    row: &'a markdown::Row,
) -> Element<'a, Message>
where
    V: markdown::Viewer<'a, Message, cosmic::Theme, Renderer>,
{
    let mut data_row = row![].spacing(0).width(Length::Fill);

    for (index, _column) in columns.iter().enumerate() {
        if index > 0 {
            data_row = data_row.push(accent_v_rule());
        }
        let cells = row.cells.get(index).map(|c| c.as_slice()).unwrap_or(&[]);
        data_row = data_row.push(
            container(markdown::items(viewer, settings, cells))
                .width(Length::Fill)
                .padding(TABLE_CELL_PADDING)
                .align_y(alignment::Vertical::Top),
        );
    }

    data_row.into()
}

fn accent_h_rule() -> Element<'static, Message> {
    rule::horizontal(TABLE_LINE)
        .class(theme::iced::Rule::custom(selectable_text::accent_rule_style))
        .into()
}

fn accent_v_rule() -> Element<'static, Message> {
    rule::vertical(TABLE_LINE)
        .class(theme::iced::Rule::custom(selectable_text::accent_rule_style))
        .into()
}

fn table_frame_style(theme: &Theme) -> container::Style {
    let accent = selectable_text::accent_highlight(theme);
    container::Style {
        border: Border {
            width: TABLE_LINE,
            color: accent,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

fn column_alignment(align: impl std::fmt::Debug) -> alignment::Horizontal {
    match format!("{align:?}").as_str() {
        "Center" => alignment::Horizontal::Center,
        "Right" => alignment::Horizontal::Right,
        _ => alignment::Horizontal::Left,
    }
}

fn render_image<'a>(
    image_cache: &'a HashMap<String, ImageState>,
    settings: markdown::Settings,
    url: &'a str,
    title: &'a str,
) -> Element<'a, Message> {
    match image_cache.get(url) {
        Some(ImageState::Raster(handle)) => container(
            widget::image::Image::new(handle.clone())
                .width(Length::Fill)
                .height(Length::Shrink)
                .content_fit(ContentFit::ScaleDown),
        )
        .width(Length::Fill)
        .padding(settings.spacing.0)
        .into(),

        Some(ImageState::Svg(bytes)) => {
            let svg_handle = widget::svg::Handle::from_memory(bytes.clone());
            container(
                widget::Svg::new(svg_handle)
                    .width(Length::Fill)
                    .height(Length::Shrink),
            )
            .width(Length::Fill)
            .padding(settings.spacing.0)
            .into()
        }

        Some(ImageState::Error(err)) => {
            let label = if title.is_empty() {
                format!("⚠ Image error: {err}")
            } else {
                format!("⚠ {title} (error: {err})")
            };
            container(
                text(label).size(12).class(cosmic::style::Text::Color(
                    cosmic::iced::Color::from_rgb(0.7, 0.4, 0.4),
                )),
            )
            .padding(settings.spacing.0)
            .class(cosmic::style::Container::Card)
            .width(Length::Fill)
            .into()
        }

        Some(ImageState::Fetching) | None => {
            let label = if !title.is_empty() {
                format!("⏳ Loading: {title}")
            } else {
                "⏳ Loading image…".to_string()
            };
            container(
                text(label).size(12).class(cosmic::style::Text::Color(
                    cosmic::iced::Color::from_rgb(0.5, 0.6, 0.7),
                )),
            )
            .padding(settings.spacing.0)
            .class(cosmic::style::Container::Card)
            .width(Length::Fill)
            .into()
        }
    }
}

// ============================================================================
// Helpers: collect image URLs from parsed markdown items
// ============================================================================

fn is_supported_url(url: &str) -> bool {
    url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("file://")
        || url.starts_with("data:")
}

/// Collects all supported image URLs recursively from `items`.
pub fn collect_image_urls(items: &[markdown::Item]) -> Vec<String> {
    let mut urls = Vec::new();
    for item in items {
        collect_from_item(item, &mut urls);
    }
    urls
}

fn collect_from_item(item: &markdown::Item, out: &mut Vec<String>) {
    match item {
        markdown::Item::Image { url, .. } => {
            if is_supported_url(url) && !out.contains(url) {
                out.push(url.clone());
            }
        }
        markdown::Item::Quote(items) => {
            for i in items {
                collect_from_item(i, out);
            }
        }
        markdown::Item::List { bullets, .. } => {
            for bullet in bullets {
                let inner = match bullet {
                    markdown::Bullet::Point { items } | markdown::Bullet::Task { items, .. } => {
                        items
                    }
                };
                for i in inner {
                    collect_from_item(i, out);
                }
            }
        }
        markdown::Item::Table { columns, rows } => {
            for column in columns {
                collect_from_item_slice(&column.header, out);
            }
            for row in rows {
                for cell in &row.cells {
                    collect_from_item_slice(cell, out);
                }
            }
        }
        _ => {}
    }
}

fn collect_from_item_slice(items: &[markdown::Item], out: &mut Vec<String>) {
    for item in items {
        collect_from_item(item, out);
    }
}
