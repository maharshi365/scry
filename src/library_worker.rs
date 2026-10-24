//! Bounded, nonblocking UI boundary for the synchronous library.
//!
//! Send parent series with `Upsert` before sending commands for their episodes.
//! Mutations emit `Changed`, never snapshots; the UI chooses when to refresh.
use anyhow::{Result, anyhow};
use scry::library::{
    Library, MAX_PAGE_SIZE, Media, MediaKey, MediaKind, Page, Playlist, Progress, WatchedState,
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

const CAPACITY: usize = 32;

#[derive(Debug)]
pub enum Command {
    /// `offset` applies to watchlist, watched items, and the selected playlist.
    /// The playlist selector contains its first 200 playlists.
    Refresh {
        generation: u64,
        selected: Option<MediaKey>,
        playlist: Option<i64>,
        offset: u32,
        /// Visible episode identities, at most 200. Empty uses stored episodes.
        episodes: Vec<MediaKey>,
    },
    /// Entries are upserted in order (parents before episodes).
    Upsert(Vec<Media>),
    ToggleWatchlist(Media),
    SetWatched(Media, bool),
    ClearOverride(Media),
    CreatePlaylist(String),
    RenamePlaylist(i64, String),
    DeletePlaylist(i64),
    AddToPlaylist(i64, Media),
    RemoveFromPlaylist(i64, MediaKey),
    ReorderPlaylist(i64, Vec<MediaKey>),
    SaveProgress(Media, f64, f64),
    /// A FIFO barrier for shutdown: all preceding writes finish before this ack.
    Flush(u64),
}

#[derive(Debug)]
pub struct SelectedState {
    pub key: MediaKey,
    pub watched: WatchedState,
    pub watchlist: bool,
    pub progress: Option<Progress>,
}

#[derive(Debug)]
pub struct Snapshot {
    pub watchlist: Vec<Media>,
    pub watched: Vec<Media>,
    pub playlists: Vec<Playlist>,
    pub items: Vec<Media>,
    /// `None` also covers a selected key not yet stored locally.
    pub selected_state: Option<SelectedState>,
    /// First 200 registered episodes of the selected series, or the selected
    /// episode's parent series. No provider calls are made here.
    pub episode_states: Vec<(MediaKey, WatchedState)>,
}

#[derive(Debug)]
pub enum Event {
    Ready,
    Snapshot {
        generation: u64,
        data: Snapshot,
    },
    Changed,
    Flushed(u64),
    /// Fixed messages only: database details and media values stay private.
    Error(String),
}

pub struct Worker {
    commands: SyncSender<Command>,
    events: Receiver<Event>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    /// Only thread creation can fail synchronously. Opening the database occurs
    /// on that thread and reports either `Ready` or `Error` asynchronously.
    /// The caller must create the parent directory before starting the worker.
    pub fn start(path: PathBuf, wake: rune::Waker) -> Result<Self> {
        let (commands, incoming) = mpsc::sync_channel(CAPACITY);
        let (outgoing, events) = mpsc::sync_channel(CAPACITY);
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let thread = thread::Builder::new()
            .name("scry-library".into())
            .spawn(move || run(path, incoming, outgoing, stopping, wake))
            .map_err(|_| anyhow!("Cannot start the library worker"))?;
        Ok(Self {
            commands,
            events,
            stop,
            thread: Some(thread),
        })
    }

    /// Never blocks. A full queue rejects the command; it is not executed.
    pub fn send(&self, command: Command) -> Result<()> {
        self.commands
            .try_send(command)
            .map_err(|error| match error {
                TrySendError::Full(_) => anyhow!("Library is busy; try again"),
                TrySendError::Disconnected(_) => anyhow!("Library worker is unavailable"),
            })
    }

    pub fn try_recv(&self) -> std::result::Result<Event, TryRecvError> {
        self.events.try_recv()
    }

    pub fn drain(&self) -> impl Iterator<Item = Event> + '_ {
        self.events.try_iter()
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // Cancel pending work. Never join on the UI thread, including while the
        // completion queue is full or SQLite is finishing an in-flight call.
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread.thread().unpark();
        }
    }
}

fn emit(
    events: &SyncSender<Event>,
    stop: &AtomicBool,
    wake: &rune::Waker,
    mut event: Event,
) -> bool {
    while !stop.load(Ordering::Acquire) {
        match events.try_send(event) {
            Ok(()) => {
                wake();
                return true;
            }
            Err(TrySendError::Disconnected(_)) => return false,
            Err(TrySendError::Full(pending)) => event = pending,
        }
        // Backpressure must not drop acknowledgments or prevent shutdown.
        thread::park_timeout(Duration::from_millis(25));
    }
    false
}

