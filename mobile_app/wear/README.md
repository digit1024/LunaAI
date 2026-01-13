# Luna Wear OS App

Wear OS version of Luna AI companion for smartwatches running Wear OS 5.0+ (e.g., OnePlus Watch 3).

## Features

- ✅ **Voice-first interface** - Optimized for watch form factor
- ✅ **Direct internet connection** - Connects directly to backend server
- ✅ **TTS/STT support** - Full text-to-speech and speech-to-text capabilities
- ✅ **Ambient mode handling** - Pauses TTS/STT when screen dims
- ✅ **Compact UI** - Designed for small watch screens
- ✅ **Shared codebase** - Reuses 90%+ of mobile app code

## Architecture

The Wear OS app shares code with the mobile app:

```
mobile_app/
├── lib/                    # Shared code (used by both mobile & wear)
│   ├── services/           # TtsService, SpeechService (reused!)
│   ├── data/ws/            # LunaWsClient (reused!)
│   └── application/         # AppController (reused!)
│
└── wear/                   # Wear OS entry point
    └── lib/
        ├── main.dart       # Wear OS entry point
        ├── wear_chat_screen.dart
        └── widgets/        # Wear-specific UI components
```

## Building

### Prerequisites

- Flutter 3.24+
- Android SDK with Wear OS support
- Wear OS device or emulator

### Build Commands

**Mobile app:**
```bash
cd mobile_app
flutter run
```

**Wear OS app:**
```bash
cd mobile_app
flutter run -t wear/lib/main.dart -d <wear_device_id>
```

Or from the wear directory:
```bash
cd mobile_app/wear
flutter run -d <wear_device_id>
```

### Android Configuration

The Wear OS module is configured in:
- `android/wear/build.gradle` - Build configuration
- `android/wear/src/main/AndroidManifest.xml` - App manifest
- `android/settings.gradle` - Includes wear module

## Usage

1. **First Launch:**
   - App opens to setup screen
   - Enter server host, port, API key, and profile
   - Tap "Connect" to connect to backend

2. **Voice Mode:**
   - Tap microphone button to start voice mode
   - Speak your message
   - App automatically sends after pause
   - Response is spoken via TTS

3. **Ambient Mode:**
   - When watch screen dims, app shows minimal "Luna" text
   - TTS/STT automatically paused to save battery

## Technical Details

### Shared Code

The Wear OS app imports and reuses:
- `LunaWsClient` - WebSocket client (direct connection)
- `TtsService` - Text-to-speech service
- `SpeechService` - Speech-to-text service
- `AppController` - State management
- All data models and DTOs

### Platform-Specific

Wear OS specific code:
- `WatchShape` widget - Detects round vs square watch
- `AmbientMode` widget - Handles active/ambient transitions
- Compact UI components - Optimized for small screens
- Battery optimizations - Pauses services in ambient mode

### Dependencies

All dependencies are shared via `pubspec.yaml`:
- `wear: ^1.1.0` - Wear OS widgets
- `flutter_tts: ^4.2.0` - TTS (works on Wear OS)
- `speech_to_text: ^7.0.0` - STT (works on Wear OS)
- All other dependencies same as mobile app

## Testing

### Emulator Setup

1. Create Wear OS emulator in Android Studio
2. Set minSdkVersion to 23 (required by wear package)
3. Run: `flutter run -t wear/lib/main.dart -d <emulator_id>`

### Real Device

1. Enable developer options on watch
2. Enable ADB debugging
3. Connect via ADB: `adb connect <watch_ip>:5555`
4. Run: `flutter run -t wear/lib/main.dart -d <device_id>`

## Troubleshooting

### Build Issues

- **"minSdkVersion must be 23"**: Already set in `android/wear/build.gradle`
- **"Cannot find wear module"**: Check `android/settings.gradle` includes `:wear`
- **Import errors**: Ensure you're running from `mobile_app/` directory

### Runtime Issues

- **No internet connection**: Check watch Wi-Fi or phone tethering
- **TTS not working**: Check microphone permissions in Android settings
- **STT not working**: Check RECORD_AUDIO permission

## Future Enhancements

- [ ] Phone config sync via Wearable Data Layer (optional)
- [ ] Haptic feedback for voice mode states
- [ ] Digital crown support (OnePlus Watch 3)
- [ ] Battery usage optimizations
- [ ] Offline mode support






