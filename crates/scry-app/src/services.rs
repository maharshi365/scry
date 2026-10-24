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

#[derive(Debug)]
pub enum Event {
    Addons(Result<Vec<Addon>, String>),
    Catalog(u64, Result<Vec<Meta>, String>),
    Search(u64, Vec<SearchResult>, Vec<String>, bool),
    Detail(u64, Result<(Meta, String), String>),
    Streams(u64, Vec<(String, Stream)>, Vec<String>, bool),
    Playing(u64, PathBuf),
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
    control: Option<watch::Sender<Option<Control>>>,
    commands: Option<mpsc::Sender<(u64, Vec<String>)>>,
    player: Option<JoinHandle<()>>,
}

impl Services {
    pub fn data_dir(&self) -> &std::path::Path {
        &self.data
    }
    pub fn start(wake: rune::Waker) -> Result<(Self, mpsc::Receiver<Event>)> {
        let data = directories::ProjectDirs::from("app", "Scry", "Scry")
            .context("Cannot locate the private application data folder")?
            .data_local_dir()
            .to_owned();
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
                std::fs::create_dir_all(&data)?;
                let path = data.join("addons.json");
                let addons: Vec<Addon> = match std::fs::read(&path) {
                    Ok(bytes) => serde_json::from_slice(&bytes).context(
                        "Cannot read add-ons configuration; original file was not changed",
                    )?,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        let addons: Vec<Addon> = Vec::new();
                        save(&data, &addons)?;
                        addons
                    }
                    Err(_) => bail!("Cannot open add-ons configuration"),
                };
                if addons.len() > 32 {
                    bail!("At most 32 installed add-ons are supported");
                }
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
                save(&data, &next)?;
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
            let result = save(&data, &next).map(|()| {
                current.clone_from(&next);
                next
            });
            inbox
                .send(Event::Addons(result.map_err(|e| e.to_string())))
                .await;
        });
    }

    pub fn catalog(&mut self, generation: u64, addon: Addon, catalog: Catalog, query: String) {
        if let Some(task) = self.catalog_task.take() {
            task.abort();
        }
        let (client, inbox) = (self.client.clone(), self.inbox.clone());
        self.catalog_task = Some(self.runtime.spawn(async move {
            let search = (!query.trim().is_empty()).then_some(query.trim());
            let result = client
                .catalog(&addon, &catalog, search)
                .await
                .map_err(|e| e.to_string());
            inbox.send(Event::Catalog(generation, result)).await;
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
        for task in [
            &mut self.catalog_task,
            &mut self.search_task,
            &mut self.detail_task,
            &mut self.stream_task,
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

fn save(data: &std::path::Path, addons: &[Addon]) -> Result<()> {
    std::fs::create_dir_all(data)?;
    let temp = data.join("addons.pending.json");
    std::fs::write(&temp, serde_json::to_vec_pretty(addons)?)
        .context("Cannot save add-ons configuration")?;
    std::fs::rename(temp, data.join("addons.json")).context("Cannot replace add-ons configuration")
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
    fn config_replacement_persists_removal_without_a_pending_file() {
        let data = std::env::temp_dir()
            .join("opencode")
            .join(format!("scry-config-test-{}", uuid::Uuid::new_v4()));
        let addon: Addon = serde_json::from_value(serde_json::json!({
            "manifest_url":"https://example.com/private-token/manifest.json",
            "manifest":{"id":"example", "name":"Example"}
        }))
        .unwrap();
        save(&data, &[addon]).unwrap();
        let first: Vec<Addon> =
            serde_json::from_slice(&std::fs::read(data.join("addons.json")).unwrap()).unwrap();
        assert_eq!(first[0].manifest.id, "example");
        save(&data, &[]).unwrap();
        let removed: Vec<Addon> =
            serde_json::from_slice(&std::fs::read(data.join("addons.json")).unwrap()).unwrap();
        assert!(removed.is_empty());
        assert!(!data.join("addons.pending.json").exists());
        std::fs::remove_file(data.join("addons.json")).unwrap();
        std::fs::remove_dir(data).unwrap();
    }
}
