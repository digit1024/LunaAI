# Wear OS Smartwatch App - Feasibility Analysis

## ✅ **Yes, it's absolutely possible!**

A Wear OS app for conversation-only mode is feasible and would be a great addition to your Luna AI project.

---

## 🏗️ **Current Architecture Understanding**

Your mobile app:
- **Flutter-based** with WebSocket streaming
- **Direct connection** to Rust backend server via **Tailscale VPN**
- **Conversation mode** = chat interface with voice input/output
- Uses `web_socket_channel` for real-time communication
- Has STT (speech-to-text) and TTS (text-to-speech) capabilities

### **⚠️ Critical Constraint: Tailscale VPN**

Your phone connects to the backend server through **Tailscale VPN**. This means:
- ✅ Phone can access backend (on Tailscale network)
- ❌ Watch **cannot** access backend directly (not on Tailscale)
- ✅ **Solution:** Phone must act as proxy/bridge

**Network Flow:**
```
Backend Server (Tailscale IP: 100.x.x.x)
    ↑
    │ Tailscale VPN
    │
Phone App (on Tailscale)
    ↑
    │ Bluetooth / Wearable Data Layer
    │
Watch App (NOT on Tailscale)
```

---

## 🎯 **Implementation Approach: Phone Proxy (REQUIRED)**

### **Architecture: Watch → Phone → Backend**

```
┌─────────┐      Bluetooth/      ┌─────────┐      Tailscale      ┌─────────┐
│  Watch  │ ◄──► Wearable Data ──►│  Phone  │ ◄──►   VPN    ────►│ Backend │
│   App   │      Layer API        │   App   │      WebSocket      │ Server  │
└─────────┘                        └─────────┘                     └─────────┘
     │                                   │                              │
     │                                   │                              │
  NOT on                            ON Tailscale                  On Tailscale
  Tailscale                         (has VPN access)              Network
```

**Message Flow Example:**
1. User speaks on watch → STT converts to text
2. Watch sends `{"type": "send_message", "content": "Hello"}` to phone
3. Phone receives message, forwards to backend via WebSocket: `ClientCommand.sendMessage(...)`
4. Backend responds with `AssistantDeltaEvent` → Phone receives via WebSocket
5. Phone forwards to watch: `{"type": "server_event", "event": {...}}`
6. Watch displays response and plays TTS

**How it works:**
1. **Watch** sends messages to **Phone** via Wearable Data Layer API (Bluetooth)
2. **Phone** forwards messages to **Backend** via existing WebSocket connection (Tailscale VPN)
3. **Backend** responses flow back: Backend → Phone → Watch
4. **Phone** maintains WebSocket connection (already does this for mobile app)
5. **Watch** only needs Bluetooth connection to phone (no VPN needed)

