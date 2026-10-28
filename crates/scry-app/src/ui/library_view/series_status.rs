//! Show posters summarize episode state across all seasons, not a paged episode list.
use super::{MediaKey, MediaKind, State};
use crate::library_worker::{Command, SeriesState};
use scry_addons::Meta;
use scry_library::Progress;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

#[derive(Default)]
pub(in crate::ui) struct SeriesUi {
    pub generation: u64,
    pub requested: Vec<MediaKey>,
    pub states: Vec<SeriesState>,
    pub metadata_generation: u64,
    pub busy: bool,
    pub details: HashMap<MediaKey, (Option<Meta>, Instant)>,
}

impl State {
    pub(in crate::ui) fn series_progress_label(&self, key: &MediaKey) -> Option<String> {
        let summary = self
            .library
            .series
            .states
            .iter()
            .find(|state| state.key == *key)?
            .summary?;
        let fraction = summary.fraction()?;
        Some(format!(
            "{} / {} episodes watched · {:.0}%",
            summary.watched_episodes,
            summary.total_episodes,
            fraction * 100.0
        ))
    }

    pub(in crate::ui) fn request_series_states(&mut self) {
        let keys = super::super::poster_grid::series_keys(self);
        self.library.series.details.retain(|_, (meta, fetched)| {
            fetched.elapsed()
                < if meta.is_some() {
                    Duration::from_mins(10)
                } else {
                    Duration::from_secs(30)
                }
        });
        while self.library.series.details.len() > 32.max(keys.len()) {
            let oldest = self
                .library
                .series
                .details
                .iter()
                .min_by_key(|(key, (_, fetched))| (keys.contains(key), *fetched))
                .map(|(key, _)| key.clone());
            if let Some(key) = oldest {
                self.library.series.details.remove(&key);
            }
        }
        if keys != self.library.series.requested
            && let Some(worker) = &self.library.worker
        {
            let generation = self.library.series.generation + 1;
            if worker
                .send(Command::SeriesStates {
                    generation,
                    keys: keys.clone(),
                })
                .is_ok()
            {
                self.library.series.generation = generation;
                self.library.series.requested.clone_from(&keys);
            }
        }
        if self.library.series.busy || self.addons.is_empty() {
            return;
        }
        let missing: Vec<_> = keys
            .into_iter()
            .filter(|key| {
                self.library
                    .series
                    .states
                    .iter()
                    .any(|state| state.key == *key && state.tracked)
                    && !self.library.series.details.contains_key(key)
                    && (self.navigation.page != super::super::Page::Detail
                        || self
                            .detail
                            .as_ref()
                            .is_none_or(|meta| meta.id != key.id || meta.videos.is_empty()))
            })
            .take(12)
            .collect();
        if !missing.is_empty()
            && let Some(services) = &mut self.services
        {
            self.library.series.busy = true;
            services.series_details(
                self.library.series.metadata_generation,
                self.addons.clone(),
                missing,
                false,
            );
        }
    }

    pub(in crate::ui) fn apply_series_details(
        &mut self,
        generation: u64,
        found: Vec<(MediaKey, Option<Meta>)>,
        done: bool,
    ) {
        if generation != self.library.series.metadata_generation {
            return;
        }
        let visible = super::super::poster_grid::series_keys(self);
        for (key, meta) in found {
            if visible.contains(&key) {
                let meta = meta.filter(|m| m.kind == "series" && m.id == key.id);
                if let Some(meta) = &meta {
                    self.reconcile_series_metadata(&key, meta);
                    self.remember_series_poster(&key, meta);
                }
                self.library
                    .series
                    .details
                    .insert(key, (meta, Instant::now()));
            }
        }
        if done {
            self.library.series.busy = false;
        }
    }

    pub(in crate::ui) fn retry_series_details(&mut self) {
        self.reset_series_tracking();
        self.library.series.metadata_generation += 1;
        self.library.series.busy = false;
        self.library.series.details.clear();
    }

    pub(in crate::ui) fn remember_series_detail(&mut self) {
        if let Some(meta) = self.detail.as_ref().filter(|meta| meta.kind == "series") {
            let key = super::key(&self.detail_namespace, MediaKind::Series, &meta.id);
            let meta = meta.clone();
            self.reconcile_series_metadata(&key, &meta);
            self.library
                .series
                .details
                .insert(key, (Some(meta), Instant::now()));
        }
    }

    pub(in crate::ui) fn series_status(&self, key: &MediaKey) -> (bool, Option<Progress>) {
        if let Some(state) = self
            .library
            .series
            .states
            .iter()
            .find(|state| state.key == *key)
        {
            return (state.watched, state.progress);
        }
        (
            false,
            self.library
                .watching
                .iter()
                .find(|(media, _)| media.parent.as_ref() == Some(key))
                .map(|(_, p)| *p),
        )
    }
}
