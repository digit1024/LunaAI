# Testing Plan

## Automated

### Rust (desktop + server)

- `cargo test wal_mode_enabled_when_requested` – verifies SQLite WAL mode is
  enabled whenever the UI/server open the database so both processes can run
  concurrently.
- `cargo test` – existing storage regression suite plus the WAL test above.

Recommended smoke test command:

```bash
env -u ARGV0 cargo test --package cosmic_llm
```

### Flutter (mobile)

After running `flutter create .` inside `mobile_app/`:

```bash
cd mobile_app
flutter test
```

Adds coverage for widget layout + controller behaviour (extend with Riverpod
unit tests as needed).

## Manual

### Desktop Server Mode

1. `env -u ARGV0 cargo run -- --server --config ./path/to/config.toml`
2. `websocat ws://127.0.0.1:8080 -H "x-api-key: LUna"`
   - send `{"type":"health_check"}` → expect `health_ok`.
   - send `{"type":"start_conversation","title":"Mobile Sync"}`.
   - send `{"type":"send_message","conversation_id":"...","content":"Hello"}` and
     watch streamed `assistant_delta`, tool events, and `conversation_complete`.

### Mobile Client

1. Run desktop server as above.
2. From `mobile_app/`, `flutter run` on a device/emulator.
3. Confirm:
   - Connecting screen appears, then conversation list loads.
   - Tapping a conversation loads chat with streaming updates.
   - Sending a prompt shows bubbles (user right aligned, assistant + tools left).
   - Minimising the app mid-response keeps the foreground service alive and a
     notification arrives when the response finishes.
   - Search box filters history via websocket `list_conversations` query.
4. Toggle `⚙ Settings` to change profile/API key and verify the server reflects
   the new profile (`profile_changed` event in logs).





