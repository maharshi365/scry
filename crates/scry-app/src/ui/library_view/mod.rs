//! Library presentation and nonblocking worker integration.
mod progress;
use super::{
    State,
    components::{action, clock, heading, input, label, scroll},
};
use crate::library_worker::{Command, Event as DbEvent, Snapshot, Worker};
use rune::{El, TextEdit, col, row, tok};
use scry_library::{Media, MediaKey, MediaKind};
use std::{collections::VecDeque, time::Instant};

pub(super) struct LibraryUi {
    pub worker: Option<Worker>,
    pub snapshot: Option<Snapshot>,
    pub error: Option<String>,
    generation: u64,
    pending: VecDeque<bool>,
    selected_playlist: Option<i64>,
    list: u8,
    offset: u32,
    name: TextEdit,
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
            generation: 0,
            pending: VecDeque::new(),
            selected_playlist: None,
            list: 0,
            offset: 0,
            name: TextEdit::new(""),
            playing: None,
            last_sample: None,
            last_save: Instant::now(),
            saved_sample: None,
        }
    }
}

fn key(namespace: &str, kind: MediaKind, id: &str) -> MediaKey {
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
    fn parent_media(&self) -> Option<Media> {
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

    fn db(&mut self, command: Command, refresh: bool) -> bool {
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
        let command = Command::Refresh {
            generation: self.library.generation,
            selected: self.library_media().map(|m| m.key),
            episodes: self.visible_episode_keys(),
            playlist: self.library.selected_playlist,
            offset: self.library.offset,
        };
        if let Some(worker) = &self.library.worker
            && let Err(error) = worker.send(command)
        {
            self.library.error = Some(error.to_string());
        }
    }

    pub(super) fn register_library_detail(&mut self) {
        let Some(parent) = self.parent_media() else {
            return;
        };
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
                DbEvent::Ready => refresh = true,
                DbEvent::Snapshot { generation, data } => self.apply_snapshot(generation, data),
                DbEvent::Changed => refresh |= self.library.pending.pop_front().unwrap_or(false),
                DbEvent::Error(error) => {
                    self.library.pending.pop_front();
                    self.library.error = Some(error);
                }
                DbEvent::Flushed(_) => {}
            }
        }
        if refresh {
            self.refresh_library();
        }
        changed
    }

    pub(super) fn episode_label(&self, id: &str) -> &'static str {
        let Some(snapshot) = &self.library.snapshot else {
            return "";
        };
        let Some(parent) = self.parent_media() else {
            return "";
        };
        let episode = key(&parent.key.namespace, MediaKind::Episode, id);
        if snapshot
            .episode_states
            .iter()
            .any(|(k, state)| *k == episode && state.watched)
        {
            " · Watched"
        } else {
            ""
        }
    }

    fn open_library_media(&mut self, media: &Media) {
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

    fn choose_playlist(&mut self, id: i64, name: String) {
        self.library.selected_playlist = Some(id);
        self.library.list = 2;
        self.library.offset = 0;
        self.library.name = TextEdit::new(name);
        self.refresh_library();
    }

    fn move_item(&mut self, index: usize, delta: isize) {
        let Some(snapshot) = &self.library.snapshot else {
            return;
        };
        // Backend reorder is whole-membership. Never submit a truncated page.
        if self.library.offset != 0 || snapshot.items.len() >= 200 {
            return;
        }
        let Some(other) = index.checked_add_signed(delta) else {
            return;
        };
        if other >= snapshot.items.len() {
            return;
        }
        let mut keys: Vec<_> = snapshot.items.iter().map(|m| m.key.clone()).collect();
        keys.swap(index, other);
        if let Some(id) = self.library.selected_playlist {
            self.db(Command::ReorderPlaylist(id, keys), true);
        }
    }
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
    let mut body = col().gap(8.0).child(label(format!(
        "{}{}",
        if watched { "Watched" } else { "Unwatched" },
        if state.is_some_and(|v| v.watched.manual_override.is_some()) {
            " · manual"
        } else {
            " · auto tracking"
        }
    )));
    if let Some(progress) = state.and_then(|v| v.progress) {
        body = body.child(label(format!(
            "Saved: {} / {}",
            clock(Some(progress.position_seconds)),
            clock(Some(progress.duration_seconds))
        )));
    }
    let a = media.clone();
    let b = media.clone();
    let c = media.clone();
    body = body.child(
        row()
            .gap(8.0)
            .child(action(
                "library-watchlist",
                if in_watchlist {
                    "Remove watchlist"
                } else {
                    "Want to watch"
                },
                move |s| {
                    s.db(Command::ToggleWatchlist(a.clone()), true);
                },
            ))
            .child(action(
                "library-watched",
                if watched {
                    "Mark unwatched"
                } else {
                    "Mark watched"
                },
                move |s| {
                    s.db(Command::SetWatched(b.clone(), !watched), true);
                },
            ))
            .child(action("library-auto", "Auto track", move |s| {
                s.db(Command::ClearOverride(c.clone()), true);
            })),
    );
    if s.selected_video_id.is_some() && media.key.kind == MediaKind::Episode {
        body = body.child(row().child(action("library-series", "Series actions", |s| {
            s.selected_video_id = None;
            s.stream_id += 1;
            s.loading = false;
            s.streams.clear();
            s.refresh_library();
        })));
    }
    let mut playlists = col().equal_columns(3).gap(6.0).w_full();
    if let Some(snapshot) = &s.library.snapshot {
        for playlist in &snapshot.playlists {
            let id = playlist.id;
            let item = media.clone();
            playlists = playlists.child(action(
                format!("add-playlist-{id}"),
                format!("+ {}", playlist.name),
                move |s| {
                    s.db(Command::AddToPlaylist(id, item.clone()), true);
                },
            ));
        }
    }
    if s.library
        .snapshot
        .as_ref()
        .is_some_and(|data| !data.playlists.is_empty())
    {
        body = body.child(scroll("detail-playlists", playlists).h(40.0).grow(0.0));
    }
    body
}

