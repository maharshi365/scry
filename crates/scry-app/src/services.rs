//! Background work and the narrow, bounded boundary to the native UI.
use anyhow::{Context, Result, bail};
use scry_addons::{Addon, Catalog, Client, Meta, Stream};
use scry_playback::{
    PlaybackEngine, PlaybackEvent, PlaybackTarget, Player,
    telemetry::{Report, Sample},
};
use scry_torrents::Torrents;
use std::{
    path::PathBuf,
    sync::Arc,
    thread::JoinHandle,
    time::{Duration, Instant},
};
use tokio::{
    runtime::Runtime,
    sync::{Mutex, mpsc, watch},
};

pub use crate::search::SearchResult;

pub const CINEMETA: &str = "https://v3-cinemeta.strem.io/manifest.json";

#[derive(Clone, Debug)]
pub struct LibraryPoster {
    pub poster: Option<String>,
    pub year: String,
}

impl LibraryPoster {
    fn for_media(media: &scry_library::Media, meta: &Meta) -> Option<Self> {
        let kind = match media.key.kind {
            scry_library::MediaKind::Movie => "movie",
            scry_library::MediaKind::Series => "series",
            scry_library::MediaKind::Episode => return None,
        };
        if meta.id != media.key.id
            || meta.kind != kind
            || meta.poster.as_deref().is_none_or(|p| p.trim().is_empty())
        {
            return None;
        }
        Some(Self {
            poster: meta.poster.clone(),
            year: meta.year.clone(),
        })
    }
}

#[derive(Debug)]
pub enum Event {
    WatchMetadata(u64, scry_library::Media, bool, Result<Box<Meta>, String>),
    SeriesTrackingDetails(u64, Vec<(scry_library::MediaKey, Option<Meta>)>, bool),
    SeriesDetails(u64, Vec<(scry_library::MediaKey, Option<Meta>)>, bool),
    LibraryPosters(
        u64,
        Vec<(scry_library::MediaKey, Option<LibraryPoster>)>,
        bool,
    ),
    Calendar(u64, Vec<(scry_library::Media, Meta)>, usize),
    Addons(Result<Vec<Addon>, String>),
    Catalog(u64, u32, Result<Vec<Meta>, String>),
    Search(u64, Vec<SearchResult>, Vec<String>, bool),
    Detail(u64, Result<(Meta, String), String>),
    Streams(u64, Vec<(String, Stream)>, Vec<String>, bool),
    Playing(u64, PathBuf),
    Ready(u64),
    Metrics(u64, Sample),
    Stopped(u64, Option<String>),
    PlayerWarning(u64, String),
}

#[derive(Clone)]
pub struct Inbox {
    tx: mpsc::Sender<Event>,
    wake: rune::Waker,
}

impl Inbox {
    async fn send(&self, event: Event) {
        if self.tx.send(event).await.is_ok() {
            (self.wake)();
        }
    }
    fn notify(&self, event: Event) {
        if self.tx.try_send(event).is_ok() {
            (self.wake)();
        }
    }
}

#[derive(Clone)]
pub enum Source {
    Stream(Stream),
    Local(PathBuf),
}

#[derive(Clone)]
pub enum Control {
    Play(u64, PlaybackTarget, Source),
    Command(u64, Vec<String>),
    Stop(u64),
}

pub struct Services {
    runtime: Arc<Runtime>,
    client: Client,
    inbox: Inbox,
    config: Arc<Mutex<Vec<Addon>>>,
    data: PathBuf,
    catalog_task: Option<tokio::task::JoinHandle<()>>,
    search_task: Option<tokio::task::JoinHandle<()>>,
    detail_task: Option<tokio::task::JoinHandle<()>>,
    stream_task: Option<tokio::task::JoinHandle<()>>,
    calendar_task: Option<tokio::task::JoinHandle<()>>,
    library_poster_task: Option<tokio::task::JoinHandle<()>>,
    series_detail_task: Option<tokio::task::JoinHandle<()>>,
    series_tracking_task: Option<tokio::task::JoinHandle<()>>,
    watch_tasks: Vec<tokio::task::JoinHandle<()>>,
    control: Option<watch::Sender<Option<Control>>>,
    commands: Option<mpsc::Sender<(u64, Vec<String>)>>,
    player: Option<JoinHandle<()>>,
}

