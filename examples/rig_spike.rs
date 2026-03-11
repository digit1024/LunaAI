//! Standalone spike to verify Rig integration.
//!
//! Run with: cargo run --example rig_spike --features "rig,openai"
//! Requires OPENAI_API_KEY (or set in your Luna config preset).

use cosmic_llm::config::ModelPreset;
use cosmic_llm::rig_core::{run_turn_streaming, RigConversationContext, StreamChunk};
use futures::StreamExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_else(|_| {
        eprintln!("Set OPENAI_API_KEY to run this spike");
        std::process::exit(1);
    });

    let preset = ModelPreset {
        backend: "openai".to_string(),
        model: "gpt-4o-mini".to_string(),
        endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
        api_key,
        max_tokens: Some(256),
        temperature: Some(0.7),
        ..Default::default()
    };

    let context = RigConversationContext {
        messages: vec![],
        user_message: "Say hello in exactly 5 words.".to_string(),
        preset,
        preamble: "You are a helpful assistant.".to_string(),
        mcp_servers: vec![],
        internal_tools: vec![],
    };

    println!("Running Rig turn...");
    let mut stream = run_turn_streaming(context).await?;
    let mut content = String::new();
    while let Some(chunk) = stream.next().await {
        match chunk? {
            StreamChunk::Delta(text) | StreamChunk::Final(text) => content.push_str(&text),
            _ => {}
        }
    }
    println!("Response: {}", content);
    Ok(())
}
