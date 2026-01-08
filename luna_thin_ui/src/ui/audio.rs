use crate::resources::AudioAssets;
use std::io::Cursor;
use tracing;

/// Simple audio service for playing sound effects
/// Uses embedded audio files, so sounds work regardless of launch location
pub struct AudioService;

impl AudioService {
    /// Play a sound file asynchronously without blocking the UI
    /// The sound file is embedded in the executable, so it's always available
    pub fn play_sound(filename: &str) {
        let filename = filename.to_string();
        tokio::spawn(async move {
            if let Err(e) = Self::play_sound_internal(&filename).await {
                tracing::error!(
                    filename = %filename,
                    error = %e,
                    "Failed to play sound"
                );
            }
        });
    }

    async fn play_sound_internal(filename: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Get embedded audio file
        let audio_data = AudioAssets::get(filename)
            .ok_or_else(|| format!("Audio file '{}' not found in embedded resources", filename))?;

        // Convert to owned bytes for the cursor
        let audio_bytes = audio_data.data.into_owned();

        // Create audio stream using rodio's API (0.18+)
        let (_stream, stream_handle) = rodio::OutputStream::try_default()
            .map_err(|e| format!("Failed to open audio stream: {}", e))?;
        let sink = rodio::Sink::try_new(&stream_handle)
            .map_err(|e| format!("Failed to create sink: {}", e))?;

        // Decode from memory using Cursor
        let cursor = Cursor::new(audio_bytes);
        let source = rodio::Decoder::new(cursor)?;
        
        sink.append(source);
        sink.sleep_until_end();
        
        Ok(())
    }
}

