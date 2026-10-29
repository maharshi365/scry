//! Private, bounded HTTP caching and shared in-flight requests. No URLs in errors.
mod storage;
#[cfg(test)]
mod tests;

use anyhow::{Result, anyhow, bail};
use http_cache_semantics::{AfterResponse, BeforeRequest, CacheOptions, CachePolicy};
use std::{
    any::TypeId,
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::SystemTime,
};
use tokio::sync::{Semaphore, watch};

/// Shared across foreground and background add-on work, not a browser worker count.
pub const CONCURRENCY: usize = 8;
const MAX_PENDING: usize = 64;
const MAX_ENTRIES: usize = 256;
const MEMORY_BUDGET: usize = 16 * 1024 * 1024;
const MAX_BODY: usize = 4 * 1024 * 1024;

#[derive(Clone)]
pub struct Response {
    pub bytes: Arc<Vec<u8>>,
    /// False for no-store responses; callers must not persist derived thumbnails.
    pub storable: bool,
}

#[derive(Clone)]
struct Entry {
    policy: CachePolicy,
    bytes: Arc<Vec<u8>>,
    received: SystemTime,
    headers: http::HeaderMap,
}

type Outcome = std::result::Result<Response, String>;

#[derive(Clone, Hash, PartialEq, Eq)]
struct Flight {
    key: String,
    revision: u64,
    validation: Option<TypeId>,
}

struct Active {
    result: watch::Sender<Option<Outcome>>,
    foreground: watch::Sender<bool>,
}

#[derive(Default)]
struct State {
    entries: VecDeque<(String, Entry)>,
    active: HashMap<Flight, Active>,
    invalidated: Option<SystemTime>,
    revision: u64,
}

struct Shared {
    client: reqwest::Client,
    state: Mutex<State>,
    permits: Semaphore,
    background: Semaphore,
    disk: Option<storage::Disk>,
}

#[derive(Clone)]
pub struct HttpCache(Arc<Shared>, bool);

impl HttpCache {
    #[must_use]
    pub fn new(client: reqwest::Client, directory: Option<PathBuf>) -> Self {
        Self(
            Arc::new(Shared {
                client,
                state: Mutex::default(),
                permits: Semaphore::new(CONCURRENCY),
                background: Semaphore::new(CONCURRENCY / 2),
                disk: directory.map(storage::Disk::new),
            }),
            false,
        )
    }

    #[must_use]
    pub fn background(&self) -> Self {
        Self(self.0.clone(), true)
    }

    /// Force the next use of each entry to validate, without deleting good data.
    pub fn invalidate(&self) {
        if let Ok(mut state) = self.0.state.lock() {
            state.invalidated = Some(SystemTime::now());
            state.revision = state.revision.wrapping_add(1);
        }
    }

    /// Remove an image source that passed HTTP checks but could not be decoded.
    pub async fn reject_image(&self, url: &str, limit: usize) {
        let key = format!("{}:false:{url}", limit.min(MAX_BODY));
        if let Ok(mut state) = self.0.state.lock() {
            state.entries.retain(|(k, _)| k != &key);
        }
        if let Some(disk) = self.0.disk.clone() {
            let _ = tokio::task::spawn_blocking(move || disk.remove(&key)).await;
        }
    }

    /// Requests share an owned task. Dropping one waiter cannot cancel other waiters.
    /// Work and queued requests have separate bounds. Streams can disable stale reuse.
    /// # Errors
    /// Returns a private, URL-free error on failed, oversized, or excess requests.
    pub async fn get(&self, url: url::Url, limit: usize, allow_stale: bool) -> Result<Response> {
        self.get_inner(url, limit, allow_stale, None, |_| Ok(()))
            .await
    }

