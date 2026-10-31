use super::*;
use crate::library_worker::Worker;
use scry_library::{
    Library,
    guide::{EpisodeOrder, GuideAction, SourcePolicy},
};

#[test]
fn sidebar_shows_the_build_version() {
    let mut rt = rune::Runtime::new(init(), view, (880.0, 600.0), rune::Env::default());
    rt.frame(0.0);
    let expected = super::super::formatting::build_version(
        option_env!("SCRY_BUILD_VERSION").unwrap_or(env!("CARGO_PKG_VERSION")),
    );
    assert!(
        rt.elements()
            .iter()
            .any(|element| element.text.as_deref() == Some(expected.as_str())),
        "The sidebar must identify the installed build as {expected}"
    );
}

#[test]
fn settings_tabs_show_one_heading() {
    use super::super::settings::SettingsTab;
    for (tab, title) in [
        (SettingsTab::Streaming, "Streaming"),
        (SettingsTab::Subtitles, "Subtitles"),
        (SettingsTab::Guide, "Guide"),
    ] {
        let mut state = init();
        state.navigation.page = Page::Settings;
        state.settings.tab = tab;
        let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
        rt.frame(0.0);
        let elements = rt.elements();
        assert_eq!(
            elements
                .iter()
                .filter(|element| element.path.contains("/settings-page/"))
                .filter(|element| element.text.as_deref() == Some(title))
                .count(),
            1,
            "Repeated heading {title}"
        );
    }
}

#[test]
fn settings_tabs_show_only_the_selected_page_and_dismiss_language_menus_before_switching() {
    let mut state = init();
    state.navigation.page = Page::Settings;
    let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
    rt.frame(0.0);
    assert!(rt.find("streaming-settings").is_some());
    tap(&mut rt, "settings-tab-subtitles");
    // Let the new panel's layout settle before clicking its language trigger.
    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rt.find("subtitle-settings").is_some());
    tap(&mut rt, "subtitle-primary-language");
    assert!(rt.state.settings.subtitles.language_menu.is_some());
    // An outside press dismisses the modal without activating the tab below it.
    tap(&mut rt, "settings-tab-guide");
    assert!(rt.state.settings.subtitles.language_menu.is_none());
    assert!(rt.find("subtitle-settings").is_some());
    tap(&mut rt, "settings-tab-guide");
    assert!(rt.find("guide-autoplay-off").is_some());
    tap(&mut rt, "settings-tab-streaming");
    assert!(rt.find("streaming-settings").is_some());
}

#[test]
fn clear_cache_is_available_beside_sizes_but_disabled_during_playback_or_clearing() {
    for (playing, clearing, enabled) in [
        (false, false, true),
        (true, false, false),
        (false, true, false),
    ] {
        let mut state = init();
        state.navigation.page = Page::Settings;
        state.player.playing = playing;
        state.settings.streaming.clearing = clearing;
        let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
        rt.frame(0.0);
        let control = rt
            .elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some("streaming-cache-clear"))
            .unwrap();
        assert_eq!(control.focusable, enabled);
        assert!(rt.find("streaming-cache-2").is_some());
    }
}

#[test]
fn cache_clear_completion_reports_the_result_without_changing_the_budget() {
    for result in [Ok(()), Err("Cache is in use".into())] {
        let (mut state, tx) = state_with_events();
        state.settings.streaming.preferences.cache_gib = 10;
        state.settings.streaming.clearing = true;
        let failed = result.is_err();
        tx.try_send(Event::TorrentCacheCleared(result)).unwrap();
        poll(&mut state, 0.0, &mut Tx::default());
        assert!(!state.settings.streaming.clearing);
        assert_eq!(state.settings.streaming.cache_error, failed);
        assert!(state.settings.streaming.cache_message.is_some());
        assert_eq!(state.settings.streaming.preferences.cache_gib, 10);
    }
}

#[test]
fn streaming_cache_choices_apply_without_changing_current_playback() {
    let mut state = init();
    state.navigation.page = Page::Settings;
    state.player.playing = true;
    let generation = state.player.generation;
    let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
    rt.frame(0.0);
    assert_eq!(rt.state.settings.streaming.preferences.cache_gib, 2);
    for cache_gib in [5, 10, 20, 2] {
        tap(&mut rt, &format!("streaming-cache-{cache_gib}"));
        assert_eq!(rt.state.settings.streaming.preferences.cache_gib, cache_gib);
        assert!(rt.state.player.playing);
        assert_eq!(rt.state.player.generation, generation);
    }
}

#[test]
fn streaming_cache_choices_are_disabled_until_settings_finish_loading_or_saving() {
    let mut state = init();
    state.navigation.page = Page::Settings;
    state.settings.streaming.busy = true;
    let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
    rt.frame(0.0);
    let choice = rt
        .elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some("streaming-cache-5"))
        .unwrap();
    assert!(!choice.focusable);
    tap(&mut rt, "streaming-cache-5");
    assert_eq!(rt.state.settings.streaming.preferences.cache_gib, 2);
}

#[test]
fn streaming_cache_save_failure_keeps_saved_value_and_requested_retry() {
    let mut state = init();
    state.navigation.page = Page::Settings;
    state.runtime.services = Some(crate::services::Services::install_fixture().unwrap().0);
    let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
    rt.frame(0.0);
    tap(&mut rt, "streaming-cache-10");
    assert_eq!(rt.state.settings.streaming.preferences.cache_gib, 2);
    assert_eq!(
        rt.state
            .settings
            .streaming
            .pending
            .as_ref()
            .unwrap()
            .cache_gib,
        10
    );
    assert!(rt.find("streaming-settings-error").is_some());
    tap(&mut rt, "streaming-settings-retry");
    assert_eq!(
        rt.state
            .settings
            .streaming
            .pending
            .as_ref()
            .unwrap()
            .cache_gib,
        10
    );
}

