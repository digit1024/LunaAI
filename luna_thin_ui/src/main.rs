//! Luna Thin UI - Entry Point
//!
//! A thin client that connects to a Luna server via WebSocket.

use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[derive(Parser, Debug)]
#[command(name = "luna-thin")]
#[command(author = "Michał Banaś")]
#[command(version = "0.1.0")]
#[command(about = "Luna AI Thin Client - connects to Luna server")]
struct Args {
    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,
}

fn main() -> cosmic::iced::Result {
    let args = Args::parse();

    // Initialize logging
    let filter = if args.debug {
        EnvFilter::new("debug,cosmic=info,iced=info,wgpu=warn")
    } else {
        EnvFilter::new("info,cosmic=warn,iced=warn,wgpu=error")
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("🌙 Starting Luna AI ThinUI");

    // Run the COSMIC application
    cosmic::app::run::<luna_thin_ui::ui::LunaThinApp>(
        luna_thin_ui::ui::settings(),
        (),
    )
}
