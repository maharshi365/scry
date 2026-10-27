//! Library presentation and nonblocking worker integration.
mod movie_runtimes;
mod movie_status;
mod posters;
mod progress;
pub(super) mod series_status;
mod tracking;
mod watch_actions;
use super::{
    State,
    controls::action,
    design::icons,
    primitives::{
        button::{Button, Variant, segment, segmented_group},
        surface::empty_state,
        text::{heading, label},
    },
};
use crate::library_worker::{Command, Event as DbEvent, Snapshot, Worker};
#[cfg(test)]
use rune::TextEdit;
use rune::{El, col, row};
use scry_library::{Media, MediaKey, MediaKind};
use std::{collections::VecDeque, time::Instant};

pub(super) struct LibraryUi {
    pub worker: Option<Worker>,
    pub snapshot: Option<Snapshot>,
    pub error: Option<String>,
    watch_generation: u64,
    watch_pending: std::collections::HashMap<MediaKey, u64>,
    pub next_release_at: Option<i64>,
    pub grid_generation: u64,
    generation: u64,
    pending: VecDeque<bool>,
    list: u8,
    offset: u32,
    watching_offset: u32,
    kind: MediaKind,
    pub(in crate::ui) entries: Option<Vec<Media>>,
    pub(in crate::ui) posters: posters::PosterUi,
    pub watching: Vec<(Media, scry_library::Progress)>,
    pub watching_watched: std::collections::HashSet<MediaKey>,
    pub(in crate::ui) up_next: Vec<(Media, Option<scry_library::Progress>)>,
    pub(in crate::ui) up_next_offset: u32,
    pub(in crate::ui) up_next_loaded: bool,
    pub(in crate::ui) up_next_has_more: bool,
    pub(in crate::ui) quick_play: Option<u64>,
    pub(in crate::ui) movies: movie_status::MovieUi,
    pub(in crate::ui) movie_runtimes: movie_runtimes::MovieRuntimeUi,
    pub(in crate::ui) series: series_status::SeriesUi,
    pub(in crate::ui) tracking: tracking::TrackingUi,
    pub next_episode: Option<Media>,
    pub watched_pending: f64,
    pub observed: Option<scry_playback::telemetry::Sample>,
    pub resume_position: Option<f64>,
    pub playing: Option<Media>,
    pub last_sample: Option<(f64, f64)>,
    pub last_save: Instant,
    pub saved_sample: Option<(f64, f64)>,
}

impl Default for LibraryUi {
    fn default() -> Self {
        Self {
            worker: None,
            snapshot: None,
            error: None,
            watch_generation: 0,
            watch_pending: std::collections::HashMap::new(),
            next_release_at: None,
            grid_generation: 0,
            generation: 0,
            pending: VecDeque::new(),
            list: 0,
            offset: 0,
            watching_offset: 0,
            kind: MediaKind::Movie,
            entries: None,
            posters: posters::PosterUi::default(),
            watching: Vec::new(),
            watching_watched: std::collections::HashSet::new(),
            up_next: Vec::new(),
            up_next_offset: 0,
            up_next_loaded: false,
            up_next_has_more: false,
            quick_play: None,
            movies: movie_status::MovieUi::default(),
            movie_runtimes: movie_runtimes::MovieRuntimeUi::default(),
            series: series_status::SeriesUi::default(),
            tracking: tracking::TrackingUi::default(),
            next_episode: None,
            watched_pending: 0.0,
            observed: None,
            resume_position: None,
            playing: None,
            last_sample: None,
            last_save: Instant::now(),
            saved_sample: None,
        }
    }
}

pub(in crate::ui) fn key(namespace: &str, kind: MediaKind, id: &str) -> MediaKey {
    let base = id.split(':').next().unwrap_or("");
    let imdb = base
        .strip_prefix("tt")
        .is_some_and(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()));
    MediaKey::new(
        if imdb {
            "imdb"
        } else if namespace.is_empty() {
            "unknown"
        } else {
            namespace
        },
        kind,
        id,
    )
}

