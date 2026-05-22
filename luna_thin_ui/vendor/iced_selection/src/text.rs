//! Text widgets display information through writing.
//!
//! Keyboard shortcuts (applies to both [`Text`] and [`Rich`]):
//!
//! |MacOS|Linux/Windows|Effect|
//! |-|-|-|
//! |`Cmd + A`|`Ctrl + A`|Selects all text in the currently focused paragraph|
//! |`Cmd + C`|`Ctrl + C`|Copies the selected text to clipboard|
//! |`Shift + Left Arrow`|`Shift + Left Arrow`|Moves the selection to the left by one character|
//! |`Shift + Right Arrow`|`Shift + Right Arrow`|Moves the selection to the right by one character|
//! |`Shift + Opt + Left Arrow`|`Shift + Ctrl + Left Arrow`|Extends the selection to the previous start of a word|
//! |`Shift + Opt + Right Arrow`|`Shift + Ctrl + Right Arrow`|Extends the selection to the next end of a word|
//! |`Shift + Home`<br>`Shift + Cmd + Left Arrow`<br>`Shift + Opt + Up Arrow`|`Shift + Home`<br>`Shift + Ctrl + Up Arrow`|Selects to the beginning of the line|
//! |`Shift + End`<br>`Shift + Cmd + Right Arrow`<br>`Shift + Opt + Down Arrow`|`Shift + End`<br>`Shift + Ctrl + Down Arrow`|Selects to the end of the line|
//! |`Shift + Up Arrow`|`Shift + Up Arrow`|Moves the selection up by one line if possible, or to the start of the current line otherwise|
//! |`Shift + Down Arrow`|`Shift + Down Arrow`|Moves the selection down by one line if possible, or to the end of the current line otherwise|
//! |`Shift + Opt + Home`<br>`Shift + Cmd + Up Arrow`|`Shift + Ctrl + Home`|Selects to the beginning of the paragraph|
//! |`Shift + Opt + End`<br>`Shift + Cmd + Down Arrow`|`Shift + Ctrl + End`|Selects to the end of the paragraph|
mod rich;

use iced_widget::graphics::text::Paragraph;
use iced_widget::graphics::text::cosmic_text;
pub use rich::Rich;
use text::{Alignment, LineHeight, Shaping, Wrapping};
pub use text::{Fragment, Highlighter, IntoFragment, Span};

use crate::click;
use crate::core::alignment;
use crate::core::clipboard;
use crate::core::keyboard::{self, key};
use crate::core::layout;
use crate::core::mouse;
use crate::core::renderer;
use crate::core::text;
use crate::core::text::paragraph::Paragraph as _;
use crate::core::time::Duration;
use crate::core::touch;
use crate::core::widget::Operation;
use crate::core::widget::text::Format;
use crate::core::widget::tree::{self, Tree};
use crate::core::{
    self, Color, Element, Event, Font, Layout, Length, Pixels, Point, Size,
    Theme, Widget,
};
use crate::selection::{Selection, SelectionEnd};

/// A bunch of text.
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
pub struct Text<
    'a,
    Theme = iced_widget::Theme,
    Renderer = iced_widget::Renderer,
> where
    Theme: Catalog,
    Renderer: text::Renderer,
{
    fragment: Fragment<'a>,
    format: Format<Renderer::Font>,
    click_interval: Option<Duration>,
    class: Theme::Class<'a>,
}

