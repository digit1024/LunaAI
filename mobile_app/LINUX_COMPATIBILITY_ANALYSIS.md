# Linux Compatibility Analysis for Luna Mobile App

## Executive Summary

The mobile app is written in Flutter/Dart and currently targets Android/iOS. To make it work on Linux, several platform-specific dependencies and features need to be addressed. The main blockers are:

1. **Foreground Services** - Not available on Linux (Android/iOS only)
2. **Platform-specific packages** - Several packages may have limited or no Linux support
3. **Platform detection** - Code assumes Android/iOS only

---

## 🔴 Critical Issues (App Won't Work Without Fixes)

### 1. Foreground Service (`flutter_foreground_task`)

**Location:** `lib/services/foreground_guard.dart`

**Problem:**
- `flutter_foreground_task` is Android/iOS only
- Used to keep WebSocket connection alive when app is backgrounded
- Linux desktop apps don't have foreground services concept

**Impact:**
- App will crash on initialization when `ForegroundGuard.init()` is called
- Connection will drop when window is minimized (if not handled)

**Solution Options:**
1. **Platform-specific implementation** - Create a Linux stub/no-op version
2. **Use Flutter's lifecycle management** - Linux apps can use `WidgetsBindingObserver` to detect window state
3. **Keep-alive mechanism** - Use periodic WebSocket pings instead of foreground service

**Recommended Approach:**
```dart
// Wrap in platform check
if (Platform.isAndroid || Platform.isIOS) {
  await FlutterForegroundTask.init(...);
} else {
  // Linux: Use alternative keep-alive mechanism
  // WebSocket client should handle reconnection automatically
}
```

---

### 2. Notification Service (`flutter_local_notifications`)

**Location:** `lib/services/notification_service.dart`

**Problem:**
- Package may have limited Linux support
- Currently only configured for Android (`AndroidInitializationSettings`)

**Impact:**
- Notifications won't work on Linux
- App may crash if initialization fails

**Solution:**
- Check if package supports Linux (likely needs Linux-specific initialization)
- Use platform checks to conditionally initialize
- Consider using desktop notifications API directly on Linux

**Recommended Approach:**
```dart
Future<void> init() async {
  if (_initialized) return;
  
  if (Platform.isAndroid) {
    const androidSettings = AndroidInitializationSettings('@mipmap/ic_launcher');
    const initializationSettings = InitializationSettings(android: androidSettings);
    await _plugin.initialize(initializationSettings);
  } else if (Platform.isLinux) {
    // Linux-specific initialization if supported
    // Or use desktop_notifications package
  }
  // iOS handling...
  
  _initialized = true;
}
```

---

### 3. Speech-to-Text (`speech_to_text`)

**Location:** `lib/services/speech_service.dart`

**Problem:**
- Package may not support Linux
- Linux requires different speech recognition backends (e.g., Speech Dispatcher, Google Cloud Speech)

**Impact:**
- Voice mode won't work on Linux
- App may crash when trying to initialize speech recognition

**Solution:**
- Check package Linux support
- Use platform checks before initializing
- Consider alternative: `speech_to_text` may work with Linux if proper backend is installed
- Provide fallback: disable voice mode on Linux or show warning

**Recommended Approach:**
```dart
Future<bool> initialize() async {
  if (_initialized) return true;
  
  // Check platform support
  if (Platform.isLinux) {
    // Try to initialize, but handle gracefully if not supported
    try {
      final available = await _speech.initialize(...);
      _initialized = available;
      return available;
    } catch (e) {
      debugPrint('Speech recognition not available on Linux: $e');
      return false;
    }
  }
  
  // Android/iOS initialization...
}
```

---

### 4. Text-to-Speech (`flutter_tts`)

**Location:** `lib/services/tts_service.dart`

**Problem:**
- Package may have limited Linux support
- Linux requires TTS engine (e.g., espeak, festival, or system TTS)

