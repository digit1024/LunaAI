//! A text selection API built around [`iced`]'s [`Paragraph`].
//!
//! [`iced`]: https://iced.rs
//! [`Paragraph`]: https://docs.iced.rs/iced_graphics/text/paragraph/struct.Paragraph.html

mod click;
#[cfg(feature = "cosmic")]
mod cosmic_catalog;
#[cfg(feature = "markdown")]
pub mod markdown;
pub mod selection;
pub mod text;

use iced_widget::core;
use iced_widget::graphics::text::Paragraph;
#[cfg(feature = "markdown")]
pub use markdown::view as markdown;
#[doc(no_inline)]
pub use text::Text;

/// Creates a new [`Text`] widget with the provided content.
///
/// [`Text`]: core::widget::Text
///
/// This macro uses the same syntax as [`format!`], but creates a new [`Text`] widget instead.
///
/// See [the formatting documentation in `std::fmt`](std::fmt)
/// for details of the macro argument syntax.
///
/// # Examples
///
/// ```no_run,ignore
/// use iced_selection::text;
///
/// enum Message {
///     // ...
/// }
///
/// fn view(_state: &State) -> Element<Message> {
///     let simple = text!("Hello, world!");
///
///     let keyword = text!("Hello, {}", "world!");
///
///     let planet = "Earth";
///     let local_variable = text!("Hello, {planet}!");
///     // ...
/// }
/// ```
#[macro_export]
macro_rules! text {
    ($($arg:tt)*) => {
        $crate::Text::new(format!($($arg)*))
    };
}

/// Creates some [`Rich`] text with the given spans.
///
/// # Example
/// ```no_run,ignore
/// use iced::font;
/// use iced_selection::{rich_text, span};
/// use iced::{color, never, Font};
///
/// #[derive(Debug, Clone)]
/// enum Message {
///     // ...
/// }
///
/// fn view(state: &State) -> Element<'_, Message> {
///     rich_text![
///         span("I am red!").color(color!(0xff0000)),
///         span(" "),
///         span("And I am bold!").font(Font { weight: font::Weight::Bold, ..Font::default() }),
///     ]
///     .on_link_click(never)
///     .size(20)
///     .into()
/// }
/// ```
///
/// [`Rich`]: crate::text::Rich
#[macro_export]
macro_rules! rich_text {
    () => (
        $crate::text::Rich::new()
    );
    ($($x:expr),+ $(,)?) => (
        $crate::text::Rich::from_iter([$($crate::text::Span::from($x)),+])
    );
}

/// Creates a new [`Span`] of text with the provided content.
///
/// A [`Span`] is a fragment of some [`Rich`] text.
///
/// This macro uses the same syntax as [`format!`], but creates a new [`Span`] widget instead.
///
/// See [the formatting documentation in `std::fmt`](std::fmt)
/// for details of the macro argument syntax.
///
/// # Example
/// ```no_run,ignore
/// use iced::font;
/// use iced_selection::{rich_text, span};
/// use iced::{color, never, Font};
///
/// #[derive(Debug, Clone)]
/// enum Message {
///     // ...
/// }
///
/// fn view(state: &State) -> Element<'_, Message> {
///     rich_text![
///         span!("I am {}!", red).color(color!(0xff0000)),
///         " ",
///         span!("And I am bold!").font(Font { weight: font::Weight::Bold, ..Font::default() }),
///     ]
///     .on_link_click(never)
///     .size(20)
///     .into()
/// }
/// ```
///
/// [`Rich`]: crate::text::Rich
/// [`Span`]: crate::text::Span
#[macro_export]
macro_rules! span {
    ($($arg:tt)*) => {
        $crate::text::Span::new(format!($($arg)*))
    };
}

/// Creates a new [`Text`] widget with the provided content.
///
/// # Example
/// ```no_run,ignore
/// use iced_selection::text;
///
/// enum Message {
///     // ...
/// }
///
/// fn view(state: &State) -> Element<'_, Message> {
///     text("Hello, this is iced!")
///         .size(20)
///         .into()
/// }
/// ```
pub fn text<'a, Theme, Renderer>(
    text: impl text::IntoFragment<'a>,
) -> Text<'a, Theme, Renderer>
where
    Theme: text::Catalog + 'a,
    Renderer: core::text::Renderer,
{
    Text::new(text)
}

/// Creates some [`Rich`] text with the given spans.
///
/// [`Rich`]: crate::text::Rich
///
/// # Example
/// ```no_run,ignore
/// use iced::font;
/// use iced_selection::{rich_text, span};
/// use iced::{color, never, Font};
///
/// #[derive(Debug, Clone)]
/// enum Message {
///     LinkClicked(&'static str),
///     // ...
/// }
///
/// fn view(state: &State) -> Element<'_, Message> {
///     rich_text([
///         span("I am red!").color(color!(0xff0000)),
///         span(" "),
///         span("And I am bold!").font(Font { weight: font::Weight::Bold, ..Font::default() }),
///     ])
///     .on_link_click(never)
///     .size(20)
///     .into()
/// }
/// ```
pub fn rich_text<'a, Link, Message, Theme, Renderer>(
    spans: impl AsRef<[text::Span<'a, Link, core::Font>]> + 'a,
) -> text::Rich<'a, Link, Message, Theme, Renderer>
where
    Link: Clone + 'static,
    Theme: text::Catalog + 'a,
    Renderer: core::text::Renderer<Paragraph = Paragraph, Font = core::Font>,
{
    text::Rich::with_spans(spans)
}

/// Creates a new [`Span`] of text with the provided content.
///
/// A [`Span`] is a fragment of some [`Rich`] text.
///
/// [`Rich`]: crate::text::Rich
/// [`Span`]: crate::text::Span
///
/// # Example
/// ```no_run,ignore
/// use iced::font;
/// use iced_selection::{rich_text, span};
/// use iced::{color, never, Font};
///
/// #[derive(Debug, Clone)]
/// enum Message {
///     // ...
/// }
///
/// fn view(state: &State) -> Element<'_, Message> {
///     rich_text![
///         span("I am red!").color(color!(0xff0000)),
///         " ",
///         span("And I am bold!").font(Font { weight: font::Weight::Bold, ..Font::default() }),
///     ]
///     .on_link_click(never)
///     .size(20)
///     .into()
/// }
/// ```
pub fn span<'a, Link>(
    text: impl text::IntoFragment<'a>,
) -> text::Span<'a, Link, core::Font> {
    text::Span::new(text)
}
