//! Watched actions capture release metadata before sending a database mutation.
use super::{Media, MediaKind, State};
use crate::library_worker::Command;
use crate::media_metadata::{episode_inventory, meta_release_timestamp, release_timestamp};
use scry_addons::Meta;

impl State {
    fn watch_metadata(&self, media: &Media) -> Option<&Meta> {
        let kind = if media.key.kind == MediaKind::Movie {
            "movie"
        } else {
            "series"
        };
        self.detail
            .iter()
            .filter(|meta| {
                meta.id == media.key.id
                    && meta.kind == kind
                    && super::key(&self.detail_namespace, media.key.kind, &meta.id) == media.key
            })
            .chain(
                self.library
                    .series
                    .details
                    .get(&media.key)
                    .and_then(|(meta, _)| meta.as_ref()),
            )
            .chain(
                self.metas
                    .iter()
                    .enumerate()
                    .filter(|(_, meta)| meta.id == media.key.id && meta.kind == kind)
                    .filter_map(|(index, meta)| {
                        self.result_media(index)
                            .filter(|item| {
                                item.key == media.key
                                    && (media.key.kind == MediaKind::Series
                                        || meta_release_timestamp(meta).is_some())
                            })
                            .map(|_| meta)
                    }),
            )
            .find(|meta| {
                meta.kind == kind
                    && meta.id == media.key.id
                    && (kind != "series" || !meta.videos.is_empty())
            })
    }

    fn known_release_date(&self, media: &Media) -> Option<i64> {
        if let Some(parent) = &media.parent {
            self.detail
                .iter()
                .filter(|meta| {
                    meta.id == parent.id
                        && self.parent_media().is_some_and(|item| item.key == *parent)
                })
                .chain(
                    self.library
                        .series
                        .details
                        .get(parent)
                        .and_then(|(meta, _)| meta.as_ref()),
                )
                .find_map(|meta| {
                    meta.videos
                        .iter()
                        .find(|video| video.id == media.key.id)
                        .and_then(|video| video.released.as_deref())
                        .and_then(release_timestamp)
                })
                .or_else(|| {
                    self.calendar
                        .release_times
                        .get(&(parent.clone(), media.key.id.clone()))
                        .copied()
                })
        } else {
            self.watch_metadata(media)
                .and_then(meta_release_timestamp)
                .or_else(|| {
                    self.calendar
                        .release_times
                        .get(&(media.key.clone(), media.key.id.clone()))
                        .copied()
                })
        }
    }

    pub(in crate::ui) fn can_mark_released(&self, media: &Media) -> bool {
        let season = media.parent.as_ref().and_then(|parent| {
            self.detail
                .iter()
                .filter(|meta| {
                    meta.id == parent.id
                        && self.parent_media().is_some_and(|item| item.key == *parent)
                })
                .chain(
                    self.library
                        .series
                        .details
                        .get(parent)
                        .and_then(|(meta, _)| meta.as_ref()),
                )
                .find_map(|meta| {
                    meta.videos
                        .iter()
                        .find(|video| video.id == media.key.id)?
                        .season
                })
                .or_else(|| {
                    self.calendar
                        .releases
                        .iter()
                        .find(|release| {
                            release.media.key == *parent && release.episode_id == media.key.id
                        })?
                        .season
                })
        });
        scry_library::can_mark_watched(
            media.key.kind,
            season,
            self.known_release_date(media),
            chrono::Utc::now().timestamp(),
        )
    }

    pub(in crate::ui) fn mark_media_watched(&mut self, media: Media, watched: bool) {
        let runtime_key = media.key.clone();
        let runtime_meta = self.watch_metadata(&media).cloned();
        self.library.watch_pending.remove(&media.key);
        let date = self.known_release_date(&media);
        if !watched
            || media.key.kind == MediaKind::Episode
            || (media.key.kind == MediaKind::Movie && date.is_some())
        {
            self.db(
                Command::SetWatchedMetadata {
                    item: media,
                    released_at: date,
                    inventory: None,
                    watched,
                },
                true,
            );
        } else if let Some(meta) = self.watch_metadata(&media).cloned() {
            self.mark_with_metadata(media, watched, &meta);
        } else if media.key.kind == MediaKind::Series
            && self.library.series.states.iter().any(|state| {
                state.key == media.key
                    && state
                        .summary
                        .is_some_and(|summary| summary.released_episodes > 0)
            })
        {
            self.db(Command::SetWatched(media, watched), true);
        } else if let Some(services) = &mut self.services {
            self.library.watch_generation += 1;
            let token = self.library.watch_generation;
            match services.watch_metadata(token, self.addons.clone(), media.clone(), watched) {
                Ok(()) => {
                    self.library.watch_pending.insert(media.key, token);
                }
                Err(error) => self.library.error = Some(error.to_string()),
            }
        } else {
            self.db(Command::SetWatched(media, watched), true);
        }
        if let Some(meta) = runtime_meta {
            self.register_known_runtimes(&runtime_key, &meta);
        }
    }

