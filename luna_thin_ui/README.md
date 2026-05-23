# Luna Thin UI (`luna-thin`)

COSMIC desktop client that connects to the Luna AI server over WebSocket. Lives in the workspace crate `luna_thin_ui`; binary name **`luna-thin`**.

## Build & run

```bash
unset ARGV0 && cargo run -p luna_thin_ui
unset ARGV0 && cargo check -p luna_thin_ui
```

Server connection config: `~/.config/luna_thin_ui/server_config.toml` (see [quick_setup/docs/QUICK_SETUP.md](../quick_setup/docs/QUICK_SETUP.md)).

## Documentation

| Topic | File |
|--------|------|
| **Selectable chat text & markdown** | [docs/TEXT_SELECTION_AND_MARKDOWN.md](docs/TEXT_SELECTION_AND_MARKDOWN.md) |
| Code review / architecture notes | [codereview.md](codereview.md) |
| Vendored `iced_selection` | [vendor/iced_selection/VENDOR.md](vendor/iced_selection/VENDOR.md) |

## Workspace integration

The root `Cargo.toml` workspace includes `luna_thin_ui` and patches **iced** crates to **pop-os/libcosmic** so `iced_selection` widgets compose inside `cosmic::Element`. Do not remove `[patch.crates-io]` without re-validating selectable text and markdown.