    /// # Errors
    /// Returns an error without storing malformed resource responses.
    pub async fn get_json<T: serde::de::DeserializeOwned + 'static>(
        &self,
        url: url::Url,
        limit: usize,
        allow_stale: bool,
    ) -> Result<Response> {
        self.get_json_checked::<T, _>(url, limit, allow_stale, |_| Ok(()))
            .await
    }

    pub(crate) async fn get_json_checked<T, F>(
        &self,
        url: url::Url,
        limit: usize,
        allow_stale: bool,
        check: F,
    ) -> Result<Response>
    where
        T: serde::de::DeserializeOwned + 'static,
        F: Fn(&T) -> Result<()> + Send + Sync + 'static,
    {
        self.get_inner(
            url,
            limit,
            allow_stale,
            Some(TypeId::of::<(T, F)>()),
            move |bytes| {
                let value = serde_json::from_slice::<T>(bytes)
                    .map_err(|_| anyhow!("Invalid add-on response"))?;
                check(&value)
            },
        )
        .await
    }

    /// Image requests can stop when their last visible consumer goes away.
    /// # Errors
    /// Returns a URL-free error on cancellation or HTTP failure.
    pub async fn get_cancellable(&self, url: url::Url, limit: usize) -> Result<Response> {
        self.get_inner(url, limit, false, None, |_| Ok(())).await
    }

    async fn get_inner(
        &self,
        url: url::Url,
        limit: usize,
        allow_stale: bool,
        validation: Option<TypeId>,
        validate: impl Fn(&[u8]) -> Result<()> + Send + Sync + 'static,
    ) -> Result<Response> {
        let limit = limit.min(MAX_BODY);
        if url.as_str().len() > 16 * 1024 {
            bail!("HTTP request URL exceeds size limit");
        }
        let key = format!("{limit}:{allow_stale}:{url}");
        let mut receiver = {
            let mut state = self
                .0
                .state
                .lock()
                .map_err(|_| anyhow!("HTTP cache unavailable"))?;
            let flight = Flight {
                key,
                revision: state.revision,
                validation,
            };
            if let Some(active) = state.active.get(&flight) {
                if !self.1 {
                    let _ = active.foreground.send(true);
                }
                active.result.subscribe()
            } else {
                if state.active.len() >= MAX_PENDING {
                    bail!("HTTP request queue is full; try again");
                }
                let (sender, receiver) = watch::channel(None);
                let (foreground, priority) = watch::channel(!self.1);
                state.active.insert(
                    flight.clone(),
                    Active {
                        result: sender.clone(),
                        foreground,
                    },
                );
                let cache = self.clone();
                tokio::spawn(async move {
                    let fetching =
                        cache.fetch(&flight, url, limit, allow_stale, validate, priority);
                    tokio::pin!(fetching);
                    let result = tokio::select! {
                        result = &mut fetching => result,
                        () = sender.closed() => {
                            let revalidating = sender.borrow().is_some();
                            if revalidating { fetching.await } else { Err(anyhow!("HTTP request cancelled")) }
                        },
                    }.map_err(|e| e.to_string());
                    let _ = sender.send(Some(result));
                    if let Ok(mut state) = cache.0.state.lock() {
                        state.active.remove(&flight);
                    }
                });
                receiver
            }
        };
        loop {
            if let Some(result) = receiver.borrow_and_update().clone() {
                return result.map_err(anyhow::Error::msg);
            }
            receiver
                .changed()
                .await
                .map_err(|_| anyhow!("HTTP request interrupted"))?;
        }
    }

    async fn cached(&self, key: &str) -> Option<Entry> {
        {
            let mut state = self.0.state.lock().ok()?;
            if let Some(index) = state.entries.iter().position(|(k, _)| k == key) {
                let entry = state.entries.remove(index)?;
                let result = entry.1.clone();
                state.entries.push_back(entry);
                return Some(result);
            }
        }
        let disk = self.0.disk.clone()?;
        let disk_key = key.to_owned();
        let entry = tokio::task::spawn_blocking(move || disk.load(&disk_key))
            .await
            .ok()??;
        self.remember(key, &entry);
        Some(entry)
    }

    fn remember(&self, key: &str, entry: &Entry) -> bool {
        if let Ok(mut state) = self.0.state.lock() {
            if state
                .entries
                .iter()
                .any(|(k, e)| k == key && e.received > entry.received)
            {
                return false;
            }
            state.entries.retain(|(k, _)| k != key);
            if entry.policy.is_storable() {
                state.entries.push_back((key.to_owned(), entry.clone()));
                while state.entries.len() > MAX_ENTRIES
                    || state
                        .entries
                        .iter()
                        .map(|(_, e)| e.bytes.len())
                        .sum::<usize>()
                        > MEMORY_BUDGET
                {
                    state.entries.pop_front();
                }
            }
        }
        true
    }

    async fn store(&self, key: &str, entry: Entry) {
        if !self.remember(key, &entry) {
            return;
        }
        if let Some(disk) = self.0.disk.clone() {
            let key = key.to_owned();
            let _ = tokio::task::spawn_blocking(move || disk.store(&key, &entry)).await;
        }
    }

    async fn fetch(
        &self,
        flight: &Flight,
        url: url::Url,
        limit: usize,
        allow_stale: bool,
        validate: impl Fn(&[u8]) -> Result<()> + Send + Sync,
        mut priority: watch::Receiver<bool>,
    ) -> Result<Response> {
        let cached = self
            .cached(&flight.key)
            .await
            .filter(|entry| entry.bytes.len() <= limit && validate(&entry.bytes).is_ok());
        let request = http::Request::get(url.as_str())
            .body(())
            .map_err(|_| anyhow!("Invalid HTTP request"))?;
        let invalidated = self
            .0
            .state
            .lock()
            .map_err(|_| anyhow!("HTTP cache unavailable"))?
            .invalidated;
        let forced = cached
            .as_ref()
            .is_some_and(|entry| invalidated.is_some_and(|time| entry.received <= time));
        let mut headers = http::HeaderMap::new();
        let mut matches = false;
        if let Some(entry) = &cached {
            match entry.policy.before_request(&request, SystemTime::now()) {
                BeforeRequest::Fresh(_) if !forced => return Ok(entry.response()),
                BeforeRequest::Fresh(_) => {
                    let mut refresh = request.clone();
                    refresh
                        .headers_mut()
                        .insert("cache-control", http::HeaderValue::from_static("no-cache"));
                    if let BeforeRequest::Stale {
                        request,
                        matches: same,
                    } = entry.policy.before_request(&refresh, SystemTime::now())
                    {
                        headers = request.headers;
                        matches = same;
                    }
                }
                BeforeRequest::Stale {
                    request,
                    matches: same,
                } => {
                    headers = request.headers;
                    matches = same;
                }
            }
        }
        if forced {
            headers.insert("cache-control", http::HeaderValue::from_static("no-cache"));
        }
        let swr = !forced
            && allow_stale
            && matches
            && cached
                .as_ref()
                .is_some_and(|entry| entry.allows_stale(&request, "stale-while-revalidate"));
        if swr {
            // Publish the old value now. The same owned task keeps the flight occupied
            // until revalidation ends, so subsequent readers cannot start a refresh storm.
            if let Some(entry) = &cached {
                self.publish(flight, entry.response());
            }
        }
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(25),
            self.network(
                url,
                headers,
                limit,
                &request,
                cached.as_ref().filter(|_| matches),
                &mut priority,
            ),
        )
        .await
        .unwrap_or_else(|_| Err(Failure::request()));
        match result {
            Ok(entry) => {
                validate(&entry.bytes)?;
                let response = entry.response();
                self.store(&flight.key, entry).await;
                Ok(response)
            }
            Err(error) => {
                if !forced
                    && allow_stale
                    && matches
                    && let Some(entry) =
                        cached.filter(|entry| entry.allows_stale(&request, "stale-if-error"))
                    && error.stale_allowed
                {
                    return Ok(entry.response());
                }
                Err(anyhow!(error.message))
            }
        }
    }

    fn publish(&self, flight: &Flight, response: Response) {
        if let Ok(state) = self.0.state.lock()
            && let Some(active) = state.active.get(flight)
        {
            let _ = active.result.send(Some(Ok(response)));
        }
    }

    async fn network(
        &self,
        url: url::Url,
        headers: http::HeaderMap,
        limit: usize,
        request: &http::Request<()>,
        old: Option<&Entry>,
        priority: &mut watch::Receiver<bool>,
    ) -> std::result::Result<Entry, Failure> {
        let started = SystemTime::now();
        // Reserve half the connections for interactive work. Background requests
        // wait here before they can occupy the global semaphore's FIFO queue.
        let _background = if self.1 && !*priority.borrow() {
            tokio::select! {
                permit = self.0.background.acquire() => Some(permit.map_err(|_| Failure::request())?),
                promoted = priority.wait_for(|foreground| *foreground) => {
                    drop(promoted.map_err(|_| Failure::request())?);
                    None
                }
            }
        } else {
            None
        };
        let _permit = self
            .0
            .permits
            .acquire()
            .await
            .map_err(|_| Failure::request())?;
        let mut response = self
            .0
            .client
            .get(url)
            .headers(headers)
            .send()
            .await
            .map_err(|_| Failure::request())?;
        if response
            .headers()
            .iter()
            .map(|(key, value)| key.as_str().len() + value.len())
            .sum::<usize>()
            > 64 * 1024
        {
            return Err(Failure {
                message: "HTTP headers exceed size limit".into(),
                stale_allowed: false,
            });
        }
        let mut parts = http::Response::new(());
        *parts.status_mut() = response.status();
        *parts.headers_mut() = response.headers().clone();
        let received = SystemTime::now();
        let policy = if let Some(old) = old {
            match old.policy.after_response(request, &parts, received) {
                AfterResponse::NotModified(policy, parts) => {
                    return Ok(Entry {
                        policy,
                        received: started,
                        bytes: old.bytes.clone(),
                        headers: parts.headers,
                    });
                }
                AfterResponse::Modified(policy, _) => policy,
            }
        } else {
            CachePolicy::new_options(
                request,
                &parts,
                received,
                CacheOptions {
                    shared: false,
                    cache_heuristic: 0.0,
                    ..CacheOptions::default()
                },
            )
        };
        if !response.status().is_success() {
            return Err(Failure {
                message: format!("Add-on returned HTTP {}", response.status().as_u16()),
                stale_allowed: matches!(response.status().as_u16(), 500 | 502 | 503 | 504),
            });
        }
        if response
            .content_length()
            .is_some_and(|length| length > limit as u64)
        {
            return Err(Failure {
                message: "HTTP response exceeds size limit".into(),
                stale_allowed: false,
            });
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Failure::request())? {
            if chunk.len() > limit.saturating_sub(bytes.len()) {
                return Err(Failure {
                    message: "HTTP response exceeds size limit".into(),
                    stale_allowed: false,
                });
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(Entry {
            policy,
            bytes: Arc::new(bytes),
            received: started,
            headers: parts.headers().clone(),
        })
    }
}

impl Entry {
    fn response(&self) -> Response {
        Response {
            bytes: self.bytes.clone(),
            storable: self.policy.is_storable(),
        }
    }

    fn allows_stale(&self, request: &http::Request<()>, directive: &str) -> bool {
        let directives: Vec<_> = self
            .headers
            .get_all("cache-control")
            .iter()
            .filter_map(|h| h.to_str().ok())
            .flat_map(|s| s.split(','))
            .map(str::trim)
            .collect();
        if directives.iter().any(|s| {
            matches!(
                s.split('=')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase()
                    .as_str(),
                "no-cache" | "must-revalidate" | "no-store"
            )
        }) {
            return false;
        }
        let seconds = directives.iter().find_map(|s| {
            let (key, value) = s.split_once('=')?;
            key.trim()
                .eq_ignore_ascii_case(directive)
                .then(|| value.trim().trim_matches('"').parse::<u64>().ok())
                .flatten()
        });
        let Some(seconds) = seconds else {
            return false;
        };
        let mut request = request.clone();
        let Ok(value) = http::HeaderValue::from_str(&format!("max-stale={seconds}")) else {
            return false;
        };
        request.headers_mut().insert("cache-control", value);
        matches!(
            self.policy.before_request(&request, SystemTime::now()),
            BeforeRequest::Fresh(_)
        )
    }
}

struct Failure {
    message: String,
    stale_allowed: bool,
}

impl Failure {
    fn request() -> Self {
        Self {
            message: "Add-on request failed or timed out".into(),
            stale_allowed: true,
        }
    }
}
