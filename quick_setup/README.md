# Luna AI Quick Setup

**Opinionated one-shot configuration** for Luna AI: server, thin UI, MCP policy, memory RAG, and deep sleep — so Luna is usable end-to-end after one run.

## What it does

0. **Dependencies** – Checks for `uvx` and `npx`. Installs **uv** if missing; tries Node/npx via apt or prints instructions.
1. **Provider & model** – OpenAI, Claude, Gemini, DeepSeek, OpenRouter + API key.
2. **Personality** – Luna, Vera, or Jude system prompts.
3. **Profile** – Temperature, max tokens, profile name → `[model_presets]` + `[profiles]` in `config.toml`.
4. **Server** – `[server]` + thin UI `server_config.toml` (`0.0.0.0` → thin UI uses `localhost`).
4b. **Skills** – Installs **self_config** into `~/.local/share/cosmic_llm/skills/self_config/`.
5. **MCP** – Catalog: shell, filesystem, fetch, skills, markitdown (`[all]` default). Luna memory MCP **`[Y/n]` default yes** (binary auto-download).
5b. **Full Luna** – Memory RAG, deep sleep, auto titles **`[Y/n]` default yes**; sets `enabled_mcp` to match your MCP picks; writes `[embedding]`, `[deep_sleep]`, `[title_summary]`.
6. **Systemd** – Optional `luna-server.service` enable + start. Use `loginctl enable-linger $USER` for boot without login.

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

- **Providers/models**: `catalog/providers_models.json`
- **MCP servers**: `catalog/mcp_servers.json` – shell, filesystem, fetch, skills, markitdown (no API keys). Luna memory (`cosmic-llm-memory`) added separately, **on by default**. See `docs/MCP_SERVERS_CURATED.md`.
- **Personas**: `sample_data/personas/` – `luna.md`, `vera.md`, `jude.md`

## Project layout

- `quick_setup/main.py` – CLI flow
- `quick_setup/profile_creator.py` – Presets, profiles, config load/save
- `quick_setup/luna_features.py` – enabled_mcp, embedding, deep_sleep, finalize
- `quick_setup/server_config.py` – Server + thin_ui
- `quick_setup/mcp_config.py` – MCP catalog → JSON
- `quick_setup/prompts.py` – Personas
- `quick_setup/deps.py` – uvx/npx
- `quick_setup/systemd_setup.py` – User systemd unit
- `quick_setup/paths.py` – Path helpers

## Testing

```bash
cd quick_setup
PYTHONPATH=. python3 run_tests.py
```

See `docs/QUICK_SETUP.md` for full documentation.
