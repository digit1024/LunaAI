# Quick Setup – Flow Logic & Flaws

## Flow diagram (Mermaid)

```mermaid
flowchart TD
    START([Start]) --> DEPS[Check deps: uvx, npx]
    DEPS --> LOAD_CAT[Load providers catalog]
    LOAD_CAT --> ASK_PROV[Provider, model, API key]
    ASK_PROV --> PROFILE[Profile + preset + prompts]
    PROFILE --> SERVER[Server + thin_ui]
    SERVER --> SKILLS[Install self_config skill]
    SKILLS --> ASK_MCP[MCP catalog + Luna memory default Y]
    ASK_MCP --> SAVE_MCP[save mcp_config.json]
    SAVE_MCP --> FULL_LUNA[Full Luna Y/n default Y]
    FULL_LUNA --> FINALIZE[finalize_full_luna_config]
    FINALIZE --> SET_MCP[enabled_mcp = selected ids]
    FINALIZE --> SET_EMB[embedding deep_sleep titles if Y]
    SET_MCP --> SYSTEMD[Systemd optional]
    SET_EMB --> SYSTEMD
    SYSTEMD --> SUMMARY[Setup summary + smoke hints]
```

---

## Historical flaws (fixed)

| Id | Was | Fix |
|----|-----|-----|
| **P0** | `enabled_mcp = []` while MCP JSON written | `finalize_full_luna_config` sets explicit server ids after MCP selection |
| **P0** | No `[embedding]` / `[deep_sleep]` | Full Luna step (default on) writes both |
| **F1** | Empty API key silent | Warn for non-ollama backends |
| **F4** | Port not validated | `_clamp_port` 1–65535 |
| **F5** | thin_ui localhost for 0.0.0.0 | Documented + runtime message |
| **F6** | Missing MCP catalog silent | Clear messages when catalog missing/empty |
| **F8** | Duplicate MCP picks | `dict.fromkeys` dedupe |

---

## Flow overview (linear)

```
  START → deps → provider/model/key → profile/preset → server/thin_ui
    → self_config skill → MCP pick + Luna memory [Y/n] → mcp_config.json
    → Full Luna [Y/n] → finalize (enabled_mcp + embedding + deep_sleep + titles)
    → systemd [Y/n] → setup summary
```

---

## Remaining limitations (by design or follow-up)

| Topic | Notes |
|-------|--------|
| **F5** | Remote thin UI still requires editing `server_config.toml` when server binds `0.0.0.0` |
| **Gemini endpoint** | Catalog uses API base URL; verify with `GeminiClient` if issues arise |
| **Health probe** | No automatic post-setup HTTP/MCP connection test |
| **Brave / Ollama** | Not in catalog |
| **Deb package** | Rebuild with `packaging/build_debs.sh` after changes (rsyncs `quick_setup/`) |
