# Curated MCP Servers (from awesome-mcp-servers)

Analysis of [awesome-mcp-servers](https://github.com/punkpeye/awesome-mcp-servers) with focus on **File Systems** and other high-value, low-friction servers for Luna AI quick_setup.

---

## File Systems (awesome-mcp-servers #file-systems)

| Server | No-setup? | Notes |
|--------|-----------|--------|
| **modelcontextprotocol/server-filesystem** | Yes | `npx -y @modelcontextprotocol/server-filesystem <path>`. **In catalog.** |
| **microsoft/markitdown** | Yes | PDF/Office/URLs → Markdown. `npx -y markitdown-mcp-npx`. **In catalog.** |
| **j0hanz/filesystem-context-mcp-server** | Yes | Read-only exploration. |
| **efforthye/fast-filesystem-mcp** | Yes | Large files, streaming. |
| **Xuanwo/mcp-server-opendal** | Config | S3, GCS, etc. |
| **box / gdrive** | No | OAuth / API keys. |

---

## Other useful categories

### In quick_setup catalog (`catalog/mcp_servers.json`)

| id | Purpose | Command |
|----|---------|---------|
| shell | Whitelisted shell | `uvx mcp-shell-server` |
| filesystem | Home directory RW | `npx @modelcontextprotocol/server-filesystem` |
| fetch | URL → markdown | `uvx mcp-server-fetch` |
| skills | Agent skills folder | `uvx agent-skills-mcp` |
| markitdown | Files/URLs → markdown | `npx markitdown-mcp-npx` |

### Added by quick setup (not in catalog JSON)

| id | Purpose | Notes |
|----|---------|--------|
| **cosmic-llm-memory** | Luna conversation DB via MCP | Binary `mcp_luna_history`; default **on** in quick setup; must be in `tools_policies.*.enabled_mcp` |

### Documented but not in catalog (need keys / setup)

| Server | Setup |
|--------|--------|
| Brave Search | `BRAVE_API_KEY` |
| GitHub | Token |
| Postgres / SQLite | Connection string |
| Official MCP memory (knowledge graph) | Separate from Luna `cosmic-llm-memory` |

---

## Catalog policy

- **In JSON:** No API keys, no OAuth, no connection strings.
- **Placeholders:** `{{HOME}}`, `{{COSMIC_LLM_DIR}}` expanded at setup time.
- **Policy wiring:** `finalize_full_luna_config` sets `enabled_mcp` to selected ids (+ `cosmic-llm-memory` when opted in).

---

## References

- [awesome-mcp-servers](https://github.com/punkpeye/awesome-mcp-servers)
- [mcp_luna_memory release](https://github.com/digit1024/mcp_luna_memory/releases/download/1.0/mcp_luna_history)
- [MEMORY.md](MEMORY.md) – embedding, deep sleep, RAG
