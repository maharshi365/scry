#![expect(
    clippy::unwrap_used,
    reason = "Local HTTP fixtures and expected results must fail tests on error"
)]

use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn typed_stale_readers_use_shared_bytes_until_revalidation_finishes() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (release, released) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request(&mut socket).await;
        socket.write_all(b"HTTP/1.1 200 OK\r\nCache-Control: max-age=0, stale-while-revalidate=60\r\nContent-Length: 2\r\nConnection: close\r\n\r\n42").await.unwrap();
        drop(socket);
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request(&mut socket).await;
        released.await.unwrap();
        socket.write_all(b"HTTP/1.1 200 OK\r\nCache-Control: max-age=60\r\nContent-Length: 2\r\nConnection: close\r\n\r\n43").await.unwrap();
    });
    let url = url::Url::parse(&format!("http://{address}/metadata")).unwrap();
    let cache = HttpCache::new(reqwest::Client::new(), None);
    for _ in 0..3 {
        let value = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            cache.json_checked(url.clone(), 100, true, |_: &u32| Ok(())),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(value, 42);
    }
    assert_eq!(cache.0.state.lock().unwrap().active.len(), 1);
    assert!(
        cache
            .0
            .state
            .lock()
            .unwrap()
            .active
            .values()
            .all(|active| matches!(&*active.result.borrow(), Some(Ok(Payload::Bytes(_))))),
        "stalled revalidation must not retain decoded results"
    );
    release.send(()).unwrap();
    server.await.unwrap();
    until(&cache, |state| state.active.is_empty()).await;
    assert_eq!(
        cache
            .json_checked(url, 100, true, |_: &u32| Ok(()))
            .await
            .unwrap(),
        43
    );
}

#[tokio::test]
async fn chunked_response_allocation_does_not_exceed_the_body_limit() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request(&mut socket).await;
        socket.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n").await.unwrap();
        for size in [60_000, 35_000] {
            socket
                .write_all(format!("{size:x}\r\n").as_bytes())
                .await
                .unwrap();
            socket.write_all(&vec![42; size]).await.unwrap();
            socket.write_all(b"\r\n").await.unwrap();
        }
        socket.write_all(b"0\r\n\r\n").await.unwrap();
    });
    let cache = HttpCache::new(reqwest::Client::new(), None);
    let response = cache
        .get(
            url::Url::parse(&format!("http://{address}/body")).unwrap(),
            100_000,
            false,
        )
        .await
        .unwrap();
    assert_eq!(response.bytes.len(), 95_000);
    assert!(response.bytes.iter().all(|byte| *byte == 42));
    assert!(
        response.bytes.capacity() <= 100_000,
        "unused capacity must respect the body budget"
    );
    server.await.unwrap();
}

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
    held_body(b"body", 1).await
}

async fn held_body(
    body: &[u8],
    requests: usize,
) -> (
    url::Url,
    tokio::sync::oneshot::Receiver<()>,
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (seen, received) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nCache-Control: max-age=60\r\nConnection: close\r\n\r\n",
        body.len()
    ).into_bytes();
    let response = [response, body.to_vec()].concat();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request(&mut socket).await;
        seen.send(()).unwrap();
        let _ = released.await;
        let _ = socket.write_all(&response).await;
        for _ in 1..requests {
            let (mut socket, _) = listener.accept().await.unwrap();
            read_request(&mut socket).await;
            socket.write_all(&response).await.unwrap();
        }
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
async fn json_consumers_use_their_own_response_schema() {
    let (url, received, release, server) = held_body(b"42", 2).await;
    let cache = HttpCache::new(reqwest::Client::new(), None);
    let (first_cache, first_url) = (cache.clone(), url.clone());
    let first =
        tokio::spawn(async move { first_cache.get_json::<u32>(first_url, 100, true).await });
    received.await.unwrap();
    let second_cache = cache.clone();
    let second = tokio::spawn(async move { second_cache.get_json::<String>(url, 100, true).await });
    until(&cache, |state| {
        state
            .active
            .values()
            .map(|active| active.result.receiver_count())
            .sum::<usize>()
            == 2
    })
    .await;
    release.send(()).unwrap();
    assert_eq!(first.await.unwrap().unwrap().bytes.as_slice(), b"42");
    assert_eq!(
        second.await.unwrap().err().unwrap().to_string(),
        "Invalid add-on response"
    );
    server.await.unwrap();
}

#[tokio::test]
async fn raw_and_json_consumers_do_not_share_validation_failures() {
    for json_first in [false, true] {
        let (url, received, release, server) = held_body(b"body", 2).await;
        let cache = HttpCache::new(reqwest::Client::new(), None);
        let (first_cache, first_url) = (cache.clone(), url.clone());
        let first = tokio::spawn(async move {
            if json_first {
                first_cache
                    .get_json::<serde_json::Value>(first_url, 100, true)
                    .await
            } else {
                first_cache.get(first_url, 100, true).await
            }
        });
        received.await.unwrap();
        let second_cache = cache.clone();
        let second = tokio::spawn(async move {
            if json_first {
                second_cache.get(url, 100, true).await
            } else {
                second_cache
                    .get_json::<serde_json::Value>(url, 100, true)
                    .await
            }
        });
        // Both consumers must join or start their own request before releasing the first.
        until(&cache, |state| {
            state
                .active
                .values()
                .map(|active| active.result.receiver_count())
                .sum::<usize>()
                == 2
        })
        .await;
        release.send(()).unwrap();
        let first = first.await.unwrap();
        let second = second.await.unwrap();
        let (json, raw) = if json_first {
            (first, second)
        } else {
            (second, first)
        };
        assert!(
            json.is_err(),
            "Malformed JSON must be rejected for either arrival order"
        );
        assert_eq!(raw.unwrap().bytes.as_slice(), b"body");
        server.await.unwrap();
    }
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
