//! Show posters summarize episode state across all seasons, not a paged episode list.
use super::{MediaKey, MediaKind, State};
use crate::library_worker::{Command, SeriesState};
use chrono::DateTime;
use scry_addons::Meta;
use scry_library::Progress;
use std::{
    collections::{HashMap, HashSet},
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

pub(in crate::ui) fn release_timestamp(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .map(|date| date.timestamp())
        .ok()
        .or_else(|| {
            chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .ok()?
                .and_hms_opt(0, 0, 0)
                .map(|date| date.and_utc().timestamp())
        })
}

pub(in crate::ui) fn meta_release_timestamp(meta: &Meta) -> Option<i64> {
    meta.released
        .as_deref()
        .and_then(release_timestamp)
        .or_else(|| {
            // A future release year is a lower bound, not proof of a past release date.
            let year = meta.year.get(..4)?.parse::<i32>().ok()?;
            let date = chrono::NaiveDate::from_ymd_opt(year, 1, 1)?
                .and_hms_opt(0, 0, 0)?
                .and_utc()
                .timestamp();
            (date > chrono::Utc::now().timestamp()).then_some(date)
        })
}

pub(in crate::ui) fn episode_inventory(
    key: &MediaKey,
    meta: &Meta,
) -> Vec<scry_library::EpisodeRelease> {
    let mut seen = HashSet::new();
    meta.videos
        .iter()
        .filter_map(|video| {
            let released_at = video.released.as_deref().and_then(release_timestamp);
            if video.id.trim().is_empty() {
                return None;
            }
            let episode_key = super::key(&key.namespace, MediaKind::Episode, &video.id);
            if !seen.insert(episode_key.clone()) {
                return None;
            }
            let title = format!(
                "{} · S{} E{} · {}",
                meta.name,
                video.season.unwrap_or(0),
                video.episode.unwrap_or(0),
                if video.title.trim().is_empty() {
                    &video.id
                } else {
                    &video.title
                }
            );
            Some(scry_library::EpisodeRelease {
                media: super::Media {
                    key: episode_key,
                    title,
                    parent: Some(key.clone()),
                },
                released_at,
                season: video.season,
                episode: video.episode,
            })
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_includes_future_and_undated_episodes_without_duplicates() -> anyhow::Result<()> {
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
            episode_inventory(&key, &meta)
                .into_iter()
                .map(|release| release.media.key)
                .collect::<Vec<_>>(),
            vec![
                MediaKey::new("imdb", MediaKind::Episode, "tt123:1:1"),
                MediaKey::new("imdb", MediaKind::Episode, "tt123:2:1"),
                MediaKey::new("imdb", MediaKind::Episode, "tt123:2:2"),
                MediaKey::new("imdb", MediaKind::Episode, "tt123:3:1"),
                MediaKey::new("imdb", MediaKind::Episode, "tt123:3:2"),
                MediaKey::new("imdb", MediaKind::Episode, "tt123:3:3"),
            ]
        );
        Ok(())
    }
}
