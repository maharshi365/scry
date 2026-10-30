//! On-demand `IntroDB` lookup with persistent caching and a single bounded worker.
use super::{Event, Inbox};
use anyhow::{Context, Result, ensure};
use reqwest::{Client, StatusCode};
use scry_library::{Library, SkipSegmentsRecord};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::watch;

const ENDPOINT: &str = "https://api.introdb.app/segments";
const PROVIDER: &str = "introdb";
const MAX_RESPONSE: usize = 65536;
// Unknown data may be contributed later. Known intros and explicit absence are durable.
const UNKNOWN_LIFETIME: i64 = 7 * 24 * 60 * 60;
const ERROR_BACKOFF: Duration = Duration::from_secs(30);
const MAX_ERROR_BACKOFF: Duration = Duration::from_mins(30);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EpisodeKey {
    imdb_id: String,
    season: u32,
    episode: u32,
}

impl EpisodeKey {
    pub(crate) fn from_video(kind: &str, series: &str, video: &str) -> Option<Self> {
        if kind != "series" || !valid_imdb(series) {
            return None;
        }
        let mut parts = video.split(':');
        if parts.next()? != series {
            return None;
        }
        let season = parts.next()?.parse().ok()?;
        let episode = parts.next()?.parse().ok()?;
        if parts.next().is_some() || episode == 0 {
            return None;
        }
        Some(Self {
            imdb_id: series.into(),
            season,
            episode,
        })
    }
}

fn valid_imdb(id: &str) -> bool {
    id.strip_prefix("tt").is_some_and(|digits| {
        (7..=8).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit())
    })
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Segment {
    pub start_ms: u32,
    pub end_ms: u32,
    pub confidence: f64,
    pub submission_count: u32,
    pub updated_at: Option<String>,
}

impl Segment {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.end_ms > self.start_ms && self.end_ms <= 7 * 24 * 60 * 60 * 1000,
            "invalid segment interval"
        );
        ensure!(
            self.confidence.is_finite()
                && (0.0..=1.0).contains(&self.confidence)
                && self.submission_count > 0,
            "invalid segment confidence"
        );
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct Segments {
    imdb_id: String,
    season: u32,
    episode: u32,
    intro: Option<Segment>,
    recap: Option<Segment>,
    outro: Option<Segment>,
    post_credits: Option<Segment>,
    #[serde(default)]
    no_intro: bool,
    #[serde(default)]
    no_recap: bool,
    #[serde(default)]
    no_outro: bool,
}

impl Segments {
    fn empty(key: &EpisodeKey) -> Self {
        Self {
            imdb_id: key.imdb_id.clone(),
            season: key.season,
            episode: key.episode,
            intro: None,
            recap: None,
            outro: None,
            post_credits: None,
            no_intro: false,
            no_recap: false,
            no_outro: false,
        }
    }

    fn parse(bytes: &[u8], key: &EpisodeKey) -> Result<Self> {
        let data: Self = serde_json::from_slice(bytes)?;
        ensure!(
            data.imdb_id == key.imdb_id && data.season == key.season && data.episode == key.episode,
            "segment identity mismatch"
        );
        for segment in [&data.intro, &data.recap, &data.outro, &data.post_credits]
            .into_iter()
            .flatten()
        {
            segment.validate()?;
        }
        ensure!(
            !(data.no_intro && data.intro.is_some()
                || data.no_recap && data.recap.is_some()
                || data.no_outro && data.outro.is_some()),
            "contradictory segment response"
        );
        Ok(data)
    }

    fn durable(&self) -> bool {
        self.intro.is_some() || self.no_intro
    }
}

pub(super) struct Lookup {
    requests: watch::Sender<Option<(u64, EpisodeKey)>>,
    task: tokio::task::JoinHandle<()>,
}

impl Lookup {
    pub(super) fn start(
        runtime: &tokio::runtime::Runtime,
        data: &Path,
        inbox: Inbox,
    ) -> Result<Self> {
        let client = Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(8))
            .build()?;
        Ok(Self::spawn(
            runtime.handle(),
            client,
            ENDPOINT.into(),
            data.join(scry_library::DATABASE_NAME),
            inbox,
        ))
    }

    fn spawn(
        runtime: &tokio::runtime::Handle,
        client: Client,
        endpoint: String,
        path: PathBuf,
        inbox: Inbox,
    ) -> Self {
        let (requests, mut pending) = watch::channel::<Option<(u64, EpisodeKey)>>(None);
        let task = runtime.spawn(async move {
            let mut retry_at = tokio::time::Instant::now();
            let mut backoff = ERROR_BACKOFF;
            while pending.changed().await.is_ok() {
                let request = pending.borrow_and_update().clone();
                let Some((generation, episode)) = request else {
                    continue;
                };
                // Cached data remains usable during provider-wide network backoff.
                let result = resolve(
                    &client,
                    &endpoint,
                    &path,
                    &episode,
                    unix_now(),
                    tokio::time::Instant::now() >= retry_at,
                )
                .await;
                let intro = match result {
                    Ok(Resolution::Ready(intro)) => {
                        backoff = ERROR_BACKOFF;
                        intro
                    }
                    Ok(Resolution::Backoff(delay)) => {
                        retry_at = tokio::time::Instant::now() + delay;
                        None
                    }
                    Err(_) => {
                        retry_at = tokio::time::Instant::now() + backoff;
                        backoff = backoff.saturating_mul(2).min(MAX_ERROR_BACKOFF);
                        None
                    }
                };
                inbox.send(Event::IntroSegments(generation, intro)).await;
            }
        });
        Self { requests, task }
    }

    pub(super) fn request(&self, generation: u64, episode: Option<EpisodeKey>) {
        self.requests
            .send_replace(episode.map(|episode| (generation, episode)));
    }
}

