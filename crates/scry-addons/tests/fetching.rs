#![expect(
    clippy::unwrap_used,
    reason = "Local server fixtures must fail tests on error"
)]

use scry_addons::{Addon, Client};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn provider(
    responses: Vec<String>,
) -> (Addon, Arc<Mutex<Vec<String>>>, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured = requests.clone();
    let task = tokio::spawn(async move {
        for response in responses {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 8192];
            let count = socket.read(&mut bytes).await.unwrap();
            captured
                .lock()
                .unwrap()
                .push(String::from_utf8_lossy(&bytes[..count]).into_owned());
            socket.write_all(response.as_bytes()).await.unwrap();
        }
    });
    let addon = serde_json::from_value(serde_json::json!({
        "manifest_url":format!("http://{address}/private/manifest.json"),
        "manifest":{"id":"test","name":"Test","resources":["meta"]}
    }))
    .unwrap();
    (addon, requests, task)
}

fn response(status: &str, headers: &str, name: &str) -> String {
    let body = if status.starts_with("304") {
        String::new()
    } else {
        serde_json::json!({"meta":{"id":"film","type":"movie","name":name,"poster":"https://images.example/saved.jpg"}}).to_string()
    };
    format!(
        "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[tokio::test]
async fn repeated_metadata_reuses_a_fresh_response() {
    let (addon, requests, server) = provider(vec![response(
        "200 OK",
        "Cache-Control: max-age=60\r\n",
        "Cached",
    )])
    .await;
    let client = Client::new().unwrap();
    assert_eq!(
        client.meta(&addon, "movie", "film").await.unwrap().name,
        "Cached"
    );
    assert_eq!(
        client.meta(&addon, "movie", "film").await.unwrap().name,
        "Cached"
    );
    assert_eq!(requests.lock().unwrap().len(), 1);
    server.await.unwrap();
}

#[tokio::test]
async fn stale_metadata_revalidates_and_uses_the_304_body() {
    let (addon, requests, server) = provider(vec![
        response(
            "200 OK",
            "Cache-Control: no-cache\r\nETag: \"film-v1\"\r\n",
            "Original",
        ),
        response(
            "304 Not Modified",
            "Cache-Control: max-age=60\r\nETag: \"film-v1\"\r\n",
            "",
        ),
    ])
    .await;
    let client = Client::new().unwrap();
    client.meta(&addon, "movie", "film").await.unwrap();
    assert_eq!(
        client.meta(&addon, "movie", "film").await.unwrap().name,
        "Original"
    );
    assert_eq!(
        client.meta(&addon, "movie", "film").await.unwrap().name,
        "Original"
    );
    assert_eq!(requests.lock().unwrap().len(), 2);
    assert!(
        requests.lock().unwrap()[1]
            .to_lowercase()
            .contains("if-none-match: \"film-v1\"")
    );
    server.await.unwrap();
}

#[tokio::test]
async fn no_store_and_explicit_refresh_do_not_reuse_a_fresh_body() {
    for directive in ["no-store", "max-age=60"] {
        let headers = format!("Cache-Control: {directive}\r\nETag: \"v1\"\r\n");
        let (addon, requests, server) = provider(vec![
            response("200 OK", &headers, "First"),
            response("200 OK", &headers, "Updated"),
        ])
        .await;
        let client = Client::new().unwrap();
        client.meta(&addon, "movie", "film").await.unwrap();
        if directive != "no-store" {
            client.invalidate_cache();
        }
        assert_eq!(
            client.meta(&addon, "movie", "film").await.unwrap().name,
            "Updated"
        );
        assert_eq!(requests.lock().unwrap().len(), 2);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn persistent_metadata_restores_the_poster_url_without_a_network_lookup() {
    let directory = tempfile::tempdir().unwrap();
    let (addon, requests, server) = provider(vec![response(
        "200 OK",
        "Cache-Control: max-age=60\r\n",
        "Saved",
    )])
    .await;
    let first = Client::with_cache_dir(Some(directory.path().into())).unwrap();
    first.meta(&addon, "movie", "film").await.unwrap();
    server.await.unwrap();
    drop(first);
    let restarted = Client::with_cache_dir(Some(directory.path().into())).unwrap();
    let restored = restarted.meta(&addon, "movie", "film").await.unwrap();
    assert_eq!(restored.name, "Saved");
    assert_eq!(
        restored.poster.as_deref(),
        Some("https://images.example/saved.jpg")
    );
    assert_eq!(requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn stale_if_error_is_opt_in_and_does_not_hide_a_missing_resource() {
    for (headers, status, usable) in [
        (
            "Cache-Control: max-age=0, stale-if-error=60\r\n",
            "503 Unavailable",
            true,
        ),
        ("Cache-Control: max-age=0\r\n", "503 Unavailable", false),
        (
            "Cache-Control: no-cache, stale-if-error=60\r\n",
            "503 Unavailable",
            false,
        ),
        (
            "Cache-Control: max-age=0, stale-if-error=60\r\n",
            "404 Not Found",
            false,
        ),
    ] {
        let (addon, _, server) = provider(vec![
            response("200 OK", headers, "Old"),
            response(status, "", ""),
        ])
        .await;
        let client = Client::new().unwrap();
        client.meta(&addon, "movie", "film").await.unwrap();
        let result = client.meta(&addon, "movie", "film").await;
        assert_eq!(result.is_ok(), usable);
        if usable {
            assert_eq!(result.unwrap().name, "Old");
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn configured_paths_never_share_cached_metadata() {
    let (addon, requests, server) = provider(vec![
        response(
            "200 OK",
            "Cache-Control: max-age=60\r\n",
            "Configuration one",
        ),
        response(
            "200 OK",
            "Cache-Control: max-age=60\r\n",
            "Configuration two",
        ),
    ])
    .await;
    let mut other = addon.clone();
    other.manifest_url = addon.manifest_url.replace("/private/", "/other/");
    let client = Client::new().unwrap();
    client.meta(&addon, "movie", "film").await.unwrap();
    assert_eq!(
        client.meta(&other, "movie", "film").await.unwrap().name,
        "Configuration two"
    );
    assert_eq!(requests.lock().unwrap().len(), 2);
    server.await.unwrap();
}

#[tokio::test]
async fn concurrent_consumers_share_even_a_no_store_request() {
    let (addon, requests, server) = provider(vec![response(
        "200 OK",
        "Cache-Control: no-store\r\n",
        "Shared",
    )])
    .await;
    let client = Client::new().unwrap();
    let (first, second) = tokio::join!(
        client.meta(&addon, "movie", "film"),
        client.meta(&addon, "movie", "film")
    );
    let (first, second) = (first.unwrap(), second.unwrap());
    assert_eq!(first.name, "Shared");
    assert_eq!(second.name, "Shared");
    assert!(
        std::ptr::eq(&raw const *first, &raw const *second),
        "concurrent consumers share the parsed metadata payload"
    );
    assert_eq!(requests.lock().unwrap().len(), 1);
    server.await.unwrap();
}

#[tokio::test]
async fn malformed_resource_is_not_persisted_or_used_to_replace_good_data() {
    let directory = tempfile::tempdir().unwrap();
    let malformed = "HTTP/1.1 200 OK\r\nCache-Control: max-age=60\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_owned();
    let (addon, requests, server) = provider(vec![
        malformed,
        response("200 OK", "Cache-Control: max-age=60\r\n", "Valid"),
    ])
    .await;
    let client = Client::with_cache_dir(Some(directory.path().into())).unwrap();
    assert!(client.meta(&addon, "movie", "film").await.is_err());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    assert_eq!(
        client.meta(&addon, "movie", "film").await.unwrap().name,
        "Valid"
    );
    assert_eq!(requests.lock().unwrap().len(), 2);
    server.await.unwrap();
}

#[tokio::test]
async fn mismatched_metadata_is_not_cached_before_retrying() {
    let directory = tempfile::tempdir().unwrap();
    let wrong = response("200 OK", "Cache-Control: max-age=60\r\n", "Wrong")
        .replace("\"id\":\"film\"", "\"id\":\"fake\"");
    let (addon, requests, server) = provider(vec![
        wrong,
        response("200 OK", "Cache-Control: max-age=60\r\n", "Valid"),
    ])
    .await;
    let client = Client::with_cache_dir(Some(directory.path().into())).unwrap();
    assert!(client.meta(&addon, "movie", "film").await.is_err());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    assert_eq!(
        client.meta(&addon, "movie", "film").await.unwrap().name,
        "Valid"
    );
    assert_eq!(requests.lock().unwrap().len(), 2);
    server.await.unwrap();
}

#[tokio::test]
async fn invalid_manifest_identity_is_not_cached_before_retrying() {
    let directory = tempfile::tempdir().unwrap();
    let responses = ["", "valid"].map(|id| {
        let body = serde_json::json!({"id":id,"name":"Provider"}).to_string();
        format!("HTTP/1.1 200 OK\r\nCache-Control: max-age=60\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
    });
    let (addon, requests, server) = provider(responses.into()).await;
    let client = Client::with_cache_dir(Some(directory.path().into())).unwrap();
    assert!(client.install(&addon.manifest_url).await.is_err());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    assert_eq!(
        client
            .install(&addon.manifest_url)
            .await
            .unwrap()
            .manifest
            .id,
        "valid"
    );
    assert_eq!(requests.lock().unwrap().len(), 2);
    server.await.unwrap();
}

#[tokio::test]
async fn invalid_identity_refresh_preserves_good_memory_and_disk_entries() {
    for manifest in [false, true] {
        let headers = "Cache-Control: max-age=0, stale-if-error=60\r\n";
        let (good, invalid) = if manifest {
            let encode = |id: &str| {
                let body = serde_json::json!({"id":id,"name":"Saved"}).to_string();
                format!(
                    "HTTP/1.1 200 OK\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
            };
            (encode("valid"), encode(""))
        } else {
            (
                response("200 OK", headers, "Saved"),
                response("200 OK", headers, "Invalid")
                    .replace("\"id\":\"film\"", "\"id\":\"fake\""),
            )
        };
        let directory = tempfile::tempdir().unwrap();
        let (addon, requests, server) = provider(vec![
            good,
            invalid,
            response("503 Unavailable", "", ""),
            response("503 Unavailable", "", ""),
        ])
        .await;
        let client = Client::with_cache_dir(Some(directory.path().into())).unwrap();
        if manifest {
            assert_eq!(
                client
                    .install(&addon.manifest_url)
                    .await
                    .unwrap()
                    .manifest
                    .id,
                "valid"
            );
            assert!(client.install(&addon.manifest_url).await.is_err());
            assert_eq!(
                client
                    .install(&addon.manifest_url)
                    .await
                    .unwrap()
                    .manifest
                    .id,
                "valid"
            );
        } else {
            assert_eq!(
                client.meta(&addon, "movie", "film").await.unwrap().name,
                "Saved"
            );
            assert!(client.meta(&addon, "movie", "film").await.is_err());
            assert_eq!(
                client.meta(&addon, "movie", "film").await.unwrap().name,
                "Saved"
            );
        }
        drop(client);
        let restarted = Client::with_cache_dir(Some(directory.path().into())).unwrap();
        if manifest {
            assert_eq!(
                restarted
                    .install(&addon.manifest_url)
                    .await
                    .unwrap()
                    .manifest
                    .id,
                "valid"
            );
        } else {
            assert_eq!(
                restarted.meta(&addon, "movie", "film").await.unwrap().name,
                "Saved"
            );
        }
        assert_eq!(requests.lock().unwrap().len(), 4);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn streams_never_use_the_metadata_stale_fallback_policy() {
    let body = r#"{"streams":[]}"#;
    let good = format!(
        "HTTP/1.1 200 OK\r\nCache-Control: max-age=0, stale-if-error=60, stale-while-revalidate=60\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let (addon, requests, server) = provider(vec![good, response("503 Unavailable", "", "")]).await;
    let client = Client::new().unwrap();
    assert!(
        client
            .streams(&addon, "movie", "film")
            .await
            .unwrap()
            .is_empty()
    );
    assert!(client.streams(&addon, "movie", "film").await.is_err());
    assert_eq!(requests.lock().unwrap().len(), 2);
    server.await.unwrap();
}
