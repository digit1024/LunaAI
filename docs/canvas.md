# Canvas: serving rich outputs (images + HTML/CSS/JS)

Summary of the approach for serving agent-generated images and multi-file "canvas" pages.

---

## Overview

- **Server** serves content under a conversation-scoped base (path or subdomain).
- **Images**: AI response is Markdown; images use URLs under that base (e.g. `![](/serve/conv_id/pic.png)` or subdomain).
- **Canvas**: Multi-file HTML/CSS/JS bundle; user opens via a button that loads the page in a WebView. Content is treated as trusted (written by our tools).
- **Auth**: Content only accessible when cookie `x-auth` equals the API token. Access is scoped per conversation.

---

## Serving

**Option A — Path-based**

- Base: `myaddress/serve/{conversation_id}/`
- Single files: `GET /serve/{conv_id}/{artifact_id}` or `.../image.png`
- Bundle (canvas): `GET /serve/{conv_id}/{bundle_id}/` → `index.html`; `.../style.css`, `.../script.js` etc.
- **Caveat**: Agent HTML must use **relative** paths (`index.css`). Absolute paths like `/index.css` resolve to site root and break.

**Option B — Subdomain (recommended for canvas)**

- Base: `{convid}.myaddress/` (e.g. `conv123.preview.myaddress`)
- Document root for that origin = that conversation’s (or bundle’s) content.
- **Benefit**: Absolute paths in HTML (e.g. `/index.css`) resolve to `{convid}.myaddress/index.css` — correct. No need to force relative paths.

---

## Auth

- **Cookie**: `x-auth` = API token. Server checks it on every request to the serve base.
- **Scope**: Validate that the token’s user is allowed **this** `conversation_id`. Otherwise any user could access others’ content via `/serve/other_conv_id/` or `other_conv_id.myaddress`.
- **Cookie scope**: If using subdomains, set cookie on parent domain (e.g. `.myaddress`) so one token works for all `*.myaddress` (e.g. `conv123.myaddress`, `conv456.myaddress`).

---

## Canvas (multi-file bundle)

- Agent (or tool) writes e.g. `index.html`, `style.css`, `script.js` under a `bundle_id` for that conversation.
- **Entry URL**: Path-based: `/serve/{conv_id}/{bundle_id}/`; subdomain: `{conv_id}.myaddress/` (if one bundle per conv) or e.g. `{bundle_id}.{conv_id}.myaddress/` if multiple bundles per conv.
- **UI**: Button “Open canvas” / “Preview” opens that URL in a WebView. WebView sends same `x-auth` cookie; page and assets load.
- **Trust**: Content runs with the user’s auth (same cookie). Acceptable because the content is produced by our tools; if third-party tools could write the bundle, consider a separate origin or read-only token.

---

## Security checklist

| Item | Action |
|------|--------|
| Conversation scope | Check user can access this `conversation_id` on every request. |
| Path traversal | Resolve paths only under the conversation/bundle dir; reject `..` and absolute escape. |
| Cookie | Use `Secure` (HTTPS); optional `HttpOnly`. Same token for API and serve. |
| WebView | Same auth as user; acceptable if we control who writes the canvas. |

---

## Tool result shape (for client)

When the agent creates a canvas bundle, the tool can return e.g.:

```json
{
  "description": "Created a sample home page.",
  "bundle_id": "uuid",
  "entry_url": "/serve/{conversation_id}/{bundle_id}/"
}
```

Or with subdomain: `entry_url` could be `https://{conv_id}.myaddress/` (or include `bundle_id` if needed). Client uses `entry_url` for the WebView or “Open preview” link.

---

## Managing subdomains with Cloudflare Tunnel

You don’t create each subdomain manually. Use a **wildcard** so every `{convid}.something.yourdomain` goes to the same backend; the app decides what to serve from the **Host** header.

### 1. DNS (Cloudflare)

Add a **wildcard** CNAME for the canvas/serve host:

- **Name**: `*.serve` (or `*.preview`, `*.canvas`)  
  → Full hostname: `*.serve.digit1024.win` (or your domain).
- **Target**: Your tunnel’s DNS target (e.g. `LUNAAI.cfargotunnel.com` or the tunnel ID from the dashboard).

Cloudflare will resolve **any** subdomain (e.g. `abc123.serve.digit1024.win`, `conv-uuid.serve.digit1024.win`) to the tunnel.

### 2. Tunnel ingress (cloudflared)

In `cloudflared-config.yml`, add one **wildcard** hostname and point it at the same HTTP service as your API (e.g. 8081):

```yaml
ingress:
  - hostname: luna.digit1024.win
    service: ws://localhost:8080
    # ... existing WebSocket config ...

  - hostname: luna-api.digit1024.win
    service: http://localhost:8081
    # ... existing API config ...

  # Wildcard: all *.serve.digit1024.win → same app
  - hostname: "*.serve.digit1024.win"
    service: http://localhost:8081

  - service: http_status:404
```

Order: more specific hostnames first; wildcard last among real routes; catch‑all 404 at the end.

### 3. App behaviour

- Every request to e.g. `conv-uuid.serve.digit1024.win` hits your app on 8081.
- **Host** header will be `conv-uuid.serve.digit1024.win` (or similar).
- Parse the **first label** of the host (before the first `.`) → that’s your `conversation_id` (e.g. `conv-uuid`). Validate auth and that the user can access that conversation, then serve files from that conversation’s root (or that conv’s canvas bundle).
- No per-conversation DNS or tunnel config — one wildcard, routing by Host in the app.

### 4. Cookie for subdomains

So the cookie is sent to every canvas subdomain, set it on the **parent domain** (e.g. `digit1024.win`). Then `conv123.serve.digit1024.win` and `conv456.serve.digit1024.win` both get the cookie. If your main app is on `luna.digit1024.win` or `luna-api.digit1024.win`, ensure the cookie domain is `.digit1024.win` (with leading dot) so it applies to all subdomains.

### Summary (Cloudflare)

| Step | Action |
|------|--------|
| DNS | Add CNAME `*.serve.digit1024.win` → tunnel. |
| Tunnel | Add ingress `hostname: "*.serve.digit1024.win"` → `http://localhost:8081`. |
| App | Read Host, first label = `conversation_id`; scope and serve. |
| Cookie | Set on `.digit1024.win` so all `*.serve.digit1024.win` get it. |

---

## Summary

- **Serve** conversation (and optionally bundle) content as static files under a path or subdomain.
- **Images** in Markdown via URLs under that base.
- **Canvas** = multi-file bundle; open in WebView via button; auth by cookie `x-auth` = API token.
- **Subdomain** `{convid}.myaddress/` avoids absolute-path issues in agent HTML (`/index.css` works).
- **Must**: scope access by conversation; prevent path traversal; accept that WebView content has user’s auth.

---

## Code changes required

See **docs/canvas-code-changes.md** for the full checklist. Summary:

| # | Change | Where |
|---|--------|--------|
| 1 | Add `serve_dir()` | Config |
| 2 | Auth: cookie `x-auth` | `http.rs` |
| 3 | Serve handler (auth, scope, path, file) | `http.rs` or `serve.rs` |
| 4 | Route `GET /serve/:conversation_id/*path` | `create_http_router` |
| 5 | Write API or tool pipeline for files | New handler or MCP pipeline |
| 6 | Tool result `entry_url` | Tool / result processing |
| 7 | Tunnel wildcard | `cloudflared-config.yml` + DNS |