fn run(
    path: PathBuf,
    commands: Receiver<Command>,
    events: SyncSender<Event>,
    stop: Arc<AtomicBool>,
    wake: rune::Waker,
) {
    if stop.load(Ordering::Acquire) {
        return;
    }
    let mut library = match Library::open(path) {
        Ok(library) => library,
        Err(_) => {
            emit(
                &events,
                &stop,
                &wake,
                Event::Error("Cannot open the local library".into()),
            );
            return;
        }
    };
    if !emit(&events, &stop, &wake, Event::Ready) {
        return;
    }
    while !stop.load(Ordering::Acquire) {
        let command = match commands.recv_timeout(Duration::from_millis(250)) {
            Ok(command) => command,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        if stop.load(Ordering::Acquire) {
            break;
        }
        let event = match apply(&mut library, command) {
            Ok(event) => event,
            Err(_) => Event::Error("Cannot complete the library request".into()),
        };
        if !emit(&events, &stop, &wake, event) {
            break;
        }
    }
}

fn apply(library: &mut Library, command: Command) -> Result<Event> {
    match command {
        Command::Refresh {
            generation,
            selected,
            playlist,
            offset,
            episodes,
        } => {
            let page = Page::new(MAX_PAGE_SIZE, 0)?;
            let selected_media = match selected {
                Some(key) => library.get_media(&key)?,
                None => None,
            };
            let series = selected_media
                .as_ref()
                .and_then(|media| match media.key.kind {
                    MediaKind::Series => Some(&media.key),
                    MediaKind::Episode => media.parent.as_ref(),
                    MediaKind::Movie => None,
                });
            let mut episode_states = Vec::new();
            anyhow::ensure!(
                episodes.len() <= MAX_PAGE_SIZE as usize,
                "Too many visible episodes"
            );
            if !episodes.is_empty() {
                for key in episodes {
                    let watched = if library.get_media(&key)?.is_some() {
                        library.watched_state(&key)?
                    } else {
                        WatchedState {
                            watched: false,
                            manual_override: None,
                        }
                    };
                    episode_states.push((key, watched));
                }
            } else if let Some(series) = series {
                for episode in library.list_episodes(series, page)? {
                    let watched = library.watched_state(&episode.key)?;
                    episode_states.push((episode.key, watched));
                }
            }
            let selected_state = match selected_media {
                Some(media) => Some(SelectedState {
                    watched: library.watched_state(&media.key)?,
                    watchlist: library.is_in_watchlist(&media.key)?,
                    progress: library.load_progress(&media.key)?,
                    key: media.key,
                }),
                _ => None,
            };
            return Ok(Event::Snapshot {
                generation,
                data: Snapshot {
                    watchlist: library.list_watchlist(Page::new(MAX_PAGE_SIZE, offset)?)?,
                    watched: library.list_watched(Page::new(MAX_PAGE_SIZE, offset)?)?,
                    playlists: library.list_playlists(page)?,
                    items: match playlist {
                        Some(id) => {
                            library.list_playlist_items(id, Page::new(MAX_PAGE_SIZE, offset)?)?
                        }
                        None => Vec::new(),
                    },
                    selected_state,
                    episode_states,
                },
            });
        }
        Command::Upsert(items) => {
            for item in items {
                library.upsert_media(&item)?;
            }
        }
        Command::ToggleWatchlist(item) => {
            library.upsert_media(&item)?;
            if library.is_in_watchlist(&item.key)? {
                library.remove_from_watchlist(&item.key)?;
            } else {
                library.add_to_watchlist(&item.key)?;
            }
        }
        Command::SetWatched(item, watched) => {
            library.upsert_media(&item)?;
            library.set_watched(&item.key, watched)?;
        }
        Command::ClearOverride(item) => {
            library.upsert_media(&item)?;
            library.clear_watched_override(&item.key)?;
        }
        Command::CreatePlaylist(name) => {
            library.create_playlist(&name)?;
        }
        Command::RenamePlaylist(id, name) => library.rename_playlist(id, &name)?,
        Command::DeletePlaylist(id) => library.delete_playlist(id)?,
        Command::AddToPlaylist(id, item) => {
            library.upsert_media(&item)?;
            library.add_playlist_item(id, &item.key)?;
        }
        Command::RemoveFromPlaylist(id, key) => library.remove_playlist_item(id, &key)?,
        Command::ReorderPlaylist(id, keys) => library.reorder_playlist(id, &keys)?,
        Command::SaveProgress(item, position, duration) => {
            library.upsert_media(&item)?;
            library.save_progress(&item.key, position, duration)?;
        }
        Command::Flush(token) => return Ok(Event::Flushed(token)),
    }
    Ok(Event::Changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use scry::library::MediaKind;
    use std::time::Instant;

    fn movie() -> Media {
        Media {
            key: MediaKey::new("test", MediaKind::Movie, "one"),
            title: "One".into(),
            parent: None,
        }
    }

    fn start(path: PathBuf) -> Worker {
        Worker::start(path, Arc::new(|| {})).unwrap()
    }

    fn receive(worker: &Worker) -> Event {
        worker.events.recv_timeout(Duration::from_secs(10)).unwrap()
    }

    fn refresh(worker: &Worker, generation: u64, playlist: Option<i64>, offset: u32) -> Snapshot {
        worker
            .send(Command::Refresh {
                generation,
                selected: Some(movie().key),
                playlist,
                offset,
                episodes: Vec::new(),
            })
            .unwrap();
        match receive(worker) {
            Event::Snapshot {
                generation: actual,
                data,
            } => {
                assert_eq!(actual, generation);
                data
            }
            event => panic!("Expected snapshot, got {event:?}"),
        }
    }

    fn change(worker: &Worker, command: Command) {
        worker.send(command).unwrap();
        assert!(matches!(receive(worker), Event::Changed));
        assert!(matches!(worker.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn mutations_snapshots_and_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        let mut worker = start(path.clone());
        assert!(matches!(receive(&worker), Event::Ready));
        assert!(refresh(&worker, 1, None, 0).selected_state.is_none());
        change(&worker, Command::ToggleWatchlist(movie()));
        change(&worker, Command::SaveProgress(movie(), 95.0, 100.0));
        change(&worker, Command::CreatePlaylist("Favorites".into()));
        let snapshot = refresh(&worker, 9, None, 0);
        let id = snapshot.playlists[0].id;
        change(&worker, Command::AddToPlaylist(id, movie()));
        let snapshot = refresh(&worker, 10, Some(id), 0);
        assert_eq!(snapshot.watchlist, vec![movie()]);
        assert_eq!(snapshot.watched, vec![movie()]);
        assert_eq!(snapshot.items, vec![movie()]);
        let selected = snapshot.selected_state.unwrap();
        assert!(selected.watched.watched);
        assert!(selected.watchlist);
        assert_eq!(selected.key, movie().key);
        assert_eq!(selected.progress.unwrap().position_seconds, 95.0);
        assert!(refresh(&worker, 11, Some(id), 1).items.is_empty());

        // Tests may join off the UI thread to prove the connection was closed.
        let handle = worker.thread.take().unwrap();
        drop(worker);
        handle.join().unwrap();
        worker = start(path);
        assert!(matches!(receive(&worker), Event::Ready));
        let snapshot = refresh(&worker, 12, Some(id), 0);
        assert_eq!(snapshot.watchlist, vec![movie()]);
        assert_eq!(snapshot.items, vec![movie()]);
        assert!(snapshot.selected_state.unwrap().watched.watched);
    }

    #[test]
    fn flush_acknowledges_final_progress_before_shutdown() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        let mut worker = start(path.clone());
        assert!(matches!(receive(&worker), Event::Ready));
        worker
            .send(Command::SaveProgress(movie(), 73.0, 100.0))
            .unwrap();
        worker.send(Command::Flush(42)).unwrap();
        assert!(matches!(receive(&worker), Event::Changed));
        assert!(matches!(receive(&worker), Event::Flushed(42)));
        let thread = worker.thread.take().unwrap();
        drop(worker);
        thread.join().unwrap();
        let library = Library::open(path).unwrap();
        assert_eq!(
            library
                .load_progress(&movie().key)
                .unwrap()
                .unwrap()
                .position_seconds,
            73.0
        );
    }

    #[test]
    fn watchlist_snapshot_can_reach_items_after_the_first_page() {
        let mut library = Library::memory().unwrap();
        for i in 0..205 {
            let mut item = movie();
            item.key.id = format!("item-{i}");
            apply(&mut library, Command::ToggleWatchlist(item)).unwrap();
        }
        let Event::Snapshot { data, .. } = apply(
            &mut library,
            Command::Refresh {
                generation: 1,
                selected: None,
                playlist: None,
                offset: 200,
                episodes: Vec::new(),
            },
        )
        .unwrap() else {
            panic!("Expected snapshot");
        };
        assert_eq!(data.watchlist.len(), 5);
        assert_eq!(data.watchlist[0].key.id, "item-4");
        assert_eq!(data.watchlist[4].key.id, "item-0");
    }

    #[test]
    fn errors_are_async_private_and_do_not_stop_requests() {
        let dir = tempfile::tempdir().unwrap();
        let failed = start(dir.path().join("missing").join("private-token.db"));
        match receive(&failed) {
            Event::Error(message) => assert_eq!(message, "Cannot open the local library"),
            event => panic!("Expected open error, got {event:?}"),
        }
        let worker = start(dir.path().join("library.db"));
        assert!(matches!(receive(&worker), Event::Ready));
        worker
            .send(Command::RenamePlaylist(-1, "private-token".into()))
            .unwrap();
        match receive(&worker) {
            Event::Error(message) => assert_eq!(message, "Cannot complete the library request"),
            event => panic!("Expected request error, got {event:?}"),
        }
        assert!(refresh(&worker, 77, None, 0).playlists.is_empty());
    }

    #[test]
    fn visible_episode_queries_do_not_depend_on_database_insertion_order() {
        let mut library = Library::memory().unwrap();
        let parent = Media {
            key: MediaKey::new("test", MediaKind::Series, "show"),
            title: "Show".into(),
            parent: None,
        };
        let episode = Media {
            key: MediaKey::new("test", MediaKind::Episode, "episode-1000"),
            title: "Episode 1000".into(),
            parent: Some(parent.key.clone()),
        };
        library.upsert_media(&parent).unwrap();
        library.upsert_media(&episode).unwrap();
        library.set_watched(&episode.key, true).unwrap();
        let unknown = MediaKey::new("test", MediaKind::Episode, "episode-1001");
        let Event::Snapshot { data, .. } = apply(
            &mut library,
            Command::Refresh {
                generation: 1,
                selected: Some(parent.key),
                playlist: None,
                offset: 0,
                episodes: vec![episode.key.clone(), unknown.clone()],
            },
        )
        .unwrap() else {
            panic!("Expected snapshot");
        };
        assert_eq!(data.episode_states[0].0, episode.key);
        assert!(data.episode_states[0].1.watched);
        assert_eq!(data.episode_states[1].0, unknown);
        assert!(!data.episode_states[1].1.watched);
    }

    #[test]
    fn episode_states_follow_selected_series_or_episode_parent() {
        let dir = tempfile::tempdir().unwrap();
        let worker = start(dir.path().join("library.db"));
        assert!(matches!(receive(&worker), Event::Ready));
        let series = Media {
            key: MediaKey::new("test", MediaKind::Series, "series"),
            title: "Series".into(),
            parent: None,
        };
        let episodes: Vec<_> = (0..201)
            .map(|index| Media {
                key: MediaKey::new("test", MediaKind::Episode, index.to_string()),
                title: format!("Episode {index}"),
                parent: Some(series.key.clone()),
            })
            .collect();
        let mut items = vec![series.clone()];
        items.extend(episodes.clone());
        change(&worker, Command::Upsert(items));
        change(&worker, Command::SetWatched(episodes[0].clone(), true));
        for selected in [series.key, episodes[200].key.clone()] {
            worker
                .send(Command::Refresh {
                    generation: 23,
                    selected: Some(selected),
                    playlist: None,
                    offset: 0,
                    episodes: Vec::new(),
                })
                .unwrap();
            match receive(&worker) {
                Event::Snapshot { data, .. } => {
                    assert_eq!(data.episode_states.len(), 200);
                    assert_eq!(data.episode_states[0].0, episodes[0].key);
                    assert!(data.episode_states[0].1.watched);
                    assert!(!data.episode_states[1].1.watched);
                    assert!(
                        !data
                            .episode_states
                            .iter()
                            .any(|(key, _)| key == &episodes[200].key)
                    );
                }
                event => panic!("Expected episode snapshot, got {event:?}"),
            }
        }
    }

    #[test]
    fn full_completion_queue_can_shutdown_without_ui_drain() {
        let dir = tempfile::tempdir().unwrap();
        let mut worker = start(dir.path().join("library.db"));
        // Ready plus these acknowledgments fill the completion queue. More
        // commands then fill the command queue behind the blocked completion.
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut accepted = 0;
        while accepted < CAPACITY * 2 {
            if worker.send(Command::Upsert(Vec::new())).is_ok() {
                accepted += 1;
            } else if Instant::now() >= deadline {
                break;
            } else {
                thread::sleep(Duration::from_millis(5));
            }
        }
        assert_eq!(accepted, CAPACITY * 2);
        assert_eq!(
            worker
                .send(Command::Upsert(Vec::new()))
                .unwrap_err()
                .to_string(),
            "Library is busy; try again"
        );
        let handle = worker.thread.take().unwrap();
        let (done, finished) = mpsc::channel();
        let before = Instant::now();
        drop(worker);
        assert!(before.elapsed() < Duration::from_secs(1));
        thread::spawn(move || {
            handle.join().unwrap();
            done.send(()).unwrap();
        });
        finished.recv_timeout(Duration::from_secs(3)).unwrap();
    }
}