**Pros:**
- ✅ **Only way it works** with Tailscale constraint
- ✅ Phone handles VPN connectivity (watch doesn't need VPN)
- ✅ Shares server config automatically from phone
- ✅ Better battery life on watch (Bluetooth vs Wi-Fi)
- ✅ Phone can manage connection state

**Cons:**
- ⚠️ More complex (need phone companion app changes)
- ⚠️ Phone must be nearby and app running
- ⚠️ Additional latency (minimal, ~50-100ms)
- ⚠️ Need to implement WebSocket proxy layer in phone app

---

## 📋 **What You Need to Do**

### **1. Flutter Wear OS Support**

Flutter has **experimental Wear OS support** (as of 2024). You'll need:

```yaml
# In pubspec.yaml
environment:
  sdk: ">=3.4.0 <4.0.0"
  # Add Wear OS support
```

**Key considerations:**
- Wear OS apps are essentially Android apps with special UI constraints
- Screen sizes: Round (360x360) and Square (390x390) variants
- Battery optimization is critical
- Limited input methods (voice-first is perfect!)

### **2. Create Wear OS Module**

**Structure:**
```
mobile_app/
├── lib/
│   ├── wear/              # New Wear OS specific code
│   │   ├── wear_main.dart
│   │   ├── wear_chat_screen.dart
│   │   └── wear_ws_client.dart  # Reuse or adapt existing
│   └── ... (existing mobile code)
├── wear/                   # New Wear OS app entry point
│   └── main.dart
└── android/
    └── wear/               # Wear OS specific Android config
        └── AndroidManifest.xml
```

### **3. Reuse Existing Code**

**What you can reuse:**
- ✅ `LunaWsClient` (WebSocket client) - **mostly reusable**
- ✅ `ws_dto.dart` (protocol definitions) - **100% reusable**
- ✅ `ServerConfig` - **reusable** (may need phone sync)
- ✅ `SpeechService` (STT) - **reusable**
- ✅ `TtsService` (TTS) - **reusable**
- ✅ `AppController` logic - **adaptable**

**What needs adaptation:**
- 🔄 UI components (watch screens are tiny)
- 🔄 Server config management (sync from phone or manual entry)
- 🔄 Connection retry logic (watch has less reliable network)

### **4. Minimal UI for Conversation Mode**

**Watch UI should be:**
- **Voice-first**: Mic button → speak → show response
- **Scrollable chat bubbles**: Compact message display
- **Status indicators**: Connection, streaming, speaking
- **Minimal controls**: Stop button, maybe settings

**Example layout:**
```
┌─────────────────┐
│  🔴 Listening   │  ← Status bar
├─────────────────┤
│  You: "Hello"   │  ← Chat bubbles
│  AI: "Hi there" │     (scrollable)
│  You: "..."     │
├─────────────────┤
│  [🎤] [⚙️]      │  ← Bottom controls
└─────────────────┘
```

### **5. Communication Architecture (Phone Proxy Required)**

Since phone is on Tailscale VPN, watch MUST communicate through phone:

#### **Phone App Changes (Required):**

You need to add a **WebSocket Proxy Layer** in your phone app:

```dart
// mobile_app/lib/services/wear_proxy_service.dart
class WearProxyService {
  final LunaWsClient _wsClient; // Your existing WebSocket client
  StreamSubscription? _wearMessageSubscription;
  StreamSubscription? _wsEventSubscription;
  
  Future<void> start() async {
    // Listen for messages from watch
    WearableDataLayer.listen((message) {
      _handleWatchMessage(message);
    });
    
    // Forward WebSocket events to watch
    _wsEventSubscription = _wsClient.events.listen((event) {
      _forwardToWatch(event);
    });
  }
  
  void _handleWatchMessage(Map<String, dynamic> message) {
    switch (message['type']) {
      case 'send_message':
        // Forward to backend via existing WebSocket
        _wsClient.send(ClientCommand.sendMessage(
          content: message['content'],
          conversationId: message['conversation_id'],
        ));
        break;
      case 'connect':
        // Watch wants to connect - ensure phone is connected
        _wsClient.connect(_getServerConfig());
        break;
      // ... other commands
    }
  }
  
  void _forwardToWatch(ServerEvent event) {
    // Convert ServerEvent to JSON and send to watch
    WearableDataLayer.sendMessage({
      'type': 'server_event',
      'event': event.toJson(), // Need to add toJson() to ServerEvent
    });
  }
}
```

#### **Watch App:**

```dart
// wear/lib/wear_main.dart
void main() {
  runApp(WearLunaApp());
}

class WearLunaApp extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return ProviderScope(
      child: MaterialApp(
        home: WearChatScreen(), // Conversation-only screen
      ),
    );
  }
}
```

**Watch uses Wearable Data Layer instead of WebSocket:**
- Watch sends commands to phone via `WearableDataLayer.sendMessage()`
- Watch receives events from phone via `WearableDataLayer.listen()`
- Phone app handles all WebSocket communication with backend

---

## 🛠️ **Implementation Steps**

### **Phase 1: Basic Setup**
1. ✅ Add Wear OS support to Flutter project
2. ✅ Create `wear/` module structure
3. ✅ Configure Android Wear manifest
4. ✅ Test basic Flutter app on watch emulator

### **Phase 2: Phone Proxy Layer (CRITICAL)**
1. ✅ Add `WearProxyService` to phone app
2. ✅ Implement message forwarding: Watch → Phone → Backend
3. ✅ Implement event forwarding: Backend → Phone → Watch
4. ✅ Handle connection state sync between watch and phone
5. ✅ Add Wearable Data Layer API dependency

### **Phase 3: Watch Core Functionality**
1. ✅ Create `WearDataLayerClient` (replaces WebSocket client on watch)
2. ✅ Create minimal `WearChatScreen` UI
3. ✅ Integrate STT for voice input
4. ✅ Integrate TTS for voice output
5. ✅ Handle connection state (phone connectivity)

### **Phase 4: Server Config Sync**
1. ✅ Phone automatically shares server config with watch
2. ✅ Watch receives config via Wearable Data Layer
3. ✅ No manual entry needed (phone handles it)

### **Phase 5: Polish**
1. ✅ Optimize battery usage
2. ✅ Handle network transitions (Wi-Fi ↔ Bluetooth)
3. ✅ Add haptic feedback
4. ✅ Error handling and user feedback

---

## 📦 **Dependencies Needed**

### **Phone App (Add to mobile_app/pubspec.yaml):**

```yaml
dependencies:
  # Existing dependencies...
  
  # NEW: For Wear OS communication
  wear: ^1.0.0  # Wearable Data Layer API wrapper
  # OR use platform channels with native Android Wear API
```

### **Watch App (wear/pubspec.yaml):**

```yaml
dependencies:
  flutter:
    sdk: flutter
  # Existing dependencies work on Wear OS:
  speech_to_text: ^7.0.0      # ✅ Works
  flutter_tts: ^4.2.0         # ✅ Works
  shared_preferences: ^2.2.3  # ✅ Works
  flutter_riverpod: ^3.0.3    # ✅ Works
  
  # NEW: For phone communication (NOT WebSocket!)
  wear: ^1.0.0  # Wearable Data Layer API
  
  # Shared code (if you create a shared package):
  # Or copy ws_dto.dart to watch app
```

**Note:** 
- Watch app does **NOT** need `web_socket_channel` (phone handles WebSocket)
- Watch app uses `wear` package for phone communication
- Most other Flutter packages work on Wear OS since it's Android-based

---

## ⚠️ **Challenges & Solutions**

### **1. Network Connectivity (Tailscale Constraint)**
**Challenge:** Watch cannot access backend directly (not on Tailscale VPN).

**Solution:**
- ✅ Phone proxy handles all backend communication
- ✅ Watch only needs Bluetooth connection to phone
- ✅ Show connection status: Phone connected? Backend connected?
- ✅ Handle phone disconnection gracefully

### **2. Battery Life**
**Challenge:** WebSocket + STT/TTS can drain battery.

**Solution:**
- Use `Wakelock` only when actively listening
- Close WebSocket when screen off (or use background service)
- Optimize TTS playback (shorter chunks)
- Consider phone proxy for better battery

### **3. Screen Size**
**Challenge:** Tiny screen, limited UI space.

**Solution:**
- Voice-first design (minimal typing)
- Compact chat bubbles
- Scrollable message list
- Status icons instead of text

### **4. Server Config Management**
**Challenge:** Watch needs server config, but can't access it directly.

**Solution:**
- ✅ **Automatic:** Phone syncs config to watch via Wearable Data Layer
- ✅ Watch receives config when phone app starts proxy service
- ✅ No manual entry needed (phone already has config)

---

## 🚀 **Quick Start Guide**

### **Step 1: Add Wear OS to Android Project**

```bash
# In mobile_app/android/
# Add wear module (Android Studio can generate this)
```

### **Step 2: Create Wear App Entry Point**

```dart
// mobile_app/wear/lib/main.dart
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

void main() {
  runApp(
    ProviderScope(
      child: WearLunaApp(),
    ),
  );
}

class WearLunaApp extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Luna Wear',
      theme: ThemeData.dark(),
      home: WearChatScreen(),
    );
  }
}
```

### **Step 3: Create Minimal Chat Screen**

```dart
// mobile_app/lib/wear/wear_chat_screen.dart
class WearChatScreen extends ConsumerStatefulWidget {
  @override
  ConsumerState<WearChatScreen> createState() => _WearChatScreenState();
}

class _WearChatScreenState extends ConsumerState<WearChatScreen> {
  // Reuse AppController or create WearAppController
  // Reuse LunaWsClient
  // Minimal UI: messages + mic button
}
```

### **Step 4: Test on Emulator**

```bash
# Start Wear OS emulator
# Run Flutter app targeting wear device
flutter run -d <wear_device_id>
```

---

## 📱 **Phone-Watch Communication (REQUIRED)**

Since watch cannot access backend directly (Tailscale constraint), phone MUST act as proxy:

### **On Phone App (Mobile):**

```dart
// mobile_app/lib/services/wear_proxy_service.dart
import 'package:wear/wear.dart'; // Need to add wear package
import '../data/ws/luna_ws_client.dart';
import '../data/ws/ws_dto.dart';
import '../core/config/server_config.dart';

class WearProxyService {
  final LunaWsClient _wsClient;
  StreamSubscription? _wearSubscription;
  StreamSubscription? _wsSubscription;
  ServerConfig? _serverConfig;
  
  WearProxyService(this._wsClient);
  
  Future<void> start(ServerConfig config) async {
    _serverConfig = config;
    
    // Send server config to watch
    await _sendToWatch({
      'type': 'server_config',
      'host': config.host,
      'port': config.port,
      'apiKey': config.apiKey,
      'profile': config.profile,
    });
    
    // Listen for messages from watch
    _wearSubscription = WearableDataLayer.listen((message) {
      _handleWatchMessage(message);
    });
    
    // Forward WebSocket events to watch
    _wsSubscription = _wsClient.events.listen((event) {
      _forwardEventToWatch(event);
    });
  }
  
  void _handleWatchMessage(Map<String, dynamic> message) {
    switch (message['type']) {
      case 'connect':
        // Watch wants to connect - ensure phone is connected
        if (!_wsClient.isConnected && _serverConfig != null) {
          _wsClient.connect(_serverConfig!);
        }
        break;
        
      case 'send_message':
        // Forward message to backend
        _wsClient.send(ClientCommand.sendMessage(
          content: message['content'] as String,
          conversationId: message['conversation_id'] as String?,
        ));
        break;
        
      case 'start_conversation':
        _wsClient.send(ClientCommand.startConversation(
          message['title'] as String? ?? 'Watch Conversation',
        ));
        break;
        
      case 'stop_streaming':
        _wsClient.send(ClientCommand.stopStreaming(
          conversationId: message['conversation_id'] as String?,
        ));
        break;
        
      // Add other commands as needed
    }
  }
  
  void _forwardEventToWatch(ServerEvent event) {
    // Convert ServerEvent to JSON for watch
    final eventJson = _eventToJson(event);
    _sendToWatch({
      'type': 'server_event',
      'event': eventJson,
    });
  }
  
  Map<String, dynamic> _eventToJson(ServerEvent event) {
    // Convert ServerEvent to JSON
    // You'll need to add toJson() methods to ServerEvent classes
    // Or use a switch statement to serialize each event type
    return {'type': 'unknown'}; // Placeholder
  }
  
  Future<void> _sendToWatch(Map<String, dynamic> message) async {
    try {
      await WearableDataLayer.sendMessage(message);
    } catch (e) {
      debugPrint('Failed to send to watch: $e');
    }
  }
  
  void dispose() {
    _wearSubscription?.cancel();
    _wsSubscription?.cancel();
  }
}
```

### **On Watch App:**

```dart
// wear/lib/services/wear_data_client.dart
import 'package:wear/wear.dart';

class WearDataClient {
  StreamSubscription? _subscription;
  final _eventController = StreamController<ServerEvent>.broadcast();
  
  Stream<ServerEvent> get events => _eventController.stream;
  
  Future<void> connect() async {
    // Request connection from phone
    await WearableDataLayer.sendMessage({'type': 'connect'});
    
    // Listen for messages from phone
    _subscription = WearableDataLayer.listen((message) {
      _handlePhoneMessage(message);
    });
  }
  
  void _handlePhoneMessage(Map<String, dynamic> message) {
    switch (message['type']) {
      case 'server_config':
        // Store config (not needed if phone handles everything)
        break;
        
      case 'server_event':
        // Convert JSON back to ServerEvent
        final event = ServerEvent.fromJson(message['event']);
        _eventController.add(event);
        break;
    }
  }
  
  void send(ClientCommand command) {
    // Convert command to message and send to phone
    WearableDataLayer.sendMessage({
      'type': _commandTypeToMessageType(command.type),
      ...command.payload,
    });
  }
  
  String _commandTypeToMessageType(String commandType) {
    // Map ClientCommand types to message types
    switch (commandType) {
      case 'send_message': return 'send_message';
      case 'start_conversation': return 'start_conversation';
      case 'stop_streaming': return 'stop_streaming';
      default: return commandType;
    }
  }
  
  void dispose() {
    _subscription?.cancel();
    _eventController.close();
  }
}
```

---

## 🎨 **UI Design Recommendations**

### **Conversation Screen Layout:**

1. **Top Bar (Compact):**
   - Connection status icon (🟢/🔴)
   - Streaming indicator (if active)

2. **Message List (Scrollable):**
   - Compact chat bubbles
   - User messages: Right-aligned, smaller
   - AI messages: Left-aligned, slightly larger
   - Truncate long messages (tap to expand?)

3. **Bottom Controls:**
   - 🎤 Mic button (primary action)
   - ⚙️ Settings (optional, drawer?)
   - 🛑 Stop button (when streaming)

### **Voice Mode Overlay:**
- Similar to mobile app
- Show "Listening..." / "Speaking..." / "Processing..."
- Haptic feedback on state changes

---

## ✅ **Feasibility Verdict**

**YES, absolutely feasible!** Here's why:

1. ✅ **Flutter supports Wear OS** (experimental but functional)
2. ✅ **Your existing code is mostly reusable** (WebSocket protocol, STT, TTS)
3. ✅ **Conversation-only mode simplifies UI** (no complex navigation)
4. ✅ **Phone proxy approach is well-documented** (Wearable Data Layer API)
5. ✅ **Voice-first design fits watch perfectly**
6. ⚠️ **Phone proxy adds complexity** but is necessary due to Tailscale constraint

**Estimated effort:**
- **Phone proxy layer:** 2-3 days (critical, must do first)
- **Basic watch app:** 2-3 days
- **Integration & testing:** 1-2 days
- **Polished version:** 1-2 weeks total

---

## 🔗 **Next Steps**

1. **Research Flutter Wear OS** (check latest Flutter docs)
2. **Set up Wear OS emulator** for testing
3. **Create minimal wear module** structure
4. **Port WebSocket client** (or create shared package)
5. **Build minimal chat UI** for watch
6. **Test on real device** (if available)

---

## 📚 **Resources**

- [Flutter Wear OS Support](https://docs.flutter.dev/get-started/wear-os)
- [Wearable Data Layer API](https://developer.android.com/training/wearables/data-layer)
- [Wear OS Design Guidelines](https://developer.android.com/design/wear)

---

**Bottom line:** This is a great idea and totally doable! The conversation-only mode is perfect for a watch form factor. 

**⚠️ Important:** Due to Tailscale VPN constraint, phone proxy is **REQUIRED**, not optional. The watch cannot access the backend directly. Plan for 2-3 days to implement the phone proxy layer before building the watch app.

