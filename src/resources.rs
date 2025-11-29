use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "res/audio"]
pub struct AudioAssets;
