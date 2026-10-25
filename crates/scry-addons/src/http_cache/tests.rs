#![expect(
    clippy::unwrap_used,
    reason = "Local HTTP fixtures and expected results must fail tests on error"
)]

use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn read_request(socket: &mut tokio::net::TcpStream) -> String {
    let mut request = Vec::new();
    while !request.ends_with(b"\r\n\r\n") {
        let mut bytes = [0; 1024];
        let count = socket.read(&mut bytes).await.unwrap();
        assert!(
            count > 0 && request.len() + count <= 8192,
            "Expected a bounded complete fixture request"
        );
        request.extend_from_slice(&bytes[..count]);
    }
    String::from_utf8(request).unwrap()
}

async fn held() -> (
    url::Url,
    tokio::sync::oneshot::Receiver<()>,
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (seen, received) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request(&mut socket).await;
        seen.send(()).unwrap();
        let _ = released.await;
        let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nCache-Control: max-age=60\r\nConnection: close\r\n\r\nbody").await;
    });
    (
        url::Url::parse(&format!("http://{address}/private")).unwrap(),
        received,
        release,
        server,
    )
}

async fn until(cache: &HttpCache, predicate: impl Fn(&State) -> bool) {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if predicate(&cache.0.state.lock().unwrap()) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn cancelling_one_consumer_preserves_another_and_last_consumer_cancels_work() {
    let (url, received, release, server) = held().await;
    let cache = HttpCache::new(reqwest::Client::new(), None);
    let (first_cache, first_url) = (cache.clone(), url.clone());
    let first = tokio::spawn(async move { first_cache.get(first_url, 100, true).await });
    received.await.unwrap();
    let second_cache = cache.clone();
    let second = tokio::spawn(async move { second_cache.get(url, 100, true).await });
    until(&cache, |state| {
        state
            .active
            .values()
            .any(|active| active.result.receiver_count() == 2)
    })
    .await;
    first.abort();
    let _ = first.await;
    release.send(()).unwrap();
    assert_eq!(second.await.unwrap().unwrap().bytes.as_slice(), b"body");
    server.await.unwrap();
    until(&cache, |state| state.active.is_empty()).await;

    let (url, received, release, server) = held().await;
    let first_cache = cache.clone();
    let first = tokio::spawn(async move { first_cache.get(url, 100, true).await });
    received.await.unwrap();
    first.abort();
    let _ = first.await;
    until(&cache, |state| state.active.is_empty()).await;
    assert_eq!(cache.0.permits.available_permits(), CONCURRENCY);
    drop(release);
    server.await.unwrap();
}

#[tokio::test]
async fn stale_while_revalidate_returns_immediately_and_keeps_one_refresh_alive() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (release, released) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request(&mut socket).await;
        socket.write_all(b"HTTP/1.1 200 OK\r\nCache-Control: max-age=0, stale-while-revalidate=60\r\nContent-Length: 3\r\nConnection: close\r\n\r\nold").await.unwrap();
        drop(socket);
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request(&mut socket).await;
        released.await.unwrap();
        socket.write_all(b"HTTP/1.1 200 OK\r\nCache-Control: max-age=60\r\nContent-Length: 3\r\nConnection: close\r\n\r\nnew").await.unwrap();
    });
    let url = url::Url::parse(&format!("http://{address}/film")).unwrap();
    let cache = HttpCache::new(reqwest::Client::new(), None);
    assert_eq!(
        cache
            .get(url.clone(), 100, true)
            .await
            .unwrap()
            .bytes
            .as_slice(),
        b"old"
    );
    assert_eq!(
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            cache.get(url.clone(), 100, true)
        )
        .await
        .unwrap()
        .unwrap()
        .bytes
        .as_slice(),
        b"old"
    );
    assert_eq!(
        cache
            .get(url.clone(), 100, true)
            .await
            .unwrap()
            .bytes
            .as_slice(),
        b"old"
    );
    assert_eq!(cache.0.state.lock().unwrap().active.len(), 1);
    release.send(()).unwrap();
    server.await.unwrap();
    until(&cache, |state| state.active.is_empty()).await;
    assert_eq!(
        cache.get(url, 100, true).await.unwrap().bytes.as_slice(),
        b"new"
    );
}

