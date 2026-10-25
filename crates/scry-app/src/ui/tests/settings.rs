use super::*;
use crate::library_worker::Worker;
use scry_library::{
    Library,
    guide::{EpisodeOrder, GuideAction, SourcePolicy},
};

#[test]
fn sidebar_settings_exposes_guide_preferences_and_keeps_addons_separate() {
    let mut state = init();
    state.config_busy = false;
    let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
    rt.frame(0.0);
    tap(&mut rt, "nav-settings");
    assert!(rt.find("settings-page").is_some());
    for key in [
        "guide-provider-any",
        "guide-quality-1080",
        "guide-sources-torrents",
        "guide-order-sequential",
        "guide-episodes-watched",
    ] {
        assert!(rt.find(key).is_some(), "Missing preference {key}");
    }
    assert!(rt.find("guide-create-collection").is_none());
    assert!(rt.find("add-addon").is_none());
    let settings = rect(&rt, "nav-settings");
    assert!(settings[1] + settings[3] <= 600.0);
    rt.update(|_, tx| tx.scroll_into_view("nav-addons"));
    rt.frame(0.0);
    tap(&mut rt, "nav-addons");
    assert_eq!(rt.state.navigation.page, Page::Addons);
    assert!(rt.find("add-addon").is_some());
    assert!(rt.find("guide-quality-1080").is_none());
    rt.update(|s, _| s.travel(false));
    rt.frame(0.0);
    assert!(rt.find("settings-page").is_some());
}

#[test]
fn guide_settings_shortcut_is_separate_from_collection_management() {
    let mut state = init();
    state.config_busy = false;
    state.navigation.page = Page::Guide;
    let mut rt = rune::Runtime::new(state, view, (1280.0, 820.0), rune::Env::default());
    rt.frame(0.0);
    tap(&mut rt, "guide-settings");
    assert!(rt.find("settings-page").is_some());
    assert!(rt.find("guide-create-collection").is_none());
    rt.update(|s, _| s.travel(false));
    rt.frame(0.0);
    assert_eq!(rt.state.navigation.page, Page::Guide);
    tap(&mut rt, "guide-manage-collections");
    assert!(rt.find("guide-create-collection").is_some());
    assert!(rt.find("guide-quality-1080").is_none());
    tap(&mut rt, "guide-close-collections");
    assert!(rt.find("guide-manage-collections").is_some());
}

#[test]
fn settings_loads_existing_guide_preferences_and_saves_all_five_controls() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.db");
    let now = chrono::Utc::now().timestamp();
    let saved = SourcePolicy {
        provider: "Streams".into(),
        quality: "720".into(),
        allow_torrents: true,
        order: EpisodeOrder::Sequential,
        watched_only: true,
    };
    Library::open(&path)
        .unwrap()
        .guide_update(GuideAction::Policy(saved.clone()), now)
        .unwrap();
    let mut state = init();
    state.config_busy = false;
    state.stream_provider = Some("Regular playback filter".into());
    state.addons = vec![
        serde_json::from_value(serde_json::json!({
            "manifest_url":"https://example.com/private/manifest.json",
            "manifest":{"id":"streams","name":"Streams","resources":["stream"]}
        }))
        .unwrap(),
    ];
    state.library.worker = Some(Worker::start(path.clone(), Arc::new(|| {})).unwrap());
    state.navigate(Page::Settings);
    let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
    rt.poll = Some(poll);
    settle_library(&mut rt, |s| !s.guide.busy);
    assert_eq!(rt.state.guide.data.policy.provider, saved.provider);
    assert_eq!(rt.state.guide.data.policy.quality, saved.quality);
    assert!(rt.state.guide.data.policy.allow_torrents);
    assert_eq!(rt.state.guide.data.policy.order, saved.order);
    assert!(rt.state.guide.data.policy.watched_only);
    for key in [
        "guide-provider-any",
        "guide-quality-1080",
        "guide-sources-direct",
        "guide-order-shuffle",
        "guide-episodes-all",
    ] {
        rt.update(|_, tx| tx.scroll_into_view(key));
        rt.frame(0.0);
        tap(&mut rt, key);
        settle_library(&mut rt, |s| !s.guide.busy);
        assert!(rt.state.guide.error.is_none());
    }
    let mut db = Library::open(&path).unwrap();
    let policy = db.guide_update(GuideAction::Refresh, now).unwrap().policy;
    assert!(policy.provider.is_empty());
    assert_eq!(policy.quality, "1080");
    assert!(!policy.allow_torrents);
    assert_eq!(policy.order, EpisodeOrder::Shuffle);
    assert!(!policy.watched_only);
    assert_eq!(
        rt.state.stream_provider.as_deref(),
        Some("Regular playback filter")
    );
    assert!(!rt.state.player.playing);
    assert!(rt.state.guide.tuned.is_none());
}