impl State {
    pub(in crate::ui) fn parent_media(&self) -> Option<Media> {
        let meta = self.detail.as_ref()?;
        if meta.id.trim().is_empty() {
            return None;
        }
        let kind = match meta.kind.as_str() {
            "movie" => MediaKind::Movie,
            "series" => MediaKind::Series,
            _ => return None,
        };
        Some(Media {
            key: key(&self.detail_namespace, kind, &meta.id),
            title: if meta.name.trim().is_empty() {
                meta.id.clone()
            } else {
                meta.name.clone()
            },
            parent: None,
        })
    }

    pub(super) fn library_media(&self) -> Option<Media> {
        let parent = self.parent_media()?;
        if parent.key.kind != MediaKind::Series {
            return Some(parent);
        }
        let Some(id) = &self.selected_video_id else {
            return Some(parent);
        };
        if id.trim().is_empty() {
            return None;
        }
        let title = self
            .detail
            .as_ref()?
            .videos
            .iter()
            .find(|v| &v.id == id)
            .map(|v| v.title.clone())
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| id.clone());
        Some(Media {
            key: key(&parent.key.namespace, MediaKind::Episode, id),
            title: format!("{} · {title}", parent.title),
            parent: Some(parent.key),
        })
    }

    pub(in crate::ui) fn db(&mut self, command: Command, refresh: bool) -> bool {
        let Some(worker) = &self.library.worker else {
            self.library.error = Some("Local library is unavailable".into());
            return false;
        };
        match worker.send(command) {
            Ok(()) => {
                self.library.pending.push_back(refresh);
                true
            }
            Err(error) => {
                self.library.error = Some(error.to_string());
                false
            }
        }
    }

    pub(super) fn refresh_library(&mut self) {
        if self.library.worker.is_none() {
            return;
        }
        self.library.generation += 1;
        self.library.movies.generation += 1;
        self.library.movies.requested.clear();
        self.library.series.generation += 1;
        self.library.series.requested.clear();
        let command = Command::Refresh {
            generation: self.library.generation,
            selected: self.library_media().map(|m| m.key),
            episodes: self.visible_episode_keys(),
        };
        if let Some(worker) = &self.library.worker
            && let Err(error) = worker.send(command)
        {
            self.library.error = Some(error.to_string());
        }
        if let Some(worker) = &self.library.worker {
            let generation = self.library.generation;
            let result = worker
                .send(Command::Browse {
                    generation,
                    watched: self.library.list == 1,
                    kind: self.library.kind,
                    offset: self.library.offset,
                })
                .and_then(|()| {
                    worker.send(Command::Overview {
                        generation,
                        watching_offset: self.library.watching_offset,
                    })
                })
                .and_then(|()| {
                    worker.send(Command::UpNext {
                        generation,
                        offset: self.library.up_next_offset,
                    })
                });
            if let Err(error) = result {
                self.library.error = Some(error.to_string());
            }
        }
    }

    pub(super) fn register_library_detail(&mut self) {
        let Some(parent) = self.parent_media() else {
            return;
        };
        if parent.key.kind == MediaKind::Movie {
            let key = parent.key.clone();
            let date = self
                .detail
                .as_ref()
                .and_then(crate::media_metadata::meta_release_timestamp);
            self.library.generation += 1;
            self.db(Command::RegisterRelease(parent, date), true);
            if let Some(meta) = self.detail.clone() {
                self.register_known_runtimes(&key, &meta);
            }
            return;
        }
        let mut entries = vec![parent.clone()];
        if parent.key.kind == MediaKind::Series {
            for video in self
                .visible_episode_indices()
                .into_iter()
                .filter_map(|index| self.detail.as_ref()?.videos.get(index))
                .filter(|v| !v.id.trim().is_empty())
            {
                entries.push(Media {
                    key: key(&parent.key.namespace, MediaKind::Episode, &video.id),
                    title: format!(
                        "{} · {}",
                        parent.title,
                        if video.title.trim().is_empty() {
                            &video.id
                        } else {
                            &video.title
                        }
                    ),
                    parent: Some(parent.key.clone()),
                });
            }
            if let Some(selected) = self.library_media()
                && selected.key.kind == MediaKind::Episode
                && !entries.iter().any(|entry| entry.key == selected.key)
            {
                entries.push(selected);
            }
        }
        // Reject snapshots for the old page while the upsert acknowledgement is pending.
        self.library.generation += 1;
        self.db(Command::Upsert(entries), true);
        if let Some(meta) = self.detail.clone() {
            self.register_known_runtimes(&parent.key, &meta);
        }
    }

    pub(super) fn visible_episode_keys(&self) -> Vec<MediaKey> {
        let Some(parent) = self
            .parent_media()
            .filter(|m| m.key.kind == MediaKind::Series)
        else {
            return Vec::new();
        };
        self.visible_episode_indices()
            .into_iter()
            .filter_map(|index| self.detail.as_ref()?.videos.get(index))
            .filter(|video| !video.id.trim().is_empty())
            .map(|video| key(&parent.key.namespace, MediaKind::Episode, &video.id))
            .collect()
    }

    fn apply_snapshot(&mut self, generation: u64, data: Snapshot) {
        if generation == self.library.generation {
            self.library.snapshot = Some(data);
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "The bounded completion match keeps each feature's generation check beside its database event"
    )]
    pub(super) fn poll_library(&mut self) -> bool {
        let events: Vec<_> = self
            .library
            .worker
            .as_ref()
            .map(|w| w.drain().collect())
            .unwrap_or_default();
        let changed = !events.is_empty();
        let mut refresh = false;
        for event in events {
            match event {
                DbEvent::Guide { generation, data } => self.apply_guide(generation, data),
                DbEvent::GuideError { generation } => self.guide_failed(generation),
                DbEvent::NerdStats { generation, data } => self.apply_stats(generation, data),
                DbEvent::StatsError { generation } => self.stats_failed(generation),
                DbEvent::Achievements(result) => self.apply_achievements(result),
                DbEvent::UpNext {
                    generation,
                    items,
                    has_more,
                } if generation == self.library.generation => {
                    if items.is_empty() && self.library.up_next_offset > 0 {
                        self.library.up_next_offset =
                            self.library.up_next_offset.saturating_sub(200);
                        if let Some(worker) = &self.library.worker {
                            let _ = worker.send(Command::UpNext {
                                generation,
                                offset: self.library.up_next_offset,
                            });
                        }
                    } else {
                        self.library.up_next_has_more = has_more;
                        self.apply_up_next_entries(items);
                    }
                }
                DbEvent::TrackedSeries { generation, items } => {
                    self.apply_tracked_series(generation, items);
                }
                DbEvent::MissingMovieRuntimes { generation, items } => {
                    self.apply_missing_movie_runtimes(generation, items);
                }
                DbEvent::SeriesSynced { changed } => refresh |= changed,
                DbEvent::SeriesStates { generation, states } => {
                    if generation == self.library.series.generation {
                        self.library.series.states = states;
                    }
                }
                DbEvent::MovieStates { generation, states } => {
                    if generation == self.library.movies.generation {
                        self.library.movies.states = states;
                    }
                }
                DbEvent::Ready => {
                    refresh = true;
                    self.request_series_tracking(true);
                }
                DbEvent::Snapshot { generation, data } => self.apply_snapshot(generation, data),
                DbEvent::Browse { generation, items } if generation == self.library.generation => {
                    self.apply_library_entries(items);
                }
                DbEvent::Overview {
                    generation,
                    watching,
                    watching_watched,
                    next_release_at,
                    series,
                } if generation == self.library.generation => {
                    self.apply_watching_entries(watching);
                    self.library.watching_watched = watching_watched.into_iter().collect();
                    self.library.next_release_at = next_release_at;
                    self.calendar.series = series;
                    if self.navigation.page == super::Page::Calendar {
                        self.fetch_calendar();
                    }
                }
                DbEvent::GuideMetadataStored
                | DbEvent::RuntimesStored
                | DbEvent::UpNext { .. }
                | DbEvent::Browse { .. }
                | DbEvent::Overview { .. }
                | DbEvent::Flushed(_) => {}
                DbEvent::Changed => {
                    let requested = self.library.pending.pop_front().unwrap_or(false);
                    refresh |= requested;
                    if requested {
                        self.retry_movie_runtimes();
                        self.request_series_tracking(true);
                    }
                }
                DbEvent::Error(error) => {
                    self.library.pending.pop_front();
                    self.library.error = Some(error);
                    self.library.tracking.busy = false;
                    self.movie_runtimes_failed();
                }
                DbEvent::RequestError(error) => {
                    self.library.error = Some(error);
                    self.library.tracking.busy = false;
                    self.movie_runtimes_failed();
                }
            }
        }
        if refresh {
            self.refresh_library();
        }
        self.request_movie_states();
        self.request_series_states();
        changed
    }

    pub(super) fn episode_watched(&self, id: &str) -> bool {
        let Some(snapshot) = &self.library.snapshot else {
            return false;
        };
        let Some(parent) = self.parent_media() else {
            return false;
        };
        let episode = key(&parent.key.namespace, MediaKind::Episode, id);
        snapshot
            .episode_states
            .iter()
            .any(|(k, state)| *k == episode && state.watched)
    }

    pub(in crate::ui) fn open_library_media(&mut self, media: &Media) {
        let target = media.parent.as_ref().unwrap_or(&media.key);
        self.detail_namespace.clone_from(&target.namespace);
        self.pending_episode = media.parent.as_ref().map(|_| media.key.id.clone());
        let meta = serde_json::from_value(serde_json::json!({
            "id": target.id, "type": if target.kind == MediaKind::Movie { "movie" } else { "series" }, "name": media.title
        }));
        match meta {
            Ok(meta) => self.open_detail_from(meta),
            Err(_) => self.library.error = Some("Cannot open the saved title".into()),
        }
    }

    pub(in crate::ui) fn play_next_saved(&mut self, kind: MediaKind) {
        let Some(media) = self
            .library
            .up_next
            .iter()
            .map(|(media, _)| media)
            .find(|media| media.key.kind == kind)
            .cloned()
        else {
            return;
        };
        self.open_library_media(&media);
        self.library.quick_play = Some(self.detail_id);
    }
}

