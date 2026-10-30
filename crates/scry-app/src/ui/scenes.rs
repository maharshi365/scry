//! Explicit tooling scenes only. Normal startup uses the user's local state.
use super::{Page, State};
use crate::library_worker::Snapshot;
use scry_addons::Stream;
use scry_library::{Media, MediaKey, MediaKind};

pub(super) fn scene_settings(s: &mut State) {
    s.navigation.page = Page::Settings;
    s.addons.busy = false;
    s.settings.subtitles.busy = false;
}

pub(super) fn scene_player_fullscreen(s: &mut State) {
    s.addons.busy = false;
    s.navigation.page = Page::Player;
    s.player.playing = true;
    s.player.ready = true;
    s.player.fullscreen = true;
    s.player.sample.duration_s = Some(7200.0);
    s.player.sample.position_s = Some(1800.0);
    s.player.sample.cache_s = Some(120.0);
}

pub(super) fn scene_addons(s: &mut State) {
    s.navigation.page = Page::Addons;
    s.addons.busy = false;
}

pub(super) fn scene_discover(s: &mut State) {
    if let Err(error) = discover(s) {
        s.discover.request.error = Some(error.to_string());
    }
}

fn discover(s: &mut State) -> serde_json::Result<()> {
    s.addons.busy = false;
    s.navigation.page = Page::Discover;
    s.addons.installed = vec![serde_json::from_value(serde_json::json!({
        "manifest_url":"https://example.invalid/manifest.json",
        "manifest":{"id":"scene.catalog", "name":"Cinema collection", "catalogs":[
            {"id":"popular","type":"movie","name":"Popular movies"},
            {"id":"classics","type":"movie","name":"Modern classics"}
        ]}
    }))?];
    s.discover.catalog = s.catalogs().into_iter().next();
    let names = [
        "The Grand Budapest Hotel",
        "Arrival",
        "Past Lives",
        "Perfect Days",
        "Moonlight",
        "The Secret Life of Walter Mitty",
        "Aftersun",
        "Interstellar",
        "The Holdovers",
        "Dune: Part Two",
        "Little Women",
        "The Wild Robot",
    ];
    s.discover.results = crate::ui::catalog::CatalogResults::from(names.iter().enumerate().map(|(index, name)| {
        serde_json::from_value(serde_json::json!({
            "id":format!("scene-{index}"), "type":"movie", "name":name, "year":2024 - index % 10,
            "description":"Stories that stay with you."
        }))
    }).collect::<serde_json::Result<Vec<_>>>()?);
    s.discover.request.status.clear();
    Ok(())
}

pub(super) fn scene_detail(s: &mut State) {
    if let Err(error) = detail(s) {
        s.detail.sources.request.error = Some(error.to_string());
    }
}

pub(super) fn scene_detail_watched(s: &mut State) {
    scene_detail(s);
    let Some(media) = s.library_media() else {
        return;
    };
    let watched = scry_library::WatchedState { watched: true };
    s.library.snapshot = Some(Snapshot {
        selected_state: Some(crate::library_worker::SelectedState {
            key: media.key.clone(),
            watched,
            watchlist: true,
            progress: None,
        }),
        episode_states: vec![(media.key, watched)],
    });
}

pub(super) fn scene_episodes(s: &mut State) {
    scene_detail(s);
    s.detail.episodes.selected = None;
    s.detail.sources.streams.clear();
}

pub(super) fn scene_movie(s: &mut State) {
    scene_detail(s);
    if let Some(meta) = &mut s.detail.meta {
        meta.kind = "movie".into();
        meta.videos.clear();
    }
    s.detail.episodes.selected = Some("scene-series".into());
}

pub(super) fn scene_preview(s: &mut State) {
    scene_discover(s);
    s.open_result(0);
    s.detail.request.loading = false;
    if let Some(meta) = &mut s.detail.meta {
        meta.genres = vec!["Drama".into(), "Mystery".into(), "Adventure".into()];
        meta.cast = vec![
            "Alex Morgan".into(),
            "Sam Jordan".into(),
            "Taylor Reed".into(),
        ];
        meta.director = vec!["Jamie Walker".into(), "Casey Lee".into()];
    }
}