impl Services {
    pub fn data_dir(&self) -> &std::path::Path {
        &self.data
    }
    pub fn start(wake: rune::Waker) -> Result<(Self, mpsc::Receiver<Event>)> {
        let data = scry_app::project_dir(false)
            .context("Cannot locate the private application data folder")?;
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_name("scry-network")
                .enable_all()
                .build()?,
        );
        let (tx, rx) = mpsc::channel(64);
        let inbox = Inbox { tx, wake };
        let (control, lifecycle) = watch::channel(None);
        let (commands, command_rx) = mpsc::channel(16);
        let rt = runtime.clone();
        let output = inbox.clone();
        let folder = data.clone();
        let player = std::thread::Builder::new()
            .name("scry-player".into())
            .spawn(move || {
                rt.block_on(player_loop(lifecycle, command_rx, output, folder));
            })?;
        let services = Self {
            runtime,
            client: Client::new()?,
            inbox,
            config: Arc::new(Mutex::new(Vec::new())),
            data,
            catalog_task: None,
            search_task: None,
            detail_task: None,
            stream_task: None,
            calendar_task: None,
            library_poster_task: None,
            series_detail_task: None,
            series_tracking_task: None,
            watch_tasks: Vec::new(),
            control: Some(control),
            commands: Some(commands),
            player: Some(player),
        };
        services.load();
        Ok((services, rx))
    }

    fn load(&self) {
        let (inbox, config, data) = (self.inbox.clone(), self.config.clone(), self.data.clone());
        self.runtime.spawn(async move {
            let result: Result<Vec<Addon>> = async {
                let addons = tokio::task::spawn_blocking(move || load(&data))
                    .await
                    .context("Cannot load add-ons configuration")??;
                *config.lock().await = addons.clone();
                Ok(addons)
            }
            .await;
            inbox
                .send(Event::Addons(result.map_err(|e| e.to_string())))
                .await;
        });
    }

    pub fn install(&self, input: String) {
        let (client, inbox, config, data) = (
            self.client.clone(),
            self.inbox.clone(),
            self.config.clone(),
            self.data.clone(),
        );
        self.runtime.spawn(async move {
            let result: Result<Vec<Addon>> = async {
                let addon = client.install(&input).await?;
                let mut current = config.lock().await;
                let mut next = current.clone();
                next.retain(|a| a.manifest.id != addon.manifest.id);
                if next.len() >= 32 {
                    bail!("Remove an add-on before installing more (limit: 32)");
                }
                next.push(addon);
                persist(data, next.clone()).await?;
                current.clone_from(&next);
                Ok(next)
            }
            .await;
            inbox
                .send(Event::Addons(result.map_err(|e| e.to_string())))
                .await;
        });
    }

    pub fn remove(&self, id: String) {
        let (inbox, config, data) = (self.inbox.clone(), self.config.clone(), self.data.clone());
        self.runtime.spawn(async move {
            let mut current = config.lock().await;
            let mut next = current.clone();
            next.retain(|a| a.manifest.id != id);
            let result = persist(data, next.clone()).await.map(|()| {
                current.clone_from(&next);
                next
            });
            inbox
                .send(Event::Addons(result.map_err(|e| e.to_string())))
                .await;
        });
    }

    pub fn set_addon_disabled(&self, id: String, disabled: bool) {
        let (inbox, config, data) = (self.inbox.clone(), self.config.clone(), self.data.clone());
        self.runtime.spawn(async move {
            let mut current = config.lock().await;
            let mut next = current.clone();
            let result = async {
                let addon = next
                    .iter_mut()
                    .find(|a| a.manifest.id == id)
                    .context("Add-on is no longer installed")?;
                addon.disabled = disabled;
                persist(data, next.clone()).await?;
                current.clone_from(&next);
                Ok(next)
            }
            .await;
            inbox
                .send(Event::Addons(
                    result.map_err(|e: anyhow::Error| e.to_string()),
                ))
                .await;
        });
    }

    pub fn watch_metadata(
        &mut self,
        token: u64,
        mut addons: Vec<Addon>,
        media: scry_library::Media,
        watched: bool,
    ) -> Result<()> {
        self.watch_tasks.retain(|task| !task.is_finished());
        anyhow::ensure!(
            self.watch_tasks.len() < 8,
            "Watched actions are busy; try again"
        );
        let (client, inbox) = (self.client.clone(), self.inbox.clone());
        self.watch_tasks.push(self.runtime.spawn(async move {
            let kind = if media.key.kind == scry_library::MediaKind::Movie {
                "movie"
            } else {
                "series"
            };
            addons.sort_by_key(|addon| addon.manifest.id != media.key.namespace);
            let mut result = Err("Cannot load release metadata for this watched action".into());
            for addon in addons
                .iter()
                .filter(|addon| addon.supports("meta", kind, &media.key.id))
            {
                if let Ok(meta) = client.meta(addon, kind, &media.key.id).await
                    && meta.id == media.key.id
                    && meta.kind == kind
                {
                    result = Ok(Box::new(meta));
                    break;
                }
            }
            inbox
                .send(Event::WatchMetadata(token, media, watched, result))
                .await;
        }));
        Ok(())
    }

    pub fn catalog(&mut self, generation: u64, addon: Addon, catalog: Catalog, offset: u32) {
        if let Some(task) = self.catalog_task.take() {
            task.abort();
        }
        let (client, inbox) = (self.client.clone(), self.inbox.clone());
        self.catalog_task = Some(self.runtime.spawn(async move {
            let result = client
                .catalog_page(&addon, &catalog, offset)
                .await
                .map_err(|e| e.to_string());
            inbox.send(Event::Catalog(generation, offset, result)).await;
        }));
    }

    pub fn search(&mut self, generation: u64, addons: Vec<Addon>, query: String) {
        if let Some(task) = self.search_task.take() {
            task.abort();
        }
        let (client, inbox) = (self.client.clone(), self.inbox.clone());
        self.search_task = Some(self.runtime.spawn(async move {
            let query = match crate::search::query(&query) {
                Ok(query) => query,
                Err(warning) => {
                    inbox
                        .send(Event::Search(
                            generation,
                            Vec::new(),
                            vec![warning.into()],
                            true,
                        ))
                        .await;
                    return;
                }
            };
            if query.is_empty() {
                inbox
                    .send(Event::Search(generation, Vec::new(), Vec::new(), true))
                    .await;
                return;
            }
            let requests = match crate::search::plan(addons) {
                Ok(requests) => requests,
                Err(warning) => {
                    inbox
                        .send(Event::Search(
                            generation,
                            Vec::new(),
                            vec![warning.into()],
                            true,
                        ))
                        .await;
                    return;
                }
            };
            if requests.is_empty() {
                inbox
                    .send(Event::Search(
                        generation,
                        Vec::new(),
                        vec![
                            "No installed add-on has a compatible movie or series search catalog"
                                .into(),
                        ],
                        true,
                    ))
                    .await;
                return;
            }
            let mut requests = requests.into_iter().enumerate();
            // Dropping this set on cancellation aborts its in-flight requests too.
            let mut tasks = tokio::task::JoinSet::new();
            let mut results = crate::search::Results::default();
            let mut interrupted = false;
            loop {
                while tasks.len() < crate::search::CONCURRENCY {
                    let Some((index, request)) = requests.next() else {
                        break;
                    };
                    let (client, query) = (client.clone(), query.clone());
                    tasks.spawn(async move {
                        let result = client
                            .catalog(&request.addon, &request.catalog, Some(&query))
                            .await
                            .map_err(|_| ());
                        (index, request, result)
                    });
                }
                let Some(result) = tasks.join_next().await else {
                    break;
                };
                match result {
                    Ok((index, request, result)) => results.record(index, &request, result),
                    Err(_) => interrupted = true,
                }
                let (found, mut warnings) = results.snapshot(&query);
                if interrupted {
                    warnings.push("An add-on search could not finish".into());
                }
                inbox
                    .send(Event::Search(generation, found, warnings, false))
                    .await;
            }
            let (found, mut warnings) = results.snapshot(&query);
            if interrupted {
                warnings.push("An add-on search could not finish".into());
            }
            inbox
                .send(Event::Search(generation, found, warnings, true))
                .await;
        }));
    }

    pub fn detail(
        &mut self,
        generation: u64,
        mut addons: Vec<Addon>,
        meta: Meta,
        namespace: String,
    ) {
        if let Some(task) = self.detail_task.take() {
            task.abort();
        }
        let (client, inbox) = (self.client.clone(), self.inbox.clone());
        addons.sort_by_key(|addon| addon.manifest.id != namespace);
        self.detail_task = Some(self.runtime.spawn(async move {
            let mut errors = Vec::new();
            for addon in addons
                .iter()
                .filter(|a| a.supports("meta", &meta.kind, &meta.id))
            {
                match client.meta(addon, &meta.kind, &meta.id).await {
                    Ok(detail) => {
                        inbox
                            .send(Event::Detail(
                                generation,
                                Ok((detail, addon.manifest.id.clone())),
                            ))
                            .await;
                        return;
                    }
                    Err(_) => {
                        errors.push(format!("{}: detail request failed", addon.manifest.name));
                    }
                }
            }
            let result = if meta.kind == "movie" {
                Ok((meta, namespace))
            } else {
                Err(if errors.is_empty() {
                    "No installed add-on provides episode details".into()
                } else {
                    errors.join(" · ")
                })
            };
            inbox.send(Event::Detail(generation, result)).await;
        }));
    }

    pub fn streams(&mut self, generation: u64, addons: Vec<Addon>, kind: String, id: String) {
        if let Some(task) = self.stream_task.take() {
            task.abort();
        }
        let (client, inbox) = (self.client.clone(), self.inbox.clone());
        self.stream_task = Some(self.runtime.spawn(async move {
            let mut tasks = tokio::task::JoinSet::new();
            for addon in addons
                .into_iter()
                .filter(|a| a.supports("stream", &kind, &id))
            {
                let (client, kind, id) = (client.clone(), kind.clone(), id.clone());
                tasks.spawn(async move {
                    let result = client.streams(&addon, &kind, &id).await;
                    (addon.manifest.name, result)
                });
            }
            let mut streams = Vec::new();
            let mut errors = Vec::new();
            while let Some(result) = tasks.join_next().await {
                match result {
                    Ok((name, Ok(found))) => {
                        let remaining = 300usize.saturating_sub(streams.len());
                        streams.extend(
                            found
                                .into_iter()
                                .take(remaining)
                                .map(|stream| (name.clone(), stream)),
                        );
                    }
                    Ok((name, Err(_))) => {
                        errors.push(format!("{name}: stream request failed or timed out"));
                    }
                    Err(_) => errors.push("An add-on request could not finish".into()),
                }
                inbox
                    .send(Event::Streams(
                        generation,
                        streams.clone(),
                        errors.clone(),
                        false,
                    ))
                    .await;
            }
            inbox
                .send(Event::Streams(generation, streams, errors, true))
                .await;
        }));
    }

    pub fn reload_addons(&self) {
        self.load();
    }

    pub fn calendar(
        &mut self,
        generation: u64,
        addons: Vec<Addon>,
        series: Vec<scry_library::Media>,
    ) {
        if let Some(task) = self.calendar_task.take() {
            task.abort();
        }
        let (client, inbox) = (self.client.clone(), self.inbox.clone());
        self.calendar_task = Some(self.runtime.spawn(async move {
            let mut remaining = series.into_iter();
            let mut tasks = tokio::task::JoinSet::new();
            let mut found = Vec::new();
            let mut failed = 0;
            loop {
                while tasks.len() < 4 {
                    let Some(media) = remaining.next() else {
                        break;
                    };
                    let (client, mut addons) = (client.clone(), addons.clone());
                    tasks.spawn(async move {
                        addons.sort_by_key(|a| a.manifest.id != media.key.namespace);
                        let kind = if media.key.kind == scry_library::MediaKind::Movie {
                            "movie"
                        } else {
                            "series"
                        };
                        for addon in addons
                            .iter()
                            .filter(|a| a.supports("meta", kind, &media.key.id))
                        {
                            if let Ok(meta) = client.meta(addon, kind, &media.key.id).await {
                                return Some((media, meta));
                            }
                        }
                        None
                    });
                }
                let Some(result) = tasks.join_next().await else {
                    break;
                };
                if let Ok(Some(item)) = result {
                    found.push(item);
                } else {
                    failed += 1;
                }
            }
            inbox.send(Event::Calendar(generation, found, failed)).await;
        }));
    }

    pub fn library_posters(
        &mut self,
        generation: u64,
        addons: Vec<Addon>,
        media: Vec<scry_library::Media>,
    ) {
        if let Some(task) = self.library_poster_task.take() {
            task.abort();
        }
        let (client, inbox) = (self.client.clone(), self.inbox.clone());
        self.library_poster_task = Some(self.runtime.spawn(async move {
            let mut remaining = media.into_iter().take(12);
            let mut tasks = tokio::task::JoinSet::new();
            loop {
                while tasks.len() < 4 {
                    let Some(media) = remaining.next() else {
                        break;
                    };
                    let (client, mut addons) = (client.clone(), addons.clone());
                    tasks.spawn(async move {
                        addons.sort_by_key(|a| a.manifest.id != media.key.namespace);
                        let kind = if media.key.kind == scry_library::MediaKind::Movie {
                            "movie"
                        } else {
                            "series"
                        };
                        for addon in addons
                            .iter()
                            .filter(|a| a.supports("meta", kind, &media.key.id))
                        {
                            if let Ok(meta) = client.meta(addon, kind, &media.key.id).await
                                && let Some(poster) = LibraryPoster::for_media(&media, &meta)
                            {
                                return (media.key, Some(poster));
                            }
                        }
                        (media.key, None)
                    });
                }
                let Some(result) = tasks.join_next().await else {
                    break;
                };
                if let Ok(item) = result {
                    inbox
                        .send(Event::LibraryPosters(generation, vec![item], false))
                        .await;
                }
            }
            inbox
                .send(Event::LibraryPosters(generation, Vec::new(), true))
                .await;
        }));
    }

    pub fn series_details(
        &mut self,
        generation: u64,
        addons: Vec<Addon>,
        keys: Vec<scry_library::MediaKey>,
        background: bool,
    ) {
        let slot = if background {
            &mut self.series_tracking_task
        } else {
            &mut self.series_detail_task
        };
        if let Some(task) = slot.take() {
            task.abort();
        }
        let (client, inbox) = (self.client.clone(), self.inbox.clone());
        *slot = Some(self.runtime.spawn(async move {
            let mut remaining = keys.into_iter().take(12);
            let mut tasks = tokio::task::JoinSet::new();
            loop {
                while tasks.len() < 4 {
                    let Some(key) = remaining.next() else { break };
                    let (client, mut addons) = (client.clone(), addons.clone());
                    tasks.spawn(async move {
                        addons.sort_by_key(|addon| addon.manifest.id != key.namespace);
                        for addon in addons
                            .iter()
                            .filter(|a| a.supports("meta", "series", &key.id))
                        {
                            if let Ok(meta) = client.meta(addon, "series", &key.id).await
                                && meta.id == key.id
                                && meta.kind == "series"
                            {
                                return (key, Some(meta));
                            }
                        }
                        (key, None)
                    });
                }
                let Some(result) = tasks.join_next().await else {
                    break;
                };
                if let Ok(item) = result {
                    let event = if background {
                        Event::SeriesTrackingDetails(generation, vec![item], false)
                    } else {
                        Event::SeriesDetails(generation, vec![item], false)
                    };
                    inbox.send(event).await;
                }
            }
            let event = if background {
                Event::SeriesTrackingDetails(generation, vec![], true)
            } else {
                Event::SeriesDetails(generation, vec![], true)
            };
            inbox.send(event).await;
        }));
    }

    pub fn control(&self, command: Control) -> Result<()> {
        match command {
            Control::Command(id, args) => self
                .commands
                .as_ref()
                .context("Player is closed")?
                .try_send((id, args))
                .map_err(|_| anyhow::anyhow!("Player is busy; try again")),
            command => self
                .control
                .as_ref()
                .context("Player is closed")?
                .send(Some(command))
                .map_err(|_| anyhow::anyhow!("Player is closed")),
        }
    }

    pub fn shutdown(&mut self) {
        self.shutdown_then(|| {});
    }

    pub fn shutdown_then(&mut self, finished: impl FnOnce() + Send + 'static) {
        for task in self.watch_tasks.drain(..) {
            task.abort();
        }
        for task in [
            &mut self.catalog_task,
            &mut self.search_task,
            &mut self.detail_task,
            &mut self.stream_task,
            &mut self.calendar_task,
            &mut self.library_poster_task,
            &mut self.series_detail_task,
            &mut self.series_tracking_task,
        ] {
            if let Some(task) = task.take() {
                task.abort();
            }
        }
        // Closing the channel also interrupts torrent metadata resolution.
        self.control.take();
        self.commands.take();
        let Some(player) = self.player.take() else {
            finished();
            return;
        };
        let runtime = self.runtime.clone();
        // mpv teardown can require the parent's native message pump. Never join
        // from Rune's UI thread. Keep the runtime alive until teardown completes.
        std::thread::spawn(move || {
            let _ = player.join();
            drop(runtime);
            finished();
        });
    }
}

