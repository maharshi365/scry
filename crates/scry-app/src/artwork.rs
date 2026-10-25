//! Bounded artwork cache. Call `set_visible` with viewport URLs, not
//! the whole catalogue; `get` never starts work. Nothing is fetched at startup.
//!
//! We retain at most 32 thumbnails (160 × 240; about 4.7 MiB), plus two
//! in-flight jobs. Source buffers are capped at 2 MiB and
//! decoded pixel buffers at 32 MiB per job. Decoder scratch space is additional.
//! Keep returned image handles only in the current view, not another cache.
//! A separate 32 MiB disposable disk cache keeps thumbnails for up to seven days.
//! The player profile instead keeps two images up to 1280 × 720 in RAM only.
//!
//! This is NOT a total renderer memory bound: Rune's `ImageAtlas` uses shelf
//! pages. It reclaims empty pages, but live images can prevent fragmented pages
//! from being reclaimed. Rune exposes no atlas compaction API to the app.
use anyhow::{Result, anyhow};
use image::ImageDecoder;
use std::{
    collections::{HashMap, VecDeque},
    io::Cursor,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use tokio::sync::watch;

const CAPACITY: usize = 32;
const FAILURE_CAPACITY: usize = 64;
const WORKERS: usize = 2;
const MAX_DOWNLOAD: usize = 2 * 1024 * 1024;
const MAX_DECODE: u64 = 32 * 1024 * 1024;
const MAX_DIMENSION: u32 = 4096;
const WIDTH: u32 = 160;
const HEIGHT: u32 = 240;
const MAX_URL: usize = 4096;

#[derive(Clone, Copy, Default)]
enum Profile {
    #[default]
    Poster,
    Player,
}

impl Profile {
    fn capacity(self) -> usize {
        match self {
            Self::Poster => CAPACITY,
            Self::Player => 2,
        }
    }

    fn size(self) -> (u32, u32) {
        match self {
            Self::Poster => (WIDTH, HEIGHT),
            Self::Player => (1280, 720),
        }
    }
}

struct Cache<T>(VecDeque<(String, T)>);

impl<T> Default for Cache<T> {
    fn default() -> Self {
        Self(VecDeque::new())
    }
}

impl<T> Cache<T> {
    fn get(&self, url: &str) -> Option<&T> {
        self.0
            .iter()
            .find(|(key, _)| key == url)
            .map(|(_, value)| value)
    }

    fn touch(&mut self, url: &str) {
        if let Some(index) = self.0.iter().position(|(key, _)| key == url)
            && let Some(entry) = self.0.remove(index)
        {
            self.0.push_back(entry);
        }
    }

    fn insert(&mut self, url: String, value: T, capacity: usize, visible: &[String]) {
        self.0.retain(|(key, _)| key != &url);
        if self.0.len() == capacity {
            let victim = self
                .0
                .iter()
                .position(|(key, _)| !visible.contains(key))
                .unwrap_or(0);
            self.0.remove(victim);
        }
        self.0.push_back((url, value));
    }
}

#[derive(Default)]
struct State {
    profile: Profile,
    visible: Vec<String>,
    ready: Cache<rune::Pixels>,
    failed: Cache<()>,
    active: HashMap<String, watch::Sender<bool>>,
    stop: bool,
}

impl State {
    fn next(&self) -> Option<String> {
        self.visible
            .iter()
            .find(|url| {
                self.ready.get(url).is_none()
                    && self.failed.get(url).is_none()
                    && !self.active.contains_key(*url)
            })
            .cloned()
    }

    fn set_visible(&mut self, urls: &[String]) {
        self.visible.clear();
        for url in urls {
            if self.visible.len() == self.profile.capacity() {
                break;
            }
            if valid_url(url) && !self.visible.contains(url) {
                self.visible.push(url.clone());
            }
        }
        for (url, cancel) in &self.active {
            if !self.visible.contains(url) {
                let _ = cancel.send(true);
            }
        }
        // No artwork is drawn on the new page. Drop pixel owners so the renderer
        // can also retire their atlas entries; posters remain in the disk cache.
        if self.visible.is_empty() {
            self.ready.0.clear();
        }
        for url in &self.visible {
            self.ready.touch(url);
            self.failed.touch(url);
        }
    }
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    available: Condvar,
    changed: AtomicBool,
}

pub struct Artwork {
    shared: Arc<Shared>,
}

impl Artwork {
    #[cfg(feature = "profiling")]
    pub fn stats(&self) -> Option<serde_json::Value> {
        let state = self.shared.state.lock().ok()?;
        let bytes: u64 = state.ready.0.iter().map(|(_, pixels)| {
            let (width, height) = pixels.size();
            u64::from(width) * u64::from(height) * 4
        }).sum();
        Some(serde_json::json!({"images": state.ready.0.len(), "pixel_bytes": bytes, "active": state.active.len()}))
    }
    /// Two sleeping workers each run async HTTP on a current-thread runtime.
    /// Decode happens on those same workers, never on the UI thread. Thumbnails
    /// are already display-sized; use Pixels directly rather than queueing
    /// another worker to build a mip chain that these poster sizes never sample.
    pub fn start(wake: &rune::Waker) -> Result<Self> {
        Self::start_profile(wake, Profile::Poster)
    }

    /// Two display-sized images, memory-only; at most 7 MiB of retained RGBA.
    pub fn start_player(wake: &rune::Waker) -> Result<Self> {
        Self::start_profile(wake, Profile::Player)
    }

    fn start_profile(wake: &rune::Waker, profile: Profile) -> Result<Self> {
        let client = reqwest::Client::builder()
            .https_only(!cfg!(feature = "profiling"))
            .redirect(reqwest::redirect::Policy::limited(3))
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .pool_max_idle_per_host(1)
            .build()
            .map_err(|_| anyhow!("Cannot create the artwork client"))?;
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                profile,
                ..State::default()
            }),
            ..Shared::default()
        });
        let artwork = Self { shared };
        let disk = scry_app::project_dir(true)
            .filter(|_| matches!(profile, Profile::Poster))
            .map(|dirs| {
                Arc::new(Mutex::new(crate::artwork_cache::DiskCache::new(
                    dirs.join("artwork"),
                )))
            });
        for index in 0..WORKERS {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .max_blocking_threads(1)
                .build()
                .map_err(|_| anyhow!("Cannot create the artwork runtime"))?;
            let state = artwork.shared.clone();
            let client = client.clone();
            let wake = wake.clone();
            let disk = disk.clone();
            thread::Builder::new()
                .name(format!("scry-artwork-{index}"))
                .spawn(move || worker(&state, &client, &wake, &runtime, disk.as_deref()))
                .map_err(|_| anyhow!("Cannot start the artwork worker"))?;
        }
        Ok(artwork)
    }

    /// Replace pending work with at most 32 distinct visible URLs. Offscreen
    /// downloads are cancelled; an already running decode finishes but its
    /// result is discarded. Queue replacement does not wait for a worker.
    pub fn set_visible(&mut self, urls: &[String]) {
        if let Ok(mut state) = self.shared.state.lock() {
            state.set_visible(urls);
        }
        self.shared.available.notify_all();
    }

    /// Read-only and nonblocking apart from a short cache lock. Returns only
    /// fully decoded images; draw a local placeholder for missing artwork.
    pub fn get(&self, url: &str) -> Option<rune::Pixels> {
        self.shared.state.lock().ok()?.ready.get(url).cloned()
    }

    #[cfg(test)]
    pub(crate) fn player_fixture(images: &[(&str, rune::Pixels)]) -> Self {
        let mut state = State {
            profile: Profile::Player,
            ..State::default()
        };
        for (url, image) in images {
            state.ready.insert((*url).into(), image.clone(), 2, &[]);
        }
        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(state),
                ..Shared::default()
            }),
        }
    }

    /// Called by the app's poll hook after a wake. Failures also trigger one
    /// update, but do not retry on every frame. Failure history is bounded.
    pub fn poll(&mut self) -> bool {
        self.shared.changed.swap(false, Ordering::AcqRel)
    }
}

