//! Refresh tracked shows even when the active Library filter hides them.
use super::{Media, MediaKey, State};
use crate::library_worker::Command;
use crate::media_metadata::episode_inventory;
use scry_addons::Meta;
use std::time::{Duration, Instant};

#[derive(Default)]
pub(in crate::ui) struct TrackingUi {
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