pub(super) fn up_next_header(s: &State) -> El {
    let mut header = row()
        .key("up-next-heading")
        .w_full()
        .gap(10.0)
        .center_y()
        .child(heading("What to watch next").grow(1.0));
    for (key, title, kind) in [
        ("play-next-movie", "Play next movie", MediaKind::Movie),
        ("play-next-show", "Play next TV show", MediaKind::Episode),
    ] {
        header = header.child(
            Button::new(key, title)
                .enabled(
                    s.library
                        .up_next
                        .iter()
                        .any(|(media, _)| media.key.kind == kind),
                )
                .on_press(move |s: &mut State| s.play_next_saved(kind)),
        );
    }
    header
}

pub(super) fn detail_actions(s: &State) -> El {
    let Some(media) = s.library_media() else {
        return col();
    };
    let state = s
        .library
        .snapshot
        .as_ref()
        .and_then(|data| data.selected_state.as_ref())
        .filter(|v| v.key == media.key);
    let in_watchlist = state.is_some_and(|v| v.watchlist);
    let watched = state.is_some_and(|v| v.watched.watched);
    let mut body = col().gap(8.0);
    let a = media.clone();
    let b = media.clone();
    body = body.child(
        row()
            .gap(8.0)
            .child(
                Button::new(
                    "library-watchlist",
                    if in_watchlist {
                        "Remove from library"
                    } else {
                        "Add to library"
                    },
                )
                .selected(in_watchlist)
                .icon(&super::design::icons::LIBRARY)
                .on_press(move |s: &mut State| {
                    s.db(Command::ToggleWatchlist(a.clone()), true);
                })
                .w(52.0)
                .h(52.0)
                .radius(26.0),
            )
            .child(
                Button::new(
                    "library-watched",
                    if watched {
                        "Mark unwatched"
                    } else {
                        "Mark watched"
                    },
                )
                .selected(watched)
                .icon(&super::design::icons::EYE)
                .enabled(watched || s.can_mark_released(&media))
                .on_press(move |s: &mut State| {
                    s.mark_media_watched(b.clone(), !watched);
                })
                .w(52.0)
                .h(52.0)
                .radius(26.0),
            )
            .child(super::detail::quick_actions(s, s.detail_preview)),
    );
    body
}