impl<'a, Theme, Renderer> Text<'a, Theme, Renderer>
where
    Theme: Catalog,
    Renderer: text::Renderer,
{
    /// Create a new fragment of [`Text`] with the given contents.
    pub fn new(fragment: impl IntoFragment<'a>) -> Self {
        Self {
            fragment: fragment.into_fragment(),
            format: Format::default(),
            click_interval: None,
            class: Theme::default(),
        }
    }

    /// Sets the size of the [`Text`].
    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.format.size = Some(size.into());
        self
    }

    /// Sets the [`LineHeight`] of the [`Text`].
    pub fn line_height(mut self, line_height: impl Into<LineHeight>) -> Self {
        self.format.line_height = line_height.into();
        self
    }

    /// Sets the [`Font`] of the [`Text`].
    pub fn font(mut self, font: impl Into<Renderer::Font>) -> Self {
        self.format.font = Some(font.into());
        self
    }

    /// Sets the width of the [`Text`] boundaries.
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.format.width = width.into();
        self
    }

    /// Sets the height of the [`Text`] boundaries.
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.format.height = height.into();
        self
    }

    /// Centers the [`Text`], both horizontally and vertically.
    pub fn center(mut self) -> Self {
        self.format.align_x = Alignment::Center;
        self.format.align_y = alignment::Vertical::Center;
        self
    }

    /// Sets the [`alignment::Horizontal`] of the [`Text`].
    pub fn align_x(mut self, alignment: impl Into<text::Alignment>) -> Self {
        self.format.align_x = alignment.into();
        self
    }

    /// Sets the [`alignment::Vertical`] of the [`Text`].
    pub fn align_y(
        mut self,
        alignment: impl Into<alignment::Vertical>,
    ) -> Self {
        self.format.align_y = alignment.into();
        self
    }

    /// Sets the [`Shaping`] strategy of the [`Text`].
    pub fn shaping(mut self, shaping: Shaping) -> Self {
        self.format.shaping = shaping;
        self
    }

    /// Sets the [`Wrapping`] strategy of the [`Text`].
    pub fn wrapping(mut self, wrapping: Wrapping) -> Self {
        self.format.wrapping = wrapping;
        self
    }

    /// The maximum delay required for two consecutive clicks to be interpreted as a double click
    /// (also applies to triple clicks).
    ///
    /// Defaults to 300ms.
    pub fn click_interval(mut self, click_interval: Duration) -> Self {
        self.click_interval = Some(click_interval);
        self
    }

    /// Sets the style of the [`Text`].
    #[must_use]
    pub fn style(mut self, style: impl Fn(&Theme) -> Style + 'a) -> Self
    where
        Theme::Class<'a>: From<StyleFn<'a, Theme>>,
    {
        self.class = (Box::new(style) as StyleFn<'a, Theme>).into();
        self
    }

    /// Sets the style class of the [`Text`].
    #[must_use]
    pub fn class(mut self, class: impl Into<Theme::Class<'a>>) -> Self {
        self.class = class.into();
        self
    }
}

/// The internal state of a [`Text`] widget.
#[derive(Debug, Default, Clone)]
pub struct State {
    paragraph: Paragraph,
    content: String,
    is_hovered: bool,
    selection: Selection,
    dragging: Option<Dragging>,
    last_click: Option<click::Click>,
    keyboard_modifiers: keyboard::Modifiers,
    visual_lines_bounds: Vec<core::Rectangle>,
}

/// The type of dragging selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum Dragging {
    Grapheme,
    Word,
    Line,
}

impl State {
    fn grapheme_line_and_index(
        &self,
        point: Point,
        bounds: core::Rectangle,
    ) -> Option<(usize, usize)> {
        let bounded_x = if point.y < bounds.y {
            bounds.x
        } else if point.y > bounds.y + bounds.height {
            bounds.x + bounds.width
        } else {
            point.x.max(bounds.x).min(bounds.x + bounds.width)
        };
        let bounded_y = point.y.max(bounds.y).min(bounds.y + bounds.height);
        let bounded_point = Point::new(bounded_x, bounded_y);
        let mut relative_point =
            bounded_point - core::Vector::new(bounds.x, bounds.y);

        let buffer = self.paragraph.buffer();
        let line_height = buffer.metrics().line_height;
        let visual_line = (relative_point.y / line_height).floor() as usize;
        let visual_line_start_offset = self
            .visual_lines_bounds
            .get(visual_line)
            .map(|r| r.x)
            .unwrap_or_default();
        let visual_line_end = self
            .visual_lines_bounds
            .get(visual_line)
            .map(|r| r.x + r.width)
            .unwrap_or_default();

        if relative_point.x < visual_line_start_offset {
            relative_point.x = visual_line_start_offset;
        }

        if relative_point.x > visual_line_end {
            relative_point.x = visual_line_end;
        }

        let cursor = buffer.hit(relative_point.x, relative_point.y)?;
        let value = buffer.lines[cursor.line].text();

        Some((
            cursor.line,
            unicode_segmentation::UnicodeSegmentation::graphemes(
                &value[..cursor.index.min(value.len())],
                true,
            )
            .count(),
        ))
    }

