use anyhow::{Context, Result, ensure};
use scry_addons::{Addon, Client};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::JoinHandle,
};

pub struct Provider {
    address: SocketAddr,
    stopped: Arc<AtomicBool>,
    requests: Arc<AtomicUsize>,
    thread: Option<JoinHandle<Result<()>>>,
    pub addon: Addon,
    pub bytes: usize,
}

impl Provider {
    pub fn new(episodes: u32) -> Result<Self> {
        let videos: Vec<_> = (0..episodes)
            .map(|index| {
                serde_json::json!({
                    "id": format!("film:{index}"), "title": format!("Episode {index}"),
                    "season": index / 20 + 1, "episode": index % 20 + 1,
                    "released": "2020-01-01T00:00:00Z", "runtime": "45 min",
                    "thumbnail": format!("https://images.example/episode-{index}.jpg")
                })
            })
            .collect();
        let body = serde_json::json!({"meta": {
            "id": "film", "type": "series", "name": "Benchmark title",
            "description": "Synthetic metadata. ".repeat(100),
            "genres": ["Drama", "Comedy"], "imdbRating": "8.2", "releaseInfo": "2020–2026",
            "videos": videos
        }})
        .to_string();
        let bytes = body.len();
        let response = format!(
            "HTTP/1.1 200 OK\r\nCache-Control: no-store\r\nContent-Length: {bytes}\r\nConnection: close\r\n\r\n{body}"
        );
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        let stopped = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(AtomicUsize::new(0));
        let stop = stopped.clone();
        let hits = requests.clone();
        // A separate server thread keeps fixture allocations out of client heap measurements.
        let thread = std::thread::spawn(move || -> Result<()> {
            while !stop.load(Ordering::Relaxed) {
                let (mut socket, _) = listener.accept()?;
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                socket.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
                let mut request = [0; 8_192];
                let mut used = 0;
                while !request[..used].windows(4).any(|w| w == b"\r\n\r\n") {
                    let count =
                        socket.read(request.get_mut(used..).context("request too large")?)?;
                    ensure!(count > 0, "incomplete benchmark request");
                    used += count;
                    ensure!(used < request.len(), "request too large");
                }
                hits.fetch_add(1, Ordering::Relaxed);
                socket.write_all(response.as_bytes())?;
            }
            Ok(())
        });
        let addon = serde_json::from_value(serde_json::json!({
            "manifest_url": format!("http://{address}/manifest.json"),
            "manifest": {"id":"benchmark", "name":"Benchmark", "resources":["meta"]}
        }))?;
        Ok(Self {
            address,
            stopped,
            requests,
            thread: Some(thread),
            addon,
            bytes,
        })
    }

    pub fn requests(&self) -> usize {
        self.requests.load(Ordering::Relaxed)
    }
}

impl Drop for Provider {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(self.address);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub async fn fetch(
    client: &Client,
    provider: &Provider,
    episodes: u32,
    concurrent: usize,
) -> Result<()> {
    let before = provider.requests();
    let mut tasks = tokio::task::JoinSet::new();
    for index in 0..concurrent {
        let client = client.clone();
        let mut addon = provider.addon.clone();
        // Distinct resource URLs prevent in-flight deduplication from hiding work.
        addon.manifest_url = addon
            .manifest_url
            .replace("/manifest.json", &format!("/{index}/manifest.json"));
        tasks.spawn(async move {
            let meta = client.meta(&addon, "series", "film").await?;
            ensure!(
                meta.videos.len() == episodes as usize,
                "metadata was truncated"
            );
            std::hint::black_box(meta);
            Ok::<_, anyhow::Error>(())
        });
    }
    while let Some(result) = tasks.join_next().await {
        result??;
    }
    ensure!(
        provider.requests() - before == concurrent,
        "benchmark reused a cached response"
    );
    Ok(())
}