pub(super) fn view(s: &State, cx: &rune::Cx<State>) -> El {
    let mut body = col().w_full().gap(12.0).child(heading("Library")).child(
        row()
            .w_full()
            .center_y()
            .gap(16.0)
            .child(
                segmented_group()
                    .child(segment(
                        "library-watchlist-tab",
                        "To watch",
                        s.library.list == 0,
                        |s: &mut State| {
                            s.library.list = 0;
                            s.library.offset = 0;
                            s.library.entries = None;
                            s.refresh_library();
                        },
                    ))
                    .child(segment(
                        "library-watched-tab",
                        "Watched",
                        s.library.list == 1,
                        |s: &mut State| {
                            s.library.list = 1;
                            s.library.offset = 0;
                            s.library.entries = None;
                            s.refresh_library();
                        },
                    )),
            )
            .child(rune::el().grow(1.0))
            .child(
                segmented_group()
                    .child(segment(
                        "library-movies",
                        "Movies",
                        s.library.kind == MediaKind::Movie,
                        |s: &mut State| {
                            s.library.kind = MediaKind::Movie;
                            s.library.offset = 0;
                            s.library.entries = None;
                            s.refresh_library();
                        },
                    ))
                    .child(segment(
                        "library-series-filter",
                        "TV series",
                        s.library.kind == MediaKind::Series,
                        |s: &mut State| {
                            s.library.kind = MediaKind::Series;
                            s.library.offset = 0;
                            s.library.entries = None;
                            s.refresh_library();
                        },
                    )),
            ),
    );
    if let Some(items) = &s.library.entries {
        if items.is_empty() {
            if s.library.offset == 0 {
                return body.grow(1.0).min_h(0.0).child(library_empty(s));
            }
            body = body.child(label("No more titles on this page."));
        }
        body = body.child(super::poster_grid::view(s, cx));
    } else {
        body = body.child(label("Loading local library…"));
    }
    let mut pages = row().gap(8.0);
    if s.library.offset > 0 {
        pages = pages.child(action("library-prev", "Previous", |s| {
            s.library.offset = s.library.offset.saturating_sub(200);
            s.library.entries = None;
            s.refresh_library();
        }));
    }
    if s.library
        .entries
        .as_ref()
        .is_some_and(|items| items.len() == 200)
    {
        pages = pages.child(action("library-next", "Next", |s| {
            s.library.offset = s.library.offset.saturating_add(200);
            s.library.entries = None;
            s.refresh_library();
        }));
    }
    body.grow(1.0).min_h(0.0).child(pages)
}