    fn selection(&self) -> Vec<core::Rectangle> {
        let Selection { start, end, .. } = self.selection;

        let buffer = self.paragraph.buffer();
        let line_height = self.paragraph.buffer().metrics().line_height;
        let selected_lines = end.line - start.line + 1;

        let visual_lines_offset = visual_lines_offset(start.line, buffer);

        buffer
            .lines
            .iter()
            .skip(start.line)
            .take(selected_lines)
            .enumerate()
            .flat_map(|(i, line)| {
                highlight_line(
                    line,
                    if i == 0 { start.index } else { 0 },
                    if i == selected_lines - 1 {
                        end.index
                    } else {
                        line.text().len()
                    },
                )
            })
            .enumerate()
            .filter_map(|(visual_line, (x, width))| {
                if width > 0.0 {
                    Some(core::Rectangle {
                        x,
                        width,
                        y: (visual_line as i32 + visual_lines_offset) as f32
                            * line_height
                            - buffer.scroll().vertical,
                        height: line_height,
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    fn update(&mut self, text: text::Text<&str, Font>) {
        if self.content != text.content {
            text.content.clone_into(&mut self.content);
            self.paragraph = Paragraph::with_text(text);
            self.update_visual_bounds();
            return;
        }

        match self.paragraph.compare(text.with_content(())) {
            text::Difference::None => {}
            text::Difference::Bounds => {
                self.paragraph.resize(text.bounds);
                self.update_visual_bounds();
            }
            text::Difference::Shape => {
                self.paragraph = Paragraph::with_text(text);
                self.update_visual_bounds();
            }
        }
    }

    fn update_visual_bounds(&mut self) {
        let buffer = self.paragraph.buffer();
        let line_height = buffer.metrics().line_height;
        self.visual_lines_bounds = buffer
            .lines
            .iter()
            .flat_map(|line| highlight_line(line, 0, line.text().len()))
            .enumerate()
            .map(|(visual_line, (x, width))| core::Rectangle {
                x,
                width,
                y: visual_line as f32 * line_height - buffer.scroll().vertical,
                height: line_height,
            })
            .collect();
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Text<'_, Theme, Renderer>
where
    Theme: Catalog,
    Renderer: text::Renderer<Paragraph = Paragraph, Font = Font>,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: self.format.width,
            height: self.format.height,
        }
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout(
            tree.state.downcast_mut::<State>(),
            renderer,
            limits,
            &self.fragment,
            self.format,
        )
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        clipboard: &mut dyn core::Clipboard,
        shell: &mut core::Shell<'_, Message>,
        viewport: &core::Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();

        let bounds = layout.bounds();
        let click_position = cursor.position_in(bounds);

        if viewport.intersection(&bounds).is_none()
            && state.selection.is_empty()
            && state.dragging.is_none()
        {
            return;
        }

        let was_hovered = state.is_hovered;
        let selection_before = state.selection;
        state.is_hovered = click_position.is_some();

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerPressed { .. }) => {
                if let Some(position) = cursor.position_over(bounds) {
                    let click = click::Click::new(
                        position,
                        mouse::Button::Left,
                        state.last_click,
                        self.click_interval,
                    );

                    let (line, index) = state
                        .grapheme_line_and_index(position, bounds)
                        .unwrap_or((0, 0));

                    match click.kind() {
                        click::Kind::Single => {
                            let new_end = SelectionEnd { line, index };

                            if state.keyboard_modifiers.shift() {
                                state.selection.change_selection(new_end);
                            } else {
                                state.selection.select_range(new_end, new_end);
                            }

                            state.dragging = Some(Dragging::Grapheme);
                        }
                        click::Kind::Double => {
                            state.selection.select_word(
                                line,
                                index,
                                &state.paragraph,
                            );

                            state.dragging = Some(Dragging::Word);
                        }
                        click::Kind::Triple => {
                            state.selection.select_line(line, &state.paragraph);
                            state.dragging = Some(Dragging::Line);
                        }
                    }

                    state.last_click = Some(click);

                    shell.capture_event();
                } else {
                    state.selection = Selection::default();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerLifted { .. })
            | Event::Touch(touch::Event::FingerLost { .. }) => {
                state.dragging = None;
            }
            Event::Mouse(mouse::Event::CursorMoved { .. })
            | Event::Touch(touch::Event::FingerMoved { .. }) => {
                if let Some(position) = cursor.land().position()
                    && let Some(dragging) = state.dragging
                {
                    let (line, index) = state
                        .grapheme_line_and_index(position, bounds)
                        .unwrap_or((0, 0));

                    match dragging {
                        Dragging::Grapheme => {
                            let new_end = SelectionEnd { line, index };

                            state.selection.change_selection(new_end);
                        }
                        Dragging::Word => {
                            let new_end = SelectionEnd { line, index };

                            state.selection.change_selection_by_word(
                                new_end,
                                &state.paragraph,
                            );
                        }
                        Dragging::Line => {
                            state.selection.change_selection_by_line(
                                line,
                                &state.paragraph,
                            );
                        }
                    };
                }
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) => {
                match key.as_ref() {
                    keyboard::Key::Character("c")
                        if state.keyboard_modifiers.command()
                            && !state.selection.is_empty() =>
                    {
                        clipboard.write(
                            clipboard::Kind::Standard,
                            state.selection.text(&state.paragraph),
                        );

                        shell.capture_event();
                    }
                    keyboard::Key::Character("a")
                        if state.keyboard_modifiers.command()
                            && !state.selection.is_empty() =>
                    {
                        state.selection.select_all(&state.paragraph);

                        shell.capture_event();
                    }
                    keyboard::Key::Named(key::Named::Home)
                        if state.keyboard_modifiers.shift()
                            && !state.selection.is_empty() =>
                    {
                        if state.keyboard_modifiers.jump() {
                            state.selection.select_beginning();
                        } else {
                            state.selection.select_line_beginning();
                        }

                        shell.capture_event();
                    }
                    keyboard::Key::Named(key::Named::End)
                        if state.keyboard_modifiers.shift()
                            && !state.selection.is_empty() =>
                    {
                        if state.keyboard_modifiers.jump() {
                            state.selection.select_end(&state.paragraph);
                        } else {
                            state.selection.select_line_end(&state.paragraph);
                        }

                        shell.capture_event();
                    }
                    keyboard::Key::Named(key::Named::ArrowLeft)
                        if state.keyboard_modifiers.shift()
                            && !state.selection.is_empty() =>
                    {
                        if state.keyboard_modifiers.macos_command() {
                            state.selection.select_line_beginning();
                        } else if state.keyboard_modifiers.jump() {
                            state
                                .selection
                                .select_left_by_words(&state.paragraph);
                        } else {
                            state.selection.select_left(&state.paragraph);
                        }

                        shell.capture_event();
                    }
                    keyboard::Key::Named(key::Named::ArrowRight)
                        if state.keyboard_modifiers.shift()
                            && !state.selection.is_empty() =>
                    {
                        if state.keyboard_modifiers.macos_command() {
                            state.selection.select_line_end(&state.paragraph);
                        } else if state.keyboard_modifiers.jump() {
                            state
                                .selection
                                .select_right_by_words(&state.paragraph);
                        } else {
                            state.selection.select_right(&state.paragraph);
                        }

                        shell.capture_event();
                    }
                    keyboard::Key::Named(key::Named::ArrowUp)
                        if state.keyboard_modifiers.shift()
                            && !state.selection.is_empty() =>
                    {
                        if state.keyboard_modifiers.macos_command() {
                            state.selection.select_beginning();
                        } else if state.keyboard_modifiers.jump() {
                            state.selection.select_line_beginning();
                        } else {
                            state.selection.select_up(&state.paragraph);
                        }

                        shell.capture_event();
                    }
                    keyboard::Key::Named(key::Named::ArrowDown)
                        if state.keyboard_modifiers.shift()
                            && !state.selection.is_empty() =>
                    {
                        if state.keyboard_modifiers.macos_command() {
                            state.selection.select_end(&state.paragraph);
                        } else if state.keyboard_modifiers.jump() {
                            state.selection.select_line_end(&state.paragraph);
                        } else {
                            state.selection.select_down(&state.paragraph);
                        }

                        shell.capture_event();
                    }
                    keyboard::Key::Named(key::Named::Escape) => {
                        state.dragging = None;
                        state.selection = Selection::default();

                        state.keyboard_modifiers =
                            keyboard::Modifiers::default();

                        if state.selection != selection_before {
                            shell.capture_event();
                        }
                    }
                    _ => {}
                }
            }
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.keyboard_modifiers = *modifiers;
            }
            _ => {}
        }

        if state.is_hovered != was_hovered
            || state.selection != selection_before
        {
            shell.request_redraw();
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        defaults: &renderer::Style,
        layout: Layout<'_>,
        _cursor_position: mouse::Cursor,
        viewport: &core::Rectangle,
    ) {
        if !layout.bounds().intersects(viewport) {
            return;
        }

        let state = tree.state.downcast_ref::<State>();
        let style = theme.style(&self.class);

        if !state.selection.is_empty() {
            let bounds = layout.bounds();
            let translation = bounds.position() - Point::ORIGIN;
            let ranges = state.selection();

            for range in ranges
                .into_iter()
                .filter_map(|range| bounds.intersection(&(range + translation)))
            {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: range,
                        ..renderer::Quad::default()
                    },
                    style.selection,
                );
            }
        }

        draw(
            renderer,
            defaults,
            layout.bounds(),
            &state.paragraph,
            style,
            viewport,
        );
    }

    fn operate(
        &mut self,
        _state: &mut Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.text(None, layout.bounds(), &self.fragment);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &core::Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();

        if state.is_hovered || state.dragging.is_some() {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::default()
        }
    }
}

/// Produces the [`layout::Node`] of a [`Text`] widget.
///
/// [`layout::Node`]: https://docs.iced.rs/iced_core/layout/struct.Node.html
pub fn layout<Renderer>(
    state: &mut State,
    renderer: &Renderer,
    limits: &layout::Limits,
    content: &str,
    format: Format<Font>,
) -> layout::Node
where
    Renderer: text::Renderer<Paragraph = Paragraph, Font = Font>,
{
    layout::sized(limits, format.width, format.height, |limits| {
        let bounds = limits.max();

        let size = format.size.unwrap_or_else(|| renderer.default_size());
        let font = format.font.unwrap_or_else(|| renderer.default_font());

        state.update(text::Text {
            content,
            bounds,
            size,
            line_height: format.line_height,
            font,
            align_x: format.align_x,
            align_y: format.align_y,
            shaping: format.shaping,
            wrapping: format.wrapping,
            ellipsize: format.ellipsize,
        });

        state.paragraph.min_bounds()
    })
}

/// Draws text using the same logic as the [`Text`] widget.
pub fn draw<Renderer>(
    renderer: &mut Renderer,
    style: &renderer::Style,
    bounds: core::Rectangle,
    paragraph: &Paragraph,
    appearance: Style,
    viewport: &core::Rectangle,
) where
    Renderer: text::Renderer<Paragraph = Paragraph, Font = Font>,
{
    let anchor = bounds.anchor(
        paragraph.min_bounds(),
        paragraph.align_x(),
        paragraph.align_y(),
    );

    renderer.fill_paragraph(
        paragraph,
        anchor,
        appearance.color.unwrap_or(style.text_color),
        *viewport,
    );
}

impl<'a, Message, Theme, Renderer> From<Text<'a, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Theme: Catalog + 'a,
    Renderer: text::Renderer<Paragraph = Paragraph, Font = Font> + 'a,
{
    fn from(
        text: Text<'a, Theme, Renderer>,
    ) -> Element<'a, Message, Theme, Renderer> {
        Element::new(text)
    }
}

impl<'a, Theme, Renderer> From<&'a str> for Text<'a, Theme, Renderer>
where
    Theme: Catalog + 'a,
    Renderer: text::Renderer<Paragraph = Paragraph, Font = Font> + 'a,
{
    fn from(content: &'a str) -> Self {
        Self::new(content)
    }
}

/// The appearance of some text.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Style {
    /// The [`Color`] of the text.
    ///
    /// The default, `None`, means using the inherited color.
    pub color: Option<Color>,
    /// The [`Color`] of text selections.
    pub selection: Color,
}