impl Drop for Artwork {
    fn drop(&mut self) {
        // Shutdown must still cancel workers if a previous holder panicked.
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.stop = true;
        for cancel in state.active.values() {
            let _ = cancel.send(true);
        }
        drop(state);
        self.shared.available.notify_all();
        // Do not join on the UI thread. At most two bounded decodes can finish
        // after shutdown; workers then release their shared state and exit.
    }
}

fn worker(
    shared: &Shared,
    client: &reqwest::Client,
    wake: &rune::Waker,
    runtime: &tokio::runtime::Runtime,
    disk: Option<&Mutex<crate::artwork_cache::DiskCache>>,
) {
    let profile = shared
        .state
        .lock()
        .map(|state| state.profile)
        .unwrap_or_default();
    loop {
        let (url, mut cancel) = {
            let Ok(mut state) = shared.state.lock() else {
                return;
            };
            loop {
                if state.stop {
                    return;
                }
                if let Some(url) = state.next() {
                    let (sender, receiver) = watch::channel(false);
                    state.active.insert(url.clone(), sender);
                    break (url, receiver);
                }
                let Ok(ready) = shared.available.wait(state) else {
                    return;
                };
                state = ready;
            }
        };
        let cached = disk.as_ref().and_then(|disk| disk.lock().ok()?.load(&url));
        let decoded = cached.map_or_else(
            || {
                runtime
                    .block_on(async {
                        tokio::select! {
                            biased;
                            _ = cancel.changed() => Err(()),
                            result = download(client, &url) => result,
                        }
                    })
                    .and_then(|bytes| {
                        if *cancel.borrow() {
                            return Err(());
                        }
                        let (width, height, pixels) = thumbnail(&bytes, profile)?;
                        if let Some(disk) = &disk
                            && let Ok(disk) = disk.lock()
                        {
                            disk.store(&url, width, height, &pixels);
                        }
                        Ok((width, height, pixels))
                    })
            },
            Ok,
        );
        let result = decoded.and_then(|(width, height, pixels)| {
            if *cancel.borrow() {
                return Err(());
            }
            let image = rune::Pixels::new();
            image.set(width, height, pixels);
            Ok(image)
        });
        let Ok(mut state) = shared.state.lock() else {
            return;
        };
        state.active.remove(&url);
        let changed = !state.stop && !*cancel.borrow() && state.visible.contains(&url);
        if changed {
            let State {
                ready,
                failed,
                visible,
                ..
            } = &mut *state;
            match result {
                Ok(image) => ready.insert(url, image, profile.capacity(), visible),
                Err(()) => failed.insert(url, (), FAILURE_CAPACITY, visible),
            }
        }
        drop(state);
        if changed {
            shared.changed.store(true, Ordering::Release);
            wake();
        }
        shared.available.notify_all();
    }
}