pub(super) fn grid_key(s: &State) -> String {
    format!(
        "library-scroll-{}-{}-{:?}",
        s.library.list, s.library.offset, s.library.kind
    )
}

fn library_empty(s: &State) -> El {
    let series = s.library.kind == MediaKind::Series;
    let watched = s.library.list == 1;
    let title = match (watched, series) {
        (false, false) => "No movies saved",
        (false, true) => "No TV series saved",
        (true, false) => "No watched movies",
        (true, true) => "No watched TV series",
    };
    let description = if watched {
        "Titles marked as watched will appear here."
    } else {
        "Choose Add to library on a title to track it here."
    };
    empty_state(
        "library-empty",
        title,
        description,
        if watched { &icons::CHECK } else { &icons::ADD },
        row().child(
            Button::new(
                "library-browse",
                if series {
                    "Browse TV series"
                } else {
                    "Browse movies"
                },
            )
            .variant(Variant::Primary)
            .on_press(move |s: &mut State| {
                s.choose_kind(if series { "series" } else { "movie" });
            }),
        ),
    )
    .grow(0.0)
    .h(306.0)
    .min_h(306.0)
    .py(24.0)
}

pub(super) fn watching(s: &State, cx: &rune::Cx<State>) -> El {
    let mut body = col().w_full().gap(12.0);
    if !s.library.watching.is_empty() || s.library.up_next.is_empty() {
        body = body.child(heading("Currently watching").key("watching-heading"));
    }
    let mut pages = row().gap(8.0);
    if s.library.watching_offset > 0 {
        pages = pages.child(action("watching-prev", "Previous", |s| {
            s.library.watching_offset = s.library.watching_offset.saturating_sub(200);
            s.refresh_library();
        }));
    }
    if s.library.watching.len() == 200 {
        pages = pages.child(action("watching-next", "Next", |s| {
            s.library.watching_offset = s.library.watching_offset.saturating_add(200);
            s.refresh_library();
        }));
    }
    body.grow(1.0)
        .min_h(0.0)
        .child(super::poster_grid::view(s, cx))
        .child(pages)
}

