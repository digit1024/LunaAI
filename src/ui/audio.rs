use std::path::PathBuf;

/// Simple audio service for playing sound effects
pub struct AudioService;

impl AudioService {
    /// Play a sound file asynchronously without blocking the UI
    pub fn play_sound(filename: &str) {
        if let Some(path) = Self::get_audio_path(filename) {
            let path_clone = path.clone();
            tokio::spawn(async move {
                if let Err(e) = Self::play_sound_internal(path_clone) {
                    eprintln!("Failed to play sound: {}", e);
                }
            });
        } else {
            eprintln!("Audio file not found: {}", filename);
        }
    }

    fn play_sound_internal(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
        let (_stream, stream_handle) = rodio::OutputStream::try_default()?;
        let sink = rodio::Sink::try_new(&stream_handle)?;

        let file = std::fs::File::open(&path)?;
        let source = rodio::Decoder::new(std::io::BufReader::new(file))?;
        
        sink.append(source);
        sink.sleep_until_end();
        
        Ok(())
    }

    /// Get path to audio file in res/audio directory
    pub fn get_audio_path(filename: &str) -> Option<PathBuf> {
        // Try different possible locations for the audio files
        let possible_paths = [
            PathBuf::from("res/audio").join(filename),
            PathBuf::from("../res/audio").join(filename),
            PathBuf::from("../../res/audio").join(filename),
            std::env::current_dir()
                .ok()?
                .join("res/audio")
                .join(filename),
        ];

        for path in possible_paths.iter() {
            if path.exists() {
                return Some(path.clone());
            }
        }

        None
    }
}

