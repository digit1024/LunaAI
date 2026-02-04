# Luna AI – Debian packages (x86_64)

Build and install `.deb` packages for **luna-ai-server**, **luna-ai-quick-setup**, and **luna-thin-ui**.

## Build

From the **repo root** (with `ARGV0` unset if you use a proxy that breaks cargo):

```bash
./packaging/build_debs.sh
```

**Output:** `packaging/out/luna-ai-server_0.1.0_amd64.deb`, `luna-ai-quick-setup_0.1.0_amd64.deb`, `luna-thin-ui_0.1.0_amd64.deb`

**Requirements:** `cargo`, `rustc`, `python3`, `rsync`, `dpkg-deb`. For thin_ui, the build uses **libcosmic** from git (Pop OS COSMIC stack).

## Install

```bash
sudo dpkg -i packaging/out/luna-ai-server_0.1.0_amd64.deb
sudo dpkg -i packaging/out/luna-ai-quick-setup_0.1.0_amd64.deb
sudo dpkg -i packaging/out/luna-thin-ui_0.1.0_amd64.deb
```

If `dpkg` reports missing dependencies (e.g. `python3-toml`), run:

```bash
sudo apt-get install -f
```

## After install

| Component        | Command / usage |
|-----------------|-----------------|
| **Quick setup** | `luna_ai_quick_setup` – interactive config (server path, profiles, MCP, systemd) |
| **Server**      | Binary: `/usr/bin/cosmic_llm`. User service: `systemctl --user start luna-server.service` (after quick setup) |
| **Desktop app** | `luna-thin` or **Luna AI** in the application menu (desktop file + icon under `hicolor`) |

## Dependencies (declared in packages)

- **luna-ai-server:** `libc6`, `libssl3`, `liblzma5`
- **luna-ai-quick-setup:** `python3 (>= 3.10)`, `python3-toml`; recommends `luna-ai-server`
- **luna-thin-ui:** `libc6`, `libgcc-s1`, `libasound2`, `libssl3`, `libxkbcommon0`

## Layout

- `packaging/luna-ai-server/` – server .deb staging (binary in `usr/bin`)
- `packaging/luna-ai-quick-setup/` – quick-setup .deb (wrapper `usr/bin/luna_ai_quick_setup`, data under `usr/share/luna-ai-quick-setup`)
- `packaging/luna-thin-ui/` – desktop .deb (`usr/bin/luna-thin`, `.desktop`, icon in `usr/share/icons/hicolor/scalable/apps`)