impl Drop for Services {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn database(data: &std::path::Path) -> Result<scry_library::Library> {
    std::fs::create_dir_all(data)?;
    scry_library::Library::open(data.join(scry_library::DATABASE_NAME))
}

// Local preferences share the stored manifest document, not the remote manifest.
// The optional flag needs no database schema change or data migration.
#[derive(serde::Serialize, serde::Deserialize)]
struct StoredManifest {
    #[serde(flatten)]
    manifest: scry_addons::Manifest,
    #[serde(default, rename = "_scry_disabled")]
    disabled: bool,
}

fn load(data: &std::path::Path) -> Result<Vec<Addon>> {
    let records = database(data)?.load_addons()?;
    if records.len() > 32 {
        bail!("At most 32 installed add-ons are supported");
    }
    records
        .into_iter()
        .map(|record| {
            let stored: StoredManifest = serde_json::from_str(&record.manifest_json)
                .context("Cannot read stored add-on manifest")?;
            if stored.manifest.id != record.id {
                bail!("Stored add-on identity does not match its manifest");
            }
            Ok(Addon {
                manifest_url: record.manifest_url,
                manifest: stored.manifest,
                disabled: stored.disabled,
            })
        })
        .collect()
}

fn save(data: &std::path::Path, addons: &[Addon]) -> Result<()> {
    let records = addons
        .iter()
        .map(|addon| {
            Ok(scry_library::AddonRecord {
                id: addon.manifest.id.clone(),
                manifest_url: addon.manifest_url.clone(),
                manifest_json: serde_json::to_string(&StoredManifest {
                    manifest: addon.manifest.clone(),
                    disabled: addon.disabled,
                })?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    database(data)?
        .replace_addons(&records)
        .context("Cannot save add-ons configuration")
}

async fn persist(data: PathBuf, addons: Vec<Addon>) -> Result<()> {
    tokio::task::spawn_blocking(move || save(&data, &addons))
        .await
        .context("Cannot save add-ons configuration")?
}

fn direct_url(stream: &Stream) -> Result<Option<String>> {
    stream.validate_playback()?;
    Ok(stream.url.clone())
}

#[expect(
    clippy::too_many_lines,
    reason = "One task owns the native engine, torrent session, and ordered cancellation/teardown lifecycle"
)]
async fn player_loop(
    mut lifecycle: watch::Receiver<Option<Control>>,
    mut commands: mpsc::Receiver<(u64, Vec<String>)>,
    inbox: Inbox,
    data: PathBuf,
) {
    let mut player: Option<Box<dyn PlaybackEngine>> = None;
    let mut torrents: Option<Torrents> = None;
    let mut report: Option<Report> = None;
    let mut generation = 0;
    let mut pending = None;
    let mut started = Instant::now();
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        let command = if let Some(command) = pending.take() {
            Some(command)
        } else {
            tokio::select! {
                biased;
                changed = lifecycle.changed() => {
                    if changed.is_err() { break; }
                    lifecycle.borrow_and_update().clone()
                }
                args = commands.recv() => args.map(|(id, args)| Control::Command(id, args)),
                _ = tick.tick(), if player.is_some() => {
                    let Some(engine) = player.as_ref() else { continue; };
                    let mut ended = None;
                    // libmpv also emits unhandled events. Bound draining per sample.
                    for _ in 0..64 {
                        match engine.event(0.0) {
                            Some(PlaybackEvent::Failed | PlaybackEvent::Shutdown) => { ended = Some(true); break; }
                            Some(PlaybackEvent::Ended) => { ended = Some(false); break; }
                            Some(PlaybackEvent::Ready) => inbox.send(Event::Ready(generation)).await,
                            _ => {}
                        }
                    }
                    let sample = engine.sample(started.elapsed().as_secs_f64());
                    if let Some(active) = &mut report
                        && active.sample(&sample).is_err() {
                        ended = Some(true);
                    }
                    inbox.notify(Event::Metrics(generation, sample));
                    if let Some(failed) = ended {
                        destroy_player(&mut player);
                        if let Some(report) = report.take() { let _ = report.finish(failed); }
                        if let Some(torrents) = torrents.take() { torrents.stop().await; }
                        inbox.send(Event::Stopped(generation, failed.then(|| "Playback or report writing failed. Check the runtime and stream source.".into()))).await;
                    }
                    continue;
                }
            }
        };
        let Some(command) = command else {
            break;
        };
        if let Control::Command(id, args) = command {
            if id != generation {
                continue;
            }
            if let Some(engine) = &player {
                let refs: Vec<_> = args.iter().map(String::as_str).collect();
                if engine.command(&refs).is_err() {
                    inbox
                        .send(Event::PlayerWarning(
                            generation,
                            "This control is not available for the current media".into(),
                        ))
                        .await;
                }
            }
            continue;
        }
        destroy_player(&mut player);
        if let Some(report) = report.take() {
            let _ = report.finish(false);
        }
        if let Some(session) = torrents.take() {
            session.stop().await;
        }
        match command {
            Control::Stop(id) => {
                generation = id;
                inbox.send(Event::Stopped(id, None)).await;
            }
            Control::Play(id, target, source) => {
                generation = id;
                started = Instant::now();
                let reports = data.join("reports");
                let path = reports.join(format!("{}.jsonl", uuid::Uuid::new_v4()));
                let created = std::fs::create_dir_all(&reports)
                    .map_err(anyhow::Error::from)
                    .and_then(|()| Report::create(&path, "native"));
                let Ok(active) = created else {
                    inbox
                        .send(Event::Stopped(
                            id,
                            Some("Cannot create private performance report".into()),
                        ))
                        .await;
                    continue;
                };
                report = Some(active);
                let result: Result<Box<dyn PlaybackEngine>> = {
                    let prepare = async {
                        let url = match source {
                            Source::Local(path) => {
                                let path = std::fs::canonicalize(path)
                                    .context("Cannot open the local video file")?;
                                if !path.is_file() {
                                    bail!("Local video path is not a file");
                                }
                                url::Url::from_file_path(path)
                                    .map_err(|()| anyhow::anyhow!("Invalid local video path"))?
                                    .to_string()
                            }
                            Source::Stream(stream) => {
                                if let Some(url) = direct_url(&stream)? {
                                    url
                                } else {
                                    let session =
                                        Torrents::new(data.join("torrent-cache")).await.map_err(
                                            |_| anyhow::anyhow!("Cannot start torrent engine"),
                                        )?;
                                    torrents = Some(session);
                                    let session = torrents
                                        .as_ref()
                                        .context("Torrent session is unavailable")?;
                                    session.play(&stream).await
                                        .map_err(|_| anyhow::anyhow!("Torrent could not resolve metadata or select a video file"))?
                                }
                            }
                        };
                        let engine: Box<dyn PlaybackEngine> = Box::new(Player::open(target)?);
                        engine.load(&url)?;
                        Ok(engine)
                    };
                    tokio::select! {
                        biased;
                        changed = lifecycle.changed() => {
                            if changed.is_err() { break; }
                            pending.clone_from(&lifecycle.borrow_and_update());
                            // Dropping preparation cancels metadata resolution. The next
                            // command stops and drops its session before any replacement.
                            continue;
                        }
                        result = prepare => result,
                    }
                };
                match result {
                    Ok(engine) => {
                        player = Some(engine);
                        tick.reset();
                        inbox.send(Event::Playing(id, path)).await;
                    }
                    Err(error) => {
                        if let Some(active) = report.take() {
                            let _ = active.finish(true);
                        }
                        if let Some(session) = torrents.take() {
                            session.stop().await;
                        }
                        inbox
                            .send(Event::Stopped(id, Some(error.to_string())))
                            .await;
                    }
                }
            }
            #[expect(
                clippy::unreachable,
                reason = "Control::Command is handled above and always continues before lifecycle teardown"
            )]
            Control::Command(_, _) => unreachable!(),
        }
    }
    destroy_player(&mut player);
    if let Some(report) = report {
        let _ = report.finish(false);
    }
    if let Some(torrents) = torrents {
        torrents.stop().await;
    }
}

