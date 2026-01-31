# Code changes required (canvas / serve)

Minimal code changes to implement serve + canvas (path-based first; subdomain uses same handler with Host parsing).

### 1. Config: serve directory

**File**: `src/config/mod.rs`

- Add a method for the serve root, e.g. `serve_dir() -> PathBuf` on `AppConfig`:
  - `config_dir().join("serve")` → e.g. `~/.local/share/cosmic_llm/serve`.

### 2. Auth: accept cookie `x-auth` as well as header

**File**: `src/server/http.rs`

- In `extract_api_key()`: also parse the `Cookie` header for `x-auth=<token>`.
- In `authorize()`: return true if **either** header **or** cookie matches `expected_key`.
- Same key as today (`ctx.server_cfg.api_key`).

### 3. Serve handler: static files scoped by conversation

**File**: `src/server/http.rs` (or new `serve.rs`)

- **Route**: `GET /serve/:conversation_id/*path`.
- **Handler**:
  1. **Auth**: `authorize(headers, &ctx.server_cfg.api_key)` → 401 if false.
  2. **Conversation scope**: Parse `conversation_id` as `Uuid`. `storage.lock().await; storage.get_conversation(&uuid)`. If `None` → 404.
  3. **Path**: Base = `config.serve_dir().join(conversation_id.to_string())`. Join base + request path, normalise, ensure result is under base (no `..`). 400/404 if escape.
  4. **Serve**: If file exists, return body + `Content-Type`. If path is dir or empty, try `index.html`. Else 404.

- **Subdomain**: Read `Host` header; first label = `conversation_id`. Request path from URI. Same logic.

### 4. Router: register serve route

**File**: `src/server/http.rs` (`create_http_router`)

- Add: `.route("/serve/:conversation_id/*path", get(serve_handler))`.

### 5. Writing files into serve dir

- **Option A**: `POST /api/serve/:conversation_id` (or `.../:conversation_id/:bundle_id`) with multipart/JSON. Auth + conversation scope; validate path; write to `serve_dir/conv_id/` (+ optional bundle_id).
- **Option B**: In tool-result pipeline (e.g. MCP → ToolResult), when tool returns file content, write to serve_dir and add `entry_url` to result. Needs `conversation_id` in that path.

Minimal first step: Option A (write API) so a tool or frontend can upload canvas files.

### 6. Tool result: `entry_url` for client

Tool (or server post-processing) includes `entry_url` in tool result JSON so client can open WebView. No change to `ServerEvent` if kept inside `result_json`.

### 7. Cloudflare Tunnel

- DNS: `*.serve.digit1024.win` → tunnel.
- Ingress: `hostname: "*.serve.digit1024.win"` → same HTTP service. No app code change.

---

### Summary checklist

| # | Change | Where |
|---|--------|--------|
| 1 | Add `serve_dir()` | Config |
| 2 | Auth: cookie `x-auth` | `http.rs` |
| 3 | Serve handler (auth, scope, path, file) | `http.rs` or `serve.rs` |
| 4 | Route `GET /serve/:conversation_id/*path` | `create_http_router` |
| 5 | Write API or tool pipeline for files | New handler or MCP pipeline |
| 6 | Tool result `entry_url` | Tool / result processing |
| 7 | Tunnel wildcard | `cloudflared-config.yml` + DNS |
