# Luna AI Quick Setup

**Opinionated one-shot configuration** for Luna AI: server (`config.toml`) and thin UI (`server_config.toml`), so you can start using Luna quickly.

## What it does

0. **Dependencies** – Checks for `uvx` and `npx` (needed for MCP). Installs **uv** (provides uvx) via official script if missing; tries to install Node/npx via apt, or prints install instructions.
1. **Provider & model** – Choose provider (OpenAI, Claude, Gemini, DeepSeek, OpenRouter), pick a model (with a suggested default: good balance of speed and cost), enter API key.
2. **Personality** – Pick one of 3 sample system prompts (Luna, Vera, Jude) and install it.
3. **Profile** – Asks temperature (default 0.3), max tokens (default 4000), profile name (suggested: `{persona}_{model}`), writes `[profiles.<name>]` to `config.toml`.
4. **Server** – Asks for server API (host, port, api_key) and writes `[server]` to `config.toml`. Writes **Thin UI** config to `~/.config/luna_thin_ui/server_config.toml` with the same port and api_key so you can **connect right away** with `luna-thin` (host becomes `localhost` when server is `0.0.0.0`).
4b. **Skills** – Installs the **self_config** skill from `quick_setup/self_config/` into `~/.local/share/cosmic_llm/skills/self_config/` so the **skills** MCP server can use it (Luna self-administration).
5. **MCP** – Pick which MCP servers to add from a **catalog** (no-setup only: shell, filesystem, fetch, skills). Optionally add Luna memory server if you have the binary. Writes `mcp_config.json`.
6. **Systemd** – Optionally install **user** systemd unit `luna-server.service` (~/.config/systemd/user/), then **enable** and **start** it. Prompts for path to `cosmic_llm` binary (default: repo `target/release/cosmic_llm`). For run-at-boot without login: `loginctl enable-linger $USER`.

## Quick run

```bash
cd quick_setup
python -m quick_setup.main
# or after install:
pip install -e .
luna-quick-setup
```

## Config locations (Linux)

| What            | Path |
|-----------------|------|
| Main config     | `~/.local/share/cosmic_llm/config.toml` |
| MCP config      | `~/.local/share/cosmic_llm/mcp_config.json` |
| System prompt   | `~/.local/share/cosmic_llm/system_prompt.md` |
| Profile prompts | `~/.local/share/cosmic_llm/profiles/<name>.md` |
| Thin UI server  | `~/.config/luna_thin_ui/server_config.toml` |

## Catalog & sample data

- **Providers/models**: `catalog/providers_models.json` – providers, models, endpoints, suggested default model, context window sizes.
- **MCP servers**: `catalog/mcp_servers.json` – **no-setup only** (no API keys, no connection strings). Listed: shell, filesystem, fetch, skills. Luna memory is added separately when you provide the binary path.
- **Sample personas**: `sample_data/personas/` – `luna.md`, `vera.md`, `jude.md`.

## Project layout

- `quick_setup/main.py` – CLI flow (questions, orchestration).
- `quick_setup/profile_creator.py` – Build profile TOML and merge into config.
- `quick_setup/server_config.py` – Server section + thin_ui `server_config.toml`.
- `quick_setup/mcp_config.py` – Load MCP catalog, build config from selected servers, optional Luna memory; save `mcp_config.json`.
- `quick_setup/prompts.py` – Persona choice and system prompt install.
- `quick_setup/deps.py` – Ensure required commands (uvx, npx); install uv or suggest Node install.
- `quick_setup/systemd_setup.py` – User systemd: write `luna-server.service`, daemon-reload, enable, start.
- `quick_setup/paths.py` – Resolve cosmic_llm, luna_thin_ui, and user systemd paths.

## Testing

```bash
cd quick_setup
# Use system Python (if your venv points at Cursor, use python3 directly):
PYTHONPATH=. python3 run_tests.py     # script-based tests, no pytest needed
# With venv that has real Python + pytest:
# PYTHONPATH=. .venv/bin/python -m pytest tests -v
```

See `docs/QUICK_SETUP.md` for full documentation.
