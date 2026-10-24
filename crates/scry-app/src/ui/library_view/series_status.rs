//! Show posters summarize episode state across all seasons, not a paged episode list.
use super::{MediaKey, MediaKind, State};
use crate::library_worker::{Command, SeriesRequest, SeriesState};
use chrono::{DateTime, Utc};
use scry_addons::Meta;
use scry_library::Progress;
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

#[derive(Default)]
pub(in crate::ui) struct SeriesUi {
    pub generation: u64,
    pub requested: Vec<SeriesRequest>,
    pub states: Vec<SeriesState>,
    pub metadata_generation: u64,
    pub busy: bool,
    pub details: HashMap<MediaKey, (Option<Meta>, Instant)>,
}

pub(super) fn released_episodes(key: &MediaKey, meta: &Meta, now: DateTime<Utc>) -> Vec<MediaKey> {
    let mut released = Vec::new();
    let mut seen = HashSet::new();
    for video in &meta.videos {
        let aired = video.released.as_deref().is_some_and(|value| {
            DateTime::parse_from_rfc3339(value).map_or_else(
                |_| {
                    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
                        .is_ok_and(|date| date <= now.date_naive())
                },
                |date| date <= now,
            )
        });
        if aired && !video.id.trim().is_empty() {
            let episode = super::key(&key.namespace, MediaKind::Episode, &video.id);
            if seen.insert(episode.clone()) {
                released.push(episode);
            }
        }
    }
    released
}

impl State {
    pub(in crate::ui) fn request_series_states(&mut self) {
        let keys = super::super::poster_grid::series_keys(self);
        self.library
            .series
            .details
            .retain(|_, (_, fetched)| fetched.elapsed() < Duration::from_mins(10));
        while self.library.series.details.len() > 32 {
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
        let requests: Vec<_> = keys
            .iter()
            .map(|key| {
                let meta = self
                    .detail
                    .as_ref()
                    .filter(|meta| {
                        self.navigation.page == super::super::Page::Detail
                            && meta.kind == "series"
                            && meta.id == key.id
                            && (key.namespace == "imdb" || key.namespace == self.detail_namespace)
                            && !meta.videos.is_empty()
                    })
                    .or_else(|| {
                        self.library
                            .series
                            .details
                            .get(key)
                            .and_then(|(meta, _)| meta.as_ref())
                    });
                SeriesRequest {
                    key: key.clone(),
                    metadata_known: meta.is_some(),
                    released: meta
                        .map_or_else(Vec::new, |meta| released_episodes(key, meta, Utc::now())),
                }
            })
            .collect();
        if requests != self.library.series.requested
            && let Some(worker) = &self.library.worker
        {
            let generation = self.library.series.generation + 1;
            if worker
                .send(Command::SeriesStates {
                    generation,
                    requests: requests.clone(),
                })
                .is_ok()
            {
                self.library.series.generation = generation;
                self.library.series.requested = requests;
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
            self.library
                .series
                .details
                .insert(key, (Some(meta.clone()), Instant::now()));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn released_episodes_include_all_seasons_but_not_future_unknown_or_duplicate_releases()
    -> anyhow::Result<()> {
        let now = DateTime::parse_from_rfc3339("2026-10-24T12:00:00Z")?.with_timezone(&Utc);
        let meta: Meta = serde_json::from_value(serde_json::json!({
            "id":"tt123", "type":"series", "name":"Show", "videos":[
                {"id":"tt123:1:1", "season":1, "episode":1, "released":"2020-01-01"},
                {"id":"tt123:2:1", "season":2, "episode":1, "released":"2026-10-24T11:00:00Z"},
                {"id":"tt123:2:1", "season":2, "episode":1, "released":"2026-10-24T11:00:00Z"},
                {"id":"tt123:2:2", "season":2, "episode":2, "released":"2026-10-24T13:00:00Z"},
                {"id":"tt123:3:1", "season":3, "episode":1, "released":"2100-01-01"},
                {"id":"tt123:3:2", "season":3, "episode":2},
                {"id":"tt123:3:3", "season":3, "episode":3, "released":"invalid"}
            ]
        }))?;
        let key = MediaKey::new("imdb", MediaKind::Series, "tt123");
        assert_eq!(
            released_episodes(&key, &meta, now),
            vec![
                MediaKey::new("imdb", MediaKind::Episode, "tt123:1:1"),
                MediaKey::new("imdb", MediaKind::Episode, "tt123:2:1"),
            ]
        );
        Ok(())
    }
}
