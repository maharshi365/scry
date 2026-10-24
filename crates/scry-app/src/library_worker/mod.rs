//! Bounded, nonblocking UI boundary for the synchronous library.
//!
//! Send parent series with `Upsert` before sending commands for their episodes.
//! Mutations emit `Changed`, never snapshots; the UI chooses when to refresh.
use anyhow::{Result, anyhow};
mod operations;
mod protocol;
mod runtime;

pub use protocol::{Command, Event, MovieState, SelectedState, SeriesState, Snapshot};
use runtime::run;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    thread::{self, JoinHandle},
};

const CAPACITY: usize = 32;

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
            .spawn(move || run(&path, &incoming, &outgoing, &stopping, &wake))
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

#[cfg(test)]
#[expect(
    clippy::float_cmp,
    reason = "Persisted fixture positions are exactly representable integers"
)]
#[expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "Worker tests assert successful operations and report unexpected event variants as failures"
)]
mod tests {
    use super::operations::apply;
    use super::*;
    use scry_library::{Library, Media, MediaKey, MediaKind};
    use std::time::{Duration, Instant};

    fn movie() -> Media {
        Media {
            key: MediaKey::new("test", MediaKind::Movie, "one"),
            title: "One".into(),
            parent: None,
        }
    }

    #[test]
    fn movie_states_reject_oversized_batches_and_non_movie_keys() {
        let mut library = Library::memory().unwrap();
        for keys in [
            vec![movie().key; 201],
            vec![MediaKey::new("test", MediaKind::Series, "show")],
            vec![MediaKey::new("test", MediaKind::Episode, "show:1:1")],
        ] {
            assert!(
                apply(
                    &mut library,
                    Command::MovieStates {
                        generation: 1,
                        keys
                    }
                )
                .is_err()
            );
        }
    }

    #[test]
    fn series_states_reject_invalid_kinds_and_oversized_batches() {
        let mut library = Library::memory().unwrap();
        for keys in [
            vec![MediaKey::new("test", MediaKind::Series, "show"); 201],
            vec![movie().key],
            vec![MediaKey::new("test", MediaKind::Episode, "episode")],
        ] {
            assert!(
                apply(
                    &mut library,
                    Command::SeriesStates {
                        generation: 1,
                        keys
                    }
                )
                .is_err()
            );
        }
    }

    #[test]
    fn series_summary_persists_for_library_filters_and_survives_missing_metadata() {
        let mut library = Library::memory().unwrap();
        let parent = Media {
            key: MediaKey::new("test", MediaKind::Series, "show"),
            title: "Show".into(),
            parent: None,
        };
        let episode = Media {
            key: MediaKey::new("test", MediaKind::Episode, "episode"),
            title: "Episode".into(),
            parent: Some(parent.key.clone()),
        };
        library.upsert_media(&parent).unwrap();
        library.upsert_media(&episode).unwrap();
        library.set_watched(&episode.key, true).unwrap();
        apply(
            &mut library,
            Command::SyncSeries {
                key: parent.key.clone(),
                releases: vec![scry_library::EpisodeRelease {
                    media: episode,
                    released_at: 0,
                    season: Some(1),
                    episode: Some(1),
                }],
            },
        )
        .unwrap();
        for _ in 0..3 {
            let Event::SeriesStates { states, .. } = apply(
                &mut library,
                Command::SeriesStates {
                    generation: 1,
                    keys: vec![parent.key.clone()],
                },
            )
            .unwrap() else {
                panic!("Expected series states")
            };
            assert!(states[0].watched);
            assert_eq!(
                library
                    .list_library(true, MediaKind::Series, scry_library::Page::default())
                    .unwrap(),
                vec![parent.clone()]
            );
        }
    }

    #[test]
    fn inventory_sync_rejects_non_series_keys() {
        let mut library = Library::memory().unwrap();
        for key in [
            movie().key,
            MediaKey::new("test", MediaKind::Episode, "episode"),
        ] {
            assert!(
                apply(
                    &mut library,
                    Command::SyncSeries {
                        key,
                        releases: vec![]
                    }
                )
                .is_err()
            );
        }
    }

    fn start(path: PathBuf) -> Worker {
        Worker::start(path, Arc::new(|| {})).unwrap()
    }

    fn receive(worker: &Worker) -> Event {
        worker.events.recv_timeout(Duration::from_secs(10)).unwrap()
    }

    fn refresh(worker: &Worker, generation: u64) -> Snapshot {
        worker
            .send(Command::Refresh {
                generation,
                selected: Some(movie().key),
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
        assert!(refresh(&worker, 1).selected_state.is_none());
        change(&worker, Command::ToggleWatchlist(movie()));
        change(&worker, Command::SavePlayback(movie(), 95.0, 100.0, 95.0));
        let snapshot = refresh(&worker, 10);
        let selected = snapshot.selected_state.unwrap();
        assert!(selected.watched.watched);
        assert!(selected.watchlist);
        assert_eq!(selected.key, movie().key);
        assert_eq!(selected.progress.unwrap().position_seconds, 95.0);

        // Tests may join off the UI thread to prove the connection was closed.
        let handle = worker.thread.take().unwrap();
        drop(worker);
        handle.join().unwrap();
        worker = start(path);
        assert!(matches!(receive(&worker), Event::Ready));
        let selected = refresh(&worker, 12).selected_state.unwrap();
        assert!(selected.watchlist);
        assert!(selected.watched.watched);
    }

    #[test]
    fn flush_acknowledges_final_progress_before_shutdown() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        let mut worker = start(path.clone());
        assert!(matches!(receive(&worker), Event::Ready));
        worker
            .send(Command::SavePlayback(movie(), 73.0, 100.0, 73.0))
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
        let Event::Browse { items, .. } = apply(
            &mut library,
            Command::Browse {
                generation: 1,
                watched: false,
                kind: MediaKind::Movie,
                offset: 200,
            },
        )
        .unwrap() else {
            panic!("Expected snapshot");
        };
        assert_eq!(items.len(), 5);
        assert_eq!(items[0].key.id, "item-4");
        assert_eq!(items[4].key.id, "item-0");
    }

    #[test]
    fn errors_are_async_private_and_do_not_stop_requests() {
        let dir = tempfile::tempdir().unwrap();
        let failed = start(dir.path().join("missing").join("private-token.db"));
        match receive(&failed) {
            Event::RequestError(message) => assert_eq!(message, "Cannot open the local library"),
            event => panic!("Expected open error, got {event:?}"),
        }
        let worker = start(dir.path().join("library.db"));
        assert!(matches!(receive(&worker), Event::Ready));
        worker
            .send(Command::SavePlayback(movie(), 1.0, 0.0, 1.0))
            .unwrap();
        match receive(&worker) {
            Event::Error(message) => assert_eq!(message, "Cannot complete the library request"),
            event => panic!("Expected request error, got {event:?}"),
        }
        assert!(refresh(&worker, 77).selected_state.is_none());
    }

    #[test]
    fn read_failures_are_not_mutation_acknowledgements() {
        let dir = tempfile::tempdir().unwrap();
        let worker = start(dir.path().join("library.db"));
        assert!(matches!(receive(&worker), Event::Ready));
        worker
            .send(Command::MovieStates {
                generation: 1,
                keys: vec![MediaKey::new("test", MediaKind::Series, "show")],
            })
            .unwrap();
        worker.send(Command::SetWatchlist(movie(), true)).unwrap();
        assert!(matches!(receive(&worker), Event::RequestError(_)));
        assert!(matches!(receive(&worker), Event::Changed));
        assert!(refresh(&worker, 2).selected_state.unwrap().watchlist);
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