pub(super) fn scene_source_wrap(s: &mut State) {
    scene_movie(s);
    for (index, (_, stream)) in s.detail.sources.streams.iter_mut().enumerate() {
        stream.name = "Torrentio\n1080p".into();
        stream.title = format!(
            "Carrie.1976.REMASTERED.1080p.BluRay.DTS-HD.x264-ReleaseGroup{index}.mkv\n15 peers · 1.53 GB\nRARBG"
        );
    }
}

fn detail(s: &mut State) -> serde_json::Result<()> {
    s.navigation.page = Page::Detail;
    s.addons.busy = false;
    s.detail.namespace = "scene.catalog".into();
    s.detail.meta = Some(serde_json::from_value(serde_json::json!({
        "id":"scene-series", "type":"series", "name":"The Observatory", "year":"2024–",
        "runtime":"52 min", "imdbRating":"7.8", "genres":["Drama", "Mystery"],
        "cast":["Alex Morgan", "Sam Jordan", "Taylor Reed"],
        "description":"At a remote mountain observatory, a small team follows a signal that could change everything. A thoughtful story of discovery, doubt, and the people we choose to trust.",
        "videos":(1..=250).map(|index| serde_json::json!({"id":format!("scene-series:1:{index}"),"season":1,"episode":index,"title":format!("Chapter {index}")})).collect::<Vec<_>>()
    }))?);
    s.detail.episodes.selected = Some("scene-series:1:1".into());
    s.detail.sources.request.status = "2 streams".into();
    s.detail.sources.streams = ["1080p · H.264 · 2.1 GB", "720p · H.264 · 1.2 GB"]
        .iter()
        .map(|title| {
            serde_json::from_value::<Stream>(serde_json::json!({
                "url":"https://example.invalid/video.mp4", "name":"Direct HTTPS", "title":title
            }))
            .map(|stream| ("Example provider".into(), stream))
        })
        .collect::<serde_json::Result<Vec<_>>>()?;
    Ok(())
}

pub(super) fn scene_library(s: &mut State) {
    s.navigation.page = Page::Library;
    s.addons.busy = false;
    s.discover.request.status = "Saved on this device.".into();
    s.library.entries = Some(
        [
            "Arrival",
            "Perfect Days",
            "The Grand Budapest Hotel",
            "Past Lives",
            "Aftersun",
        ]
        .iter()
        .enumerate()
        .map(|(index, title)| Media {
            key: MediaKey::new("scene", MediaKind::Movie, format!("scene-{index}")),
            title: (*title).into(),
            parent: None,
        })
        .collect(),
    );
    s.library.snapshot = Some(Snapshot {
        selected_state: None,
        episode_states: vec![],
    });
}

pub(super) fn scene_library_empty(s: &mut State) {
    s.navigation.page = Page::Library;
    s.addons.busy = false;
    s.library.entries = Some(Vec::new());
}

pub(super) fn scene_watching(s: &mut State) {
    scene_library(s);
    s.navigation.page = Page::Watching;
    s.discover.request.status.clear();
    let items = s
        .library
        .entries
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|media| {
            (
                media,
                scry_library::Progress {
                    position_seconds: 1800.0,
                    duration_seconds: 5400.0,
                },
            )
        })
        .collect();
    s.apply_watching_entries(items);
}

pub(super) fn scene_calendar(s: &mut State) {
    s.navigation.page = Page::Calendar;
    s.addons.busy = false;
    let date = chrono::Local::now().date_naive();
    for i in 0..12 {
        let media = Media {
            key: MediaKey::new("scene", MediaKind::Series, format!("show-{i}")),
            title: if i % 2 == 0 {
                "The Observatory".into()
            } else {
                "Northern Lights".into()
            },
            parent: None,
        };
        s.calendar.series.push(media.clone());
        s.calendar.releases.push(super::calendar::Release {
            date,
            media,
            episode_id: format!("show-{i}:1:1"),
            title: format!("Chapter {}", i + 1),
            season: Some(1),
            episode: Some(i + 1),
            poster: None,
        });
    }
}

pub(super) fn scene_calendar_month(s: &mut State) {
    scene_calendar(s);
}

pub(super) fn scene_calendar_day(s: &mut State) {
    scene_calendar(s);
    s.calendar.expanded_day = Some(s.calendar.anchor);
}

pub(super) fn scene_calendar_refresh(s: &mut State) {
    scene_calendar_month(s);
    s.calendar.loading = true;
}