#[tokio::test]
async fn background_work_leaves_connections_for_an_interactive_request() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (arrived, mut arrivals) = tokio::sync::mpsc::channel(16);
    let (release, released) = watch::channel(false);
    let server = tokio::spawn(async move {
        let mut handlers = tokio::task::JoinSet::new();
        for _ in 0..14 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let arrived = arrived.clone();
            let mut released = released.clone();
            handlers.spawn(async move {
                let request = read_request(&mut socket).await;
                arrived
                    .send(request.contains("/interactive"))
                    .await
                    .unwrap();
                if !*released.borrow() {
                    released.changed().await.unwrap();
                }
                socket
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                    )
                    .await
                    .unwrap();
            });
        }
        while let Some(result) = handlers.join_next().await {
            result.unwrap();
        }
    });
    let cache = HttpCache::new(reqwest::Client::new(), None);
    let mut consumers = tokio::task::JoinSet::new();
    for index in 0..12 {
        let client = cache.background();
        consumers.spawn(async move {
            client
                .get(
                    url::Url::parse(&format!("http://{address}/background/{index}")).unwrap(),
                    100,
                    true,
                )
                .await
        });
    }
    for _ in 0..CONCURRENCY / 2 {
        assert!(
            !tokio::time::timeout(std::time::Duration::from_secs(2), arrivals.recv())
                .await
                .unwrap()
                .unwrap()
        );
    }
    let interactive = cache.clone();
    consumers.spawn(async move {
        interactive
            .get(
                url::Url::parse(&format!("http://{address}/interactive")).unwrap(),
                100,
                true,
            )
            .await
    });
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(2), arrivals.recv())
            .await
            .unwrap()
            .unwrap()
    );
    let queued = cache.background();
    let promoted_url = url::Url::parse(&format!("http://{address}/interactive-promoted")).unwrap();
    let queued_url = promoted_url.clone();
    consumers.spawn(async move { queued.get(queued_url, 100, true).await });
    until(&cache, |state| {
        state
            .active
            .keys()
            .any(|flight| flight.key.contains("/interactive-promoted"))
    })
    .await;
    let foreground = cache.clone();
    consumers.spawn(async move { foreground.get(promoted_url, 100, true).await });
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(2), arrivals.recv())
            .await
            .unwrap()
            .unwrap()
    );
    release.send(true).unwrap();
    while let Some(result) = consumers.join_next().await {
        assert_eq!(result.unwrap().unwrap().bytes.as_slice(), b"ok");
    }
    server.await.unwrap();
}

#[tokio::test]
async fn refresh_does_not_join_or_get_overwritten_by_a_previous_flight() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (seen, received) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut older, _) = listener.accept().await.unwrap();
        read_request(&mut older).await;
        seen.send(()).unwrap();
        let (mut newer, _) = listener.accept().await.unwrap();
        read_request(&mut newer).await;
        newer.write_all(b"HTTP/1.1 200 OK\r\nCache-Control: max-age=60\r\nContent-Length: 3\r\nConnection: close\r\n\r\nnew").await.unwrap();
        drop(newer);
        released.await.unwrap();
        older.write_all(b"HTTP/1.1 200 OK\r\nCache-Control: max-age=60\r\nContent-Length: 3\r\nConnection: close\r\n\r\nold").await.unwrap();
    });
    let url = url::Url::parse(&format!("http://{address}/film")).unwrap();
    let cache = HttpCache::new(reqwest::Client::new(), None);
    let (first_cache, first_url) = (cache.clone(), url.clone());
    let first = tokio::spawn(async move { first_cache.get(first_url, 100, true).await });
    received.await.unwrap();
    cache.invalidate();
    assert_eq!(
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            cache.get(url.clone(), 100, true)
        )
        .await
        .unwrap()
        .unwrap()
        .bytes
        .as_slice(),
        b"new"
    );
    release.send(()).unwrap();
    assert_eq!(first.await.unwrap().unwrap().bytes.as_slice(), b"old");
    assert_eq!(
        cache.get(url, 100, true).await.unwrap().bytes.as_slice(),
        b"new"
    );
    server.await.unwrap();
}
