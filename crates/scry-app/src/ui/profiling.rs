//! Local, opt-in benchmark control. This entire module is absent from normal builds.
use super::{Page, State};
use rune::Tx;
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};

pub(super) struct Driver {
    root: PathBuf,
    last: u64,
}

impl Driver {
    pub(super) fn start(_: rune::Waker) -> Option<Self> {
        let root = PathBuf::from(std::env::var_os("SCRY_PROFILE_CONTROL")?);
        // Never automate the user's database accidentally.
        std::env::var_os("SCRY_PROFILE_ROOT")?;
        std::fs::create_dir_all(&root).ok()?;
        Some(Self { root, last: 0 })
    }
}

pub(super) fn poll(s: &mut State, tx: &mut Tx) -> bool {
    let Some(mut driver) = s.profiling.take() else {
        return false;
    };
    tx.wake_after(Duration::from_millis(100));
    let command = std::fs::read(driver.root.join("command.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    let mut changed = false;
    if let Some(command) = command
        && let Some(id) = command["id"].as_u64().filter(|id| *id > driver.last)
    {
        driver.last = id;
        changed = true;
        match command["action"].as_str().unwrap_or("") {
            "play" => {
                s.player.volume = 0;
                if let Some(source) = command["source"].as_str() {
                    s.startup_play = Some(source.to_owned());
                }
            }
            "play_stream" => {
                s.player.volume = 0;
                match serde_json::from_value::<scry_addons::Stream>(command["stream"].clone()) {
                    Ok(stream) => s.play_source(
                        crate::services::Source::Stream(stream),
                        "Benchmark stream".into(),
                    ),
                    Err(_) => s.error = Some("Invalid profiling stream".into()),
                }
            }
            "pause" => s.command(&["cycle", "pause"]),
            "seek" => {
                let position = command["seconds"].as_f64().unwrap_or(0.0).to_string();
                s.command(&["seek", &position, "absolute"]);
            }
            "stop" => s.stop(),
            "back" => s.travel(false),
            "forward" => s.travel(true),
            "catalog" => {
                s.navigate(Page::Discover);
                s.catalog = None;
                let count = command["count"].as_u64().unwrap_or(1000).min(10_000);
                s.metas = std::sync::Arc::new(
                    (0..count)
                        .filter_map(|index| {
                            let poster = command["artwork_base"]
                                .as_str()
                                .map(|base| format!("{base}/{index}.png"));
                            serde_json::from_value(json!({
                                "id": format!("profile-{index}"), "type": "movie",
                                "name": format!("Benchmark movie {index}"),
                                "poster": poster,
                                "description": "Deterministic catalog fixture. ".repeat(16)
                            }))
                            .ok()
                        })
                        .collect(),
                );
                s.loading = false;
            }
            "detail" => super::scenes::scene_detail(s),
            "metadata_stress" => metadata_stress(s),
            "library" => s.navigate(Page::Library),
            "calendar" => s.navigate(Page::Calendar),
            "addons" => s.navigate(Page::Addons),
            _ => s.error = Some("Unknown profiling action".into()),
        }
    }
    let status = json!({
        "command_id": driver.last,
        "page": format!("{:?}", s.navigation.page),
        "playing": s.player.playing, "ready": s.player.ready,
        "fullscreen": s.player.fullscreen,
        "sample": s.player.sample,
        "error": s.error,
        "catalog_len": s.metas.len(),
        "detail_episodes": s.detail.as_ref().map_or(0, |meta| meta.videos.len()),
        "geometry_rows": s.poster_rects.len(),
        "first_row_y": s.poster_rects.first_key_value().map(|(_, rect)| rect[1]),
        "back_len": s.navigation.back.len(), "forward_len": s.navigation.forward.len(),
        "artwork": s.artwork.as_ref().map(crate::artwork::Artwork::stats),
        "player_artwork": s.player_artwork.as_ref().map(crate::artwork::Artwork::stats),
    });
    let path = driver.root.join("status.tmp");
    if let Ok(bytes) = serde_json::to_vec(&status)
        && std::fs::write(&path, bytes).is_ok()
    {
        let _ = std::fs::rename(path, driver.root.join("status.json"));
    }
    s.profiling = Some(driver);
    changed
}

fn metadata_stress(s: &mut State) {
    super::scenes::scene_detail(s);
    if let Some(meta) = s.detail.as_mut() {
        meta.videos = (1..=10_000)
            .map(|index| scry_addons::Video {
                id: format!("scene-series:1:{index}"),
                title: format!("Benchmark chapter {index}: a long episode title"),
                season: Some(1),
                episode: Some(index),
                released: None,
            })
            .collect();
    }
    for _ in 0..32 {
        s.remember();
    }
}
