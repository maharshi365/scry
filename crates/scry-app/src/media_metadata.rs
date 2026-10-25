//! Convert provider release facts into library records, independently of UI state.
use chrono::DateTime;
use scry_addons::Meta;
use scry_library::{EpisodeRelease, Media, MediaKey, MediaKind};
use std::collections::HashSet;

pub(crate) fn release_timestamp(value: &str) -> Option<i64> {
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

pub(crate) fn meta_release_timestamp(meta: &Meta) -> Option<i64> {
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

pub(crate) fn episode_inventory(key: &MediaKey, meta: &Meta) -> Vec<EpisodeRelease> {
    let mut seen = HashSet::new();
    meta.videos
        .iter()
        .filter_map(|video| {
            let released_at = video.released.as_deref().and_then(release_timestamp);
            if video.id.trim().is_empty() {
                return None;
            }
            let episode_key = MediaKey::new(&key.namespace, MediaKind::Episode, &video.id);
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
            Some(EpisodeRelease {
                media: Media {
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

fn runtime_seconds(value: &str) -> Option<f64> {
    if value.len() > 128 {
        return None;
    }
    let value = value
        .to_ascii_lowercase()
        .replace("minutes", "m")
        .replace("minute", "m")
        .replace("mins", "m")
        .replace("min", "m")
        .replace("hours", "h")
        .replace("hour", "h")
        .replace("hrs", "h")
        .replace("hr", "h");
    let value: String = value.chars().filter(|c| !c.is_whitespace()).collect();
    let seconds = if let Some((hours, minutes)) = value.split_once('h') {
        let hours = hours.parse::<f64>().ok()?;
        let minutes = minutes.strip_suffix('m').unwrap_or(minutes);
        let minutes = if minutes.is_empty() {
            0.0
        } else {
            minutes.parse::<f64>().ok()?
        };
        if hours < 0.0 || minutes < 0.0 {
            return None;
        }
        hours * 3600.0 + minutes * 60.0
    } else {
        value
            .strip_suffix('m')
            .unwrap_or(&value)
            .parse::<f64>()
            .ok()?
            * 60.0
    };
    (seconds.is_finite() && seconds > 0.0).then_some(seconds)
}

pub(crate) fn known_runtimes(key: &MediaKey, meta: &Meta) -> Vec<(MediaKey, f64)> {
    if key.kind == MediaKind::Movie {
        return runtime_seconds(&meta.runtime)
            .map(|seconds| vec![(key.clone(), seconds)])
            .unwrap_or_default();
    }
    meta.videos
        .iter()
        .filter(|video| !video.id.trim().is_empty())
        .filter_map(|video| {
            // A series runtime is the provider's per-episode runtime. Prefer an
            // episode-specific value when supplied; never substitute a fixed guess.
            let runtime = if video.runtime.is_empty() {
                &meta.runtime
            } else {
                &video.runtime
            };
            Some((
                MediaKey::new(&key.namespace, MediaKind::Episode, &video.id),
                runtime_seconds(runtime)?,
            ))
        })
        .take(50_000)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_runtime_formats_do_not_invent_values_for_missing_or_ambiguous_metadata() {
        for (value, expected) in [("120 min", 7200.0), ("1h 30m", 5400.0), ("45", 2700.0)] {
            assert_eq!(runtime_seconds(value), Some(expected));
        }
        for value in [
            "",
            "unknown",
            "45-60 min",
            "NaN",
            "inf",
            "-5 min",
            "-1h 120m",
            "0 min",
        ] {
            assert_eq!(runtime_seconds(value), None);
        }
    }

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
