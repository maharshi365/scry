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
            if !seen.insert(video.id.as_str()) {
                return None;
            }
            let episode_key = MediaKey::new(&key.namespace, MediaKind::Episode, &video.id);
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
    let mut value = value.to_owned();
    value.make_ascii_lowercase();
    for (word, symbol) in [
        ("minutes", "m"),
        ("minute", "m"),
        ("mins", "m"),
        ("min", "m"),
        ("hours", "h"),
        ("hour", "h"),
        ("hrs", "h"),
        ("hr", "h"),
    ] {
        let mut cursor = 0;
        while let Some(offset) = value.get(cursor..)?.find(word) {
            let start = cursor + offset;
            value.replace_range(start..start + word.len(), symbol);
            cursor = start + symbol.len();
        }
    }
    value.retain(|c| !c.is_whitespace());
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
    let series_runtime = std::cell::OnceCell::new();
    meta.videos
        .iter()
        .filter(|video| !video.id.trim().is_empty())
        .filter_map(|video| {
            // A series runtime is the provider's per-episode runtime. Prefer an
            // episode-specific value when supplied; never substitute a fixed guess.
            let seconds = if video.runtime.is_empty() {
                *series_runtime.get_or_init(|| runtime_seconds(&meta.runtime))
            } else {
                runtime_seconds(&video.runtime)
            }?;
            Some((
                MediaKey::new(&key.namespace, MediaKind::Episode, &video.id),
                seconds,
            ))
        })
        .take(50_000)
        .collect()
}

pub(crate) fn validated_movie_runtimes(key: &MediaKey, meta: &Meta) -> Vec<(MediaKey, f64)> {
    if key.kind != MediaKind::Movie || meta.kind != "movie" || meta.id != key.id {
        return Vec::new();
    }
    known_runtimes(key, meta)
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
    fn runtime_units_preserve_case_whitespace_and_suffix_handling() {
        for value in [
            "1 HOUR 30 MINUTES",
            "1hrs30mins",
            "1hour30minute",
            "1\u{2003}h\t30m",
        ] {
            assert_eq!(runtime_seconds(value), Some(5400.0), "{value:?}");
        }
        for value in ["30 mi nutes", "1 ho ur", "30 minutesminutes", "30 mín"] {
            assert_eq!(runtime_seconds(value), None, "{value:?}");
        }
    }

    #[test]
    fn runtime_unit_replacements_preserve_cascades_without_repeating_a_pass() {
        for (value, expected) in [
            ("30minutesin", 1800.0),
            ("30minutein", 1800.0),
            ("30minsin", 1800.0),
            ("1hoursr", 3600.0),
            ("1hourrs", 3600.0),
            ("1hourr", 3600.0),
            ("1hrsr", 3600.0),
            ("1HOURSR30MINUTESIN", 5400.0),
        ] {
            assert_eq!(runtime_seconds(value), Some(expected), "{value:?}");
        }
        for value in ["30minin", "30minutesinin", "1hrr", "1hourrr"] {
            assert_eq!(runtime_seconds(value), None, "{value:?}");
        }
    }

    #[test]
    fn episode_runtimes_use_series_fallback_only_when_not_supplied() -> anyhow::Result<()> {
        let meta: Meta = serde_json::from_value(serde_json::json!({
            "id":"tt123", "type":"series", "name":"Show", "runtime":"45 minutes",
            "videos":[
                {"id":"tt123:1:1"},
                {"id":"tt123:1:2", "runtime":"30 min"},
                {"id":"tt123:1:3", "runtime":"unknown"},
                {"id":"tt123:1:4"}
            ]
        }))?;
        let key = MediaKey::new("imdb", MediaKind::Series, "tt123");
        let runtimes: Vec<_> = known_runtimes(&key, &meta)
            .into_iter()
            .map(|(key, seconds)| (key.id, seconds))
            .collect();
        assert_eq!(
            runtimes,
            [
                ("tt123:1:1".into(), 2700.0),
                ("tt123:1:2".into(), 1800.0),
                ("tt123:1:4".into(), 2700.0),
            ]
        );
        Ok(())
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
