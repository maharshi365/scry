use crate::services::{CINEMETA, Control, Event, Services, Source};
#[path = "library_view.rs"]
mod library_view;
#[path = "source_picker.rs"]
mod source_picker;
use library_view::LibraryUi;
use rune::tok::{self, TextStyle, type_};
use rune::{App, AppSpec, Cx, El, ScrollSpec, TextEdit, Tx, col, el, row, text, text_input};
use scry_app::platform::{Host, VideoHost};
use scry_playback::telemetry::Sample;
use scry_addons::{Addon, Catalog, Meta, Stream};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

const EPISODES_PER_PAGE: usize = 100;
const POSTER_CARD_HEIGHT: f32 = 302.0;
const POSTER_GAP: f32 = 18.0;

fn poster_columns(width: f32) -> usize {
    ((width + POSTER_GAP) / 174.0).floor().max(1.0) as usize
}

fn accent() -> rune::Color {
    rune::oklch(0.79, 0.12, 195.0)
}

fn panel() -> rune::Color {
    rune::oklch(0.205, 0.018, 240.0)
}

fn edge() -> rune::Color {
    rune::oklch(0.32, 0.025, 230.0)
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Page {
    Discover,
    Library,
    Addons,
    Detail,
    Player,
}

#[derive(Clone)]
struct Route {
    page: Page,
    kind: String,
    query: String,
    catalog: Option<(Addon, Catalog)>,
    metas: Arc<Vec<Meta>>,
    providers: Arc<Vec<Addon>>,
    detail: Option<Meta>,
    namespace: String,
    episode: Option<String>,
    pending_episode: Option<String>,
    episode_offset: usize,
    episode_season: Option<Option<u32>>,
    loading: bool,
    key: u64,
    anchor: Option<String>,
    status: String,
    warnings: Vec<String>,
}

pub struct State {
    logo: rune::Pixels,
    artwork: Option<crate::artwork::Artwork>,
    artwork_urls: Vec<String>,
    poster_rects: std::collections::BTreeMap<usize, [f32; 4]>,
    poster_columns: usize,
    poster_viewport: Option<[f32; 4]>,
    poster_route: u64,
    library: LibraryUi,
    detail_namespace: String,
    pending_episode: Option<String>,
    host: Host,
    host_error: Arc<Mutex<Option<String>>>,
    services: Option<Services>,
    events: Option<tokio::sync::mpsc::Receiver<Event>>,
    page: Page,
    addons: Vec<Addon>,
    config_busy: bool,
    install: TextEdit,
    direct: TextEdit,
    search: TextEdit,
    displayed_query: String,
    kind: String,
    catalog: Option<(Addon, Catalog)>,
    metas: Arc<Vec<Meta>>,
    providers: Arc<Vec<Addon>>,
    back: Vec<Route>,
    forward: Vec<Route>,
    route_key: u64,
    next_route_key: u64,
    anchor: Option<String>,
    reveal: Option<String>,
    detail: Option<Meta>,
    // Stable protocol ID (including season/episode suffix) for library integration.
    selected_video_id: Option<String>,
    episode_offset: usize,
    episode_season: Option<Option<u32>>,
    season_menu_open: bool,
    streams: Vec<(String, Stream)>,
    stream_provider: Option<String>,
    source_menu_open: bool,
    warnings: Vec<String>,
    error: Option<String>,
    status: String,
    loading: bool,
    catalog_id: u64,
    detail_id: u64,
    stream_id: u64,
    play_id: u64,
    playing: bool,
    sample: Sample,
    report: Option<PathBuf>,
    playing_title: String,
    // Identity of the actual catalog playback, not the last detail page visited.
    playing_meta_id: Option<(String, String)>,
    playing_video_id: Option<String>,
    volume: u32,
    startup_play: Option<String>,
    quit: Option<rune::AppHandle<State>>,
}

impl State {
    fn new(host: Host, host_error: Arc<Mutex<Option<String>>>) -> Self {
        let logo = rune::Pixels::new();
        logo.set(
            128,
            128,
            include_bytes!(concat!(env!("OUT_DIR"), "/skry.rgba")).to_vec(),
        );
        Self {
            logo,
            artwork: None,
            artwork_urls: Vec::new(),
            poster_rects: Default::default(),
            poster_columns: 3,
            poster_viewport: None,
            poster_route: 0,
            library: LibraryUi::default(),
            detail_namespace: String::new(),
            pending_episode: None,
            host,
            host_error,
            services: None,
            events: None,
            page: Page::Discover,
            addons: Vec::new(),
            config_busy: true,
            install: TextEdit::new(""),
            direct: TextEdit::new(""),
            search: TextEdit::new(""),
            displayed_query: String::new(),
            kind: "movie".into(),
            catalog: None,
            metas: Arc::default(),
            providers: Arc::default(),
            back: Vec::new(),
            forward: Vec::new(),
            route_key: 0,
            next_route_key: 0,
            anchor: None,
            reveal: None,
            detail: None,
            selected_video_id: None,
            episode_offset: 0,
            episode_season: None,
            season_menu_open: false,
            streams: Vec::new(),
            stream_provider: None,
            source_menu_open: false,
            warnings: Vec::new(),
            error: None,
            status: String::new(),
            loading: false,
            catalog_id: 0,
            detail_id: 0,
            stream_id: 0,
            play_id: 0,
            playing: false,
            sample: Sample::default(),
            report: None,
            playing_title: String::new(),
            playing_meta_id: None,
            playing_video_id: None,
            volume: 70,
            startup_play: None,
            quit: None,
        }
    }

    fn refresh_artwork(&mut self) {
        let urls = visible_artwork(self);
        if urls != self.artwork_urls {
            if let Some(artwork) = &mut self.artwork {
                artwork.set_visible(&urls);
            }
            self.artwork_urls = urls;
        }
    }

    fn stop(&mut self) {
        self.save_library_progress();
        self.library.playing = None;
        self.refresh_library();
        self.play_id += 1;
        self.playing = false;
        self.host.hide();
        self.send(Control::Stop(self.play_id));
    }

    fn navigate(&mut self, page: Page) {
        if self.page == page {
            return;
        }
        self.remember();
        if self.page == Page::Player {
            self.stop();
        }
        self.page = page;
        self.detail_id += 1;
        self.stream_id += 1;
        self.catalog_id += 1;
        self.loading = false;
        self.error = None;
        if page == Page::Discover && self.catalog.is_some() && self.metas.is_empty() {
            self.fetch_catalog();
        }
        if page == Page::Library {
            self.refresh_library();
        }
    }

    fn route(&self) -> Route {
        Route {
            page: if self.page == Page::Player {
                if self.detail.is_some() {
                    Page::Detail
                } else {
                    Page::Addons
                }
            } else {
                self.page
            },
            kind: self.kind.clone(),
            query: self.displayed_query.clone(),
            catalog: self.catalog.clone(),
            metas: self.metas.clone(),
            providers: self.providers.clone(),
            detail: self.detail.clone(),
            namespace: self.detail_namespace.clone(),
            episode: self.selected_video_id.clone(),
            pending_episode: self.pending_episode.clone(),
            episode_offset: self.episode_offset,
            episode_season: self.episode_season,
            loading: self.loading,
            key: self.route_key,
            anchor: self.anchor.clone(),
            status: self.status.clone(),
            warnings: self.warnings.clone(),
        }
    }

    fn remember(&mut self) {
        if self.back.len() == 32 {
            self.back.remove(0);
        }
        self.back.push(self.route());
        self.forward.clear();
        self.next_route_key += 1;
        self.route_key = self.next_route_key;
        self.anchor = None;
    }

    fn travel(&mut self, forward: bool) {
        let route = if forward {
            self.forward.pop()
        } else {
            self.back.pop()
        };
        let Some(route) = route else {
            return;
        };
        let current = self.route();
        if forward {
            self.back.push(current);
        } else {
            self.forward.push(current);
        }
        if self.page == Page::Player {
            self.stop();
        }
        self.catalog_id += 1;
        self.detail_id += 1;
        self.stream_id += 1;
        self.page = route.page;
        self.kind = route.kind;
        self.displayed_query = route.query.clone();
        self.search = TextEdit::new(route.query);
        self.catalog = route.catalog;
        self.metas = route.metas;
        self.providers = route.providers;
        self.detail = route.detail;
        self.detail_namespace = route.namespace;
        self.selected_video_id = route.episode;
        self.pending_episode = route.pending_episode;
        self.episode_offset = route.episode_offset;
        self.episode_season = route.episode_season;
        self.season_menu_open = false;
        self.route_key = route.key;
        self.anchor = route.anchor;
        self.reveal = self.anchor.clone();
        self.error = None;
        self.status = route.status;
        self.warnings = route.warnings;
        self.streams.clear();
        self.stream_provider = None;
        self.source_menu_open = false;
        self.loading = false;
        match self.page {
            Page::Discover if route.loading => self.fetch_catalog(),
            Page::Detail => {
                if route.loading && self.selected_video_id.is_none() {
                    if let (Some(services), Some(meta)) = (&mut self.services, &self.detail) {
                        self.loading = true;
                        services.detail(
                            self.detail_id,
                            self.addons.clone(),
                            meta.clone(),
                            self.detail_namespace.clone(),
                        );
                    }
                } else if let (Some(meta), Some(id)) = (&self.detail, &self.selected_video_id) {
                    self.find_streams(meta.kind.clone(), id.clone());
                }
                self.register_library_detail();
            }
            Page::Library => self.refresh_library(),
            _ => {}
        }
    }

    fn send(&mut self, control: Control) {
        if let Some(services) = &self.services
            && let Err(error) = services.control(control)
        {
            self.error = Some(error.to_string());
        }
    }

    fn command(&mut self, args: &[&str]) {
        self.send(Control::Command(
            self.play_id,
            args.iter().map(|s| (*s).to_owned()).collect(),
        ));
    }

    fn install(&mut self, input: String) {
        if self.config_busy {
            return;
        }
        self.error = None;
        self.config_busy = true;
        if let Some(services) = &self.services {
            services.install(input);
        }
    }

    fn catalogs(&self) -> Vec<(Addon, Catalog)> {
        self.addons
            .iter()
            .flat_map(|a| {
                a.manifest
                    .catalogs
                    .iter()
                    .filter(|c| {
                        c.kind == self.kind
                            && !c.extra.iter().any(|e| e.is_required && e.name != "search")
                    })
                    .map(|c| (a.clone(), c.clone()))
            })
            .collect()
    }

    fn choose_kind(&mut self, kind: &str) {
        self.remember();
        if self.page == Page::Player {
            self.stop();
        }
        self.page = Page::Discover;
        self.kind = kind.into();
        self.catalog = self.catalogs().into_iter().next();
        self.search = TextEdit::new("");
        self.fetch_catalog();
    }

    fn fetch_catalog(&mut self) {
        self.displayed_query = self.search.text.trim().to_owned();
        if !self.search.text.trim().is_empty() {
            self.fetch_search();
            return;
        }
        self.catalog_id += 1;
        self.metas = Arc::default();
        self.providers = Arc::default();
        self.error = None;
        let Some((addon, catalog)) = self.catalog.clone() else {
            self.loading = false;
            return;
        };
        if self.search.text.trim().is_empty()
            && catalog
                .extra
                .iter()
                .any(|e| e.name == "search" && e.is_required)
        {
            self.status = "Enter a title to search this catalog.".into();
            self.loading = false;
            return;
        }
        self.loading = true;
        self.status = "Loading catalog…".into();
        if let Some(services) = &mut self.services {
            services.catalog(self.catalog_id, addon, catalog, self.search.text.clone());
        }
    }

    fn submit_search(&mut self) {
        self.remember();
        if self.page == Page::Player {
            self.stop();
        }
        self.page = Page::Discover;
        self.detail_id += 1;
        self.stream_id += 1;
        self.fetch_catalog();
    }

    fn fetch_search(&mut self) {
        self.catalog_id += 1;
        self.metas = Arc::default();
        self.providers = Arc::default();
        self.error = None;
        self.warnings.clear();
        self.loading = true;
        self.status = "Searching…".into();
        if let Some(services) = &mut self.services {
            services.search(
                self.catalog_id,
                self.addons.clone(),
                self.search.text.trim().to_owned(),
            );
        }
    }

    fn open_detail(&mut self, meta: Meta) {
        self.pending_episode = None;
        self.detail_namespace = self
            .catalog
            .as_ref()
            .map(|(a, _)| a.manifest.id.clone())
            .unwrap_or_else(|| "unknown".into());
        self.open_detail_from(meta);
    }

    fn open_result(&mut self, index: usize) {
        let Some(meta) = self.metas.get(index).cloned() else {
            return;
        };
        self.anchor = Some(format!("title-{index}"));
        if let Some(addon) = self.providers.get(index) {
            let namespace = addon.manifest.id.clone();
            self.pending_episode = None;
            self.detail_namespace = namespace;
            self.open_detail_from(meta);
        } else {
            self.open_detail(meta);
        }
    }

    fn open_detail_from(&mut self, meta: Meta) {
        self.navigate(Page::Detail);
        self.detail = Some(meta.clone());
        self.selected_video_id = None;
        self.episode_offset = 0;
        self.episode_season = None;
        self.season_menu_open = false;
        self.source_menu_open = false;
        self.library.snapshot = None;
        self.streams.clear();
        self.warnings.clear();
        self.loading = true;
        self.status = "Loading details…".into();
        if let Some(services) = &mut self.services {
            services.detail(
                self.detail_id,
                self.addons.clone(),
                meta,
                self.detail_namespace.clone(),
            );
        }
    }

    fn seasons(&self) -> Vec<Option<u32>> {
        let mut seasons: Vec<_> = self.detail.iter().flat_map(|meta| &meta.videos).map(|video| video.season).collect();
        seasons.sort_by_key(|season| match season {
            Some(0) => (1, 0),
            Some(number) => (0, *number),
            None => (2, 0),
        });
        seasons.dedup();
        seasons
    }

    fn active_season(&self) -> Option<u32> {
        let seasons = self.seasons();
        self.episode_season.filter(|season| seasons.contains(season))
            .or_else(|| seasons.first().copied()).flatten()
    }

    fn season_episode_indices(&self) -> Vec<usize> {
        let season = self.active_season();
        let Some(meta) = &self.detail else { return Vec::new(); };
        let mut indices: Vec<_> = meta.videos.iter().enumerate()
            .filter(|(_, video)| video.season == season).map(|(index, _)| index).collect();
        indices.sort_by_key(|index| meta.videos[*index].episode);
        indices
    }

    fn visible_episode_indices(&self) -> Vec<usize> {
        self.season_episode_indices()[self.episode_range()].to_vec()
    }

    fn select_season(&mut self, season: Option<u32>) {
        self.season_menu_open = false;
        if !self.seasons().contains(&season) || self.active_season() == season { return; }
        self.episode_season = Some(season);
        self.episode_offset = 0;
        self.selected_video_id = None;
        self.stream_id += 1;
        self.streams.clear();
        self.stream_provider = None;
        self.source_menu_open = false;
        self.warnings.clear();
        self.error = None;
        self.loading = false;
        self.status = "Choose an episode to find streams.".into();
        self.register_library_detail();
    }

    fn episode_range(&self) -> std::ops::Range<usize> {
        let count = self.season_episode_indices().len();
        let last = count.saturating_sub(1) / EPISODES_PER_PAGE * EPISODES_PER_PAGE;
        let start = (self.episode_offset / EPISODES_PER_PAGE * EPISODES_PER_PAGE).min(last);
        start..start.saturating_add(EPISODES_PER_PAGE).min(count)
    }

    fn move_episode_page(&mut self, next: bool) {
        let range = self.episode_range();
        self.episode_offset = if next {
            range.start.saturating_add(EPISODES_PER_PAGE)
        } else {
            range.start.saturating_sub(EPISODES_PER_PAGE)
        };
        self.episode_offset = self.episode_range().start;
        if self.episode_offset != range.start {
            self.register_library_detail();
        }
    }

    fn select_episode(&mut self, index: usize) {
        self.anchor = Some(format!("episode-{index}"));
        if let Some(meta) = &self.detail
            && let Some(video) = meta.videos.get(index)
        {
            self.find_streams(meta.kind.clone(), video.id.clone());
        }
    }

    fn find_streams(&mut self, kind: String, id: String) {
        self.stream_id += 1;
        self.source_menu_open = false;
        self.stream_provider = None;
        self.selected_video_id = Some(id.clone());
        self.refresh_library();
        self.streams.clear();
        self.warnings.clear();
        self.error = None;
        self.loading = true;
        self.status = "Loading streams…".into();
        if let Some(services) = &mut self.services {
            services.streams(self.stream_id, self.addons.clone(), kind, id);
        }
    }

    fn play(&mut self, stream: Stream, title: String) {
        if let Err(error) = stream.validate_playback() {
            self.error = Some(error.to_string());
            return;
        }
        self.save_library_progress();
        self.library.playing = self.library_media();
        self.library.last_sample = None;
        self.library.saved_sample = None;
        self.library.last_save = std::time::Instant::now();
        self.playing_meta_id = self
            .detail
            .as_ref()
            .map(|meta| (meta.kind.clone(), meta.id.clone()));
        self.playing_video_id = self.selected_video_id.clone();
        self.play_source(Source::Stream(stream), title);
    }

    fn play_source(&mut self, source: Source, title: String) {
        self.error = None;
        if let Some(error) = self.host_error.lock().ok().and_then(|e| e.clone()) {
            self.error = Some(error);
            return;
        }
        match self.host.target() {
            Ok(target) => {
                self.remember();
                self.play_id += 1;
                self.playing = true;
                self.page = Page::Player;
                self.playing_title = title;
                self.sample = Sample::default();
                self.report = None;
                self.status = "Opening stream · torrent metadata can take up to 90 seconds…".into();
                self.send(Control::Play(self.play_id, target, source));
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn play_direct(&mut self) {
        let stream: Stream =
            serde_json::from_value(serde_json::json!({"url":self.direct.text.trim()})).unwrap();
        if let Err(error) = stream.validate_playback() {
            self.error = Some(error.to_string());
            return;
        }
        self.save_library_progress();
        self.library.playing = None;
        self.playing_meta_id = None;
        self.playing_video_id = None;
        self.play_source(Source::Stream(stream), "Direct stream".into());
    }
}

fn init() -> State {
    State::new(Host::default(), Arc::default())
}

impl Drop for State {
    fn drop(&mut self) {
        self.events.take();
        if let Some(mut services) = self.services.take() {
            services.shutdown();
        }
    }
}

fn headless() -> bool {
    std::env::args().any(|a| {
        matches!(
            a.as_str(),
            "--snap" | "--frames" | "--first-pixels" | "--tree" | "--trace" | "--strip"
        )
    })
}

fn setup(s: &mut State, wake: rune::Waker) {
    if headless() {
        s.config_busy = false;
        return;
    }
    s.artwork = crate::artwork::Artwork::start(wake.clone()).ok();
    match Services::start(wake.clone()) {
        Ok((services, events)) => {
            let path = services.data_dir().join("library.sqlite3");
            let result = std::fs::create_dir_all(services.data_dir())
                .map_err(anyhow::Error::from)
                .and_then(|()| crate::library_worker::Worker::start(path, wake));
            match result {
                Ok(worker) => s.library.worker = Some(worker),
                Err(_) => s.library.error = Some("Cannot start the local library".into()),
            }
            s.services = Some(services);
            s.events = Some(events);
        }
        Err(error) => {
            s.config_busy = false;
            s.error = Some(error.to_string());
        }
    }
}

fn poll(s: &mut State, _: f64, tx: &mut Tx) -> bool {
    if let Some(anchor) = s.reveal.take() {
        if s.page == Page::Discover
            && let Some(index) = anchor
                .strip_prefix("title-")
                .and_then(|index| index.parse::<usize>().ok())
        {
            let columns = s.poster_viewport.map_or(3, |rect| poster_columns(rect[2]));
            tx.scroll_into_view(format!("title-row-{}", index / columns));
        } else {
            tx.scroll_into_view(anchor);
        }
    }
    s.refresh_artwork();
    let mut changed = s.poll_library() | s.artwork.as_mut().is_some_and(|artwork| artwork.poll());
    if !headless() && s.services.is_some() && s.startup_play.is_some() && s.host.target().is_ok() {
        let input = s.startup_play.take().unwrap();
        if input.starts_with("http://") || input.starts_with("https://") {
            s.direct = TextEdit::new(input);
            s.play_direct();
        } else {
            s.save_library_progress();
            s.library.playing = None;
            s.playing_meta_id = None;
            s.playing_video_id = None;
            s.play_source(Source::Local(input.into()), "Local video".into());
        }
        changed = true;
    }
    for _ in 0..64 {
        let event = s.events.as_mut().and_then(|events| events.try_recv().ok());
        let Some(event) = event else {
            break;
        };
        changed = true;
        match event {
            Event::Addons(result) => {
                s.config_busy = false;
                match result {
                    Ok(addons) => {
                        s.addons = addons;
                        if s.catalog.as_ref().is_none_or(|(a, _)| {
                            !s.addons
                                .iter()
                                .any(|current| current.manifest.id == a.manifest.id)
                        }) {
                            s.catalog = s.catalogs().into_iter().next();
                            if s.page == Page::Discover {
                                s.fetch_catalog();
                            }
                        }
                    }
                    Err(error) => s.error = Some(error),
                }
            }
            Event::Catalog(id, result) if id == s.catalog_id && s.page == Page::Discover => {
                s.loading = false;
                match result {
                    Ok(metas) => {
                        s.status = format!("{} titles", metas.len());
                        s.metas = Arc::new(metas);
                    }
                    Err(error) => s.error = Some(error),
                }
            }
            Event::Search(id, results, warnings, done)
                if id == s.catalog_id && s.page == Page::Discover =>
            {
                s.loading = !done;
                s.providers = Arc::new(results.iter().map(|result| result.addon.clone()).collect());
                s.metas = Arc::new(results.into_iter().map(|result| result.meta).collect());
                s.warnings = warnings;
                s.status = format!(
                    "{} titles{}",
                    s.metas.len(),
                    if done {
                        ""
                    } else {
                        " · searching…"
                    }
                );
            }
            Event::Detail(id, result) if id == s.detail_id && s.page == Page::Detail => {
                s.loading = false;
                match result {
                    Ok((meta, namespace)) => {
                        s.detail_namespace = namespace;
                        let movie = meta.kind == "movie";
                        let (kind, id) = (meta.kind.clone(), meta.id.clone());
                        let episode = s.pending_episode.take().filter(|_| !movie);
                        if let Some(index) = episode
                            .as_ref()
                            .and_then(|id| meta.videos.iter().position(|video| &video.id == id))
                        {
                            s.episode_season = Some(meta.videos[index].season);
                            let mut indices: Vec<_> = meta.videos.iter().enumerate()
                                .filter(|(_, video)| video.season == meta.videos[index].season)
                                .map(|(index, _)| index).collect();
                            indices.sort_by_key(|index| meta.videos[*index].episode);
                            let position = indices.iter().position(|value| *value == index).unwrap_or(0);
                            s.episode_offset = position / EPISODES_PER_PAGE * EPISODES_PER_PAGE;
                        }
                        s.selected_video_id = episode.clone();
                        s.detail = Some(meta);
                        s.register_library_detail();
                        s.status = "Choose an episode to find streams.".into();
                        if movie {
                            s.find_streams(kind, id);
                        } else if let Some(episode) = episode {
                            s.find_streams(kind, episode);
                        }
                    }
                    Err(error) => s.error = Some(error),
                }
            }
            Event::Streams(id, streams, warnings, done)
                if id == s.stream_id && s.page == Page::Detail =>
            {
                s.loading = !done;
                s.streams = streams;
                s.warnings = warnings;
                s.status = if !done {
                    format!("{} streams · loading…", s.streams.len())
                } else if s.streams.is_empty() {
                    "No streams found. Install a compatible stream add-on, or try another episode."
                        .into()
                } else {
                    format!("{} streams", s.streams.len())
                };
            }
            Event::Playing(id, path) if id == s.play_id => {
                s.report = Some(path);
                s.status = "Player connected · waiting for media…".into();
                let volume = s.volume.to_string();
                s.command(&["set", "volume", &volume]);
            }
            Event::Metrics(id, sample) if id == s.play_id => {
                s.status = if sample.buffering {
                    "Buffering"
                } else if sample.paused {
                    "Paused"
                } else if sample.position_s.is_none() {
                    "Opening media…"
                } else {
                    "Playing"
                }
                .into();
                s.sample = sample;
                s.observe_library_progress();
            }
            Event::Stopped(id, error) if id == s.play_id => {
                s.finish_library_progress(error.is_none());
                s.library.playing = None;
                s.refresh_library();
                s.playing = false;
                s.host.hide();
                s.status = "Playback stopped".into();
                if error.is_some() {
                    s.error = error;
                }
            }
            Event::PlayerWarning(id, error) if id == s.play_id => s.error = Some(error),
            _ => {}
        }
    }
    changed
}

fn label(value: impl Into<String>) -> El {
    text(value)
        .font(TextStyle::new(13.0, 400.0))
        .wrap()
        .color(tok::theme(true).text_secondary)
}
fn heading(value: impl Into<String>) -> El {
    text(value)
        .wrap()
        .font(TextStyle::new(25.0, 650.0))
        .color(tok::theme(true).text)
}

fn button(
    key: impl Into<String>,
    title: impl Into<String>,
    action: impl Fn(&mut State) + 'static,
) -> El {
    let t = tok::theme(true);
    row()
        .key(key)
        .role("button")
        .label(title.into())
        .h(32.0)
        .px(10.0)
        .center()
        .radius(4.0)
        .border(1.0, edge())
        .fill(panel())
        .hover(t.hover)
        .on_press(action)
}

fn action(
    key: impl Into<String>,
    title: impl Into<String>,
    f: impl Fn(&mut State) + 'static,
) -> El {
    let title = title.into();
    button(key, title.clone(), f).child(
        text(title)
            .truncate()
            .font(type_::LABEL)
            .color(tok::theme(true).text),
    )
}

fn input(
    key: &'static str,
    edit: &TextEdit,
    placeholder: &'static str,
    get: fn(&mut State) -> &mut TextEdit,
) -> El {
    let t = tok::theme(true);
    el().key(format!("{key}-frame"))
        .h(38.0)
        .grow(1.0)
        .px(12.0)
        .center_y()
        .radius(4.0)
        .fill(panel())
        .border(1.0, edge())
        .child(
            text_input(edit, type_::BODY)
                .key(key)
                .w_full()
                .color(t.text)
                .placeholder(placeholder, t.text_secondary)
                .single_line()
                .on_input_key(move |s: &mut State, event, tx| {
                    if event.mods.alt && matches!(event.key, rune::Key::Left | rune::Key::Right) {
                        return on_key(s, event, tx);
                    }
                    // Escape in an editor must not navigate away from the user's text.
                    event.key == rune::Key::Escape
                })
                .on_submit(move |s: &mut State, _: &mut Tx| match key {
                    "search" => s.submit_search(),
                    "manifest" => s.install(s.install.text.clone()),
                    "direct-url" => s.play_direct(),
                    _ => {}
                })
                .on_edit(get),
        )
}

fn scroll(key: impl Into<String>, content: El) -> El {
    col()
        .key(key)
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(content)
}

fn visible_artwork(s: &State) -> Vec<String> {
    let mut urls = Vec::new();
    if s.page == Page::Discover {
        for (index, meta) in s.metas.iter().enumerate() {
            let visible = if s.poster_route == s.route_key && !s.poster_rects.is_empty() {
                s.poster_viewport
                    .zip(s.poster_rects.get(&(index / s.poster_columns)))
                    .is_some_and(|(viewport, rect)| {
                        rect[1] < viewport[1] + viewport[3] && rect[1] + rect[3] > viewport[1]
                    })
            } else {
                index < 12
            };
            if visible
                && let Some(url) = &meta.poster
                && !urls.contains(url)
            {
                urls.push(url.clone());
            }
            if urls.len() == 32 {
                break;
            }
        }
    } else if s.page == Page::Detail
        && let Some(url) = s.detail.as_ref().and_then(|meta| meta.poster.as_ref())
    {
        urls.push(url.clone());
    }
    urls
}

fn poster(s: &State, meta: &Meta, index: usize) -> El {
    let surface = el().w_full().h(246.0).radius(4.0).fill(rune::oklch(
        0.25 + (index % 3) as f32 * 0.025,
        0.035,
        230.0,
    ));
    if let Some(image) = meta
        .poster
        .as_ref()
        .and_then(|url| s.artwork.as_ref()?.get(url))
    {
        surface.pixels_fit(&image, rune::SurfaceFit::Fill)
    } else {
        surface.center().child(
            text(meta.name.chars().next().unwrap_or('S').to_string())
                .font(TextStyle::new(52.0, 300.0))
                .color(accent()),
        )
    }
}

fn discover(s: &State, cx: &Cx<State>) -> El {
    let t = tok::theme(true);
    let viewport_key = format!("titles-scroll-{}", s.route_key);
    let columns = cx.layout(&viewport_key).map_or(3, |rect| poster_columns(rect[2]));
    let mut cards = col().key("title-cards").gap(POSTER_GAP).w_full();
    if s.addons.is_empty() {
        cards = cards.child(col().padding(28.0).gap(16.0).radius(16.0).fill(t.card)
             .child(heading("No catalogs installed"))
             .child(label("Add Cinemeta to browse movies and TV shows."))
            .child(action("setup-cinemeta", if s.config_busy { "Loading configuration…" } else { "Add Cinemeta catalogs" }, |s| s.install(CINEMETA.into())))
             );
    } else if s.metas.is_empty() && !s.loading {
        cards = cards.child(label(
            "No titles found. Try searching for a movie or TV show.",
        ));
    }
    for (row_index, metas) in s.metas.chunks(columns).enumerate() {
        let route_key = s.route_key;
        let mut card_row = col()
            .key(format!("title-row-{row_index}"))
            .w_full()
            .h(POSTER_CARD_HEIGHT)
            .gap(POSTER_GAP)
            .equal_columns(columns)
            .on_layout(move |s: &mut State, rect| {
                if s.poster_route != route_key || s.poster_columns != columns {
                    s.poster_rects.clear();
                }
                s.poster_route = route_key;
                s.poster_columns = columns;
                s.poster_rects.insert(row_index, rect);
                // Poll runs before layout. Queue these URLs now; an idle window
                // might not get another frame until a worker or user wakes it.
                s.refresh_artwork();
            });
        // Keep cheap row-sized placeholders for scroll geometry and history.
        // Only nearby rows need poster handles, text shaping and hit targets.
        let nearby = if s.poster_route == s.route_key && s.poster_columns == columns {
            s.poster_viewport.zip(s.poster_rects.get(&row_index))
                .is_none_or(|(viewport, rect)| {
                    rect[1] < viewport[1] + viewport[3] * 2.0
                        && rect[1] + rect[3] > viewport[1] - viewport[3]
                })
        } else {
            row_index < 4
        };
        if !nearby {
            cards = cards.child(card_row);
            continue;
        }
        for (column, meta) in metas.iter().enumerate() {
            let index = row_index * columns + column;
            card_row = card_row.child(
                col()
                    .key(format!("title-{index}"))
                    .w_full()
                    .min_w(0.0)
                    .gap(8.0)
                    .padding(4.0)
                    .radius(4.0)
                    .fill(panel())
                    .hover(t.hover)
                    .on_press(move |s: &mut State| s.open_result(index))
                    .child(poster(s, meta, index))
                    .child(
                        col()
                            .gap(5.0)
                            .child(
                                text(meta.name.clone())
                                    .w_full()
                                    .truncate()
                                    .font(TextStyle::new(14.0, 550.0))
                                    .color(t.text),
                            )
                            .child(label(if s.displayed_query.is_empty() {
                                meta.year.clone()
                            } else {
                                format!("{}{}{}", meta.year,
                                    if meta.year.is_empty() { "" } else { " · " },
                                    if meta.kind == "series" { "TV show" } else { "Movie" })
                            }).key(format!("title-info-{index}"))),
                    ),
            );
        }
        cards = cards.child(card_row);
    }
    col()
        .key("discover")
        .gap(18.0)
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .child(
            row().gap(10.0).center_y().child(
                col().grow(1.0)
                    .child(heading(if s.displayed_query.is_empty() {
                        if s.kind == "series" { "TV shows" } else { "Movies" }.to_owned()
                    } else {
                        format!("Results for “{}”", s.displayed_query)
                    })),
            ),
        )
        .child(
                    scroll(viewport_key, cards)
                        .on_layout(|s: &mut State, rect| {
                            s.poster_viewport = Some(rect);
                            s.refresh_artwork();
                        }),
        )
}

fn addons(s: &State) -> El {
    let t = tok::theme(true);
    let mut list = col().key("addon-list").gap(12.0);
    for (index, addon) in s.addons.iter().enumerate() {
        let id = addon.manifest.id.clone();
        list = list.child(
            row()
                .key(format!("addon-{index}"))
                .padding(18.0)
                .radius(12.0)
                .fill(t.card)
                .gap(16.0)
                .center_y()
                .child(
                    col()
                        .grow(1.0)
                        .gap(5.0)
                        .child(
                            text(addon.manifest.name.clone())
                                .truncate()
                                .font(TextStyle::new(17.0, 550.0))
                                .color(t.text),
                        )
                        .child(label(format!(
                            "{} catalogs · manifest URL kept private",
                            addon.manifest.catalogs.len()
                        ))),
                )
                .child(action(format!("remove-{index}"), "Remove", move |s| {
                    if s.config_busy {
                        return;
                    }
                    s.config_busy = true;
                    if let Some(services) = &s.services {
                        services.remove(id.clone());
                    }
                })),
        );
    }
    col().key("addons-page").gap(10.0).grow(1.0).min_h(0.0).min_w(0.0)
        .child(heading("Add-ons & direct play"))
        .child(label("Install a Stremio-compatible HTTPS manifest URL, including a configured Torrentio URL."))
        .child(row().gap(8.0)
            .child(input("manifest", &s.install, "https://…/manifest.json", |s| &mut s.install))
            .child(action("install-addon", if s.config_busy { "Working…" } else { "Install add-on" }, |s| s.install(s.install.text.clone()))))
        .child(label("Privacy: configured manifest URLs can contain credentials. They are saved as plaintext in your private app-data addons.json. Do not share that file."))
        .child(row().gap(8.0)
            .child(action("install-cinemeta", "Add Cinemeta", |s| s.install(CINEMETA.into())))
            .child(action("install-torrentio", "Add Torrentio", |s| s.install("https://torrentio.strem.fun/manifest.json".into()))))
        .child(label("Torrentio's default sources use direct torrents. For debrid HTTP streams, paste your configured manifest instead."))
        .child(scroll("addons-scroll", list))
        .child(label("DIRECT PLAYBACK · HTTP / HTTPS"))
        .child(row().gap(8.0)
            .child(input("direct-url", &s.direct, "Paste a direct video URL…", |s| &mut s.direct))
            .child(action("play-direct", "Play URL", State::play_direct)))
        .child(label("Only play media you have permission to access. Torrent playback connects to peers and may upload data."))
}

fn detail(s: &State) -> El {
    let t = tok::theme(true);
    let mut header = col().gap(8.0).w_full();
    let mut episode_panel = None;
    if let Some(meta) = &s.detail {
        let info = std::iter::once(meta.year.as_str())
            .chain(meta.genres.iter().map(String::as_str))
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        header = header
            .child(text(info)
                .font(type_::SECTION).color(accent()))
            .child(heading(meta.name.clone()))
            .child(label(excerpt(&meta.description, 240)));
        header = header.child(library_view::detail_actions(s));
        if meta.kind == "series" {
            let mut episodes = col().key("episodes").gap(5.0).w_full();
            let range = s.episode_range();
            for index in s.visible_episode_indices() {
                let video = &meta.videos[index];
                let selected = s.selected_video_id.as_deref() == Some(video.id.as_str());
                episodes = episodes.child(
                    col().key(format!("episode-{index}")).role("button")
                    .label(format!("Episode {} {}", video.episode.unwrap_or(0), video.title))
                    .padding(12.0).gap(4.0).radius(6.0).hover(t.hover)
                    .on_press(move |s: &mut State| s.select_episode(index))
                    .w_full()
                    .fill(if selected {
                        rune::oklch(0.31, 0.065, 220.0)
                    } else {
                        t.card
                    })
                    .child(text(format!("EPISODE {}{}", video.episode.unwrap_or(0), s.episode_label(&video.id)))
                        .font(type_::SECTION).color(if selected { accent() } else { t.text_secondary }))
                    .child(text(if video.title.is_empty() { format!("Episode {}", video.episode.unwrap_or(0)) } else { video.title.clone() })
                        .wrap().font(TextStyle::new(14.0, 500.0)).color(t.text)),
                );
            }
            let mut pages = row().gap(8.0);
            if range.start > 0 {
                pages = pages.child(action("episodes-prev", "Previous 100", |s| {
                    s.move_episode_page(false);
                }));
            }
            pages = pages.child(label(format!(
                "Episodes {}–{} of {}",
                if range.is_empty() { 0 } else { range.start + 1 },
                range.end,
                s.season_episode_indices().len()
            )));
            if range.end < s.season_episode_indices().len() {
                pages = pages.child(action("episodes-next", "Next 100", |s| {
                    s.move_episode_page(true);
                }));
            }
            episode_panel = Some(
                col()
                    .w(280.0)
                    .min_w(240.0)
                    .min_h(0.0)
                    .gap(8.0)
                    .child(
                        text("EPISODES")
                            .font(type_::SECTION)
                            .color(t.text_secondary),
                    )
                    .child(season_selector(s))
                    .child(scroll(
                        format!("episodes-scroll-{}-{}-{:?}", s.route_key, range.start, s.active_season()),
                        episodes,
                    ))
                    .child(pages),
            );
        }
    }
    let sources = source_picker::view(s);
    let panels = if let Some(episodes) = episode_panel {
        row()
            .grow(1.0)
            .min_h(0.0)
            .min_w(0.0)
            .gap(16.0)
            .child(episodes)
            .child(sources)
    } else {
        row().grow(1.0).min_h(0.0).min_w(0.0).child(sources)
    };
    let header = if let Some(meta) = s.detail.as_ref().filter(|meta| meta.poster.is_some()) {
        row()
            .gap(16.0)
            .child(poster(s, meta, 0).w(120.0).h(180.0))
            .child(header.grow(1.0).min_w(0.0))
    } else {
        header
    };
    col()
        .key("detail-page")
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .gap(16.0)
        .child(header)
        .child(panels)
}

fn season_label(season: Option<u32>) -> String {
    match season {
        Some(0) => "Specials".into(),
        Some(number) => format!("Season {number}"),
        None => "Other episodes".into(),
    }
}

fn season_selector(s: &State) -> El {
    let seasons = s.seasons();
    let current = s.active_season();
    let position = seasons.iter().position(|season| *season == current).unwrap_or(0);
    let mut bar = row().w_full().gap(6.0);
    if position > 0 {
        let previous = seasons[position - 1];
        bar = bar.child(action("season-prev", "‹", move |s| s.select_season(previous)).w(32.0));
    }
    bar = bar.child(action("season-select", format!("{}  ▾", season_label(current)), |s| {
        s.season_menu_open = !s.season_menu_open;
    }).w(0.0).grow(1.0));
    if let Some(next) = seasons.get(position + 1).copied() {
        bar = bar.child(action("season-next", "›", move |s| s.select_season(next)).w(32.0));
    }
    let mut selector = col().w_full().gap(6.0).child(bar);
    if s.season_menu_open {
        let mut choices = col().w_full().gap(4.0);
        for (index, season) in seasons.iter().copied().enumerate() {
            choices = choices.child(action(format!("season-option-{index}"), season_label(season), move |s| {
                s.select_season(season);
            }).w_full().fill(if season == current { rune::oklch(0.30, 0.05, 220.0) } else { panel() }));
        }
        selector = selector.child(col().key("season-menu").w_full().h((seasons.len() as f32 * 36.0).min(144.0))
            .scroll(ScrollSpec { stick_bottom: false }).child(choices));
    }
    selector
}

fn player(s: &State, scale: f32) -> El {
    let t = tok::theme(true);
    let mut surface = el()
        .key("video")
        .w_full()
        .grow(1.0)
        .fill(rune::oklch(0.08, 0.0, 0.0));
    if s.playing {
        surface = surface.on_layout(move |s: &mut State, rect| {
            if s.playing && s.page == Page::Player {
                s.host.place(rect, scale);
            }
        });
    } else {
        surface = surface.child(label("Choose another source to continue."));
    }
    col()
        .key("player-page")
        .grow(1.0)
        .gap(12.0)
        .child(
            row()
                .center_y()
                .gap(16.0)
                .child(
                    text(s.playing_title.clone())
                        .truncate()
                        .font(TextStyle::new(20.0, 550.0))
                        .color(t.text)
                        .grow(1.0),
                )
                .child(action("player-back", "← Sources", |s| {
                    s.travel(false);
                })),
        )
        .child(surface)
        .child(
            row()
                .gap(8.0)
                .center_y()
                .child(action(
                    "pause",
                    if s.sample.paused { "Resume" } else { "Pause" },
                    |s| s.command(&["cycle", "pause"]),
                ))
                .child(action("seek-back", "−10s", |s| {
                    s.command(&["seek", "-10", "relative"])
                }))
                .child(action("seek-forward", "+10s", |s| {
                    s.command(&["seek", "10", "relative"])
                }))
                .child(action("volume-down", "Vol −", |s| {
                    s.volume = s.volume.saturating_sub(10);
                    let v = s.volume.to_string();
                    s.command(&["set", "volume", &v]);
                }))
                .child(label(format!("{}%", s.volume)))
                .child(action("volume-up", "Vol +", |s| {
                    s.volume = (s.volume + 10).min(100);
                    let v = s.volume.to_string();
                    s.command(&["set", "volume", &v]);
                }))
                .child(action("subtitles", "Subtitles ↻", |s| {
                    s.command(&["cycle", "sid"])
                }))
                .child(action("audio", "Audio ↻", |s| {
                    s.command(&["cycle", "aid"])
                }))
                .child(action("stop", "Stop", State::stop)),
        )
        .child(label(format!(
            "{} / {}   ·   cache {}s   ·   drops {}   ·   RAM {} MiB   ·   {}",
            clock(s.sample.position_s),
            clock(s.sample.duration_s),
            metric(s.sample.cache_s),
            s.sample
                .dropped_frames
                .map(|n| n.to_string())
                .unwrap_or_else(|| "—".into()),
            s.sample
                .resident_bytes
                .map(|n| (n / 1024 / 1024).to_string())
                .unwrap_or_else(|| "—".into()),
            s.sample.hw_decoder.as_deref().unwrap_or("decoder pending")
        )))
}

fn view(s: &State, cx: &mut Cx<State>) -> El {
    let mut content = col()
        .key("content")
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .gap(10.0)
        .padding(20.0);
    content = content.child(match s.page {
        Page::Discover => discover(s, cx),
        Page::Library => library_view::view(s),
        Page::Addons => addons(s),
        Page::Detail => detail(s),
        Page::Player => player(s, cx.env().scale),
    });
    if let Some(error) = &s.error {
        content = content.child(
            text(error.clone())
                .key("error")
                .font(type_::BODY)
                .color(rune::oklch(0.78, 0.14, 25.0)),
        );
    }
    if let Some(error) = &s.library.error {
        content = content.child(label(format!("Library: {error}")));
    }
    if s.page == Page::Discover
        && let Some(warning) = s.warnings.first()
    {
        content = content.child(label(format!(
            "Add-on notice: {}{}",
            excerpt(warning, 140),
            if s.warnings.len() > 1 {
                " · more providers reported a problem"
            } else {
                ""
            }
        )));
    }
    if s.page != Page::Detail && !s.status.is_empty() {
        content = content.child(label(s.status.clone()).key("status"));
    }
    let mut sidebar = col()
        .key("sidebar")
        .w(128.0)
        .min_w(128.0)
        .h_full()
        .padding(14.0)
        .gap(8.0)
        .fill(rune::oklch(0.145, 0.018, 240.0))
        .child(
            row()
                .gap(8.0)
                .child(el().key("brand-logo").w(28.0).h(28.0).pixels(&s.logo))
                .child(
                    text("SKRY")
                        .font(TextStyle::new(19.0, 700.0).tracking(2.0))
                        .color(rune::color::hex("#D8ED96").expect("brand color")),
                ),
        )
        .child(el().h(16.0));
    for (key, title, page) in [
        ("nav-discover", "Discover", Page::Discover),
        ("nav-library", "Library", Page::Library),
        ("nav-addons", "Add-ons", Page::Addons),
    ] {
        sidebar = sidebar.child(action(key, title, move |s| s.navigate(page)).w_full().fill(
            if s.page == page {
                rune::oklch(0.29, 0.05, 220.0)
            } else {
                rune::oklch(0.145, 0.018, 240.0)
            },
        ));
        if page == Page::Discover {
            sidebar = sidebar
                .child(
                    action("movies", "Movies", |s| s.choose_kind("movie"))
                        .w_full()
                        .fill(if s.page == Page::Discover && s.kind == "movie" {
                            rune::oklch(0.25, 0.035, 220.0)
                        } else {
                            rune::oklch(0.145, 0.018, 240.0)
                        }),
                )
                .child(
                    action("series", "TV shows", |s| s.choose_kind("series"))
                        .w_full()
                        .fill(if s.page == Page::Discover && s.kind == "series" {
                            rune::oklch(0.25, 0.035, 220.0)
                        } else {
                            rune::oklch(0.145, 0.018, 240.0)
                        }),
                );
        }
    }
    sidebar = sidebar.child(el().grow(1.0));
    row()
        .key("scry")
        .w_full()
        .h_full()
        .fill(rune::oklch(0.17, 0.018, 240.0))
        .child(sidebar)
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .min_h(0.0)
                .h_full()
                .child(
                    row()
                        .key("topbar")
                        .h(68.0)
                        .fill(rune::oklch(0.15, 0.018, 240.0))
                        .px(20.0)
                        .gap(8.0)
                        .center_y()
                        .child(history_button("nav-back", "←", !s.back.is_empty(), false))
                        .child(history_button(
                            "nav-forward",
                            "→",
                            !s.forward.is_empty(),
                            true,
                        ))
                        .child(input(
                            "search",
                            &s.search,
                            "Search movies and series…",
                            |s| &mut s.search,
                        ))
                        .child(action("search-go", "Search", State::submit_search)),
                )
                .child(content),
        )
}

fn history_button(key: &str, title: &str, enabled: bool, forward: bool) -> El {
    let control = row()
        .key(key)
        .h(32.0)
        .w(32.0)
        .center()
        .radius(7.0)
        .label(if forward {
            "Forward (Alt+Right)"
        } else {
            "Back (Alt+Left)"
        })
        .child(
            text(title)
                .font(TextStyle::new(20.0, 500.0))
                .color(if enabled {
                    tok::theme(true).text
                } else {
                    rune::oklch(0.4, 0.01, 275.0)
                }),
        );
    if enabled {
        control
            .role("button")
            .hover(tok::theme(true).hover)
            .on_press(move |s: &mut State| s.travel(forward))
    } else {
        control
    }
}

fn on_key(s: &mut State, event: &rune::KeyEvent, _: &mut Tx) -> bool {
    if event.mods.alt && event.key == rune::Key::Left {
        s.travel(false);
        true
    } else if event.mods.alt && event.key == rune::Key::Right {
        s.travel(true);
        true
    } else if event.key == rune::Key::Escape && matches!(s.page, Page::Detail | Page::Player) {
        s.travel(false);
        true
    } else {
        false
    }
}

fn excerpt(value: &str, max: usize) -> String {
    let mut chars = value.chars();
    let mut result: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        result.push('…');
    }
    result
}
fn metric(value: Option<f64>) -> String {
    value
        .map(|n| format!("{n:.1}"))
        .unwrap_or_else(|| "—".into())
}
fn clock(value: Option<f64>) -> String {
    value
        .map(|n| {
            let s = n.max(0.0) as u64;
            format!("{}:{:02}", s / 60, s % 60)
        })
        .unwrap_or_else(|| "—:—".into())
}

// Explicit tooling scenes only. Normal startup always uses the user's local state.
fn scene_discover(s: &mut State) {
    s.config_busy = false;
    s.page = Page::Discover;
    s.addons = vec![
        serde_json::from_value(serde_json::json!({
            "manifest_url":"https://example.invalid/manifest.json",
            "manifest":{"id":"scene.catalog", "name":"Cinema collection", "catalogs":[
                {"id":"popular","type":"movie","name":"Popular movies"},
                {"id":"classics","type":"movie","name":"Modern classics"}
            ]}
        }))
        .unwrap(),
    ];
    s.catalog = s.catalogs().into_iter().next();
    let names = [
        "The Grand Budapest Hotel",
        "Arrival",
        "Past Lives",
        "Perfect Days",
        "Moonlight",
        "The Secret Life of Walter Mitty",
        "Aftersun",
        "Interstellar",
        "The Holdovers",
        "Dune: Part Two",
        "Little Women",
        "The Wild Robot",
    ];
    s.metas = Arc::new(
        names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                serde_json::from_value(serde_json::json!({
        "id":format!("scene-{index}"), "type":"movie", "name":name, "year":2024 - index % 10,
        "description":"Stories that stay with you."
    })).unwrap()
            })
            .collect(),
    );
    s.status = format!("{} titles", s.metas.len());
}