/// The theme catalog of a [`Text`].
pub trait Catalog: Sized {
    /// The item class of this [`Catalog`].
    type Class<'a>;

    /// The default class produced by this [`Catalog`].
    fn default<'a>() -> Self::Class<'a>;

    /// The [`Style`] of a class with the given status.
    fn style(&self, item: &Self::Class<'_>) -> Style;
}

/// A styling function for a [`Text`].
///
/// This is just a boxed closure: `Fn(&Theme, Status) -> Style`.
pub type StyleFn<'a, Theme> = Box<dyn Fn(&Theme) -> Style + 'a>;

impl Catalog for Theme {
    type Class<'a> = StyleFn<'a, Self>;

    fn default<'a>() -> Self::Class<'a> {
        Box::new(default)
    }

    fn style(&self, class: &Self::Class<'_>) -> Style {
        class(self)
    }
}

/// The default text styling; color is inherited.
pub fn default(theme: &Theme) -> Style {
    Style {
        color: None,
        selection: theme.extended_palette().primary.weak.color,
    }
}

fn highlight_line(
    line: &cosmic_text::BufferLine,
    from: usize,
    to: usize,
) -> impl Iterator<Item = (f32, f32)> + '_ {
    let layout = line.layout_opt().map(Vec::as_slice).unwrap_or_default();

