# Vendored `iced_selection` (0.5.0)

Upstream: [iced_selection](https://git.sr.ht/~pml68/iced_selection) (MIT).

## Why vendored

1. **libcosmic iced fork** — Workspace `[patch.crates-io]` uses pop-os/libcosmic’s iced 0.14 tree. Crates.io `iced_selection` must compile against that fork (e.g. `Text::ellipsize` and related APIs).
2. **`cosmic` feature** — `impl iced_selection::text::Catalog for cosmic::Theme` is an orphan rule; it lives in `src/cosmic_catalog.rs` inside this crate, not in `luna_thin_ui`.
3. **Stable path dep** — `luna_thin_ui/Cargo.toml`: `iced_selection = { path = "vendor/iced_selection", features = ["markdown", "cosmic"] }`.

## Local changes vs upstream

- **`cosmic_catalog.rs`** — COSMIC theme selection style (accent @ 55% alpha). Should match `luna_thin_ui/src/ui/widgets/selectable_text.rs` (`accent_highlight` / `bubble_text_style`).
- **`Cargo.toml`** — `edition`, description, optional `libcosmic` for `cosmic` feature; iced comes from workspace patch.
- Any **iced API alignment** fixes applied when libcosmic bumps iced (search vendor for `ellipsize` / widget API diffs).

## Upgrading

1. Diff upstream release against this tree.
2. Re-apply `cosmic_catalog.rs` and compatibility patches.
3. `unset ARGV0 && cargo check -p luna_thin_ui`
4. Re-run manual selection + markdown table QA (see `luna_thin_ui/docs/TEXT_SELECTION_AND_MARKDOWN.md`).

Do not switch to git/path upstream without confirming it builds against the same patched iced as libcosmic.