#[expect(
    clippy::too_many_lines,
    reason = "Declarative library layout keeps control bindings beside their UI elements"
)]
pub(super) fn view(s: &State) -> El {
    let mut body = col()
        .w_full()
        .gap(10.0)
        .child(heading("Library"))
        .child(
            row()
                .gap(8.0)
                .child(action("library-watchlist-tab", "Want to watch", |s| {
                    s.library.list = 0;
                    s.library.offset = 0;
                    s.refresh_library();
                }))
                .child(action("library-watched-tab", "Watched", |s| {
                    s.library.list = 1;
                    s.library.offset = 0;
                    s.refresh_library();
                })),
        )
        .child(
            row()
                .gap(8.0)
                .child(input(
                    "playlist-name",
                    &s.library.name,
                    "Playlist name…",
                    |s| &mut s.library.name,
                ))
                .child(action("playlist-create", "Create playlist", |s| {
                    let name = s.library.name.text.trim().to_owned();
                    if !name.is_empty() {
                        s.db(Command::CreatePlaylist(name), true);
                    }
                })),
        );
    let Some(snapshot) = &s.library.snapshot else {
        return body.child(label("Loading local library…"));
    };
    for playlist in &snapshot.playlists {
        let id = playlist.id;
        let name = playlist.name.clone();
        body = body.child(action(
            format!("playlist-{id}"),
            format!(
                "Playlist: {}{}",
                playlist.name,
                if s.library.selected_playlist == Some(id) {
                    " · selected"
                } else {
                    ""
                }
            ),
            move |s| s.choose_playlist(id, name.clone()),
        ));
    }
    if let Some(id) = s.library.selected_playlist {
        body = body.child(
            row()
                .gap(8.0)
                .child(action("playlist-rename", "Rename selected", move |s| {
                    let name = s.library.name.text.trim().to_owned();
                    if !name.is_empty() {
                        s.db(Command::RenamePlaylist(id, name), true);
                    }
                }))
                .child(action("playlist-delete", "Delete selected", move |s| {
                    s.library.selected_playlist = None;
                    s.library.list = 0;
                    s.library.offset = 0;
                    s.db(Command::DeletePlaylist(id), true);
                })),
        );
    }
    let items = match s.library.list {
        1 => &snapshot.watched,
        2 => &snapshot.items,
        _ => &snapshot.watchlist,
    };
    body = body.child(label(match s.library.list {
        1 => "Watched",
        2 => "Selected playlist",
        _ => "Want to watch",
    }));
    if items.is_empty() {
        body = body.child(label("No titles here yet. Add titles from Discover."));
    }
    for (index, media) in items.iter().enumerate() {
        let item = media.clone();
        let mut entry = row()
            .w_full()
            .padding(6.0)
            .radius(8.0)
            .fill(tok::theme(true).card)
            .gap(8.0)
            .child(
                action(
                    format!("library-open-{index}"),
                    media.title.clone(),
                    move |s| s.open_library_media(&item),
                )
                .grow(1.0)
                .justify_start()
                .fill(tok::theme(true).card),
            );
        let item = media.clone();
        if s.library.list == 2 {
            if let Some(id) = s.library.selected_playlist {
                entry = entry.child(action(
                    format!("library-remove-{index}"),
                    "Remove",
                    move |s| {
                        s.db(Command::RemoveFromPlaylist(id, item.key.clone()), true);
                    },
                ));
                if s.library.offset == 0 && items.len() < 200 {
                    entry = entry
                        .child(action(format!("library-up-{index}"), "↑", move |s| {
                            s.move_item(index, -1);
                        }))
                        .child(action(format!("library-down-{index}"), "↓", move |s| {
                            s.move_item(index, 1);
                        }));
                }
            }
        } else if s.library.list == 0 {
            entry = entry.child(action(
                format!("library-remove-{index}"),
                "Remove",
                move |s| {
                    s.db(Command::ToggleWatchlist(item.clone()), true);
                },
            ));
        } else {
            entry = entry.child(action(
                format!("library-unwatched-{index}"),
                "Mark unwatched",
                move |s| {
                    s.db(Command::SetWatched(item.clone(), false), true);
                },
            ));
        }
        body = body.child(entry);
    }
    {
        body = body
            .child(
                row()
                    .gap(8.0)
                    .child(action("playlist-prev", "Previous 200", |s| {
                        s.library.offset = s.library.offset.saturating_sub(200);
                        s.refresh_library();
                    }))
                    .child(label(format!("Offset {}", s.library.offset)))
                    .child(action("playlist-next", "Next 200", |s| {
                        s.library.offset = s.library.offset.saturating_add(200);
                        s.refresh_library();
                    })),
            )
            .child(label(
                "Reorder is available for complete playlists under 200 items.",
            ));
    }
    scroll(
        format!(
            "library-scroll-{}-{}-{}",
            s.library.list,
            s.library.offset,
            s.library.selected_playlist.unwrap_or(0)
        ),
        body,
    )
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
            watchlist: vec![],
            watched: vec![],
            playlists: vec![],
            items: vec![],
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
            watchlist: vec![],
            watched: vec![],
            playlists: vec![],
            items: vec![],
            selected_state: None,
            episode_states: vec![(
                MediaKey::new("other", MediaKind::Episode, &episode.id),
                scry_library::WatchedState {
                    watched: true,
                    manual_override: Some(true),
                },
            )],
        });
        assert_eq!(s.episode_label(&episode.id), "");
        s.library.snapshot.as_mut().unwrap().episode_states[0].0 = episode.clone();
        assert_eq!(s.episode_label(&episode.id), " · Watched");
    }

    #[test]
    fn only_natural_eof_completes_a_short_clip_without_changing_telemetry() {
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
                if outcome == "eof" { 10.0 } else { 3.0 },
                "{outcome}"
            );
            assert_eq!(
                library.watched_state(&key).unwrap().watched,
                outcome == "eof",
                "{outcome}"
            );
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
            96.0
        );
        assert!(library.watched_state(&key).unwrap().watched);
    }
}
