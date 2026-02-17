# Luna Mobile Client

Flutter implementation of the Luna AI companion. It connects to the desktop/server
mode over a secure websocket, streams assistant/tool updates, and mirrors the
WhatsApp-like layout requested in the product brief.

## Getting Started

1. Install Flutter 3.24+ and run `flutter doctor`.
2. Add an `android/local.properties` file that points `sdk.dir` to your Android
   SDK and `flutter.sdk` to your Flutter install (standard Flutter requirement).
3. Install dependencies:
   ```bash
   flutter pub get
   ```
4. Update `lib/core/config/server_config.dart` if your server host/api key
   deviates from the defaults.
5. Run on a device/emulator:
   ```bash
   flutter run
   ```



## Key Features

- Websocket client that mirrors the desktop streaming protocol (health check,
  list/search conversations, tool streaming, markdown rendering).
- WhatsApp-style conversation list and chat window with user bubbles on the
  right, assistant/tool bubbles on the left (~70% width).
- Foreground service (declared in `android/app/src/main/AndroidManifest.xml`)
  keeps websocket + notifications alive while the app is backgrounded or the
  screen is off.
- Local notifications when a response finishes while the app is inactive.
- Markdown rendering for final assistant messages (headers, lists, code blocks).
- Bottom action bar: `Conversations`, `+ Start New`, `⚙ Settings`.