#[test]
fn streaming_cache_controls_fit_small_and_wide_settings_pages() {
    for (width, height) in [(880.0, 600.0), (1920.0, 1080.0)] {
        let mut state = init();
        state.navigation.page = Page::Settings;
        let mut rt = rune::Runtime::new(state, view, (width, height), rune::Env::default());
        rt.frame(0.0);
        let panel = rect(&rt, "streaming-settings");
        let clear = rect(&rt, "streaming-cache-clear");
        assert!(clear[0] >= panel[0]);
        assert!(clear[0] + clear[2] <= panel[0] + panel[2]);
        assert!(clear[1] + clear[3] <= height);
        for cache_gib in [2, 5, 10, 20] {
            let choice = rect(&rt, &format!("streaming-cache-{cache_gib}"));
            assert!(choice[0] >= panel[0]);
            assert!(choice[0] + choice[2] <= panel[0] + panel[2]);
            assert!(choice[1] + choice[3] <= height);
            assert_eq!(
                choice[3],
                super::super::design::tokens::control::SETTING_HEIGHT
            );
        }
    }
}

#[test]
fn sidebar_settings_exposes_guide_preferences_and_keeps_addons_separate() {
    let mut state = init();
    state.addons.busy = false;
    let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
    rt.frame(0.0);
    tap(&mut rt, "nav-settings");
    // Let route layout motion settle before clicking the settings tabs.
    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rt.find("settings-page").is_some());
    tap(&mut rt, "settings-tab-guide");
    for key in [
        "guide-provider-any",
        "guide-quality-1080",
        "guide-sources-torrents",
        "guide-order-sequential",
        "guide-episodes-watched",
    ] {
        assert!(rt.find(key).is_some(), "Missing preference {key}");
    }
    let settings = rect(&rt, "nav-settings");
    assert!(settings[1] + settings[3] <= 600.0);
    rt.update(|_, tx| tx.scroll_into_view("nav-addons"));
    rt.frame(0.0);
    tap(&mut rt, "nav-addons");
    assert_eq!(rt.state.navigation.page, Page::Addons);
    assert!(rt.find("add-addon").is_some());
    rt.update(|s, _| s.travel(false));
    rt.frame(0.0);
    assert!(rt.find("settings-page").is_some());
}

#[test]
fn guide_settings_shortcut_is_separate_from_collection_management() {
    let mut state = init();
    state.addons.busy = false;
    state.navigation.page = Page::Guide;
    let mut rt = rune::Runtime::new(state, view, (1280.0, 820.0), rune::Env::default());
    rt.frame(0.0);
    tap(&mut rt, "guide-settings");
    assert!(rt.find("settings-page").is_some());
    assert!(rt.find("guide-autoplay-off").is_some());
    rt.update(|s, _| s.travel(false));
    rt.frame(0.0);
    assert_eq!(rt.state.navigation.page, Page::Guide);
    tap(&mut rt, "guide-manage-collections");
    assert!(rt.find("guide-create-collection").is_some());
    tap(&mut rt, "guide-close-collections");
    assert!(rt.find("guide-manage-collections").is_some());
}

#[test]
fn settings_loads_existing_guide_preferences_and_saves_all_controls() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.db");
    let now = chrono::Utc::now().timestamp();
    let saved = SourcePolicy {
        autoplay_disabled: false,
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
    state.addons.busy = false;
    state.settings.tab = super::super::settings::SettingsTab::Guide;
    state.detail.sources.provider = Some("Regular playback filter".into());
    state.addons.installed = vec![
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
        "guide-autoplay-off",
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
    assert!(policy.autoplay_disabled);
    assert!(policy.provider.is_empty());
    assert_eq!(policy.quality, "1080");
    assert!(!policy.allow_torrents);
    assert_eq!(policy.order, EpisodeOrder::Shuffle);
    assert!(!policy.watched_only);
    assert_eq!(
        rt.state.detail.sources.provider.as_deref(),
        Some("Regular playback filter")
    );
    assert!(!rt.state.player.playing);
    assert!(rt.state.guide.tuned.is_none());
}

#[test]
fn settings_disables_policy_edits_while_a_guide_update_is_pending() {
    let mut state = init();
    state.navigation.page = Page::Settings;
    state.settings.tab = super::super::settings::SettingsTab::Guide;
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
    let generation = state.runtime.streams.generation;
    state.navigate(Page::Settings);
    assert!(!state.player.playing);
    assert!(!state.guide.resolving);
    assert!(state.runtime.streams.generation > generation);
}

#[test]
fn settings_choices_use_the_right_half_of_wide_rows_and_fill_the_available_width() {
    let mut state = init();
    state.navigation.page = Page::Settings;
    state.settings.tab = super::super::settings::SettingsTab::Guide;
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
    assert!(
        (last[0] + last[2]
            - (panel[0] + panel[2] - super::super::design::tokens::layout::PANEL_INSET))
            .abs()
            < 1.0
    );
    for key in [
        "guide-quality-720",
        "guide-quality-1080",
        "guide-quality-2160",
    ] {
        let choice = rect(&rt, key);
        assert!((choice[2] - first[2]).abs() < 1.0);
        assert_eq!(
            choice[3],
            super::super::design::tokens::control::SETTING_HEIGHT
        );
    }
}

#[test]
fn sidebar_utility_pages_stay_pinned_when_content_links_scroll() {
    let mut state = init();
    state.addons.installed = vec![serde_json::from_value(serde_json::json!({
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
