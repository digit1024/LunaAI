# Selectable text & markdown (luna_thin_ui)

Chat bubbles support **mouse selection and copy** for user and assistant message bodies. Tool bubbles, summary bubbles, and reasoning (“Thinking”) blocks are unchanged (non-selectable cosmic widgets).

## Scope

| UI | Selectable? | Implementation |
|----|-------------|----------------|
| User bubble (plain text) | Yes | `selectable_text::bubble_text` |
| Assistant bubble (plain text) | Yes | `selectable_text::bubble_text` |
| Assistant bubble (markdown) | Yes (text blocks) | `SelectableImageViewer` + cosmic `markdown::view_with` |
| Summary bubble (markdown) | No | `ImageViewer` |
| Tool / reasoning bubbles | No | Existing cosmic widgets |

Per-bubble **copy icon** behavior is unchanged.

## Dependencies

- **`iced_selection`** — vendored at `luna_thin_ui/vendor/iced_selection` (crates.io 0.5.0 + libcosmic compatibility).
- Features: `markdown`, `cosmic` (implements `iced_selection::text::Catalog` for `cosmic::Theme` in vendor).
- Workspace **`[patch.crates-io]`** in root `Cargo.toml` pins `iced`, `iced_core`, `iced_graphics`, `iced_renderer`, `iced_widget` to **libcosmic’s iced fork**. Required so selection widgets share one iced tree with COSMIC.

## Source layout

```
luna_thin_ui/src/ui/widgets/
├── selectable_text.rs      # Plain text; accent_highlight / bubble_text_style
├── markdown_viewer.rs      # ImageViewer, SelectableImageViewer, styled_table
└── message_bubble.rs       # Wires viewers into user/assistant/summary paths

luna_thin_ui/vendor/iced_selection/
├── src/cosmic_catalog.rs   # Default selection style for cosmic::Theme (orphan impl)
└── VENDOR.md               # Why vendored, what was patched
```

## Selection styling

Highlight uses theme **accent** at **55% alpha** (`selectable_text::accent_highlight`). Applied in:

- `bubble_text_style` — explicit style on `iced_selection::text`
- `cosmic_catalog::default_style` — default catalog for markdown-adjacent selectable paths

Keep these in sync when changing selection appearance.

**Do not** use `primary.component.hover` for selection; it matched the user bubble background and was invisible.

## Assistant markdown

Uses **`cosmic::widget::markdown::view_with`** with a custom **`SelectableImageViewer`** implementing `markdown::Viewer`. Selectable block types delegate to `iced_selection::markdown` helpers (`paragraph`, `heading`, lists, `code_block`). Tables and images use custom paths.

Not using `iced_selection::markdown::view_with` directly — cosmic’s markdown pipeline and image cache stay in app code.

## Markdown tables

Default cosmic/iced markdown tables use `separator_x(0)` (no visible grid). **`styled_table`** in `markdown_viewer.rs` builds a custom grid:

- **Width:** `Length::Fill` on container, scrollable body, rows, and cells (~100% of bubble).
- **Borders:** 1px `rule::horizontal` / `rule::vertical` with `theme::iced::Rule::custom(selectable_text::accent_rule_style)`.
- **Frame:** 1px accent border on outer `container` (`table_frame_style`).
- **Headers:** Accent horizontal rule under header row (underline).
- **Scroll:** `scrollable` wrapper for wide tables.

Shared accent helpers: `accent_highlight`, `accent_rule_style` in `selectable_text.rs`.

## Images in markdown

`SelectableImageViewer` / `ImageViewer` share **`render_image`** and the app **`image_cache`** (`HashMap<String, ImageState>` in `app.rs`).

- URLs collected during markdown parse (including **table cells**).
- Fetch triggered per chunk / retry on error; cache cleared on **`ConversationLoaded`**.
- Loading state shows explicit **Fetching** UI.

## Common pitfalls

1. **Dual iced trees** — If `[patch.crates-io]` is removed, `iced_selection` and libcosmic may use different iced versions; widgets fail to compose or compile.
2. **Orphan `Catalog` impl** — Cannot implement `iced_selection::text::Catalog for cosmic::Theme` in app code; lives in vendor `cosmic` feature.
3. **Ellipsize** — Upstream 0.5.0 may not match libcosmic iced `Text` API; vendor copy includes needed patches (see `VENDOR.md`).
4. **Table widget** — Iced `table` has no public API for accent separator colors; custom row/column grid is intentional.

## Verify after changes

```bash
unset ARGV0 && cargo check -p luna_thin_ui
```

Manual QA: select plain text (user/assistant), markdown paragraphs/code/links, streaming updates, tables (borders + header line), images, scroll, copy icon; confirm tool/summary/reasoning still non-selectable.