    // Check for multi codepoint glyphs for each previous visual line
    let mut previous_diff = 0;
    let previous_lines_diff = line
        .layout_opt()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(move |(line_nr, visual_line)| {
            if line_nr == 0 {
                let current_diff = previous_diff
                    + visual_line
                        .glyphs
                        .iter()
                        .fold(0, |d, g| d + g.start.abs_diff(g.end) - 1);
                previous_diff = current_diff;
                0
            } else {
                let current_diff = previous_diff
                    + visual_line
                        .glyphs
                        .iter()
                        .fold(0, |d, g| d + g.start.abs_diff(g.end) - 1);
                let previous_diff_temp = previous_diff;
                previous_diff = current_diff;
                previous_diff_temp
            }
        });

    layout.iter().zip(previous_lines_diff).map(
        move |(visual_line, previous_lines_diff)| {
            let start = visual_line
                .glyphs
                .first()
                .map(|glyph| glyph.start)
                .unwrap_or(0);
            let end = visual_line
                .glyphs
                .last()
                .map(|glyph| glyph.end)
                .unwrap_or(0);

            let to = to + previous_lines_diff;
            let mut range = start.max(from)..end.min(to);

            let x_offset = visual_line
                .glyphs
                .first()
                .map(|glyph| glyph.x)
                .unwrap_or_default();

            if range.is_empty() {
                (x_offset, 0.0)
            } else if range.start == start && range.end == end {
                (x_offset, visual_line.w)
            } else {
                let mut x = 0.0;
                let mut width = 0.0;
                for glyph in &visual_line.glyphs {
                    let glyph_count = glyph.start.abs_diff(glyph.end);

                    // Check for multi codepoint glyphs before or within the range
                    if glyph_count > 1 {
                        if range.start > glyph.start {
                            range.start += glyph_count - 1;
                            range.end += glyph_count - 1;
                        } else if range.end > glyph.start {
                            range.end += glyph_count - 1;
                        }
                    }

                    if range.start > glyph.start {
                        x += glyph.w;
                    }

                    if range.start <= glyph.start && range.end > glyph.start {
                        width += glyph.w;
                    } else if range.end <= glyph.start {
                        break;
                    }
                }

                (x_offset + x, width)
            }
        },
    )
}

fn visual_lines_offset(line: usize, buffer: &cosmic_text::Buffer) -> i32 {
    let scroll = buffer.scroll();

    let start = scroll.line.min(line);
    let end = scroll.line.max(line);

    let visual_lines_offset: usize = buffer.lines[start..]
        .iter()
        .take(end - start)
        .map(|line| line.layout_opt().map(Vec::len).unwrap_or_default())
        .sum();

    visual_lines_offset as i32 * if scroll.line < line { 1 } else { -1 }
}