fn valid_url(value: &str) -> bool {
    if value.len() > MAX_URL {
        return false;
    }
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    let scheme_allowed = url.scheme() == "https";
    #[cfg(feature = "profiling")]
    let scheme_allowed = scheme_allowed || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1"));
    scheme_allowed
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
}

async fn download(client: &reqwest::Client, url: &str) -> std::result::Result<Vec<u8>, ()> {
    let mut response = client.get(url).send().await.map_err(|_| ())?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|size| size > MAX_DOWNLOAD as u64)
    {
        return Err(());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| ())? {
        append_chunk(&mut bytes, &chunk)?;
    }
    Ok(bytes)
}

fn append_chunk(bytes: &mut Vec<u8>, chunk: &[u8]) -> std::result::Result<(), ()> {
    if chunk.len() > MAX_DOWNLOAD.saturating_sub(bytes.len()) {
        return Err(());
    }
    bytes.extend_from_slice(chunk);
    Ok(())
}

fn thumbnail(bytes: &[u8], profile: Profile) -> std::result::Result<(u32, u32, Vec<u8>), ()> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| ())?;
    if !matches!(
        reader.format(),
        Some(image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP)
    ) {
        return Err(());
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE);
    reader.limits(limits);
    let decoder = reader.into_decoder().map_err(|_| ())?;
    let (width, height) = decoder.dimensions();
    validate_dimensions(width, height, decoder.total_bytes())?;
    let image = image::DynamicImage::from_decoder(decoder).map_err(|_| ())?;
    // Convert to RGBA only after scaling, never allocate a full-size RGBA copy.
    let (width, height) = profile.size();
    let small = image.thumbnail(width, height).into_rgba8();
    Ok((small.width(), small.height(), small.into_raw()))
}

