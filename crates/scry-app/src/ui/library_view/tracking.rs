//! Refresh tracked shows even when the active Library filter hides them.
use super::{Media, MediaKey, State};
use crate::library_worker::Command;
use crate::media_metadata::episode_inventory;
use scry_addons::Meta;
use std::{
    collections::HashMap,
    sync::mpsc::TrySendError,
    time::{Duration, Instant},
};

#[derive(Default)]
pub(in crate::ui) struct TrackingUi {
    pub(super) pending_runtimes: HashMap<MediaKey, f64>,
    generation: u64,
    offset: u32,
    pub busy: bool,
    rerun: bool,
    more: bool,
    keys: Vec<MediaKey>,
    last_scan: Option<Instant>,
}

impl State {
    pub(in crate::ui) fn reset_series_tracking(&mut self) {
        self.library.tracking.generation += 1;
        self.library.tracking.busy = false;
        self.library.tracking.rerun = false;
        self.library.tracking.last_scan = None;
        self.library.tracking.keys.clear();
    }

    pub(in crate::ui) fn request_series_tracking(&mut self, force: bool) {
        let tracking = &mut self.library.tracking;
        if tracking.busy {
            tracking.rerun |= force;
            return;
        }
        if !force
            && tracking
                .last_scan
                .is_some_and(|time| time.elapsed() < Duration::from_mins(10))
        {
            return;
        }
        let Some(worker) = &self.library.worker else {
            return;
        };
        tracking.generation += 1;
        tracking.offset = 0;
        tracking.busy = true;
        tracking.last_scan = Some(Instant::now());
        if worker
            .send(Command::TrackedSeries {
                generation: tracking.generation,
                offset: 0,
            })
            .is_err()
        {
            tracking.busy = false;
            tracking.last_scan = None;
        }
    }

    pub(in crate::ui) fn apply_tracked_series(&mut self, generation: u64, items: Vec<Media>) {
        if generation != self.library.tracking.generation || !self.library.tracking.busy {
            return;
        }
        self.library.tracking.more = items.len() == 12;
        self.library.tracking.keys = items.into_iter().map(|media| media.key).collect();
        let mut missing = Vec::new();
        for key in self.library.tracking.keys.clone() {
            let cached = self
                .library
                .series
                .details
                .get(&key)
                .filter(|(_, fetched)| fetched.elapsed() < Duration::from_mins(10));
            if let Some((meta, _)) = cached {
                if let Some(meta) = meta.clone() {
                    self.reconcile_series_metadata(&key, &meta);
                    self.remember_series_poster(&key, &meta);
                }
            } else {
                missing.push(key);
            }
        }
        if !missing.is_empty()
            && !self.addons.is_empty()
            && let Some(services) = &mut self.services
        {
            services.series_details(generation, self.addons.clone(), missing, true);
        } else {
            self.advance_series_tracking();
        }
    }

    pub(in crate::ui) fn apply_series_tracking_details(
        &mut self,
        generation: u64,
        found: Vec<(MediaKey, Option<Meta>)>,
        done: bool,
    ) {
        if generation != self.library.tracking.generation || !self.library.tracking.busy {
            return;
        }
        for (key, meta) in found {
            if !self.library.tracking.keys.contains(&key) {
                continue;
            }
            let meta = meta.filter(|meta| meta.kind == "series" && meta.id == key.id);
            if let Some(meta) = &meta {
                self.reconcile_series_metadata(&key, meta);
                self.remember_series_poster(&key, meta);
            }
            self.library
                .series
                .details
                .insert(key, (meta, Instant::now()));
        }
        if done {
            self.advance_series_tracking();
        }
    }

    pub(super) fn reconcile_series_metadata(&mut self, key: &MediaKey, meta: &Meta) {
        // An omitted episode list is not evidence that the stored inventory is empty.
        if meta.videos.is_empty() {
            return;
        }
        if let Some(worker) = &self.library.worker
            && worker
                .send(Command::SyncSeries {
                    key: key.clone(),
                    releases: episode_inventory(key, meta),
                })
                .is_err()
        {
            self.library.tracking.rerun = true;
        }
        self.register_known_runtimes(key, meta);
    }

    pub(in crate::ui) fn register_known_runtimes(&mut self, key: &MediaKey, meta: &Meta) {
        self.register_guide_metadata(key, meta);
        self.library
            .tracking
            .pending_runtimes
            .extend(crate::media_metadata::known_runtimes(key, meta));
    }

    pub(in crate::ui) fn queue_known_runtimes(&mut self, values: Vec<(MediaKey, f64)>) {
        self.library.tracking.pending_runtimes.extend(values);
    }

    pub(in crate::ui) fn flush_known_runtimes(&mut self) -> bool {
        let Some(worker) = &self.library.worker else {
            return false;
        };
        if let Err(TrySendError::Disconnected(())) =
            flush_runtimes(&mut self.library.tracking.pending_runtimes, |batch| {
                worker.try_send(Command::Runtimes(batch))
            })
        {
            self.library.error = Some("Library worker is unavailable".into());
            return true;
        }
        false
    }

    fn advance_series_tracking(&mut self) {
        let tracking = &mut self.library.tracking;
        if tracking.more {
            tracking.offset = tracking.offset.saturating_add(12);
            if let Some(worker) = &self.library.worker
                && worker
                    .send(Command::TrackedSeries {
                        generation: tracking.generation,
                        offset: tracking.offset,
                    })
                    .is_ok()
            {
                return;
            }
        }
        tracking.busy = false;
        if std::mem::take(&mut tracking.rerun) {
            self.request_series_tracking(true);
        } else if let Some(worker) = &self.library.worker {
            let _ = worker.send(Command::UpNext {
                generation: self.library.generation,
                offset: self.library.up_next_offset,
            });
        }
    }
}

fn flush_runtimes(
    pending: &mut HashMap<MediaKey, f64>,
    send: impl FnOnce(Vec<(MediaKey, f64)>) -> Result<(), TrySendError<()>>,
) -> Result<(), TrySendError<()>> {
    if pending.is_empty() {
        return Ok(());
    }
    // Coalesce repeated metadata and send one bounded batch per UI poll.
    // Retain the originals until the worker accepts the command.
    let batch: Vec<_> = pending
        .iter()
        .take(200)
        .map(|(key, seconds)| (key.clone(), *seconds))
        .collect();
    send(batch.clone())?;
    for (key, _) in batch {
        pending.remove(&key);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_runtime_queue_retains_updates_for_a_bounded_retry() {
        let mut pending: HashMap<_, _> = (0..250)
            .map(|id| {
                (
                    MediaKey::new("test", scry_library::MediaKind::Movie, id.to_string()),
                    100.0,
                )
            })
            .collect();
        let result = flush_runtimes(&mut pending, |batch| {
            assert_eq!(batch.len(), 200);
            Err(TrySendError::Full(()))
        });
        assert!(matches!(result, Err(TrySendError::Full(()))));
        assert_eq!(pending.len(), 250);
        let result = flush_runtimes(&mut pending, |batch| {
            assert_eq!(batch.len(), 200);
            assert!(
                batch
                    .iter()
                    .all(|(_, seconds)| (*seconds - 100.0).abs() < f64::EPSILON)
            );
            Ok(())
        });
        assert!(result.is_ok());
        assert_eq!(pending.len(), 50);
    }
}