**Impact:**
- TTS won't work on Linux
- Dialog mode will be partially broken (can listen but can't speak)

**Solution:**
- Check if `flutter_tts` supports Linux
- May need to install system TTS engine
- Use platform checks and graceful degradation

---

### 5. Wakelock (`wakelock_plus`)

**Location:** `lib/ui/screens/chat_screen.dart` (lines 248, 334)

**Problem:**
- Desktop apps don't need wakelock (screen stays on when app is active)
- Package may not support Linux

**Impact:**
- May cause errors when enabling/disabling wakelock
- Not critical for functionality (desktop screens don't sleep like mobile)

**Solution:**
- Wrap in platform check - no-op on Linux
- Desktop apps don't need this feature

**Recommended Approach:**
```dart
// In _startDialogMode()
if (Platform.isAndroid || Platform.isIOS) {
  await WakelockPlus.enable();
}

// In _stopDialogMode()
if (Platform.isAndroid || Platform.isIOS) {
  await WakelockPlus.disable();
}
```

---

## 🟡 Moderate Issues (May Cause Problems)

### 6. Platform Detection in Server Config

**Location:** `lib/core/config/server_config.dart` (line 22)

**Current Code:**
```dart
if (!kIsWeb && Platform.isAndroid) {
  host = '10.0.2.2';  // Android emulator special IP
}
```

**Problem:**
- Only handles Android emulator case
- Linux should use `127.0.0.1` (default is fine, but explicit is better)

**Solution:**
```dart
factory ServerConfig.defaults() {
  var host = '127.0.0.1';
  if (!kIsWeb && Platform.isAndroid) {
    host = '10.0.2.2';  // Android emulator
  } else if (Platform.isLinux || Platform.isWindows || Platform.isMacOS) {
    host = '127.0.0.1';  // Desktop platforms
  }
  return ServerConfig(...);
}
```

---

### 7. Audio Players (`audioplayers`)

**Status:** ✅ Should work on Linux
- Package supports desktop platforms
- May need system audio libraries installed

**Action:** Test audio playback, ensure system has audio support

---

### 8. File Picker (`file_picker`)

**Status:** ✅ Should work on Linux
- Package supports desktop platforms
- Uses native file dialogs

**Action:** Test file attachment feature

---

## 🟢 Low Priority / Already Compatible

### 9. WebSocket Client (`web_socket_channel`)
- ✅ Works on all platforms

### 10. HTTP Client (`http`)
- ✅ Works on all platforms

### 11. Shared Preferences (`shared_preferences`)
- ✅ Works on Linux (uses platform-specific storage)

### 12. Markdown Rendering (`flutter_markdown`)
- ✅ Works on all platforms

---

## 📋 Implementation Strategy

### Phase 1: Make App Run (Critical Fixes)

1. **Wrap ForegroundGuard in platform checks**
   - Create no-op implementation for Linux
   - Remove foreground service initialization on Linux

2. **Fix NotificationService**
   - Add Linux initialization or disable gracefully
   - Use platform checks

3. **Fix SpeechService**
   - Add platform checks
   - Gracefully handle unsupported platforms

4. **Fix TtsService**
   - Add platform checks
   - Test Linux TTS support

5. **Fix Wakelock usage**
   - Wrap in platform checks (no-op on Linux)

6. **Update ServerConfig**
   - Add explicit Linux handling

### Phase 2: Test Core Functionality

1. Test WebSocket connection on Linux
2. Test chat functionality
3. Test file attachments
4. Test audio playback
5. Test basic UI rendering

### Phase 3: Optional Features (If Needed)

1. **Voice Mode on Linux:**
   - Research Linux speech recognition options
   - May need to install system packages (speech-dispatcher, etc.)
   - Consider alternative: disable voice mode on Linux with clear message

2. **TTS on Linux:**
   - Ensure system TTS engine is available
   - Test with `flutter_tts` or find alternative

3. **Notifications on Linux:**
   - Use desktop notifications API
   - May need `desktop_notifications` package

---

## 🔧 Recommended Code Changes Summary

### Files to Modify:

1. **`lib/services/foreground_guard.dart`**
   - Add `dart:io` import
   - Wrap all `FlutterForegroundTask` calls in platform checks
   - Create no-op methods for Linux

2. **`lib/services/notification_service.dart`**
   - Add platform checks
   - Handle Linux initialization or disable gracefully

3. **`lib/services/speech_service.dart`**
   - Add platform checks in `initialize()`
   - Return `false` gracefully on unsupported platforms

4. **`lib/services/tts_service.dart`**
   - Add platform checks
   - Handle Linux TTS initialization

5. **`lib/ui/screens/chat_screen.dart`**
   - Wrap `WakelockPlus` calls in platform checks

6. **`lib/core/config/server_config.dart`**
   - Add explicit Linux platform handling

7. **`lib/main.dart`**
   - Consider conditional initialization based on platform

---

## 🧪 Testing Checklist

- [ ] App launches without crashing on Linux
- [ ] WebSocket connection works
- [ ] Chat messages send/receive
- [ ] File attachments work
- [ ] Audio playback works (typing, done, sent sounds)
- [ ] Notifications (if implemented) work or fail gracefully
- [ ] Voice mode either works or shows appropriate message
- [ ] TTS either works or fails gracefully
- [ ] App handles window minimize/maximize correctly
- [ ] Connection stays alive when window is minimized

---

## 📚 Additional Resources

- Flutter Desktop Support: https://docs.flutter.dev/desktop
- Platform-specific code: https://docs.flutter.dev/platform-integration/platform-channels
- `flutter_foreground_task` Linux support: Check package documentation
- `speech_to_text` Linux support: Check package documentation
- `flutter_tts` Linux support: Check package documentation

---

## 🎯 Quick Win: Minimal Viable Linux Support

To get the app running on Linux with minimal changes:

1. **Disable foreground service** (no-op on Linux)
2. **Disable notifications** (graceful failure)
3. **Disable voice mode** (show message: "Voice mode not available on Linux")
4. **Disable TTS** (optional feature)
5. **Fix wakelock** (no-op on Linux)
6. **Fix server config** (explicit Linux handling)

This would allow:
- ✅ Chat functionality
- ✅ File attachments
- ✅ Audio feedback
- ✅ All core features except voice/TTS

Then gradually add Linux support for optional features.
