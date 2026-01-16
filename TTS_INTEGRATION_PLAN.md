# TTS Integration Plan

## Overview
Integrate Text-to-Speech (TTS) functionality from `/home/digit1024/proj/ttsandsttp/` into Luna ThinUI using DBus communication and libcosmic's DBus subscription capabilities.

## Key UX Behavior

**IMPORTANT**: When clicking play on a message:
- **ONLY that specific message's button** changes from play (▶) to stop (⏹)
- **All other messages** continue showing the play button
- When TTS finishes or is stopped, **only that message's button** changes back to play
- If a new message starts playing, the previous message's button automatically reverts to play

This ensures a clear visual indication of which message is currently being spoken.

## Architecture Analysis

### TTS Service (ttsandsttp)
- **DBus Service**: `com.github.digit1024.ttsstt`
- **DBus Object Path**: `/com/github/digit1024/ttsstt`
- **DBus Interface**: `com.github.digit1024.ttsstt.Service`

#### Methods:
1. **`Tts(text: String, language: String)`**
   - Converts text to speech
   - Returns immediately (non-blocking)
   - Cancels any ongoing STT/TTS operations
   - Emits `StatusChanged("speaking")` when starting
   - Emits `StatusChanged("idle")` when complete

2. **`Stop() -> String`**
   - Stops all TTS/STT operations immediately
   - Returns recognized text if STT was active
   - Emits `StatusChanged("idle")` after stopping

#### Signals:
- **`StatusChanged(status: String)`**
  - Possible values: `"idle"`, `"speaking"`, `"listening"`, `"processing"`
  - Emitted when service status changes

### Libcosmic DBus Integration
- Uses `zbus` crate for DBus communication
- Subscription pattern in `cosmic-config/src/subscription.rs`
- Can use `zbus::Connection::session()` to connect
- Signal subscription via `receive_signal()` or proxy pattern

## Implementation Plan

### Phase 1: DBus Client Setup

#### 1.1 Add Dependencies
- Add `zbus` to `luna_thin_ui/Cargo.toml` (if not already present)
- Ensure `zbus` version matches ttsandsttp (version 5.x)

#### 1.2 Create TTS Client Module
**File**: `luna_thin_ui/src/services/tts_client.rs`

```rust
// TTS Client using zbus
pub struct TtsClient {
    connection: zbus::Connection,
    proxy: TtsSttProxy<'static>,
}

impl TtsClient {
    // Connect to DBus service
    pub async fn new() -> Result<Self>;
    
    // Call Tts method
    pub async fn speak(&self, text: String, language: String) -> Result<()>;
    
    // Call Stop method
    pub async fn stop(&self) -> Result<String>;
    
    // Subscribe to StatusChanged signal
    pub async fn subscribe_status(&self) -> Result<StatusStream>;
}
```

### Phase 2: Markdown Stripping

#### 2.1 Create Markdown Stripper Utility
**File**: `luna_thin_ui/src/utils/markdown_strip.rs`

**Requirements**:
- Remove all markdown syntax:
  - Code blocks: ` ``` ` and ` ```language `
  - Inline code: `` `code` ``
  - Bold: `**text**` and `__text__`
  - Italic: `*text*` and `_text_`
  - Headers: `#`, `##`, `###`, etc.
  - Links: `[text](url)` → `text`
  - Images: `![alt](url)` → `alt`
  - Lists: `-`, `*`, `1.`
  - Blockquotes: `>`
  - Horizontal rules: `---`, `***`
  - Strikethrough: `~~text~~`
- Preserve plain text content
- Handle nested markdown
- Preserve sentence structure

**Implementation Options**:
1. Use `pulldown-cmark` to parse and extract text only
2. Use regex-based stripping (simpler but less robust)
3. Use `markdown` crate if available

**Recommended**: Use `pulldown-cmark` for robust parsing:
```rust
pub fn strip_markdown(text: &str) -> String {
    use pulldown_cmark::{Parser, Event, Tag};
    
    let parser = Parser::new(text);
    let mut result = String::new();
    
    for event in parser {
        match event {
            Event::Text(text) => result.push_str(&text),
            Event::Code(text) => result.push_str(&text),
            Event::SoftBreak | Event::HardBreak => result.push(' '),
            _ => {} // Ignore all other markdown elements
        }
    }
    
    // Clean up extra whitespace
    result.split_whitespace().collect::<Vec<_>>().join(" ")
}
```

### Phase 3: UI Integration

#### 3.1 Update Message Types
**File**: `luna_thin_ui/src/ui/app.rs`

Add new message types:
```rust
pub enum Message {
    // ... existing messages ...
    
    // TTS messages
    StartTts(String), // message_id
    StopTts,
    TtsStatusChanged(String), // "idle" | "speaking" | "listening" | "processing"
}
```