pub(super) fn watching_empty() -> El {
    empty_state(
        "watching-empty",
        "Nothing in progress",
        "Start a movie or series to save your place here.",
        &icons::PLAY,
        row()
            .gap(10.0)
            .child(
                Button::new("watching-browse-movies", "Browse movies")
                    .variant(Variant::Primary)
                    .on_press(|s: &mut State| s.choose_kind("movie")),
            )
            .child(action("watching-browse-series", "TV series", |s| {
                s.choose_kind("series");
            })),
    )
    .grow(0.0)
    .h(306.0)
    .min_h(306.0)
    .py(24.0)
}

pub(super) fn up_next_pages(s: &State) -> El {
    let mut pages = row().gap(8.0);
    if s.library.up_next_offset > 0 {
        pages = pages.child(action("up-next-prev", "Previous", |s| {
            s.library.up_next_offset = s.library.up_next_offset.saturating_sub(200);
            s.navigation.reveal = Some("up-next-heading".into());
            s.refresh_library();
        }));
    }
    if s.library.up_next_has_more {
        pages = pages.child(action("up-next-next", "Next", |s| {
            s.library.up_next_offset = s.library.up_next_offset.saturating_add(200);
            s.navigation.reveal = Some("up-next-heading".into());
            s.refresh_library();
        }));
    }
    pages
}

#[cfg(test)]
#[expect(
    clippy::float_cmp,
    reason = "Persisted fixture positions are exactly representable integers"
)]
#[expect(
    clippy::unwrap_used,
    reason = "Test fixtures and asserted successful results must fail the test on error"
)]
mod tests {
    use super::*;
    use crate::services::Event;
    use crate::ui::{app::init, events::poll};
    use rune::Tx;
    use scry_addons::Meta;
    use scry_playback::telemetry::Sample;
    use std::sync::Arc;
    use std::time::Duration;
    fn meta(id: &str) -> Meta {
        serde_json::from_value(serde_json::json!({"id": id, "type":"movie", "name":id})).unwrap()
    }