fn validate_dimensions(width: u32, height: u32, bytes: u64) -> std::result::Result<(), ()> {
    if width == 0
        || height == 0
        || width > MAX_DIMENSION
        || height > MAX_DIMENSION
        || bytes > MAX_DECODE
    {
        Err(())
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "Image fixture creation and decoding failures must fail the test"
)]
mod tests {
    use super::*;

    fn url(index: usize) -> String {
        format!("https://example.com/{index}.jpg")
    }

    #[test]
    fn visible_queue_is_bounded_deduplicated_and_replaced() {
        let mut state = State::default();
        let mut urls = vec![url(0), url(0), "http://example.com/no.jpg".into()];
        urls.extend((1..100).map(url));
        state.set_visible(&urls);
        assert_eq!(state.visible, (0..CAPACITY).map(url).collect::<Vec<_>>());
        let (sender, receiver) = watch::channel(false);
        state.active.insert(url(0), sender);
        state.set_visible(&[url(99)]);
        assert!(*receiver.borrow(), "offscreen work must be cancelled");
        assert_eq!(state.next(), Some(url(99)));
    }

    #[test]
    fn leaving_artwork_pages_releases_decoded_pixels_and_cancels_work() {
        for profile in [Profile::Poster, Profile::Player] {
            let mut state = State { profile, ..State::default() };
            let pixels = rune::Pixels::new();
            pixels.set(160, 240, vec![0; 160 * 240 * 4]);
            state.ready.insert(url(0), pixels, profile.capacity(), &[]);
            state.set_visible(&[url(0)]);
            assert!(state.ready.get(&url(0)).is_some());
            let (sender, receiver) = watch::channel(false);
            state.active.insert(url(1), sender);
            state.set_visible(&[]);
            assert!(state.ready.0.is_empty(), "Hidden pages must not pin decoded images");
            assert!(*receiver.borrow(), "Hidden artwork work must be cancelled");
            state.set_visible(&[url(0)]);
            assert_eq!(state.next(), Some(url(0)), "Returning must reload the thumbnail");
        }
    }

    #[test]
    fn cache_evicts_offscreen_entries_before_visible_entries() {
        let mut cache = Cache::default();
        cache.insert(url(0), 0, 2, &[]);
        cache.insert(url(1), 1, 2, &[]);
        cache.insert(url(2), 2, 2, &[url(0), url(2)]);
        assert_eq!(cache.get(&url(0)), Some(&0));
        assert_eq!(cache.get(&url(1)), None);
        assert_eq!(cache.get(&url(2)), Some(&2));
        assert_eq!(cache.0.len(), 2);
    }

