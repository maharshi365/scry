//! Local-only, matched release comparison of cacheable and no-store metadata.
use anyhow::{Context, Result, ensure};
use scry_addons::{Addon, Client};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const READS: usize = 100;

#[tokio::main(flavor = "current_thread")]
#[expect(
    clippy::print_stdout,
    reason = "The explicit local benchmark prints bounded summary metrics"
)]
async fn main() -> Result<()> {
    for run in 1..=3 {
        for cacheable in [false, true] {
            let (duration, requests, body_size) = measure(cacheable).await?;
            println!(
                "run={run} cacheable={cacheable} reads={READS} requests={requests} body_bytes={} elapsed_ms={:.2}",
                requests * body_size,
                duration.as_secs_f64() * 1000.0
            );
        }
    }
    Ok(())
}

async fn measure(cacheable: bool) -> Result<(Duration, usize, usize)> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let count = Arc::new(AtomicUsize::new(0));
    let requests = count.clone();
    let body = serde_json::json!({"meta":{"id":"film","type":"movie","name":"Local fixture","description":"x".repeat(4096),"poster":"https://images.example/poster.jpg"}}).to_string();
    let size = body.len();
    let response = format!(
        "HTTP/1.1 200 OK\r\nCache-Control: {}\r\nContent-Length: {size}\r\nConnection: close\r\n\r\n{body}",
        if cacheable { "max-age=60" } else { "no-store" }
    );
    let server = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let mut request = [0; 4096];
            if socket.read(&mut request).await.is_err() {
                break;
            }
            requests.fetch_add(1, Ordering::Relaxed);
            tokio::time::sleep(Duration::from_millis(5)).await;
            if socket.write_all(response.as_bytes()).await.is_err() {
                break;
            }
        }
    });
    let addon: Addon = serde_json::from_value(serde_json::json!({
        "manifest_url":format!("http://{address}/manifest.json"),"manifest":{"id":"fixture","name":"Fixture","resources":["meta"]}
    }))?;
    let client = Client::new()?;
    let started = Instant::now();
    for _ in 0..READS {
        let meta = client
            .meta(&addon, "movie", "film")
            .await
            .context("Local fixture request failed")?;
        ensure!(
            meta.poster.as_deref() == Some("https://images.example/poster.jpg"),
            "Local fixture was not preserved"
        );
    }
    let elapsed = started.elapsed();
    let count = count.load(Ordering::Relaxed);
    ensure!(
        count == if cacheable { 1 } else { READS },
        "Unexpected local request count"
    );
    server.abort();
    let _ = server.await;
    Ok((elapsed, count, size))
}