#### 3.2 Update App State
**File**: `luna_thin_ui/src/ui/app.rs`

Add to `LunaThinApp`:
```rust
pub struct LunaThinApp {
    // ... existing fields ...
    
    // TTS state
    pub tts_client: Option<Arc<TtsClient>>,
    pub tts_status: TtsStatus, // idle | speaking
    pub current_tts_message_id: Option<String>, // ID of message being spoken
}
```

#### 3.3 Update Message Bubble
**File**: `luna_thin_ui/src/ui/widgets/message_bubble.rs`

**Changes**:
1. **Conditional button rendering per message**:
   - Show `playback-symbolic.svg` button for ALL messages when TTS is idle
   - Show `playback-symbolic.svg` button for messages that are NOT currently being spoken
   - Show `stop-symbolic.svg` button ONLY for the message that is currently being spoken
   - This ensures only ONE message shows the stop button at a time

2. Pass TTS status to bubble component
3. Update button action based on status

**Function signature update**:
```rust
pub fn assistant_bubble<M: Clone + 'static>(
    // ... existing params ...
    is_current_tts_message: bool, // true if THIS message is being spoken
    on_playback: Option<M>, // Start TTS for this message
    on_stop_tts: Option<M>, // Stop TTS (only shown when is_current_tts_message == true)
) -> Element<'static, M>
```

**Button rendering logic**:
```rust
// In assistant_bubble function:
let playback_button = if is_current_tts_message {
    // This message is being spoken - show STOP button
    button::icon(icons::get_handle("stop-symbolic", 16))
        .on_press(on_stop_tts.unwrap_or_else(|| on_copy.clone()))
        .class(cosmic::style::Button::Text)
        .padding(4)
} else {
    // This message is NOT being spoken - show PLAY button
    button::icon(icons::get_handle("playback-symbolic", 16))
        .on_press(on_playback.unwrap_or_else(|| on_copy.clone()))
        .class(cosmic::style::Button::Text)
        .padding(4)
};
```

#### 3.4 Update Message List
**File**: `luna_thin_ui/src/ui/pages/chat/message_list.rs`

Pass TTS status to message bubbles - **ONLY the current message shows stop button**:
```rust
fn render_message_bubble(...) {
    // Check if THIS specific message is currently being spoken
    let is_current_tts = app.current_tts_message_id.as_ref()
        .map(|id| id == &msg.id)
        .unwrap_or(false);
    
    // Only pass stop callback if this is the current message
    // Only pass playback callback if this is NOT the current message
    message_bubble(
        // ... existing params ...
        is_current_tts, // true only for the message being spoken
        if !is_current_tts {
            // Not being spoken - show play button
            Some(Message::StartTts(msg.id.clone()))
        } else {
            None // Being spoken - will show stop button instead
        },
        if is_current_tts {
            // Being spoken - show stop button
            Some(Message::StopTts)
        } else {
            None // Not being spoken - will show play button instead
        },
    )
}
```

**Key Behavior**:
- When user clicks play on Message A:
  - Message A's button changes to STOP
  - All other messages keep their PLAY buttons
  - `current_tts_message_id` is set to Message A's ID
  
- When user clicks stop (or TTS finishes):
  - Message A's button changes back to PLAY
  - `current_tts_message_id` is cleared
  - All messages show PLAY buttons again
  
- When user clicks play on Message B while Message A is playing:
  - Message A's button changes to PLAY (previous stops)
  - Message B's button changes to STOP
  - `current_tts_message_id` is updated to Message B's ID

### Phase 4: TTS Handler

#### 4.1 Create TTS Handler
**File**: `luna_thin_ui/src/ui/handlers/tts.rs`

