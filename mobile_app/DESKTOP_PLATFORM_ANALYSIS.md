# Desktop Platform (Linux/Windows) Support Analysis

## Executive Summary

To make the mobile app work on Linux/Windows, you'll need to address **platform-specific dependencies** and **conditional feature enablement**. The biggest challenge is indeed the **Foreground Service (FG service)**, which is Android/iOS only. Additionally, TTS/STT need to be conditionally disabled on desktop platforms (including Chrome/web).

---

## 🔴 **CRITICAL BLOCKER: Foreground Service**

### **Location:** `lib/services/foreground_guard.dart`

### **Problem:**
- `flutter_foreground_task` package is **Android/iOS only**
- Used to keep WebSocket connection alive when app is backgrounded
- Desktop apps don't have the concept of "foreground services"
- **App will crash** when `ForegroundGuard.init()` is called on Linux/Windows

### **Current Usage:**
```dart
// Called in app_controller.dart:83
await guard.init();

// Called in app_controller.dart:202
unawaited(guard.startConnectionGuard());

// Called in app_controller.dart:56
unawaited(_guard.stopConnectionGuard());
```

### **Impact:**
- ❌ App crashes on startup on Linux/Windows
- ❌ Connection management breaks on desktop

### **Solution Required:**
Create platform-specific implementation:

```dart
// In foreground_guard.dart
import 'dart:io';
import 'package:flutter/foundation.dart';

Future<void> init() async {
  if (_initialized) return;
  
  // Only initialize on mobile platforms
  if (!kIsWeb && (Platform.isAndroid || Platform.isIOS)) {
    FlutterForegroundTask.init(...);
  }
  // Desktop: No-op (connection stays alive naturally)
  
  _initialized = true;
}

Future<void> startConnectionGuard() async {
  if (!kIsWeb && (Platform.isAndroid || Platform.isIOS)) {
    // Mobile: Use foreground service
    await FlutterForegroundTask.startService(...);
  }
  // Desktop: No-op (connection managed by app lifecycle)
}
```

### **Why This Is The Biggest Challenge:**
1. **Deeply integrated** - Called in multiple places in `app_controller.dart`
2. **No desktop equivalent** - Desktop apps don't need foreground services
3. **Connection management** - Need alternative strategy for desktop
4. **Package dependency** - `flutter_foreground_task` may not compile on desktop

---

## 🟡 **TTS/STT Conditional Enablement**

### **Requirement:**
- ✅ **Mobile (Android/iOS):** TTS and STT enabled
- ❌ **Desktop (Linux/Windows/Chrome):** TTS and STT disabled

### **Current Implementation:**

#### **TTS Service** (`lib/services/tts_service.dart`)
- Uses `flutter_tts` package
- Called from:
  - `chat_screen.dart:256` - `_playTtsForMessage()`
  - `chat_screen.dart:266` - Dialog mode TTS
  - `chat_screen.dart:274` - Regular TTS
  - `chat_screen.dart:712` - Settings language selection
  - `setup_screen.dart:41` - Setup language selection

#### **STT Service** (`lib/services/speech_service.dart`)
- Uses `speech_to_text` package
- Called from:
  - `chat_screen.dart:329` - `_startDialogMode()`
  - `chat_screen.dart:339` - `isAvailable()` check
  - `chat_screen.dart:421` - `startListening()`

### **Solution Required:**

#### **1. Platform Detection Helper**
```dart
// Create lib/utils/platform_utils.dart
import 'dart:io';
import 'package:flutter/foundation.dart';

bool get isMobile => !kIsWeb && (Platform.isAndroid || Platform.isIOS);
bool get isDesktop => !kIsWeb && (Platform.isLinux || Platform.isWindows || Platform.isMacOS);
bool get isWeb => kIsWeb;
```

#### **2. TTS Service Conditional**
```dart
// In tts_service.dart
import '../utils/platform_utils.dart';

Future<void> speak(String text, {VoidCallback? onComplete}) async {
  if (!isMobile) {
    debugPrint('TTS disabled on desktop/web');
    onComplete?.call(); // Call callback immediately
    return;
  }
  // ... existing mobile implementation
}
```

#### **3. STT Service Conditional**
```dart
// In speech_service.dart
import '../utils/platform_utils.dart';

Future<bool> initialize() async {
  if (!isMobile) {
    debugPrint('STT not available on desktop/web');
    return false; // Gracefully return false
  }
  // ... existing mobile implementation
}
```

#### **4. Voice Mode Button Conditional**
```dart
// In chat_screen.dart - InputArea widget
if (isMobile && !isStreaming && !state.isDialogModeActive) {
  IconButton(
    onPressed: onVoiceMode,
    icon: Icon(Icons.mic),
    tooltip: 'Voice mode',
  );
}
// Desktop: Hide voice mode button entirely
```

#### **5. Dialog Mode Prevention**
```dart
// In app_controller.dart
void startDialogMode() {
  if (!isMobile) {
    // Show error: "Voice mode not available on desktop"
    return;
  }
  // ... existing implementation
}
```

---

## 🟡 **Other Platform-Specific Issues**

### **1. Notification Service** (`lib/services/notification_service.dart`)

**Problem:**
- Currently only configured for Android
- `flutter_local_notifications` may have limited desktop support

**Solution:**
```dart
Future<void> init() async {
  if (_initialized) return;
  
  if (Platform.isAndroid) {
    const androidSettings = AndroidInitializationSettings('@mipmap/ic_launcher');
    const initializationSettings = InitializationSettings(android: androidSettings);
    await _plugin.initialize(initializationSettings);
  } else if (Platform.isLinux || Platform.isWindows) {
    // Desktop: Use desktop_notifications package or disable gracefully
    // Or check if flutter_local_notifications supports desktop
  }
  // iOS handling...
  
  _initialized = true;
}
```

