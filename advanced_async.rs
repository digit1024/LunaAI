// Advanced Rust Async Examples

use std::time::Duration;
use tokio::time::{sleep, interval};
use tokio::sync::mpsc;
use futures::stream::{self, StreamExt};

// Async stream example
async fn number_stream() -> impl futures::Stream<Item = i32> {
    stream::iter(1..=5)
        .then(|n| async move {
            sleep(Duration::from_millis(n * 200)).await;
            n
        })
}

// Producer-consumer pattern with channels
async fn producer_consumer_example() {
    let (tx, mut rx) = mpsc::channel(10);
    
    // Producer task
    let producer = tokio::spawn(async move {
        for i in 1..=5 {
            tx.send(format!("Message {}", i)).await.unwrap();
            sleep(Duration::from_millis(300)).await;
        }
    });
    
    // Consumer task
    let consumer = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            println!("Received: {}", message);
        }
    });
    
    tokio::join!(producer, consumer);
}

// Periodic task with interval
async fn periodic_task() {
    let mut interval = interval(Duration::from_secs(1));
    
    for _ in 0..3 {
        interval.tick().await;
        println!("Periodic task executed at {:?}", std::time::SystemTime::now());
    }
}

// Async function with timeout
async fn task_with_timeout() -> Result<String, tokio::time::error::Elapsed> {
    tokio::time::timeout(
        Duration::from_secs(3),
        async {
            sleep(Duration::from_secs(5)).await;
            "Task completed".to_string()
        }
    ).await
}

// Async select example
async fn select_example() {
    let fast_task = sleep(Duration::from_millis(100));
    let slow_task = sleep(Duration::from_millis(500));
    
    tokio::select! {
        _ = fast_task => println!("Fast task completed first!"),
        _ = slow_task => println!("Slow task completed first!"),
    }
}

#[tokio::main]
async fn main() {
    println!("=== Async Stream Example ===");
    let mut stream = number_stream();
    while let Some(n) = stream.next().await {
        println!("Stream item: {}", n);
    }
    
    println!("\n=== Producer-Consumer Example ===");
    producer_consumer_example().await;
    
    println!("\n=== Periodic Task Example ===");
    periodic_task().await;
    
    println!("\n=== Timeout Example ===");
    match task_with_timeout().await {
        Ok(result) => println!("Success: {}", result),
        Err(_) => println!("Task timed out!"),
    }
    
    println!("\n=== Select Example ===");
    select_example().await;
}