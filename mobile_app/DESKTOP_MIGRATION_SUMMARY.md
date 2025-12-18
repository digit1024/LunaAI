# Desktop Migration Summary - Quick Reference

## 🎯 **Goal**
Make mobile app work on Linux/Windows with TTS/STT disabled on desktop (including Chrome)

---

## 🔴 **CRITICAL: Foreground Service (FG Service)**

**Status:** ⚠️ **BIGGEST CHALLENGE** - Will crash app on desktop

**File:** `lib/services/foreground_guard.dart`

**Problem:**
```
flutter_foreground_task → Android/iOS ONLY
Called in: app_controller.dart (3 places)
Result: CRASH on Linux/Windows startup
```

**Fix:**
```dart
if (Platform.isAndroid || Platform.isIOS) {
  // Use foreground service
} else {
  // Desktop: No-op (connection stays alive naturally)
}
```

**Effort:** 🔴 **HIGH** - Deeply integrated, needs careful refactoring

---

## 🟡 **TTS/STT Conditional Enablement**

### **TTS (Text-to-Speech)**

**Status:** 🟡 **MEDIUM** - Needs conditional checks

**Files:**
- `lib/services/tts_service.dart`
- `lib/ui/screens/chat_screen.dart` (multiple calls)

**Fix:**
```dart
Future<void> speak(String text) async {
  if (!isMobile) return; // Desktop: Disable
  // ... mobile implementation
}
```

**Effort:** 🟢 **LOW** - Simple early returns

---

### **STT (Speech-to-Text)**

**Status:** 🟡 **MEDIUM** - Needs conditional checks

**Files:**
- `lib/services/speech_service.dart`
- `lib/ui/screens/chat_screen.dart` (dialog mode)

**Fix:**
```dart
Future<bool> initialize() async {
  if (!isMobile) return false; // Desktop: Not available
  // ... mobile implementation
}
```

**Effort:** 🟢 **LOW** - Simple early returns

---

## 📊 **Impact Matrix**

| Component | Mobile | Desktop | Chrome | Fix Complexity |
|-----------|--------|---------|--------|----------------|
| **FG Service** | ✅ Required | ❌ Crash | ❌ N/A | 🔴 **HIGH** |
| **TTS** | ✅ Enabled | ❌ Disable | ❌ Disable | 🟢 **LOW** |
| **STT** | ✅ Enabled | ❌ Disable | ❌ Disable | 🟢 **LOW** |
| **Voice Mode** | ✅ Works | ❌ Hide | ❌ Hide | 🟢 **LOW** |
| **Wakelock** | ✅ Needed | ❌ No-op | ❌ No-op | 🟢 **LOW** |
| **Notifications** | ✅ Works | ⚠️ Test | ⚠️ Test | 🟡 **MEDIUM** |
| **WebSocket** | ✅ Works | ✅ Works | ✅ Works | 🟢 **NONE** |
| **Chat** | ✅ Works | ✅ Works | ✅ Works | 🟢 **NONE** |

---

## 🔧 **Files to Modify**

### **Critical (App Won't Run):**
1. `lib/services/foreground_guard.dart` - **FG Service platform checks**
2. `lib/services/tts_service.dart` - **TTS disable on desktop**
3. `lib/services/speech_service.dart` - **STT disable on desktop**
4. `lib/ui/screens/chat_screen.dart` - **Voice mode conditional**

### **Important (Features May Break):**
5. `lib/services/notification_service.dart` - **Desktop notifications**
6. `lib/application/app_controller.dart` - **Prevent dialog mode on desktop**
7. `lib/core/config/server_config.dart` - **Desktop host handling**

### **New:**
8. `lib/utils/platform_utils.dart` - **Platform detection helpers**

---

## 📝 **Implementation Order**

### **Step 1: Create Platform Utils** (5 min)
```dart
// lib/utils/platform_utils.dart
bool get isMobile => !kIsWeb && (Platform.isAndroid || Platform.isIOS);
bool get isDesktop => !kIsWeb && (Platform.isLinux || Platform.isWindows || Platform.isMacOS);
```

### **Step 2: Fix FG Service** (1-2 hours)
- Add platform checks to all methods
- No-op implementation for desktop
- Test connection stays alive

### **Step 3: Disable TTS** (15 min)
- Add `isMobile` check in `speak()` method
- Early return on desktop

### **Step 4: Disable STT** (15 min)
- Add `isMobile` check in `initialize()`
- Return `false` on desktop

### **Step 5: Hide Voice Mode** (15 min)
- Conditional button rendering
- Prevent `startDialogMode()` on desktop

### **Step 6: Fix Wakelock** (10 min)
- Platform checks around `WakelockPlus` calls

---

## ✅ **Expected Result**

### **Desktop (Linux/Windows):**
- ✅ App launches without crashing
- ✅ WebSocket connection works
- ✅ Chat fully functional
- ✅ File attachments work
- ✅ Audio feedback works
- ❌ Voice mode hidden/disabled
- ❌ TTS disabled (silent)
- ❌ STT disabled (not available)

### **Chrome/Web:**
- ✅ Same as desktop
- ✅ Runs in browser
- ❌ TTS/STT disabled

---

## 🎯 **Your Assumption: CORRECT**

> "I'm assuming the biggest challenge is FG service?"

**✅ YES!** The foreground service is:
- The **only component that will crash** the app
- **Deeply integrated** in connection management
- **No desktop equivalent** (desktop doesn't need it)
- Requires **careful refactoring** to avoid breaking mobile

All other issues (TTS/STT) are simple conditional checks.

---

## 💰 **Effort Estimate**

| Task | Time | Priority |
|------|------|----------|
| Platform utils | 5 min | P0 |
| FG Service fix | 1-2 hours | **P0** |
| TTS disable | 15 min | P1 |
| STT disable | 15 min | P1 |
| Voice mode hide | 15 min | P1 |
| Wakelock fix | 10 min | P2 |
| Testing | 1 hour | P0 |
| **TOTAL** | **3-4 hours** | |

---

## 🚨 **Risks**

1. **FG Service refactoring** - Could break mobile if not careful
2. **Package compatibility** - Some packages may not compile on desktop
3. **Testing coverage** - Need to test both mobile and desktop paths

---

## 📚 **Next Steps**

1. Read full analysis: `DESKTOP_PLATFORM_ANALYSIS.md`
2. Start with platform utils
3. Fix FG service (biggest blocker)
4. Add TTS/STT conditionals
5. Test on Linux/Windows
6. Test on Chrome (if web support needed)







