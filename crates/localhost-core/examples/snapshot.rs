use std::time::Duration;

use localhost_core::{Config, Engine};

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let engine = Engine::new(Config::default());
    engine.start();
    let mut rx = engine.subscribe();
    for _ in 0..2 {
        let _ = rx.changed().await;
    }
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let snapshot = engine.snapshot();
    println!(
        "{}",
        serde_json::to_string_pretty(&*snapshot).unwrap_or_default()
    );
}