impl super::Services {
    pub(crate) fn intro_segments(&self, generation: u64, identity: Option<(&str, &str, &str)>) {
        if let Some(lookup) = &self.intro_lookup {
            let episode = identity
                .and_then(|(kind, series, video)| EpisodeKey::from_video(kind, series, video));
            lookup.request(generation, episode);
        }
    }
}

impl Drop for Lookup {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0)
}

fn retry_delay(value: Option<&str>, now: i64) -> Duration {
    let seconds = value
        .and_then(|value| {
            value.parse::<u64>().ok().or_else(|| {
                let date = chrono::DateTime::parse_from_rfc2822(value).ok()?;
                u64::try_from(date.timestamp().saturating_sub(now)).ok()
            })
        })
        .unwrap_or(60);
    // Bound hostile headers before adding them to a monotonic deadline.
    Duration::from_secs(seconds.clamp(30, 7 * 24 * 60 * 60))
}

enum Resolution {
    Ready(Option<Segment>),
    Backoff(Duration),
}

async fn load(path: &Path, key: &EpisodeKey) -> Result<Option<SkipSegmentsRecord>> {
    let (path, key) = (path.to_owned(), key.clone());
    tokio::task::spawn_blocking(move || {
        Library::open(path)?.skip_segments(PROVIDER, &key.imdb_id, key.season, key.episode)
    })
    .await?
}

async fn save(path: &Path, key: &EpisodeKey, record: SkipSegmentsRecord) -> Result<()> {
    let (path, key) = (path.to_owned(), key.clone());
    tokio::task::spawn_blocking(move || {
        Library::open(path)?.save_skip_segments(
            PROVIDER,
            &key.imdb_id,
            key.season,
            key.episode,
            &record,
        )
    })
    .await?
}

async fn resolve(
    client: &Client,
    endpoint: &str,
    path: &Path,
    key: &EpisodeKey,
    now: i64,
    network_allowed: bool,
) -> Result<Resolution> {
    let cached = load(path, key).await?;
    let saved = cached
        .as_ref()
        .map(|record| Segments::parse(record.payload.as_bytes(), key))
        .transpose()?;
    if let (Some(record), Some(data)) = (&cached, &saved)
        && (data.durable()
            || (now >= record.fetched_at
                && now.saturating_sub(record.fetched_at) < UNKNOWN_LIFETIME))
    {
        return Ok(Resolution::Ready(data.intro.clone()));
    }
    if !network_allowed {
        return Ok(Resolution::Ready(None));
    }
    let mut request = client.get(endpoint).query(&[
        ("imdb_id", key.imdb_id.clone()),
        ("season", key.season.to_string()),
        ("episode", key.episode.to_string()),
    ]);
    if let Some(etag) = cached.as_ref().and_then(|record| record.etag.as_ref()) {
        request = request.header(reqwest::header::IF_NONE_MATCH, etag);
    }
    let mut response = request.send().await?;
    if response.status() == StatusCode::TOO_MANY_REQUESTS {
        let delay = retry_delay(
            response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|h| h.to_str().ok()),
            now,
        );
        return Ok(Resolution::Backoff(delay));
    }
    if response.status() == StatusCode::NOT_MODIFIED {
        let mut record = cached.context("unexpected segment cache validation")?;
        record.fetched_at = now;
        save(path, key, record).await?;
        return Ok(Resolution::Ready(saved.and_then(|data| data.intro)));
    }
    let etag = response
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|h| h.to_str().ok())
        .filter(|s| s.len() <= 512)
        .map(str::to_owned);
    let data = if response.status() == StatusCode::NOT_FOUND {
        Segments::empty(key)
    } else {
        ensure!(
            response.status() == StatusCode::OK,
            "segment provider unavailable"
        );
        ensure!(
            response
                .content_length()
                .is_none_or(|n| n <= MAX_RESPONSE as u64),
            "segment response too large"
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            ensure!(
                bytes.len().saturating_add(chunk.len()) <= MAX_RESPONSE,
                "segment response too large"
            );
            bytes.extend_from_slice(&chunk);
        }
        Segments::parse(&bytes, key)?
    };
    let record = SkipSegmentsRecord {
        payload: serde_json::to_string(&data)?,
        etag,
        fetched_at: now,
    };
    save(path, key, record).await?;
    Ok(Resolution::Ready(data.intro))
}

#[cfg(test)]
mod tests;