    #[test]
    fn explicit_and_reported_seeks_do_not_add_watched_time() {
        let mut s = init();
        s.player.playing = true;
        s.player.sample = Sample {
            elapsed_s: 10.0,
            position_s: Some(10.0),
            duration_s: Some(100.0),
            ..Sample::default()
        };
        s.observe_library_progress();
        s.player_action(crate::ui::player::actions::PlayerAction::SeekTo(95.0));
        s.player.sample.elapsed_s = 11.0;
        s.observe_library_progress();
        assert_eq!(s.library.watched_pending, 0.0);
        s.player.sample.elapsed_s = 12.0;
        s.player.sample.position_s = Some(96.0);
        s.observe_library_progress();
        assert_eq!(s.library.watched_pending, 1.0);
        s.player.sample.elapsed_s = 13.0;
        s.player.sample.position_s = Some(5.0);
        s.observe_library_progress();
        assert_eq!(s.library.watched_pending, 1.0);
        s.player.sample.elapsed_s = 14.0;
        s.player.sample.position_s = Some(80.0);
        s.observe_library_progress();
        assert_eq!(s.library.watched_pending, 1.0);
    }
    #[test]
    fn captured_identity_and_last_valid_sample_survive_detail_changes_and_eof() {
        let mut s = init();
        s.detail_namespace = "provider".into();
        s.detail = Some(meta("tt123"));
        s.library.playing = s.library_media();
        s.detail = Some(meta("tt456"));
        s.player.sample.position_s = Some(95.0);
        s.player.sample.duration_s = Some(100.0);
        s.observe_library_progress();
        s.player.sample = Sample::default();
        s.observe_library_progress();
        assert_eq!(
            s.library.playing.as_ref().unwrap().key,
            MediaKey::new("imdb", MediaKind::Movie, "tt123")
        );
        assert_eq!(s.library.last_sample, Some((95.0, 100.0)));
        s.player.sample.duration_s = Some(f64::INFINITY);
        s.player.sample.position_s = Some(8.0);
        s.observe_library_progress();
        assert_eq!(s.library.last_sample, Some((95.0, 100.0)));
    }
    #[test]
    fn stale_snapshots_are_ignored() {
        let mut s = init();
        s.library.generation = 2;
        let snapshot = Snapshot {
            selected_state: None,
            episode_states: vec![],
        };
        s.apply_snapshot(1, snapshot);
        assert!(s.library.snapshot.is_none());
    }