fn destroy_player(player: &mut Option<Box<dyn PlaybackEngine>>) {
    if let Some(engine) = player.take() {
        let _ = engine.command(&["stop"]);
        drop(engine);
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "Configuration fixture and persistence failures must fail the test"
)]
mod tests {
    use super::*;

    #[test]
    fn library_artwork_requires_matching_identity_and_a_nonempty_poster() {
        let media = scry_library::Media {
            key: scry_library::MediaKey::new("test", scry_library::MediaKind::Movie, "film"),
            title: "Film".into(),
            parent: None,
        };
        let meta: Meta = serde_json::from_value(serde_json::json!({
            "id":"film", "type":"movie", "name":"Film", "releaseInfo":"2026", "poster":"https://example.com/poster.jpg"
        })).unwrap();
        let poster = LibraryPoster::for_media(&media, &meta).unwrap();
        assert_eq!(
            poster.poster.as_deref(),
            Some("https://example.com/poster.jpg")
        );
        assert_eq!(poster.year, "2026");
        let mut wrong_id = meta.clone();
        wrong_id.id = "another-film".into();
        assert!(LibraryPoster::for_media(&media, &wrong_id).is_none());
        let mut wrong_type = meta.clone();
        wrong_type.kind = "series".into();
        assert!(LibraryPoster::for_media(&media, &wrong_type).is_none());
        for url in [None, Some(" ".into())] {
            let mut missing = meta.clone();
            missing.poster = url;
            assert!(LibraryPoster::for_media(&media, &missing).is_none());
        }
    }