    fn mark_with_metadata(&mut self, media: Media, watched: bool, meta: &Meta) {
        let key = media.key.clone();
        let inventory =
            (media.key.kind == MediaKind::Series).then(|| episode_inventory(&media.key, meta));
        let released_at = meta_release_timestamp(meta);
        self.db(
            Command::SetWatchedMetadata {
                item: media,
                released_at,
                inventory,
                watched,
            },
            true,
        );
        self.register_known_runtimes(&key, meta);
    }

    pub(in crate::ui) fn apply_watch_metadata(
        &mut self,
        token: u64,
        media: Media,
        watched: bool,
        result: Result<Box<Meta>, String>,
    ) {
        if self.library.watch_pending.get(&media.key) != Some(&token) {
            return;
        }
        self.library.watch_pending.remove(&media.key);
        if let Ok(meta) = result {
            let kind = if media.key.kind == MediaKind::Movie {
                "movie"
            } else {
                "series"
            };
            if meta.id != media.key.id || meta.kind != kind {
                return;
            }
            self.mark_with_metadata(media.clone(), watched, &meta);
            if media.key.kind == MediaKind::Series {
                self.remember_series_poster(&media.key, &meta);
                self.library
                    .series
                    .details
                    .insert(media.key, (Some(*meta), std::time::Instant::now()));
            }
        } else {
            // Local release facts remain authoritative when providers are offline.
            let released_at = self.known_release_date(&media);
            self.db(
                Command::SetWatchedMetadata {
                    item: media,
                    released_at,
                    inventory: None,
                    watched,
                },
                true,
            );
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
    use std::sync::Arc;

    #[test]
    fn late_metadata_does_not_undo_a_newer_unwatched_action() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        let media = Media {
            key: scry_library::MediaKey::new("test", MediaKind::Movie, "movie"),
            title: "Movie".into(),
            parent: None,
        };
        {
            let mut db = scry_library::Library::open(&path).unwrap();
            db.upsert_media(&media).unwrap();
            db.set_watched(&media.key, true).unwrap();
        }
        let mut s = crate::ui::app::init();
        s.library.worker =
            Some(crate::library_worker::Worker::start(path.clone(), Arc::new(|| {})).unwrap());
        s.library.watch_pending.insert(media.key.clone(), 42);
        s.mark_media_watched(media.clone(), false);
        let meta = serde_json::from_value(
            serde_json::json!({"id":"movie","type":"movie","name":"Movie","released":"2000-01-01"}),
        )
        .unwrap();
        s.apply_watch_metadata(42, media.clone(), true, Ok(Box::new(meta)));
        std::thread::spawn(s.take_library_shutdown())
            .join()
            .unwrap();
        assert!(
            !scry_library::Library::open(path)
                .unwrap()
                .watched_state(&media.key)
                .unwrap()
                .watched
        );
    }

    #[test]
    fn offline_metadata_uses_local_release_dates_without_showing_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        let items: Vec<_> = [("past", 0, true), ("future", i64::MAX, false)]
            .into_iter()
            .map(|(id, date, expected)| {
                (
                    Media {
                        key: scry_library::MediaKey::new("test", MediaKind::Movie, id),
                        title: id.into(),
                        parent: None,
                    },
                    date,
                    expected,
                )
            })
            .collect();
        {
            let mut db = scry_library::Library::open(&path).unwrap();
            for (media, date, _) in &items {
                db.register_release(media, Some(*date)).unwrap();
            }
        }
        let mut s = crate::ui::app::init();
        s.library.worker =
            Some(crate::library_worker::Worker::start(path.clone(), Arc::new(|| {})).unwrap());
        for (media, _, _) in &items {
            s.library.watch_pending.insert(media.key.clone(), 42);
            s.apply_watch_metadata(42, media.clone(), true, Err("Provider offline".into()));
        }
        assert!(s.library.error.is_none());
        std::thread::spawn(s.take_library_shutdown())
            .join()
            .unwrap();
        let db = scry_library::Library::open(path).unwrap();
        for (media, _, expected) in items {
            assert_eq!(db.watched_state(&media.key).unwrap().watched, expected);
        }
    }
}