    #[test]
    fn episode_registration_is_lazy_and_refresh_tracks_the_visible_page() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("library.sqlite3");
        let mut s = init();
        s.library.worker = Some(Worker::start(path.clone(), Arc::new(|| {})).unwrap());
        s.detail_namespace = "provider".into();
        s.detail = Some(super::super::tests::series(250));
        s.episode_offset = 200;
        // Keep a previously selected episode registered even on another page.
        s.selected_video_id = Some("tt123:1:1".into());
        let old_generation = s.library.generation;
        s.register_library_detail();
        assert!(s.library.generation > old_generation);
        let deadline = Instant::now() + Duration::from_secs(5);
        while s.library.snapshot.is_none() {
            assert!(
                Instant::now() < deadline,
                "page refresh timed out: {:?}",
                s.library.error
            );
            s.poll_library();
            std::thread::sleep(Duration::from_millis(5));
        }
        let states = &s.library.snapshot.as_ref().unwrap().episode_states;
        assert_eq!(
            states
                .iter()
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>(),
            s.visible_episode_keys()
        );
        assert_eq!(states.len(), 50);
        assert_eq!(
            states[0].0,
            MediaKey::new("imdb", MediaKind::Episode, "tt123:1:201")
        );
        let close = s.take_library_shutdown();
        std::thread::spawn(close).join().unwrap();
        let library = scry_library::Library::open(path).unwrap();
        let parent = MediaKey::new("imdb", MediaKind::Series, "tt123");
        let episodes = library
            .list_episodes(&parent, scry_library::Page::new(100, 0).unwrap())
            .unwrap();
        assert_eq!(episodes.len(), 51);
        assert!(episodes.iter().any(|media| media.key.id == "tt123:1:1"));
        assert!(!episodes.iter().any(|media| media.key.id == "tt123:1:200"));
    }

    #[test]
    fn movies_have_no_visible_episode_keys() {
        let mut s = init();
        s.detail = Some(meta("tt123"));
        assert!(s.visible_episode_keys().is_empty());
    }

    #[test]
    fn episode_labels_match_the_full_canonical_visible_key() {
        let mut s = init();
        s.detail_namespace = "provider".into();
        s.detail = Some(
            serde_json::from_value(serde_json::json!({
                "id": "tt123", "type": "series", "name": "Show",
                "videos": [{"id": "custom-episode", "title": "Episode"}]
            }))
            .unwrap(),
        );
        let episode = s.visible_episode_keys().remove(0);
        assert_eq!(episode.namespace, "imdb");
        s.library.snapshot = Some(Snapshot {
            selected_state: None,
            episode_states: vec![(
                MediaKey::new("other", MediaKind::Episode, &episode.id),
                scry_library::WatchedState { watched: true },
            )],
        });
        assert!(!s.episode_watched(&episode.id));
        s.library.snapshot.as_mut().unwrap().episode_states[0].0 = episode.clone();
        assert!(s.episode_watched(&episode.id));
    }

    #[test]
    fn natural_eof_clears_resume_without_counting_unwatched_time() {
        for outcome in ["eof", "stop", "failure"] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("library.sqlite3");
            let mut s = init();
            s.library.worker = Some(Worker::start(path.clone(), Arc::new(|| {})).unwrap());
            s.detail = Some(meta("tt123"));
            s.library.playing = s.library_media();
            s.player.playing = true;
            s.player.sample.position_s = Some(3.0);
            s.player.sample.duration_s = Some(10.0);
            s.library.last_sample = Some((3.0, 10.0));
            s.library.watched_pending = 3.0;
            if outcome == "stop" {
                s.stop();
            }
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            s.events = Some(rx);
            tx.try_send(Event::Stopped(
                s.player.generation,
                (outcome == "failure").then(|| "failed".into()),
            ))
            .unwrap();
            poll(&mut s, 0.0, &mut Tx::default());
            assert_eq!(s.player.sample.position_s, Some(3.0), "{outcome}");
            assert_eq!(s.player.sample.duration_s, Some(10.0), "{outcome}");
            let close = s.take_library_shutdown();
            std::thread::spawn(close).join().unwrap();
            let library = scry_library::Library::open(path).unwrap();
            let key = MediaKey::new("imdb", MediaKind::Movie, "tt123");
            let progress = library.load_progress(&key).unwrap().unwrap();
            assert_eq!(
                progress.position_seconds,
                if outcome == "eof" { 0.0 } else { 3.0 },
                "{outcome}"
            );
            assert!(!library.watched_state(&key).unwrap().watched, "{outcome}");
        }
    }

    #[test]
    fn direct_playback_does_not_inherit_open_detail_identity() {
        let mut s = init();
        s.detail = Some(meta("tt123"));
        s.detail_namespace = "provider".into();
        s.library.playing = s.library_media();
        s.player.meta_id = Some(("movie".into(), "tt123".into()));
        s.direct = TextEdit::new("https://example.com/video.mp4");
        s.play_direct();
        assert!(s.library.playing.is_none());
        assert!(s.player.meta_id.is_none());
        assert!(s.player.video_id.is_none());
    }

    #[test]
    fn shutdown_flushes_captured_progress_before_worker_drop() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("library.sqlite3");
        let mut s = init();
        s.library.worker = Some(Worker::start(path.clone(), Arc::new(|| {})).unwrap());
        s.detail_namespace = "provider".into();
        s.detail = Some(meta("tt123"));
        s.library.playing = s.library_media();
        s.library.last_sample = Some((96.0, 100.0));
        s.library.watched_pending = 96.0;
        // Fill the bounded input queue. The close path must drain events and
        // retry the final save/barrier, not silently lose them.
        for _ in 0..32 {
            let _ = s
                .library
                .worker
                .as_ref()
                .unwrap()
                .send(Command::Upsert(vec![]));
        }
        let close = s.take_library_shutdown();
        std::thread::spawn(close).join().unwrap();
        let library = scry_library::Library::open(path).unwrap();
        let key = MediaKey::new("imdb", MediaKind::Movie, "tt123");
        assert_eq!(
            library
                .load_progress(&key)
                .unwrap()
                .unwrap()
                .position_seconds,
            0.0
        );
        assert!(library.watched_state(&key).unwrap().watched);
    }
}
