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

        let mut sink = rodio::DeviceSinkBuilder::open_default_sink()
            .map_err(|e| format!("Failed to open audio stream: {}", e))?;
        sink.log_on_drop(false);

        let cursor = Cursor::new(audio_bytes);
        let player = rodio::play(sink.mixer(), cursor)
            .map_err(|e| format!("Failed to play audio: {}", e))?;
        player.sleep_until_end();
        
        Ok(())
    }
}