**Impact:** Low - Notifications are nice-to-have, not critical

---

### **2. Wakelock** (`wakelock_plus`)

**Location:** `chat_screen.dart:335, 432`

**Problem:**
- Desktop screens don't sleep like mobile
- Package may not support desktop

**Solution:**
```dart
// In _startDialogMode()
if (isMobile) {
  await WakelockPlus.enable();
}

// In _stopDialogMode()
if (isMobile) {
  await WakelockPlus.disable();
}
```

**Impact:** Low - Desktop doesn't need wakelock

---

### **3. Server Config** (`lib/core/config/server_config.dart`)

**Current:**
```dart
if (!kIsWeb && Platform.isAndroid) {
  host = '10.0.2.2';  // Android emulator
}
```

**Solution:**
```dart
factory ServerConfig.defaults() {
  var host = '127.0.0.1';
  if (!kIsWeb && Platform.isAndroid) {
    host = '10.0.2.2';  // Android emulator
  } else if (isDesktop) {
    host = '127.0.0.1';  // Desktop platforms
  }
  return ServerConfig(...);
}
```

**Impact:** Low - Already defaults to 127.0.0.1

---

## 📋 **Files That Need Changes**

### **Critical (App Won't Run Without):**
1. ✅ `lib/services/foreground_guard.dart` - Platform checks for FG service
2. ✅ `lib/services/speech_service.dart` - Disable STT on desktop
3. ✅ `lib/services/tts_service.dart` - Disable TTS on desktop
4. ✅ `lib/ui/screens/chat_screen.dart` - Conditional voice mode, wakelock

### **Important (Features May Break):**
5. ✅ `lib/services/notification_service.dart` - Desktop notification support
6. ✅ `lib/core/config/server_config.dart` - Explicit desktop handling
7. ✅ `lib/application/app_controller.dart` - Prevent dialog mode on desktop

### **New Files:**
8. ✅ `lib/utils/platform_utils.dart` - Platform detection helpers

---

## 🎯 **Implementation Strategy**

### **Phase 1: Make App Run (Critical)**
1. **Fix ForegroundGuard** - Add platform checks, no-op on desktop
2. **Fix TTS/STT** - Disable on desktop with graceful handling
3. **Fix Wakelock** - Platform checks
4. **Hide Voice Mode** - Disable button on desktop

### **Phase 2: Test Core Features**
- ✅ WebSocket connection
- ✅ Chat messaging
- ✅ File attachments
- ✅ Audio playback (typing, done sounds)

### **Phase 3: Optional Enhancements**
- Desktop notifications (if needed)
- Better error messages for disabled features

---

## 🔍 **Package Compatibility Check**

### **✅ Should Work on Desktop:**
- `web_socket_channel` - ✅ Cross-platform
- `http` - ✅ Cross-platform
- `shared_preferences` - ✅ Cross-platform
- `flutter_markdown_plus` - ✅ Cross-platform
- `audioplayers` - ✅ Should work (may need system audio libs)
- `file_picker` - ✅ Should work (uses native dialogs)

### **❌ Mobile Only:**
- `flutter_foreground_task` - ❌ Android/iOS only
- `flutter_tts` - ⚠️ May have limited desktop support
- `speech_to_text` - ⚠️ May have limited desktop support
- `wakelock_plus` - ⚠️ Desktop not needed

### **⚠️ Needs Testing:**
- `flutter_local_notifications` - Check desktop support

---

## 🧪 **Testing Checklist**

### **Linux/Windows:**
- [ ] App launches without crashing
- [ ] WebSocket connection works
- [ ] Chat messages send/receive
- [ ] File attachments work
- [ ] Audio playback works
- [ ] Voice mode button is hidden/disabled
- [ ] TTS doesn't attempt to run
- [ ] STT doesn't attempt to run
- [ ] Connection stays alive when window minimized
- [ ] No foreground service errors

### **Chrome/Web:**
- [ ] App runs in browser
- [ ] TTS disabled
- [ ] STT disabled
- [ ] Voice mode disabled
- [ ] WebSocket works (may need wss://)

---

## 💡 **Key Insights**

1. **FG Service is the biggest blocker** - It's deeply integrated and will crash on desktop
2. **TTS/STT are easier** - Just need conditional checks and graceful returns
3. **Desktop doesn't need most mobile features** - Wakelock, foreground services, etc.
4. **Platform detection is key** - Create a utility to check `isMobile` vs `isDesktop` vs `isWeb`
5. **Graceful degradation** - Desktop version can work without TTS/STT

---

## 🚀 **Quick Win: Minimal Desktop Support**

To get the app running on Linux/Windows with minimal changes:

1. **Disable FG service** (no-op on desktop) - **CRITICAL**
2. **Disable TTS** (return early on desktop) - **EASY**
3. **Disable STT** (return false on desktop) - **EASY**
4. **Hide voice mode button** (platform check) - **EASY**
5. **Fix wakelock** (no-op on desktop) - **EASY**

This gives you:
- ✅ Full chat functionality
- ✅ File attachments
- ✅ Audio feedback
- ✅ All core features except voice/TTS

**Estimated effort:** 2-4 hours for critical fixes

---

## 📚 **References**

- Existing analysis: `LINUX_COMPATIBILITY_ANALYSIS.md`
- Flutter Desktop: https://docs.flutter.dev/desktop
- Platform detection: `dart:io` Platform class + `foundation.dart` kIsWeb







