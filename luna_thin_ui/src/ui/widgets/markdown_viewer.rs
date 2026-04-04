//! Custom markdown viewer with image support.
//!
//! `ImageViewer` overrides `Viewer::image` to render images from the app's
//! cache.  Supported sources: http/https (fetched async), file:// (local
//! filesystem), and data: URIs (base64-encoded inline).

use std::collections::HashMap;

use cosmic::{
    iced::{ContentFit, Length},
    widget::{self, container, markdown, text},
    Element, Renderer,
};

use crate::ui::app::{ImageState, Message};

/// Viewer that renders images from the app's image cache.
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
        match self.image_cache.get(url.as_str()) {
            Some(ImageState::Raster(handle)) => container(
                widget::image::Image::new(handle.clone())
                    .width(Length::Fill)
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
                        .height(Length::Shrink)
                        .content_fit(ContentFit::ScaleDown),
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

            _ => {
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
///
/// Supported schemes: `http://`, `https://`, `file://`, `data:`.
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
        _ => {}
    }
}
