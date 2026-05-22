# iced_selection

[![builds.sr.ht status](https://builds.sr.ht/~pml68/iced_selection.svg)](https://builds.sr.ht/~pml68/iced_selection) &nbsp;
[![docs](https://img.shields.io/website?url=https%3A%2F%2Ficed-selection.pml68.dev&label=docs)](https://iced-selection.pml68.dev)

## Text selection API for [`iced`](https://iced.rs), with reference widget implementations.

Text in iced currently isn't selectable except for `TextEditor` and `TextInput`. This is my solution, but others exist as well (see [Special thanks](#special-thanks)).

Check out the examples, or read the [documentation](https://iced-selection.pml68.dev) to get an idea about the crate.

Roughly:
- `selection.rs`: The main selection API, built around iced's [`Paragraph`](https://docs.iced.rs/iced_graphics/text/paragraph/struct.Paragraph.html).
- `text.rs`: Reference implementation for a selectable text widget.
    - `text/rich.rs`: Reference implementation for a selectable rich text widget.
- `markdown.rs`: A custom [`Viewer`](https://docs.iced.rs/iced/widget/markdown/trait.Viewer.html) and its corresponding custom methods.
- `lib.rs`: Helper methods, macros and re-exports.

## Wrapped text support
Wrapped text is supported, but by-line selection (`Shift + Up Arrow` / `Shift + Down Arrow` & triple-click mouse selection) will treat all wrapped segments as part of the same line.

## Installation
Simply add it to under your `Cargo.toml`'s `dependencies` section.
```toml
# ...

[dependencies]
iced = { git = "https://github.com/iced-rs/iced", branch = "master" }
iced_selection = { git = "https://git.sr.ht/~pml68/iced_selection" }
```

## Features

- `default`:
- `markdown`: Provides support for rendering markdown through a custom viewer.

## TODO

- [X] allow out-of-bounds selection dragging
- [X] custom markdown `Viewer`
- [X] double-click + drag for by-word selection
- [X] triple-click + drag for by-line selection
- [X] support wrapped lines
- [ ] fix by-line selection for wrapped line segments
- [ ] make "combining" multiple text widgets possible ([`feat/global-selectable`](https://git.sr.ht/~pml68/iced_selection/tree/feat/global-selectable))

## Special thanks

- [`iced`](https://iced.rs), for making this possible in the first place, and for the modified source code of `Text`, `Rich` and `Selection` (based on [`text_input/cursor.rs`](https://github.com/iced-rs/iced/blob/master/widget/src/text_input/cursor.rs)).
- [`Halloy`](https://halloy.chat), for its amazing selectable text implementation (check it out, but mind the GPLv3!).
- [`alex-ds13`](https://github.com/alex-ds13) for their incredible contributions to `iced_selection`.
