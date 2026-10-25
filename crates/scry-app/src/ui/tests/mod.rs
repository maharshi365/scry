//! Cross-feature UI tests. Pure feature behavior stays with its owning module.
use super::{
    Page, State,
    app::{init, on_key},
    artwork::visible_artwork,
    events::poll,
    scenes::{scene_detail, scene_discover},
    shell::view,
};
use crate::services::Event;
use rune::{TextEdit, Tx};
use scry_addons::{Meta, Stream};
use std::sync::Arc;

mod calendar;
mod catalog;
mod context_menu;
mod controls;
mod design_contract;
mod dropdowns;
mod episodes;
mod layout;
mod library;
mod navigation;
mod playback;
mod player;
mod presentation;
mod series;
mod settings;
mod sources;
mod stats;
mod title_screen;
mod up_next;
mod watch_rules;

fn rect(rt: &rune::Runtime<State>, key: &str) -> [f32; 4] {
    rt.elements()
        .into_iter()
        .find(|element| !element.exiting && element.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("Missing {key}"))
        .rect
}

fn settle_library(rt: &mut rune::Runtime<State>, ready: impl Fn(&State) -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !ready(&rt.state) {
        assert!(
            std::time::Instant::now() < deadline,
            "Library UI did not settle: {:?}",
            rt.state.library.error
        );
        rt.frame(1.0 / 60.0);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    rt.frame(1.0 / 60.0);
}

fn tap(rt: &mut rune::Runtime<State>, key: &str) {
    let (x, y) = rt
        .find(key)
        .unwrap_or_else(|| panic!("Missing UI action {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

pub(super) fn series(count: usize) -> Meta {
    serde_json::from_value(serde_json::json!({
        "id": "tt123", "type": "series", "name": "Long show",
        "videos": (1..=count).map(|n| serde_json::json!({
            "id": format!("tt123:1:{n}"), "title": format!("Episode {n}"), "season": 1, "episode": n
        })).collect::<Vec<_>>()
    }))
    .unwrap()
}

fn multi_season_series() -> Meta {
    serde_json::from_value(serde_json::json!({
        "id": "show", "type": "series", "name": "A show",
        "videos": [
            {"id":"show:2:2", "title":"Second finale", "season":2, "episode":2},
            {"id":"show:0:1", "title":"Special", "season":0, "episode":1},
            {"id":"show:1:2", "title":"First finale", "season":1, "episode":2},
            {"id":"show:2:1", "title":"Second premiere", "season":2, "episode":1},
            {"id":"show:1:1", "title":"First premiere", "season":1, "episode":1},
            {"id":"extra", "title":"Unnumbered"}
        ]
    }))
    .unwrap()
}

fn state_with_events() -> (State, tokio::sync::mpsc::Sender<Event>) {
    let mut state = init();
    let (tx, rx) = tokio::sync::mpsc::channel(16);
    state.events = Some(rx);
    (state, tx)
}
