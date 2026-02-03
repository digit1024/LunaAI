# Curated MCP Servers (from awesome-mcp-servers)

Analysis of [awesome-mcp-servers](https://github.com/punkpeye/awesome-mcp-servers) with focus on **File Systems** and other high-value, low-friction servers for Luna AI quick_setup.

---

## File Systems (awesome-mcp-servers #file-systems)

| Server | No-setup? | Notes |
|--------|-----------|--------|
| **modelcontextprotocol/server-filesystem** | ✅ Yes | Direct local FS; `npx -y @modelcontextprotocol/server-filesystem <path>`. **In catalog.** |
| **j0hanz/filesystem-context-mcp-server** | ✅ Yes | Read-only, secure exploration, symlink protection. |
| **efforthye/fast-filesystem-mcp** | ✅ Yes | Large file handling, streaming, backup. |
| **microsoft/markitdown** | ✅ Yes | Converts PDF/Word/Excel/HTML/URLs → Markdown. `npx -y markitdown-mcp-npx`. **In catalog.** |
| **mark3labs/mcp-filesystem-server** | ✅ Yes | Go impl, local FS. |
| **Xuanwo/mcp-server-opendal** | ⚠️ Config | Apache OpenDAL – many backends (S3, GCS, etc.); needs config. |
| **box/mcp-server-box-remote**, **hmk/box-mcp-server** | ❌ No | Box cloud; OAuth/API key. |
| **isaacphi/mcp-gdrive** | ❌ No | Google Drive; OAuth. |

---

## Other “most useful” categories

### Run locally, no API keys (good for catalog)

| Server | Purpose | Command / note |
|--------|---------|------------------|
| **Shell** | Safe command execution (whitelist) | `uvx mcp-shell-server`. **In catalog.** |
| **Fetch** | URL → content (Markdown) | `uvx mcp-server-fetch`. **In catalog.** |
| **Skills** | Agent skills / tools from folder | `uvx agent-skills-mcp`, `SKILL_FOLDER`. **In catalog.** |
| **Git** (official) | Git repo operations (status, log, diff, commit, etc.) | `uvx mcp-server-git`. Repo path per tool call. **In catalog.** |
| **Memory** (official) | Knowledge-graph persistent memory | `npx -y @modelcontextprotocol/server-memory`. **In catalog.** |

### Useful but need setup (document only)

| Server | Setup | Use case |
|--------|--------|----------|
| **Brave Search** | API key | Web search. |
| **GitHub** (official) | Token | Repos, PRs, issues. |
| **Postgres / SQLite** (official) | Connection string / path | DB queries. |
| **cosmic-llm-memory / Luna memory** | Binary path | Conversation memory; added separately in quick_setup. Script downloads [mcp_luna_history (v1.0)](https://github.com/digit1024/mcp_luna_memory/releases/download/1.0/mcp_luna_history) automatically when you opt in. |

---

## Catalog policy (quick_setup)

- **In `catalog/mcp_servers.json`:** Only servers that need **no API keys, no OAuth, no connection strings**. Optional: one path or folder (e.g. `{{HOME}}`, `{{COSMIC_LLM_DIR}}`).
- **Placeholders:** `{{HOME}}`, `{{COSMIC_LLM_DIR}}` expanded by quick_setup.
- **Luna memory:** Not in catalog; added when user opts in and provides binary path.

---

## Servers in quick_setup catalog (summary)

| id | Label | Command |
|----|--------|---------|
| shell | Shell (safe command execution) | uvx mcp-shell-server |
| filesystem | Filesystem (read/write files) | npx @modelcontextprotocol/server-filesystem {{HOME}} |
| fetch | Fetch (URL → markdown) | uvx mcp-server-fetch |
| skills | Skills (agent skills / tools) | uvx agent-skills-mcp |
| git | Git (repo operations) | uvx mcp-server-git |
| memory | Memory (knowledge graph) | npx @modelcontextprotocol/server-memory |
| markitdown | MarkItDown (files/URLs → Markdown) | npx markitdown-mcp-npx |

---

## References

- [awesome-mcp-servers](https://github.com/punkpeye/awesome-mcp-servers) – full list (File Systems, Developer Tools, etc.).
- [MCP Server Concepts](https://modelcontextprotocol.io/docs/learn/server-concepts).
- [MCP Inspector](https://modelcontextprotocol.io/docs/tools/inspector) – test servers.
