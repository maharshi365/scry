//! Explicit tooling scenes only. Normal startup uses the user's local state.
use super::{Page, State};
use crate::library_worker::Snapshot;
use scry_addons::Stream;
use scry_library::{Media, MediaKey, MediaKind};
use std::sync::Arc;

pub(super) fn scene_discover(s: &mut State) {
    if let Err(error) = discover(s) {
        s.error = Some(error.to_string());
    }
}

fn discover(s: &mut State) -> serde_json::Result<()> {
    s.config_busy = false;
    s.page = Page::Discover;
    s.addons = vec![serde_json::from_value(serde_json::json!({
        "manifest_url":"https://example.invalid/manifest.json",
        "manifest":{"id":"scene.catalog", "name":"Cinema collection", "catalogs":[
            {"id":"popular","type":"movie","name":"Popular movies"},
            {"id":"classics","type":"movie","name":"Modern classics"}
        ]}
    }))?];
    s.catalog = s.catalogs().into_iter().next();
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
    s.metas = Arc::new(names.iter().enumerate().map(|(index, name)| {
        serde_json::from_value(serde_json::json!({
            "id":format!("scene-{index}"), "type":"movie", "name":name, "year":2024 - index % 10,
            "description":"Stories that stay with you."
        }))
    }).collect::<serde_json::Result<Vec<_>>>()?);
    s.status = format!("{} titles", s.metas.len());
    Ok(())
}

pub(super) fn scene_detail(s: &mut State) {
    if let Err(error) = detail(s) {
        s.error = Some(error.to_string());
    }
}

fn detail(s: &mut State) -> serde_json::Result<()> {
    s.page = Page::Detail;
    s.config_busy = false;
    s.detail_namespace = "scene.catalog".into();
    s.detail = Some(serde_json::from_value(serde_json::json!({
        "id":"scene-series", "type":"series", "name":"The Observatory", "year":"2024–",
        "description":"At a remote mountain observatory, a small team follows a signal that could change everything. A thoughtful story of discovery, doubt, and the people we choose to trust.",
        "videos":(1..=250).map(|index| serde_json::json!({"id":format!("scene-series:1:{index}"),"season":1,"episode":index,"title":format!("Chapter {index}")})).collect::<Vec<_>>()
    }))?);
    s.selected_video_id = Some("scene-series:1:1".into());
    s.status = "2 streams".into();
    s.streams = ["1080p · H.264 · 2.1 GB", "720p · H.264 · 1.2 GB"]
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
    s.page = Page::Library;
    s.config_busy = false;
    s.status = "Saved on this device.".into();
    s.library.snapshot = Some(Snapshot {
        watchlist: [
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
        watched: vec![],
        playlists: vec![],
        items: vec![],
        selected_state: None,
        episode_states: vec![],
    });
}
