//! Bounded runtime discovery for watched movies, independent of visible filters.
use super::{Media, MediaKey, State};
use crate::library_worker::Command;
use scry_addons::Meta;
use std::time::{Duration, Instant};

const RETRY_AFTER: Duration = Duration::from_mins(10);

#[derive(Default)]
pub(in crate::ui) struct MovieRuntimeUi {
    generation: u64,
    busy: bool,
    page_pending: bool,
    after: Option<MediaKey>,
    keys: Vec<MediaKey>,
    last_scan: Option<Instant>,
}

impl State {
    pub(in crate::ui) fn retry_movie_runtimes(&mut self) {
        self.library.movie_runtimes.last_scan = None;
    }

    pub(in crate::ui) fn reset_movie_runtimes(&mut self) {
        let generation = self.library.movie_runtimes.generation + 1;
        self.library.movie_runtimes = MovieRuntimeUi {
            generation,
            ..MovieRuntimeUi::default()
        };
    }

    pub(in crate::ui) fn movie_runtimes_failed(&mut self) {
        if self.library.movie_runtimes.busy {
            self.reset_movie_runtimes();
            self.library.movie_runtimes.last_scan = Some(Instant::now());
        }
    }

    pub(in crate::ui) fn poll_movie_runtimes(&mut self, tx: &mut rune::Tx) {
        if self.library.worker.is_none() || self.services.is_none() || self.addons.is_empty() {
            return;
        }
        let scan = &mut self.library.movie_runtimes;
        if !scan.busy {
            let remaining = scan.last_scan.map_or(Duration::ZERO, |time| {
                RETRY_AFTER.saturating_sub(time.elapsed())
            });
            if !remaining.is_zero() {
                tx.wake_after(remaining);
                return;
            }
            scan.generation += 1;
            scan.busy = true;
            scan.page_pending = true;
            scan.after = None;
            scan.last_scan = Some(Instant::now());
        }
        if scan.page_pending
            && let Some(worker) = &self.library.worker
        {
            if worker
                .send(Command::MissingMovieRuntimes {
                    generation: scan.generation,
                    after: scan.after.clone(),
                })
                .is_ok()
            {
                scan.page_pending = false;
            } else {
                tx.wake_after(Duration::from_millis(100));
            }
        }
    }

    pub(in crate::ui) fn apply_missing_movie_runtimes(
        &mut self,
        generation: u64,
        items: Vec<Media>,
    ) {
        let scan = &mut self.library.movie_runtimes;
        if generation != scan.generation || !scan.busy {
            return;
        }
        scan.keys = items.into_iter().map(|media| media.key).collect();
        if let Some(last) = scan.keys.last() {
            scan.after = Some(last.clone());
        } else {
            scan.busy = false;
            return;
        }
        if let Some(services) = &mut self.services {
            services.movie_runtime_details(generation, self.addons.clone(), scan.keys.clone());
        } else {
            scan.busy = false;
        }
    }

    pub(in crate::ui) fn apply_movie_runtime_details(
        &mut self,
        generation: u64,
        found: Vec<(MediaKey, Option<Meta>)>,
        done: bool,
    ) {
        let scan = &self.library.movie_runtimes;
        if generation != scan.generation || !scan.busy {
            return;
        }
        let runtimes = found
            .into_iter()
            .filter(|(key, _)| scan.keys.contains(key))
            .filter_map(|(key, meta)| Some((key, meta?)))
            .flat_map(|(key, meta)| crate::media_metadata::validated_movie_runtimes(&key, &meta))
            .collect();
        self.queue_known_runtimes(runtimes);
        if done {
            let scan = &mut self.library.movie_runtimes;
            scan.page_pending = scan.keys.len() == 12;
            scan.busy = scan.page_pending;
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "Fixture setup and shutdown must succeed"
)]
mod tests {
    use super::*;
    use scry_library::{Library, MediaKind, StatsPeriod};
    use std::sync::Arc;

    fn meta(id: &str, kind: &str, runtime: &str) -> Meta {
        serde_json::from_value(serde_json::json!({
            "id":id, "type":kind, "name":"Movie", "runtime":runtime
        }))
        .unwrap()
    }

    #[test]
    fn discovered_runtime_updates_watch_hours_without_changing_watched_state() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        let movie = Media {
            key: MediaKey::new("test", MediaKind::Movie, "one"),
            title: "Movie".into(),
            parent: None,
        };
        {
            let mut db = Library::open(&path).unwrap();
            db.upsert_media(&movie).unwrap();
            db.set_watched(&movie.key, true).unwrap();
        }
        let mut s = crate::ui::app::init();
        s.library.worker =
            Some(crate::library_worker::Worker::start(path.clone(), Arc::new(|| {})).unwrap());
        s.library.movie_runtimes.generation = 3;
        s.library.movie_runtimes.busy = true;
        s.library.movie_runtimes.keys = vec![movie.key.clone()];
        s.apply_movie_runtime_details(
            3,
            vec![(movie.key.clone(), Some(meta("one", "movie", "120 min")))],
            true,
        );
        s.flush_known_runtimes();
        std::thread::spawn(s.take_library_shutdown())
            .join()
            .unwrap();
        let db = Library::open(path).unwrap();
        let stats = db.nerd_stats(StatsPeriod::AllTime).unwrap();
        assert!((stats.movie_seconds - 7200.0).abs() < f64::EPSILON);
        assert_eq!(stats.unknown_runtimes, 0);
        assert!(db.watched_state(&movie.key).unwrap().watched);
        assert!(!s.library.movie_runtimes.busy);
    }

    #[test]
    fn stale_or_unmatched_metadata_cannot_supply_a_runtime() {
        let mut s = crate::ui::app::init();
        let key = MediaKey::new("test", MediaKind::Movie, "one");
        s.library.movie_runtimes.generation = 3;
        s.library.movie_runtimes.busy = true;
        s.library.movie_runtimes.keys = vec![key.clone()];
        s.apply_movie_runtime_details(
            2,
            vec![(key.clone(), Some(meta("one", "movie", "120 min")))],
            true,
        );
        for (candidate, metadata) in [
            (key.clone(), Some(meta("other", "movie", "120 min"))),
            (key.clone(), Some(meta("one", "series", "120 min"))),
            (
                MediaKey::new("other", MediaKind::Movie, "one"),
                Some(meta("one", "movie", "120 min")),
            ),
            (key.clone(), Some(meta("one", "movie", "unknown"))),
            (key, None),
        ] {
            s.apply_movie_runtime_details(3, vec![(candidate, metadata)], false);
        }
        assert!(s.library.movie_runtimes.busy);
        assert!(s.library.tracking.pending_runtimes.is_empty());
        s.apply_movie_runtime_details(3, vec![], true);
        assert!(!s.library.movie_runtimes.busy);
        assert!(!s.library.movie_runtimes.page_pending);
    }

    #[test]
    fn full_page_advances_even_when_providers_return_no_runtimes() {
        let mut s = crate::ui::app::init();
        s.library.movie_runtimes.busy = true;
        s.library.movie_runtimes.keys = (0..12)
            .map(|id| MediaKey::new("test", MediaKind::Movie, id.to_string()))
            .collect();
        s.apply_movie_runtime_details(0, vec![], true);
        assert!(s.library.movie_runtimes.busy);
        assert!(s.library.movie_runtimes.page_pending);
    }
}