    #[test]
    fn disabled_addon_keeps_its_manifest_and_url_across_restarts() {
        let directory = tempfile::tempdir().unwrap();
        let mut addon: Addon = serde_json::from_value(serde_json::json!({
            "manifest_url":"https://example.com/private-token/manifest.json",
            "manifest":{"id":"example", "name":"Example", "logo":"https://example.com/logo.png"}
        }))
        .unwrap();
        addon.disabled = true;
        save(directory.path(), &[addon.clone()]).unwrap();
        let loaded = load(directory.path()).unwrap();
        assert!(loaded[0].disabled);
        assert_eq!(loaded[0].manifest_url, addon.manifest_url);
        assert_eq!(loaded[0].manifest.name, "Example");
        assert_eq!(
            loaded[0].manifest.logo.as_deref(),
            Some("https://example.com/logo.png")
        );
        addon.disabled = false;
        save(directory.path(), &[addon]).unwrap();
        assert!(!load(directory.path()).unwrap()[0].disabled);
    }

    #[test]
    fn config_replacement_persists_removal_in_sqlite() {
        let directory = tempfile::tempdir().unwrap();
        let data = directory.path();
        let addon: Addon = serde_json::from_value(serde_json::json!({
            "manifest_url":"https://example.com/private-token/manifest.json",
            "manifest":{"id":"example", "name":"Example"}
        }))
        .unwrap();
        save(data, &[addon]).unwrap();
        let first = load(data).unwrap();
        assert_eq!(first[0].manifest.id, "example");
        assert_eq!(
            first[0].manifest_url,
            "https://example.com/private-token/manifest.json"
        );
        save(data, &[]).unwrap();
        let removed = load(data).unwrap();
        assert!(removed.is_empty());
        assert!(data.join(scry_library::DATABASE_NAME).exists());
        assert!(!data.join("addons.json").exists());
        assert!(!data.join("addons.pending.json").exists());
    }
}