#[test]
fn settings_disables_policy_edits_while_a_guide_update_is_pending() {
    let mut state = init();
    state.navigation.page = Page::Settings;
    state.guide.busy = true;
    let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
    rt.frame(0.0);
    let control = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("guide-quality-1080"))
        .unwrap();
    assert!(!control.focusable);
    tap(&mut rt, "guide-quality-1080");
    assert!(rt.state.guide.data.policy.quality.is_empty());
    assert!(rt.state.guide.error.is_none());
}

#[test]
fn opening_settings_stops_guide_playback_and_invalidates_pending_sources() {
    let mut state = init();
    state.navigation.page = Page::Guide;
    state.guide.resolving = true;
    state.player.playing = true;
    let generation = state.stream_id;
    state.navigate(Page::Settings);
    assert!(!state.player.playing);
    assert!(!state.guide.resolving);
    assert!(state.stream_id > generation);
}

#[test]
fn settings_choices_use_the_right_half_of_wide_rows_and_fill_the_available_width() {
    let mut state = init();
    state.navigation.page = Page::Settings;
    let mut rt = rune::Runtime::new(state, view, (1920.0, 1080.0), rune::Env::default());
    rt.frame(0.0);
    let panel = rect(&rt, "guide-source-settings");
    let first = rect(&rt, "guide-quality-");
    let last = rect(&rt, "guide-quality-2160");
    assert!(first[0] >= panel[0] + panel[2] * 0.45);
    assert!(
        first[2] >= 100.0,
        "Quality choices should use the wide control column"
    );
    assert!((last[0] + last[2] - (panel[0] + panel[2] - 24.0)).abs() < 1.0);
    for key in [
        "guide-quality-720",
        "guide-quality-1080",
        "guide-quality-2160",
    ] {
        let choice = rect(&rt, key);
        assert!((choice[2] - first[2]).abs() < 1.0);
        assert!(choice[3] >= 44.0);
    }
}

#[test]
fn sidebar_utility_pages_stay_pinned_when_content_links_scroll() {
    let mut state = init();
    state.addons = vec![serde_json::from_value(serde_json::json!({
        "manifest_url":"https://example.com/manifest.json",
        "manifest":{"id":"categories","name":"Categories","catalogs":
            (0..20).map(|i| serde_json::json!({"id":format!("category-{i}"),"type":format!("kind-{i}")})).collect::<Vec<_>>()}
    })).unwrap()];
    let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
    rt.frame(0.0);
    let viewport = rect(&rt, "sidebar-pages");
    let keys = ["nav-stats", "nav-addons", "nav-settings"];
    let before = keys.map(|key| rect(&rt, key));
    for bounds in before {
        assert!(bounds[1] >= viewport[1] + viewport[3]);
        assert!(bounds[1] + bounds[3] <= 600.0);
    }
    assert!(before[0][1] < before[1][1] && before[1][1] < before[2][1]);
    let watching = rect(&rt, "nav-watching");
    rt.wheel(viewport[0] + 20.0, viewport[1] + 20.0, -100_000.0);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rect(&rt, "nav-watching")[1] < watching[1]);
    for (key, bounds) in keys.into_iter().zip(before) {
        assert_eq!(rect(&rt, key), bounds);
    }
    for (key, page) in [
        ("nav-stats", Page::Stats),
        ("nav-addons", Page::Addons),
        ("nav-settings", Page::Settings),
    ] {
        tap(&mut rt, key);
        assert_eq!(rt.state.navigation.page, page);
    }
}