fn scene_detail(s: &mut State) {
    s.page = Page::Detail;
    s.config_busy = false;
    s.detail_namespace = "scene.catalog".into();
    s.detail = Some(serde_json::from_value(serde_json::json!({
        "id":"scene-series", "type":"series", "name":"The Observatory", "year":"2024–",
        "description":"At a remote mountain observatory, a small team follows a signal that could change everything. A thoughtful story of discovery, doubt, and the people we choose to trust.",
        "videos":(1..=250).map(|index| serde_json::json!({"id":format!("scene-series:1:{index}"),"season":1,"episode":index,"title":format!("Chapter {index}")})).collect::<Vec<_>>()
    })).unwrap());
    s.selected_video_id = Some("scene-series:1:1".into());
    s.status = "2 streams".into();
    s.streams = ["1080p · H.264 · 2.1 GB", "720p · H.264 · 1.2 GB"].iter().map(|title| ("Example provider".into(), serde_json::from_value(serde_json::json!({"url":"https://example.invalid/video.mp4","name":"Direct HTTPS","title":title})).unwrap())).collect();
}

fn scene_library(s: &mut State) {
    s.page = Page::Library;
    s.config_busy = false;
    s.status = "Saved on this device.".into();
    s.library.snapshot = Some(crate::library_worker::Snapshot {
        watchlist: [
            "Arrival",
            "Perfect Days",
            "The Grand Budapest Hotel",
            "Past Lives",
            "Aftersun",
        ]
        .iter()
        .enumerate()
        .map(|(index, title)| scry_library::Media {
            key: scry_library::MediaKey::new(
                "scene",
                scry_library::MediaKind::Movie,
                format!("scene-{index}"),
            ),
            title: (*title).into(),
            parent: None,
        })
        .collect(),
        watched: vec![],
        playlists: vec![],
        items: vec![],
        selected_state: None,
        episode_states: vec![],
    });
}

