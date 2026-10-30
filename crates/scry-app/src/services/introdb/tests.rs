use super::*;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Server {
    endpoint: String,
    hits: Arc<AtomicUsize>,
    requests: Arc<Mutex<Vec<String>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn server(responses: Vec<String>) -> Result<Server> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = format!("http://{}/segments", listener.local_addr()?);
    let hits = Arc::new(AtomicUsize::new(0));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let (count, recorded) = (hits.clone(), requests.clone());
    let task = tokio::spawn(async move {
        for response in responses {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let mut bytes = Vec::new();
            loop {
                let mut buffer = [0; 1024];
                let Ok(n) = stream.read(&mut buffer).await else {
                    return;
                };
                if n == 0 {
                    return;
                }
                bytes.extend_from_slice(&buffer[..n]);
                if bytes.windows(4).any(|s| s == b"\r\n\r\n") {
                    break;
                }
                if bytes.len() > 8192 {
                    return;
                }
            }
            count.fetch_add(1, Ordering::SeqCst);
            if let Ok(mut requests) = recorded.lock() {
                requests.push(String::from_utf8_lossy(&bytes).into_owned());
            }
            if stream.write_all(response.as_bytes()).await.is_err() {
                return;
            }
        }
    });
    Ok(Server {
        endpoint,
        hits,
        requests,
        task,
    })
}

fn response(status: &str, headers: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nConnection: close\r\nContent-Length: {}\r\n{headers}\r\n{body}",
        body.len()
    )
}

fn episode() -> EpisodeKey {
    EpisodeKey {
        imdb_id: "tt2560140".into(),
        season: 1,
        episode: 2,
    }
}

fn payload(intro: &serde_json::Value, no_intro: bool) -> String {
    serde_json::json!({"imdb_id":"tt2560140", "season":1, "episode":2, "intro":intro, "recap":null, "outro":null, "post_credits":null, "no_intro":no_intro, "no_recap":false, "no_outro":false}).to_string()
}

fn intro() -> serde_json::Value {
    serde_json::json!({"start_ms":54312, "end_ms":144_312, "confidence":1.0, "submission_count":1, "updated_at":"2026-04-08T19:25:22.032Z"})
}

fn client() -> Result<Client> {
    Ok(Client::builder().timeout(Duration::from_secs(2)).build()?)
}

#[test]
fn only_unambiguous_imdb_episode_identities_are_queried() {
    assert_eq!(
        EpisodeKey::from_video("series", "tt2560140", "tt2560140:1:2"),
        Some(episode())
    );
    for (kind, series, video) in [
        ("series", "kitsu:7442", "kitsu:7442:2"),
        ("movie", "tt2560140", "tt2560140:1:2"),
        ("series", "tt2560140", "tt1234567:1:2"),
        ("series", "tt2560140", "tt2560140:2"),
        ("series", "tt2560140", "tt2560140:1:2:3"),
        ("series", "tt2560140", "tt2560140:1:0"),
    ] {
        assert!(
            EpisodeKey::from_video(kind, series, video).is_none(),
            "{video}"
        );
    }
}

#[test]
fn malformed_mismatched_and_contradictory_responses_are_rejected() {
    let mut invalid = intro();
    invalid["end_ms"] = 1.into();
    assert!(Segments::parse(payload(&invalid, false).as_bytes(), &episode()).is_err());
    assert!(Segments::parse(payload(&intro(), true).as_bytes(), &episode()).is_err());
    let mut wrong_key = episode();
    wrong_key.episode = 3;
    assert!(Segments::parse(payload(&intro(), false).as_bytes(), &wrong_key).is_err());
    assert!(Segments::parse(b"not json", &episode()).is_err());
}

#[test]
fn retry_after_supports_seconds_and_http_dates_with_safe_bounds() {
    assert_eq!(retry_delay(Some("120"), 0), Duration::from_mins(2));
    assert_eq!(
        retry_delay(Some("Thu, 01 Jan 1970 00:02:00 GMT"), 0),
        Duration::from_mins(2)
    );
    assert_eq!(retry_delay(Some("invalid"), 0), Duration::from_mins(1));
    assert_eq!(retry_delay(Some("0"), 0), ERROR_BACKOFF);
}

#[tokio::test]
async fn anime_intro_is_saved_and_reused_across_database_reopens_without_network() -> Result<()> {
    let fixture = server(vec![response(
        "200 OK",
        "ETag: \"anime-v1\"\r\n",
        &payload(&intro(), false),
    )])
    .await?;
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("cache.sqlite3");
    let client = client()?;
    let first = resolve(&client, &fixture.endpoint, &path, &episode(), 100, true).await?;
    assert!(matches!(
        first,
        Resolution::Ready(Some(Segment {
            start_ms: 54312,
            end_ms: 144_312,
            ..
        }))
    ));
    let later = resolve(
        &client,
        &fixture.endpoint,
        &path,
        &episode(),
        100 + UNKNOWN_LIFETIME * 10,
        false,
    )
    .await?;
    assert!(matches!(
        later,
        Resolution::Ready(Some(Segment {
            end_ms: 144_312,
            ..
        }))
    ));
    assert_eq!(fixture.hits.load(Ordering::SeqCst), 1);
    assert!(matches!(
        resolve(
            &client,
            &fixture.endpoint,
            &path,
            &episode(),
            100 + UNKNOWN_LIFETIME * 20,
            true
        )
        .await?,
        Resolution::Ready(Some(_))
    ));
    assert_eq!(fixture.hits.load(Ordering::SeqCst), 1);
    let saved = load(&path, &episode())
        .await?
        .context("missing cached anime")?;
    assert_eq!(saved.etag.as_deref(), Some("\"anime-v1\""));
    let requests = fixture
        .requests
        .lock()
        .map_err(|_| anyhow::anyhow!("fixture lock poisoned"))?;
    assert!(
        requests
            .first()
            .is_some_and(|s| s.contains("imdb_id=tt2560140&season=1&episode=2"))
    );
    Ok(())
}