    #[test]
    #[expect(
        clippy::panic,
        reason = "Inject a worker fault to exercise poisoned-cache recovery and shutdown"
    )]
    fn poisoned_cache_returns_a_placeholder_and_shutdown_still_cancels_work() {
        let shared = Arc::new(Shared::default());
        let (sender, receiver) = watch::channel(false);
        shared.state.lock().unwrap().active.insert(url(0), sender);
        let failing = shared.clone();
        let _ = thread::spawn(move || {
            let _state = failing.state.lock().unwrap();
            panic!("Injected cache fault");
        })
        .join();

        let mut artwork = Artwork { shared };
        assert!(artwork.get(&url(0)).is_none());
        artwork.set_visible(&[url(1)]);
        drop(artwork);
        assert!(
            *receiver.borrow(),
            "Shutdown must cancel work even after the cache is poisoned"
        );
    }

    #[test]
    fn failures_suppress_requests_and_history_is_bounded() {
        let mut state = State::default();
        state.set_visible(&[url(0)]);
        state.failed.insert(url(0), (), FAILURE_CAPACITY, &[]);
        assert_eq!(state.next(), None);
        for index in 1..=FAILURE_CAPACITY {
            state.failed.insert(url(index), (), FAILURE_CAPACITY, &[]);
        }
        assert_eq!(state.failed.0.len(), FAILURE_CAPACITY);
        assert_eq!(state.next(), Some(url(0)));
    }

    #[test]
    fn revisiting_an_entry_updates_eviction_priority() {
        let mut cache = Cache::default();
        cache.insert(url(0), (), 2, &[]);
        cache.insert(url(1), (), 2, &[]);
        cache.touch(&url(0));
        cache.insert(url(2), (), 2, &[]);
        assert!(cache.get(&url(0)).is_some());
        assert!(cache.get(&url(1)).is_none());
    }

    #[test]
    fn pipeline_scales_before_returning_rgba_to_rune() {
        let source = image::RgbImage::new(320, 480);
        let mut encoded = Cursor::new(Vec::new());
        source
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let (width, height, pixels) = thumbnail(encoded.get_ref(), Profile::Poster).unwrap();
        assert_eq!((width, height), (WIDTH, HEIGHT));
        assert_eq!(pixels.len(), (WIDTH * HEIGHT * 4) as usize);
    }

    #[test]
    fn player_artwork_keeps_backdrop_resolution_and_logo_transparency() {
        let source = image::RgbaImage::from_pixel(1920, 1080, image::Rgba([12, 24, 36, 128]));
        let mut encoded = Cursor::new(Vec::new());
        source
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let (width, height, pixels) = thumbnail(encoded.get_ref(), Profile::Player).unwrap();
        assert_eq!((width, height), (1280, 720));
        assert!(pixels.chunks_exact(4).all(|pixel| pixel[3] == 128));

        let mut state = State {
            profile: Profile::Player,
            ..State::default()
        };
        state.set_visible(&[url(0), url(1), url(2)]);
        assert_eq!(state.visible, [url(0), url(1)]);
    }

    #[test]
    fn download_limit_applies_without_content_length() {
        let mut bytes = vec![0; MAX_DOWNLOAD - 2];
        assert!(append_chunk(&mut bytes, &[1, 2]).is_ok());
        assert!(append_chunk(&mut bytes, &[3]).is_err());
        assert_eq!(bytes.len(), MAX_DOWNLOAD);
        assert_eq!(&bytes[MAX_DOWNLOAD - 2..], &[1, 2]);
    }

    #[test]
    fn source_limits_reject_oversized_allocations_and_dimensions() {
        assert!(validate_dimensions(4096, 4096, MAX_DECODE + 1).is_err());
        assert!(validate_dimensions(4097, 1, 4097 * 4).is_err());
        assert!(validate_dimensions(1, 4097, 4097 * 4).is_err());
        assert!(validate_dimensions(0, 1, 0).is_err());
        assert!(validate_dimensions(4096, 2048, MAX_DECODE).is_ok());
    }

    #[test]
    fn urls_require_https_without_embedded_credentials() {
        assert!(valid_url(&url(0)));
        for value in [
            "http://example.com/a",
            "file:///poster.png",
            "https://user:secret@example.com/a",
            "not a URL",
        ] {
            assert!(!valid_url(value));
        }
        assert!(!valid_url(&format!(
            "https://example.com/{}",
            "x".repeat(MAX_URL)
        )));
    }
}