pub fn run() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut rune_args = Vec::new();
    let mut startup_play = None;
    while let Some(arg) = args.next() {
        if arg == "--play" {
            startup_play = Some(
                args.next()
                    .filter(|v| !v.starts_with("--"))
                    .ok_or_else(|| anyhow::anyhow!("--play requires a URL or local file path"))?,
            );
        } else {
            rune_args.push(arg);
        }
    }
    let host = Host::default();
    let window_host = host.clone();
    let error = Arc::new(Mutex::new(None));
    let window_error = error.clone();
    let app = App::new(AppSpec {
        setup: Some(setup),
        poll: Some(poll),
        on_key: Some(on_key),
        scenes: &[
            ("discover", scene_discover),
            ("detail", scene_detail),
            ("library", scene_library),
        ],
        min_size: Some((880.0, 600.0)),
        ..AppSpec::new("Skry", (1280.0, 820.0), init, view)
    })
    .keep_running();
    let quit = app.handle();
    app.args(rune_args)
        .init(move || {
            let mut state = State::new(host, error);
            state.startup_play = startup_play;
            state.quit = Some(quit);
            state
        })
        .on_window(move |window| {
            if let Err(error) = window_host.attach(window)
                && let Ok(mut current) = window_error.lock()
            {
                *current = Some(error.to_string());
            }
        })
        .on_close(|s, tx| {
            s.save_library_progress();
            let finish_library = s.take_library_shutdown();
            s.host.hide();
            s.events.take();
            if let Some(mut services) = s.services.take() {
                let quit = s.quit.clone();
                services.shutdown_then(move || {
                    finish_library();
                    if let Some(quit) = quit {
                        quit.quit();
                    }
                });
            } else {
                if let Some(quit) = s.quit.clone() {
                    std::thread::spawn(move || {
                        finish_library();
                        quit.quit();
                    });
                } else {
                    std::thread::spawn(finish_library);
                    tx.quit();
                }
            }
        })
        .run();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidebar_keeps_the_logo_and_wordmark_visible_at_minimum_window_size() {
        let mut rt = rune::Runtime::new(init(), view, (880.0, 600.0), rune::Env::default());
        rt.frame(0.0);
        let elements = rt.elements();
        let sidebar = rect(&rt, "sidebar");
        let logo = rect(&rt, "brand-logo");
        let wordmark = elements
            .iter()
            .find(|element| element.text.as_deref() == Some("SKRY"))
            .expect("Skry wordmark")
            .rect;
        assert_eq!([logo[2], logo[3]], [28.0, 28.0]);
        for bounds in [logo, wordmark] {
            assert!(bounds[0] >= sidebar[0] && bounds[1] >= sidebar[1]);
            assert!(bounds[0] + bounds[2] <= sidebar[0] + sidebar[2]);
            assert!(bounds[1] + bounds[3] <= sidebar[1] + sidebar[3]);
        }
        assert!(
            wordmark[0] >= logo[0] + logo[2],
            "wordmark must not overlap the logo"
        );
    }

    fn rect(rt: &rune::Runtime<State>, key: &str) -> [f32; 4] {
        rt.elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some(key))
            .unwrap_or_else(|| panic!("Missing {key}"))
            .rect
    }

    #[test]
    fn explorer_shows_titles_and_years_without_provider_controls_or_repeated_type_labels() {
        let mut state = init();
        scene_discover(&mut state);
        let mut rt = rune::Runtime::new(state, view, (1000.0, 800.0), rune::Env::default());
        rt.frame(0.0);
        let elements = rt.elements();
        let texts: Vec<_> = elements.iter().filter_map(|element| element.text.as_deref()).collect();
        assert!(texts.contains(&"Movies"));
        assert!(!texts.contains(&"Cinema collection"));
        assert!(!texts.contains(&"Popular movies"));
        assert!(!texts.contains(&"Modern classics"));
        assert_eq!(elements.iter().find(|element| element.key.as_deref() == Some("title-info-0"))
            .unwrap().text.as_deref(), Some("2024"));

        rt.update(|s, _| {
            s.displayed_query = "Arrival".into();
        });
        rt.frame(0.0);
        assert_eq!(rt.elements().iter().find(|element| element.key.as_deref() == Some("title-info-0"))
            .unwrap().text.as_deref(), Some("2024 · Movie"));
    }

    #[test]
    fn enter_in_search_submits_from_other_pages_and_keeps_browse_history() {
        let mut state = init();
        state.page = Page::Library;
        state.config_busy = false;
        state.search = TextEdit::new("Arrival");
        let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
        rt.frame(0.0);
        tap(&mut rt, "search");
        rt.key(rune::KeyEvent {
            key: rune::Key::Enter,
            mods: rune::Mods::default(),
        });
        rt.frame(0.0);
        assert_eq!(rt.state.page, Page::Discover);
        assert_eq!(rt.state.displayed_query, "Arrival");
        assert!(rt.state.loading);
        rt.update(|s, _| s.travel(false));
        assert_eq!(rt.state.page, Page::Library);
        assert_eq!(rt.state.search.text, "");
        rt.update(|s, _| s.travel(true));
        assert_eq!(rt.state.search.text, "Arrival");
        assert!(
            rt.state.loading,
            "Canceled searches must restart on Forward"
        );
    }

    #[test]
    fn focused_search_forwards_alt_history_keys_but_keeps_escape_local() {
        let mut state = init();
        state.navigate(Page::Addons);
        let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
        rt.on_key = Some(on_key);
        rt.frame(0.0);
        tap(&mut rt, "search");
        for (key, expected) in [
            (rune::Key::Left, Page::Discover),
            (rune::Key::Right, Page::Addons),
        ] {
            rt.key(rune::KeyEvent {
                key,
                mods: rune::Mods {
                    alt: true,
                    ..Default::default()
                },
            });
            rt.frame(0.0);
            assert_eq!(rt.state.page, expected);
        }
        rt.update(|s, _| s.navigate(Page::Detail));
        tap(&mut rt, "search");
        rt.key(rune::KeyEvent {
            key: rune::Key::Escape,
            mods: Default::default(),
        });
        rt.frame(0.0);
        assert_eq!(rt.state.page, Page::Detail);
    }

    #[test]
    fn episode_wheel_is_bounded_and_sources_and_toolbar_stay_visible() {
        let mut state = init();
        state.page = Page::Detail;
        state.detail = Some(series(250));
        let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
        rt.frame(0.0);
        let toolbar = rect(&rt, "topbar");
        let viewport = rect(&rt, "episodes-scroll-0-0-Some(1)");
        let sources = rect(&rt, "sources-scroll-0");
        let first = rect(&rt, "episode-0");
        assert!(viewport[3] > 100.0 && viewport[1] + viewport[3] <= 600.0);
        assert!(sources[0] >= viewport[0] + viewport[2]);
        rt.wheel(viewport[0] + 20.0, viewport[1] + 20.0, -100_000.0);
        for _ in 0..120 {
            rt.frame(1.0 / 60.0);
        }
        let last = rect(&rt, "episode-99");
        assert!(
            rect(&rt, "episode-0")[1] < first[1] - 100.0,
            "first {first:?}, after {:?}, viewport {viewport:?}, last {last:?}",
            rect(&rt, "episode-0")
        );
        assert!(
            last[1] >= viewport[1] && last[1] + last[3] <= viewport[1] + viewport[3] + 1.0,
            "Last episode must be reachable: {last:?} in {viewport:?}"
        );
        assert_eq!(rect(&rt, "topbar"), toolbar);
        assert_eq!(rect(&rt, "sources-scroll-0"), sources);
    }

    #[test]
    fn large_catalog_only_lays_out_nearby_cards() {
        let mut state = init();
        scene_discover(&mut state);
        state.metas = Arc::new(
            (0..1000).map(|index| {
                let mut meta = state.metas[index % state.metas.len()].clone();
                meta.id = format!("title-{index}");
                meta.name = format!("Catalog title {index}");
                meta
            }).collect(),
        );
        let started = std::time::Instant::now();
        let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
        rt.poll = Some(poll);
        for _ in 0..5 {
            rt.frame(1.0 / 60.0);
        }
        let mounted = rt.elements().iter().filter(|element| {
            !element.exiting && element.key.as_deref().is_some_and(|key| {
                key.strip_prefix("title-").is_some_and(|index| index.parse::<usize>().is_ok())
            })
        }).count();
        eprintln!("catalog: mounted={mounted}, settle={:?}, memory={:?}",
            started.elapsed(), scry_playback::telemetry::memory());
        assert!(mounted < 100, "Only nearby cards should be laid out, got {mounted}");
        let viewport = rect(&rt, "titles-scroll-0");
        rt.wheel(viewport[0] + 30.0, viewport[1] + 30.0, -1_000_000.0);
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        let last = rect(&rt, "title-999");
        assert!(last[1] >= viewport[1] && last[1] + last[3] <= viewport[1] + viewport[3] + 1.0);
        tap(&mut rt, "title-999");
        tap(&mut rt, "nav-back");
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        let restored = rect(&rt, "title-999");
        assert!(restored[1] >= viewport[1] && restored[1] + restored[3] <= viewport[1] + viewport[3] + 1.0);
    }

    #[test]
    fn idle_scroll_queues_newly_visible_posters_without_an_extra_wake() {
        let mut state = init();
        scene_discover(&mut state);
        state.metas = Arc::new((0..12).map(|index| {
            let mut meta = state.metas[index % state.metas.len()].clone();
            meta.poster = Some(format!("https://example.invalid/poster-{index}.jpg"));
            meta
        }).collect());
        let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
        rt.poll = Some(poll);
        for _ in 0..120 { rt.frame(1.0 / 60.0); }
        let url = "https://example.invalid/poster-11.jpg".to_owned();
        assert!(!rt.state.artwork_urls.contains(&url));
        let viewport = rect(&rt, "titles-scroll-0");
        rt.wheel(viewport[0] + 30.0, viewport[1] + 30.0, -250.0);
        for _ in 0..20 {
            let moving = rt.frame(1.0 / 60.0);
            if !moving && !rt.has_pending_input() { break; }
        }
        assert!(visible_artwork(&rt.state).contains(&url), "Fixture must reveal the last row");
        assert!(rt.state.artwork_urls.contains(&url), "Visible posters must be queued before the window goes idle");
    }

    #[test]
    fn poster_grid_scrolls_to_last_title_and_back_reveals_the_opened_card() {
        let mut state = init();
        scene_discover(&mut state);
        state.metas = Arc::new(
            (0..40)
                .map(|index| {
                    let mut meta = state.metas[index % state.metas.len()].clone();
                    meta.id = format!("title-{index}");
                    meta.poster = Some(format!("https://example.invalid/poster-{index}.jpg"));
                    meta
                })
                .collect(),
        );
        let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
        rt.poll = Some(poll);
        for _ in 0..5 {
            rt.frame(1.0 / 60.0);
        }
        let toolbar = rect(&rt, "topbar");
        let viewport = rect(&rt, "titles-scroll-0");
        assert!(
            visible_artwork(&rt.state).len() < 12,
            "Do not request every result's artwork"
        );
        assert!(
            rect(&rt, "title-2")[0] + rect(&rt, "title-2")[2] <= viewport[0] + viewport[2] + 1.0
        );
        rt.wheel(viewport[0] + 30.0, viewport[1] + 30.0, -100_000.0);
        for _ in 0..20 {
            rt.frame(1.0 / 60.0);
        }
        let last = rect(&rt, "title-39");
        let artwork = visible_artwork(&rt.state);
        assert!(artwork.contains(&"https://example.invalid/poster-39.jpg".to_owned()));
        assert!(!artwork.contains(&"https://example.invalid/poster-0.jpg".to_owned()));
        assert!(last[1] >= viewport[1] && last[1] + last[3] <= viewport[1] + viewport[3] + 1.0);
        assert!(!rt.elements().iter().any(|element| !element.exiting && element.key.as_deref() == Some("title-0")), "Distant cards should be unmounted");
        assert_eq!(rect(&rt, "topbar"), toolbar);
        tap(&mut rt, "title-39");
        assert_eq!(rt.state.detail.as_ref().unwrap().id, "title-39");
        tap(&mut rt, "nav-back");
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        let restored = rect(&rt, "title-39");
        assert!(
            restored[1] >= viewport[1]
                && restored[1] + restored[3] <= viewport[1] + viewport[3] + 1.0,
            "Back must reveal the clicked card: {restored:?}"
        );
    }

    #[test]
    fn history_restores_episode_page_and_never_restores_player() {
        let mut state = init();
        state.page = Page::Detail;
        state.detail = Some(series(250));
        state.selected_video_id = Some("tt123:1:201".into());
        state.episode_offset = 200;
        state.navigate(Page::Library);
        state.travel(false);
        assert_eq!(state.episode_range(), 200..250);
        assert_eq!(state.selected_video_id.as_deref(), Some("tt123:1:201"));
        state.page = Page::Player;
        state.navigate(Page::Addons);
        state.travel(false);
        assert_eq!(state.page, Page::Detail);
        assert!(!state.playing);
        assert!(state.play_id > 0);
    }

    fn settle_library(rt: &mut rune::Runtime<State>, ready: impl Fn(&State) -> bool) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !ready(&rt.state) {
            assert!(
                std::time::Instant::now() < deadline,
                "Library UI did not settle: {:?}",
                rt.state.library.error
            );
            rt.frame(1.0 / 60.0);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        rt.frame(1.0 / 60.0);
    }

    fn tap(rt: &mut rune::Runtime<State>, key: &str) {
        let (x, y) = rt
            .find(key)
            .unwrap_or_else(|| panic!("Missing UI action {key}"));
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
    }

    #[test]
    fn rendered_detail_actions_persist_and_appear_in_the_library() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("library.sqlite3");
        let mut s = init();
        s.page = Page::Detail;
        s.config_busy = false;
        s.detail_namespace = "provider".into();
        s.detail = Some(
            serde_json::from_value(serde_json::json!({
                "id":"tt123", "type":"movie", "name":"A test movie"
            }))
            .unwrap(),
        );
        s.library.worker =
            Some(crate::library_worker::Worker::start(path.clone(), Arc::new(|| {})).unwrap());
        s.register_library_detail();
        let mut rt = rune::Runtime::new(s, view, (1000.0, 800.0), rune::Env::default());
        rt.poll = Some(poll);
        settle_library(&mut rt, |s| {
            s.library
                .snapshot
                .as_ref()
                .is_some_and(|v| v.selected_state.is_some())
        });
        tap(&mut rt, "library-watchlist");
        settle_library(&mut rt, |s| {
            s.library
                .snapshot
                .as_ref()
                .is_some_and(|v| v.selected_state.as_ref().is_some_and(|v| v.watchlist))
        });
        tap(&mut rt, "library-watched");
        settle_library(&mut rt, |s| {
            s.library
                .snapshot
                .as_ref()
                .is_some_and(|v| v.selected_state.as_ref().is_some_and(|v| v.watched.watched))
        });
        tap(&mut rt, "nav-library");
        assert!(rt.find("library-open-0").is_some());
        let library = scry_library::Library::open(path).unwrap();
        let key = scry_library::MediaKey::new("imdb", scry_library::MediaKind::Movie, "tt123");
        assert!(library.is_in_watchlist(&key).unwrap());
        assert!(library.watched_state(&key).unwrap().watched);
        assert_eq!(
            library.get_media(&key).unwrap().unwrap().title,
            "A test movie"
        );
    }

    pub(super) fn series(count: usize) -> Meta {
        serde_json::from_value(serde_json::json!({
            "id": "tt123", "type": "series", "name": "Long show",
            "videos": (1..=count).map(|n| serde_json::json!({
                "id": format!("tt123:1:{n}"), "title": format!("Episode {n}"),
                "season": 1, "episode": n
            })).collect::<Vec<_>>()
        }))
        .unwrap()
    }

    #[test]
    fn episode_201_is_reachable_and_selection_uses_its_absolute_identity() {
        let mut s = init();
        s.detail = Some(series(250));
        assert_eq!(s.episode_range(), 0..100);
        s.move_episode_page(true);
        assert_eq!(s.episode_range(), 100..200);
        s.move_episode_page(true);
        assert_eq!(s.episode_range(), 200..250);
        s.select_episode(s.episode_range().start);
        assert_eq!(s.library_media().unwrap().key.id, "tt123:1:201");
        s.move_episode_page(true);
        assert_eq!(s.episode_range(), 200..250);
        for _ in 0..3 {
            s.move_episode_page(false);
        }
        assert_eq!(s.episode_range(), 0..100);
    }

    fn multi_season_series() -> Meta {
        serde_json::from_value(serde_json::json!({
            "id": "show", "type": "series", "name": "A show",
            "videos": [
                {"id":"show:2:2", "title":"Second finale", "season":2, "episode":2},
                {"id":"show:0:1", "title":"Special", "season":0, "episode":1},
                {"id":"show:1:2", "title":"First finale", "season":1, "episode":2},
                {"id":"show:2:1", "title":"Second premiere", "season":2, "episode":1},
                {"id":"show:1:1", "title":"First premiere", "season":1, "episode":1},
                {"id":"extra", "title":"Unnumbered"}
            ]
        })).unwrap()
    }

    #[test]
    fn season_selector_filters_and_sorts_episodes_without_changing_protocol_identity() {
        let mut state = init();
        state.page = Page::Detail;
        state.detail = Some(multi_season_series());
        assert_eq!(state.seasons(), [Some(1), Some(2), Some(0), None]);
        assert_eq!(state.visible_episode_indices(), [4, 2]);
        let mut rt = rune::Runtime::new(state, view, (1000.0, 800.0), rune::Env::default());
        rt.frame(0.0);
        assert!(rt.find("episode-4").is_some());
        assert!(rt.find("episode-0").is_none());
        tap(&mut rt, "season-next");
        assert_eq!(rt.state.visible_episode_indices(), [3, 0]);
        assert!(rt.find("episode-4").is_none());
        tap(&mut rt, "episode-3");
        assert_eq!(rt.state.selected_video_id.as_deref(), Some("show:2:1"));
        assert_eq!(rt.state.visible_episode_keys().iter().map(|key| key.id.as_str()).collect::<Vec<_>>(), ["show:2:1", "show:2:2"]);
        tap(&mut rt, "season-select");
        for _ in 0..30 { rt.frame(1.0 / 60.0); }
        tap(&mut rt, "season-option-2");
        assert_eq!(rt.state.visible_episode_indices(), [1]);
        assert!(rt.state.selected_video_id.is_none());
        tap(&mut rt, "season-next");
        assert_eq!(rt.state.visible_episode_indices(), [5]);
    }

    #[test]
    fn changing_season_rejects_old_streams_and_history_restores_the_season() {
        let (mut state, tx) = state_with_events();
        state.page = Page::Detail;
        state.detail = Some(multi_season_series());
        state.select_episode(4);
        let old_id = state.stream_id;
        state.select_season(Some(2));
        tx.try_send(Event::Streams(old_id, vec![("Old provider".into(), serde_json::from_value(serde_json::json!({"url":"https://example.invalid/video"})).unwrap())], vec![], true)).unwrap();
        poll(&mut state, 0.0, &mut Tx::default());
        assert!(state.streams.is_empty());
        assert!(!state.loading);
        state.navigate(Page::Addons);
        state.travel(false);
        assert_eq!(state.active_season(), Some(2));
        assert_eq!(state.visible_episode_indices(), [3, 0]);
    }

    #[test]
    fn library_episode_opens_the_correct_season_with_unsorted_metadata() {
        let (mut state, tx) = state_with_events();
        state.pending_episode = Some("show:2:2".into());
        state.open_detail_from(series(0));
        tx.try_send(Event::Detail(state.detail_id, Ok((multi_season_series(), "provider".into())))).unwrap();
        poll(&mut state, 0.0, &mut Tx::default());
        assert_eq!(state.active_season(), Some(2));
        assert_eq!(state.selected_video_id.as_deref(), Some("show:2:2"));
        assert_eq!(state.visible_episode_indices(), [3, 0]);
    }

    #[test]
    fn pending_library_episode_opens_its_page_before_registration() {
        let (mut s, tx) = state_with_events();
        s.episode_offset = 200;
        s.pending_episode = Some("tt123:1:201".into());
        s.open_detail_from(series(0));
        assert_eq!(s.episode_offset, 0);
        tx.try_send(Event::Detail(
            s.detail_id,
            Ok((series(250), "provider".into())),
        ))
        .unwrap();
        poll(&mut s, 0.0, &mut Tx::default());
        assert_eq!(s.episode_range(), 200..250);
        assert_eq!(s.selected_video_id.as_deref(), Some("tt123:1:201"));
        assert_eq!(s.visible_episode_keys().first().unwrap().id, "tt123:1:201");
        assert!(s.pending_episode.is_none());
    }

    #[test]
    fn episode_page_bounds_handle_empty_and_out_of_range_offsets() {
        let mut s = init();
        s.detail = Some(series(0));
        s.episode_offset = usize::MAX;
        assert_eq!(s.episode_range(), 0..0);
        s.detail = Some(series(201));
        assert_eq!(s.episode_range(), 200..201);
        s.move_episode_page(true);
        assert_eq!(s.episode_offset, 200);
    }

    fn state_with_events() -> (State, tokio::sync::mpsc::Sender<Event>) {
        let mut state = init();
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        state.events = Some(rx);
        (state, tx)
    }

    #[test]
    fn search_history_keeps_result_provider_and_rejects_old_search_updates() {
        let (mut state, tx) = state_with_events();
        scene_discover(&mut state);
        let mut provider = state.addons[0].clone();
        provider.manifest.id = "search.provider".into();
        let meta: Meta = serde_json::from_value(
            serde_json::json!({"id":"custom-title", "type":"series", "name":"A search result"}),
        )
        .unwrap();
        state.search = TextEdit::new("A search result");
        state.submit_search();
        let generation = state.catalog_id;
        tx.try_send(Event::Search(
            generation,
            vec![crate::search::SearchResult {
                meta,
                addon: provider,
            }],
            vec![],
            true,
        ))
        .unwrap();
        poll(&mut state, 0.0, &mut Tx::default());
        state.choose_kind("series");
        state.travel(false);
        assert_eq!(state.search.text, "A search result");
        tx.try_send(Event::Search(
            generation,
            vec![],
            vec!["Old search failed".into()],
            true,
        ))
        .unwrap();
        poll(&mut state, 0.0, &mut Tx::default());
        assert_eq!(state.metas.len(), 1);
        assert!(state.warnings.is_empty());
        state.open_result(0);
        assert_eq!(state.detail_namespace, "search.provider");
        assert_eq!(state.detail.as_ref().unwrap().id, "custom-title");
    }

    #[test]
    fn stale_catalog_and_playback_messages_do_not_replace_current_state() {
        let (mut s, tx) = state_with_events();
        s.catalog_id = 2;
        s.play_id = 4;
        s.status = "current".into();
        tx.try_send(Event::Catalog(1, Err("old catalog failed".into())))
            .unwrap();
        tx.try_send(Event::Stopped(3, Some("old player failed".into())))
            .unwrap();
        poll(&mut s, 0.0, &mut Tx::default());
        assert!(s.error.is_none());
        assert_eq!(s.status, "current");
    }

    #[test]
    fn stream_failure_keeps_successful_sources_visible() {
        let (mut s, tx) = state_with_events();
        s.page = Page::Detail;
        s.stream_id = 7;
        let stream: Stream =
            serde_json::from_value(serde_json::json!({"url":"https://example.com/video"})).unwrap();
        tx.try_send(Event::Streams(
            7,
            vec![("Working add-on".into(), stream)],
            vec!["Other add-on failed".into()],
            true,
        ))
        .unwrap();
        poll(&mut s, 0.0, &mut Tx::default());
        assert_eq!(s.streams.len(), 1);
        assert_eq!(s.streams[0].0, "Working add-on");
        assert_eq!(s.warnings, ["Other add-on failed"]);
        assert!(!s.loading);
    }

    #[test]
    fn source_filter_shows_only_the_selected_provider_and_resets_for_another_episode() {
        let mut state = init();
        scene_detail(&mut state);
        let stream = state.streams[0].1.clone();
        state.streams = vec![("Provider A".into(), stream.clone()), ("Provider B".into(), stream)];
        let mut rt = rune::Runtime::new(state, view, (1000.0, 800.0), rune::Env::default());
        rt.frame(0.0);
        tap(&mut rt, "source-select");
        for _ in 0..30 { rt.frame(1.0 / 60.0); }
        tap(&mut rt, "source-provider-1");
        assert!(rt.find("stream-0").is_none());
        assert!(rt.find("stream-1").is_some());
        tap(&mut rt, "source-select");
        tap(&mut rt, "source-all");
        assert!(rt.find("stream-0").is_some());
        assert!(rt.find("stream-1").is_some());
        tap(&mut rt, "source-select");
        tap(&mut rt, "source-provider-0");
        rt.update(|s, _| s.select_episode(1));
        assert!(rt.state.stream_provider.is_none());
        assert!(rt.state.streams.is_empty());
        assert_eq!(rt.state.selected_video_id.as_deref(), Some("scene-series:1:2"));
    }

    #[test]
    fn unsupported_source_card_cannot_start_playback() {
        let mut state = init();
        scene_detail(&mut state);
        state.streams = vec![("Header provider".into(), serde_json::from_value(serde_json::json!({
            "url":"https://example.invalid/video.mp4",
            "behaviorHints":{"proxyHeaders":{"request":{"Authorization":"private"}}}
        })).unwrap())];
        let mut rt = rune::Runtime::new(state, view, (1000.0, 800.0), rune::Env::default());
        rt.frame(0.0);
        tap(&mut rt, "stream-0");
        assert_eq!(rt.state.page, Page::Detail);
        assert!(!rt.state.playing);
        assert_eq!(rt.state.play_id, 0);
        assert!(rt.state.error.is_none());
    }
}
