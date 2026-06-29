# Luna AI Quick Setup – Documentation

## Goal

One-shot, **opinionated** setup so a user can quickly start using **full Luna**: server (`config.toml`), thin UI, MCP tools enabled in policy, memory RAG (embeddings), deep sleep, and optional systemd.

## What gets configured

| Target | Content |
|--------|--------|
| `~/.local/share/cosmic_llm/config.toml` | `default`, `[model_presets.<name>]`, `[tools_policies.default]` with **enabled_mcp**, `[profiles.<name>]`, `[server]`, `[embedding]`, `[deep_sleep]`, `[title_summary]` (when full Luna is enabled) |
| `~/.local/share/cosmic_llm/system_prompt.md` | Global system prompt (from chosen persona) |
| `~/.local/share/cosmic_llm/profiles/<name>.md` | Per-profile prompt (same persona content) |
| `~/.config/luna_thin_ui/server_config.toml` | `host`, `port`, `api_key` for thin UI |
| `~/.local/share/cosmic_llm/mcp_config.json` | `mcpServers` from catalog + optional `cosmic-llm-memory` |

## Flow (main CLI)

1. **Dependencies** – Check/install `uvx`, `npx`.
2. **Provider** – OpenAI, Claude, Gemini, DeepSeek, OpenRouter.
3. **Model** – Pick from catalog; default from catalog `default_model_id`.
4. **API key** – Hidden input; warns if empty (except ollama).
5. **Personality** – Luna / Vera / Jude; installs system + profile prompts.
6. **Temperature / max tokens / profile name** – Writes preset + profile (tools policy placeholder).
7. **Server** – Host, port, API key; thin UI `server_config.toml`.
8. **Skills** – Installs `self_config` skill for the skills MCP server.
9. **MCP** – Pick catalog servers (`[all]` default); Luna memory server **`[Y/n]` default yes** (auto-download binary).
10. **Full Luna** – Memory RAG + deep sleep + auto titles **`[Y/n]` default yes**; for Claude/Gemini chat, prompts for OpenAI embedding API key.
11. **Finalize** – Sets `tools_policies.default.enabled_mcp` to selected server ids (+ `cosmic-llm-memory` if chosen); writes `[embedding]`, `[deep_sleep]`, `[title_summary]` when full Luna is on.
12. **Systemd** – Optional user service install/start.
13. **Summary** – Checklist of profile, MCP policy, embedding/deep sleep, paths, smoke commands.

## Catalog (`catalog/providers_models.json`)

- **providers**: `openai`, `claude`, `gemini`, `deepseek`, `openrouter`.
- Each has: `label`, `backend`, `endpoint`, `default_model_id`, `models[]` with `id`, `label`, `context_window`, `note`.
- DeepSeek/OpenRouter presets use `backend = "openai"` with provider-specific endpoint (Luna OpenAI client).

## Sample data (`sample_data/personas/`)

- **luna.md**, **vera.md**, **jude.md**

## Config shape (config refinement)

```toml
[model_presets.<preset_name>]
backend = "openai"
model = "gpt-4.1-mini"
endpoint = "https://api.openai.com/v1/chat/completions"
api_key = "..."
temperature = 0.3
max_tokens = 4000

[tools_policies.default]
enabled_mcp = ["shell", "filesystem", "fetch", "skills", "markitdown", "search", "cosmic-llm-memory"]
enabled_tools = ["*"]
disabled_tools = []

[profiles.<name>]
model_preset = "<preset_name>"
prompts = ["profiles/<name>.md"]
tools_policy = "default"
summarize_threshold = 0.7
hidden = false

[embedding]
enabled = true
endpoint = "https://api.openai.com/v1/embeddings"
model = "text-embedding-3-small"
dimensions = 1536
api_key = "..."

[deep_sleep]
enabled = true
profile = "<name>"
# summarize_prompt, evaluate_prompt, extract_prompt — optional; see MEMORY.md for defaults

[title_summary]
title_generation_profile = "<name>"
```

`enabled_mcp` lists **exact server ids** from your MCP selection (not `["*"]`), so skipped catalog servers stay disabled.

## Server TOML shape

```toml
[server]
enabled = true
host = "0.0.0.0"
port = 8080
api_key = ""
stream_timeout_secs = 300
healthcheck_interval_secs = 30
wal_enabled = true
wal_autocheckpoint = 200
sqlite_busy_timeout_ms = 5000
```

## MCP catalog servers (`catalog/mcp_servers.json`)

| Server | Purpose |
|--------|--------|
| shell | Whitelisted shell commands via `uvx mcp-shell-server` |
| filesystem | Home directory access via `npx @modelcontextprotocol/server-filesystem` |
| fetch | URL → markdown via `uvx mcp-server-fetch` |
| skills | Agent skills via `uvx agent-skills-mcp` (`SKILL_FOLDER` under cosmic_llm) |
| markitdown | PDF/Office/URL → markdown via `npx markitdown-mcp-npx` |
| search | Web search (no API key) via `uvx free-search-mcp` (DuckDuckGo / Mojeek / Startpage) |
| cosmic-llm-memory | *(separate prompt, default on)* Luna history MCP binary |

## Architecture (modular)

- **paths** – cosmic_llm and luna_thin_ui paths.
- **profile_creator** – Model preset + profile; initial `tools_policies.default` (enabled_mcp filled later).
- **luna_features** – `enabled_mcp_for_selection`, `finalize_full_luna_config` (embedding, deep_sleep, title_summary).
- **server_config** – `[server]` + thin_ui config.
- **mcp_config** – Catalog → `mcp_config.json`.
- **prompts** – Personas and install helpers.
- **main** – Interactive flow and summary.

## Memory, embedding, and deep sleep

See **[MEMORY.md](MEMORY.md)** for how curated facts, vector RAG, `cosmic-llm-memory` MCP, and deep sleep fit together.

## Notes

- **`[prompts]` in config.toml** – Not written; Luna defaults to `~/.local/share/cosmic_llm/system_prompt.md`.
- **Declining full Luna** – Skips `[embedding]` / `[deep_sleep]` / `[title_summary]` but still updates `enabled_mcp` when MCP was configured.
- **Non-OpenAI chat providers** – Embedding key must be OpenAI-compatible; setup prompts separately when using Claude/Gemini for chat.

## Running

```bash
cd quick_setup
pip install -e .
python -m quick_setup.main
# or
luna-quick-setup
```

Requires: Python 3.10+, `toml` (pip install toml).

After setup, verify server logs: MCP tools with `enabled_count > 0`, “Embedding enabled”, “Deep Sleep: enabled”. Optional: `cosmic_llm --deep-sleep`.