```rust
pub fn handle_tts_messages(
    app: &mut LunaThinApp,
    message: Message,
) -> Option<app::Task<Message>> {
    match message {
        Message::StartTts(message_id) => {
            // Find message content
            let msg = app.messages.iter()
                .find(|m| m.id == message_id)?;
            
            // Strip markdown
            let plain_text = strip_markdown(&msg.content);
            
            // Get language (default to "en-US" for now)
            let language = "en-US".to_string();
            
            // IMPORTANT: Set current_tts_message_id BEFORE starting TTS
            // This ensures the UI immediately shows stop button for this message
            app.current_tts_message_id = Some(message_id.clone());
            app.tts_status = TtsStatus::Speaking;
            
            // Start TTS
            if let Some(ref client) = app.tts_client {
                let client = client.clone();
                let message_id_clone = message_id.clone();
                return Some(app::Task::perform(
                    async move {
                        if let Err(e) = client.speak(plain_text, language).await {
                            tracing::error!("TTS error: {}", e);
                        }
                        Message::TtsStatusChanged("speaking".to_string())
                    },
                    |msg| cosmic::Action::App(msg),
                ));
            }
            None
        }
        Message::StopTts => {
            if let Some(ref client) = app.tts_client {
                let client = client.clone();
                return Some(app::Task::perform(
                    async move {
                        if let Err(e) = client.stop().await {
                            tracing::error!("TTS stop error: {}", e);
                        }
                        Message::TtsStatusChanged("idle".to_string())
                    },
                    |msg| cosmic::Action::App(msg),
                ));
            }
            None
        }
        Message::TtsStatusChanged(status) => {
            app.tts_status = match status.as_str() {
                "speaking" => TtsStatus::Speaking,
                _ => TtsStatus::Idle,
            };
            // When TTS becomes idle, clear current message ID
            // This causes the stop button to change back to play button
            if app.tts_status == TtsStatus::Idle {
                app.current_tts_message_id = None;
            }
            None
        }
        _ => None,
    }
}
```

### Phase 5: Status Subscription

#### 5.1 Subscribe to StatusChanged Signal
**File**: `luna_thin_ui/src/ui/app.rs`

In `init()` or `view()`:
```rust
// Subscribe to TTS status changes
if let Some(ref client) = self.tts_client {
    let status_stream = client.subscribe_status().await?;
    // Convert stream to iced subscription
    // This will emit Message::TtsStatusChanged when status changes
}
```

**Implementation in TtsClient**:
```rust
pub async fn subscribe_status(&self) -> Result<impl Stream<Item = String>> {
    use zbus::fdo::PropertiesProxy;
    
    let properties = PropertiesProxy::builder(&self.connection)
        .destination("com.github.digit1024.ttsstt")?
        .path("/com/github/digit1024/ttsstt")?
        .build()
        .await?;
    
    // Subscribe to StatusChanged signal
    let mut stream = self.proxy.receive_status_changed().await?;
    
    Ok(async_stream::stream! {
        while let Some(signal) = stream.next().await {
            if let Ok(args) = signal.args() {
                yield args.status().to_string();
            }
        }
    })
}
```

### Phase 6: Initialization

#### 6.1 Initialize TTS Client
**File**: `luna_thin_ui/src/ui/app.rs`

In `init()`:
```rust
// Initialize TTS client
let tts_client = match TtsClient::new().await {
    Ok(client) => {
        tracing::info!("TTS client connected");
        Some(Arc::new(client))
    }
    Err(e) => {
        tracing::warn!("Failed to connect to TTS service: {}", e);
        None // Continue without TTS
    }
};
```

## File Structure

```
luna_thin_ui/
├── src/
│   ├── services/
│   │   └── tts_client.rs          # NEW: DBus TTS client
│   ├── utils/
│   │   └── markdown_strip.rs      # NEW: Markdown stripping utility
│   ├── ui/
│   │   ├── app.rs                 # MODIFY: Add TTS state and messages
│   │   ├── handlers/
│   │   │   └── tts.rs             # NEW: TTS message handler
│   │   ├── pages/
│   │   │   └── chat/
│   │   │       └── message_list.rs # MODIFY: Pass TTS status to bubbles
│   │   └── widgets/
│   │       └── message_bubble.rs   # MODIFY: Conditional playback/stop button
│   └── main.rs
└── Cargo.toml                      # MODIFY: Add dependencies
```

## Dependencies to Add

```toml
[dependencies]
# ... existing dependencies ...
zbus = "5.12"  # Already present, verify version
pulldown-cmark = "0.9"  # For markdown stripping
async-stream = "0.3"  # Already present
```

## Testing Checklist

- [ ] TTS client connects to DBus service
- [ ] StatusChanged signal subscription works
- [ ] Markdown stripping removes all syntax correctly
- [ ] Playback button starts TTS
- [ ] Stop button appears when TTS is active
- [ ] Stop button stops TTS immediately
- [ ] Status updates correctly in UI
- [ ] Multiple messages can be spoken sequentially
- [ ] TTS works for long messages
- [ ] Error handling when TTS service is unavailable

## Edge Cases

1. **TTS service not running**: Gracefully handle connection failure
2. **Multiple rapid clicks**: Queue or cancel previous request
3. **Long messages**: Ensure markdown stripping doesn't break sentence structure
4. **Special characters**: Ensure TTS handles Unicode correctly
5. **Language detection**: For now, default to "en-US", can be enhanced later

## Future Enhancements

1. Language detection from message content
2. Voice selection
3. Speed/pitch control
4. Queue management for multiple messages
5. Visual indicator (waveform) during playback
6. Resume from last position
7. Integration with conversation language settings

