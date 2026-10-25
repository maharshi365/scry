//! Provider preference and fallback for background library metadata requests.
use scry_addons::{Addon, Client, Meta};

pub(super) async fn fetch<T>(
    client: &Client,
    addons: &[Addon],
    namespace: &str,
    kind: &str,
    id: &str,
    project: impl Fn(Meta) -> Option<T>,
) -> Option<T> {
    // Borrow providers in preference order instead of cloning/sorting every manifest.
    for addon in addons
        .iter()
        .filter(|a| a.manifest.id == namespace)
        .chain(addons.iter().filter(|a| a.manifest.id != namespace))
        .filter(|a| a.supports("meta", kind, id))
    {
        if let Ok(meta) = client.meta(addon, kind, id).await
            && let Some(value) = project(meta)
        {
            return Some(value);
        }
    }
    None
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
