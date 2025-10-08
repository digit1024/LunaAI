// Rust Async Programming Examples

use std::time::Duration;
use tokio::time::sleep;

// Basic async function
async fn hello_world() {
    println!("Hello, world!");
}

// Async function with delay
async fn delayed_greeting(name: &str) {
    println!("Starting greeting for {}", name);
    sleep(Duration::from_secs(1)).await;
    println!("Hello, {}!", name);
}

// Async function that returns a value
async fn add_numbers(a: i32, b: i32) -> i32 {
    sleep(Duration::from_millis(500)).await;
    a + b
}

// Multiple async tasks running concurrently
async fn run_concurrent_tasks() {
    let task1 = delayed_greeting("Alice");
    let task2 = delayed_greeting("Bob");
    let task3 = delayed_greeting("Charlie");
    
    // Run all tasks concurrently
    tokio::join!(task1, task2, task3);
}

// Using async with Result
async fn fetch_data() -> Result<String, Box<dyn std::error::Error>> {
    sleep(Duration::from_secs(2)).await;
    Ok("Data fetched successfully!".to_string())
}

// Async function with error handling
async fn process_data() {
    match fetch_data().await {
        Ok(data) => println!("Received: {}", data),
        Err(e) => println!("Error: {}", e),
    }
}

// Main async function
#[tokio::main]
async fn main() {
    println!("=== Basic Async Example ===");
    hello_world().await;
    
    println!("\n=== Delayed Greeting ===");
    delayed_greeting("Rust Developer").await;
    
    println!("\n=== Async with Return Value ===");
    let result = add_numbers(5, 3).await;
    println!("5 + 3 = {}", result);
    
    println!("\n=== Concurrent Tasks ===");
    run_concurrent_tasks().await;
    
    println!("\n=== Error Handling ===");
    process_data().await;
    
    println!("\n=== Using select! for multiple futures ===");
    
    let future1 = delayed_greeting("Future 1");
    let future2 = delayed_greeting("Future 2");
    
    // This would require tokio::select! macro
    // tokio::select! {
    //     _ = future1 => println!("Future 1 completed first"),
    //     _ = future2 => println!("Future 2 completed first"),
    // }
}