//! Provider preference and fallback for background library metadata requests.
use scry_addons::{Addon, Client, Meta};
use std::{collections::BTreeMap, time::Duration};

const PROVIDERS: usize = 4;
const DEADLINE: Duration = Duration::from_secs(12);
const OWNER_GRACE: Duration = Duration::from_millis(300);

pub(super) async fn fetch<T>(
    client: &Client,
    addons: &[Addon],
    namespace: &str,
    kind: &str,
    id: &str,
    project: impl Fn(Meta) -> Option<T>,
) -> Option<T> {
    fetch_from(client, addons, namespace, kind, id, |_, meta| project(meta)).await
}

pub(super) async fn fetch_from<T>(
    client: &Client,
    addons: &[Addon],
    namespace: &str,
    kind: &str,
    id: &str,
    project: impl Fn(&Addon, Meta) -> Option<T>,
) -> Option<T> {
    let providers = addons
        .iter()
        .filter(|a| a.manifest.id == namespace)
        .chain(addons.iter().filter(|a| a.manifest.id != namespace))
        .filter(|a| a.supports("meta", kind, id));
    let mut remaining = providers.enumerate();
    let mut tasks = tokio::task::JoinSet::new();
    let mut pending = std::collections::BTreeSet::new();
    let mut usable = BTreeMap::new();
    let deadline = tokio::time::Instant::now() + DEADLINE;
    let hedge = tokio::time::Instant::now() + OWNER_GRACE;
    let mut expanded = false;
    let mut grace = deadline;
    loop {
        if let Some((&first, _)) = usable.first_key_value()
            && pending.first().is_none_or(|index| *index > first)
        {
            break;
        }
        // Give the preferred provider a head start. Fast, usable responses need
        // no fallback traffic; stalled providers still allow bounded parallel work.
        while tasks.len() < if expanded { PROVIDERS } else { 1 } {
            let Some((index, addon)) = remaining.next() else {
                break;
            };
            pending.insert(index);
            let (client, addon, kind, id) = (
                client.clone(),
                addon.clone(),
                kind.to_owned(),
                id.to_owned(),
            );
            tasks.spawn(async move {
                let result = client.meta(&addon, &kind, &id).await;
                (index, addon, result)
            });
        }
        let result = tokio::select! {
            result = tasks.join_next() => result,
            () = tokio::time::sleep_until(deadline.min(grace)) => break,
            () = tokio::time::sleep_until(hedge), if !expanded => {
                expanded = true;
                continue;
            },
        };
        let Some(result) = result else {
            break;
        };
        if let Ok((index, addon, result)) = result {
            pending.remove(&index);
            if let Ok(meta) = result
                && let Some(value) = project(&addon, meta)
            {
                usable.insert(index, value);
                grace = grace.min(tokio::time::Instant::now() + OWNER_GRACE);
            }
        }
    }
    // JoinSet cancellation drops only these consumers. The shared HTTP loader
    // still serves another calendar/detail request for the same resource.
    usable.pop_first().map(|(_, value)| value)
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "Local HTTP fixtures must start and finish successfully"
)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn usable_owner_does_not_request_fallback_providers() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let requests = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = requests.clone();
        let server = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let mut request = [0; 4096];
                let received = socket.read(&mut request).await.unwrap();
                assert!(String::from_utf8_lossy(&request[..received]).contains("/owner/"));
                let body = r#"{"meta":{"id":"film","type":"movie","name":"Owner"}}"#;
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            }
        });
        let addons = ["owner", "fallback"].map(|id| serde_json::from_value(serde_json::json!({
            "manifest_url":format!("http://{address}/{id}/manifest.json"),"manifest":{"id":id,"name":id,"resources":["meta"]}
        })).unwrap());
        let client = Client::new().unwrap();
        assert_eq!(
            fetch(&client, &addons, "owner", "movie", "film", |meta| Some(
                meta.name.clone()
            ))
            .await
            .as_deref(),
            Some("Owner")
        );
        assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 1);
        server.abort();
    }

    #[tokio::test]
    async fn stalled_owner_does_not_block_a_usable_fallback() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let mut tasks = tokio::task::JoinSet::new();
            for _ in 0..2 {
                let (mut socket, _) = listener.accept().await.unwrap();
                tasks.spawn(async move {
                    let mut request = [0; 4096];
                    let received = socket.read(&mut request).await.unwrap();
                    if String::from_utf8_lossy(&request[..received]).contains("/owner/") {
                        let mut eof = [0; 1];
                        let _ = socket.read(&mut eof).await;
                    } else {
                        let body = r#"{"meta":{"id":"film","type":"movie","name":"Fallback"}}"#;
                        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
                    }
                });
            }
            while tasks.join_next().await.is_some() {}
        });
        let addons = ["owner", "fallback"].map(|id| serde_json::from_value(serde_json::json!({
            "manifest_url":format!("http://{address}/{id}/manifest.json"),"manifest":{"id":id,"name":id,"resources":["meta"]}
        })).unwrap());
        let client = Client::new().unwrap();
        let name = tokio::time::timeout(
            Duration::from_secs(2),
            fetch(&client, &addons, "owner", "movie", "film", |meta| {
                Some(meta.name.clone())
            }),
        )
        .await
        .unwrap();
        assert_eq!(name.as_deref(), Some("Fallback"));
        tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn prefers_the_owner_and_falls_back_until_metadata_is_usable() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0; 4096];
                let received = socket.read(&mut request).await.unwrap();
                let request = String::from_utf8_lossy(&request[..received]);
                let owner = request.contains("/owner/");
                let wrong = request.contains("/wrong/");
                let fallback = request.contains("/fallback/");
                let body = serde_json::json!({"meta":{
                    "id": if wrong { "another-film" } else { "film" },
                    "type":"movie", "name": if owner { "Owner" } else { "Fallback" },
                    "poster": if owner { None } else if fallback { Some("https://example.com/poster.jpg") } else { Some("https://example.com/unwanted.jpg") }
                }}).to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let addons: Vec<Addon> = ["disabled", "wrong", "fallback", "owner"]
            .into_iter()
            .map(|id| {
                serde_json::from_value(serde_json::json!({
                    "manifest_url":format!("http://{address}/{id}/manifest.json"),
                    "disabled":id == "disabled",
                    "manifest":{"id":id,"name":id,"resources":["meta"]}
                }))
                .unwrap()
            })
            .collect();
        let client = Client::new().unwrap();
        let name = fetch(&client, &addons, "owner", "movie", "film", |meta| {
            Some(meta.name.clone())
        })
        .await;
        assert_eq!(name.as_deref(), Some("Owner"));
        let poster = fetch(&client, &addons, "owner", "movie", "film", |meta| {
            meta.poster.clone()
        })
        .await;
        assert_eq!(poster.as_deref(), Some("https://example.com/poster.jpg"));
        let missing = fetch(&client, &addons, "owner", "movie", "film", |_| None::<()>).await;
        assert!(missing.is_none());
        server.abort();
    }
}