#[tokio::test]
async fn unknown_results_expire_and_use_conditional_revalidation() -> Result<()> {
    let fixture = server(vec![
        response(
            "200 OK",
            "ETag: \"unknown-v1\"\r\n",
            &payload(&serde_json::Value::Null, false),
        ),
        response("304 Not Modified", "", ""),
        response("200 OK", "", &payload(&intro(), false)),
    ])
    .await?;
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("cache.sqlite3");
    let client = client()?;
    for now in [100, 101, 100 + UNKNOWN_LIFETIME, 101 + UNKNOWN_LIFETIME] {
        assert!(matches!(
            resolve(&client, &fixture.endpoint, &path, &episode(), now, true).await?,
            Resolution::Ready(None)
        ));
    }
    assert_eq!(fixture.hits.load(Ordering::SeqCst), 2);
    assert!(
        fixture
            .requests
            .lock()
            .map_err(|_| anyhow::anyhow!("fixture lock poisoned"))?
            .get(1)
            .is_some_and(|s| s
                .to_ascii_lowercase()
                .contains("if-none-match: \"unknown-v1\""))
    );
    assert!(matches!(
        resolve(
            &client,
            &fixture.endpoint,
            &path,
            &episode(),
            100 + UNKNOWN_LIFETIME * 2,
            true
        )
        .await?,
        Resolution::Ready(Some(_))
    ));
    assert_eq!(fixture.hits.load(Ordering::SeqCst), 3);
    Ok(())
}

#[tokio::test]
async fn confirmed_absence_is_durable_but_errors_are_not_cached_as_absence() -> Result<()> {
    let fixture = server(vec![response(
        "200 OK",
        "",
        &payload(&serde_json::Value::Null, true),
    )])
    .await?;
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("known.sqlite3");
    let client = client()?;
    for now in [100, 100 + UNKNOWN_LIFETIME * 10] {
        assert!(matches!(
            resolve(&client, &fixture.endpoint, &path, &episode(), now, true).await?,
            Resolution::Ready(None)
        ));
    }
    assert_eq!(fixture.hits.load(Ordering::SeqCst), 1);
    let failing = server(vec![
        response("503 Service Unavailable", "", "{}"),
        response("429 Too Many Requests", "Retry-After: 120\r\n", "{}"),
    ])
    .await?;
    let path = directory.path().join("errors.sqlite3");
    assert!(
        resolve(&client, &failing.endpoint, &path, &episode(), 100, true)
            .await
            .is_err()
    );
    assert!(load(&path, &episode()).await?.is_none());
    assert!(
        matches!(resolve(&client, &failing.endpoint, &path, &episode(), 101, true).await?, Resolution::Backoff(delay) if delay == Duration::from_mins(2))
    );
    assert!(matches!(
        resolve(&client, &failing.endpoint, &path, &episode(), 102, false).await?,
        Resolution::Ready(None)
    ));
    assert_eq!(failing.hits.load(Ordering::SeqCst), 2);
    assert!(load(&path, &episode()).await?.is_none());
    Ok(())
}

#[tokio::test]
async fn worker_applies_provider_backoff_across_episode_changes() -> Result<()> {
    let fixture = server(vec![response(
        "429 Too Many Requests",
        "Retry-After: 120\r\n",
        "{}",
    )])
    .await?;
    let directory = tempfile::tempdir()?;
    let (tx, mut events) = tokio::sync::mpsc::channel(8);
    let inbox = Inbox {
        tx,
        wake: Arc::new(|| {}),
    };
    let lookup = Lookup::spawn(
        &tokio::runtime::Handle::current(),
        client()?,
        fixture.endpoint.clone(),
        directory.path().join("cache.sqlite3"),
        inbox,
    );
    lookup.request(1, Some(episode()));
    let first = tokio::time::timeout(Duration::from_secs(3), events.recv()).await?;
    assert!(matches!(first, Some(Event::IntroSegments(1, None))));
    let mut next = episode();
    next.episode = 3;
    lookup.request(2, Some(next));
    let second = tokio::time::timeout(Duration::from_secs(3), events.recv()).await?;
    assert!(matches!(second, Some(Event::IntroSegments(2, None))));
    assert_eq!(
        fixture.hits.load(Ordering::SeqCst),
        1,
        "Backoff must cover the provider, not just one episode"
    );
    Ok(())
}

#[tokio::test]
async fn missing_and_invalid_responses_are_not_confused_with_confirmed_absence() -> Result<()> {
    let fixture = server(vec![
        response("404 Not Found", "", "{}"),
        response("200 OK", "", "{\"intro\":\"bad\"}"),
    ])
    .await?;
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("cache.sqlite3");
    let client = client()?;
    assert!(matches!(
        resolve(&client, &fixture.endpoint, &path, &episode(), 100, true).await?,
        Resolution::Ready(None)
    ));
    let saved = load(&path, &episode())
        .await?
        .context("missing negative cache")?;
    assert!(!Segments::parse(saved.payload.as_bytes(), &episode())?.no_intro);
    assert!(
        resolve(
            &client,
            &fixture.endpoint,
            &path,
            &episode(),
            100 + UNKNOWN_LIFETIME,
            true
        )
        .await
        .is_err()
    );
    assert_eq!(
        load(&path, &episode()).await?,
        Some(saved),
        "Invalid data must not replace the previous cache entry"
    );
    Ok(())
}
