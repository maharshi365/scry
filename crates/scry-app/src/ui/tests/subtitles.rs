use super::*;
use scry_playback::subtitles::{Snapshot, Track};

fn pending_external_subtitle() -> (State, tokio::sync::mpsc::Sender<Event>) {
    let (mut s, tx) = state_with_events();
    let (services, _requests) = crate::services::Services::install_fixture().unwrap();
    s.runtime.services = Some(services);
    s.navigation.page = Page::Player;
    s.player.playing = true;
    s.player.generation = 7;
    s.player.subtitles.slot = crate::ui::player::subtitles::Slot::Secondary;
    s.player.subtitles.sources = vec![crate::services::ExternalSubtitle {
        provider: "Provider".into(),
        subtitle: serde_json::from_value(serde_json::json!({
            "id":"variant", "lang":"eng", "label":"Variant",
            "url":"https://example.invalid/subtitle.srt"
        }))
        .unwrap(),
    }];
    s.load_external_subtitle(0);
    s.player.subtitles.slot = crate::ui::player::subtitles::Slot::Primary;
    (s, tx)
}

#[test]
fn matching_subtitle_load_failure_clears_pending_and_reports_the_error() {
    let (mut s, tx) = pending_external_subtitle();
    tx.try_send(Event::SubtitleLoadFailed {
        generation: 7,
        request: 1,
        error: "Cannot load this subtitle; try another variant".into(),
    })
    .unwrap();
    poll(&mut s, 0.0, &mut Tx::default());
    assert_eq!(
        s.player.subtitles.error.as_deref(),
        Some("Cannot load this subtitle; try another variant")
    );
    s.apply_subtitle_snapshot(
        7,
        Snapshot {
            tracks: vec![Track {
                id: 9,
                title: "Provider · Variant · #1".into(),
                language: "eng".into(),
                codec: "subrip".into(),
                external: true,
            }],
            ..Snapshot::default()
        },
    );
    assert_eq!(
        s.player.subtitles.selected[1], None,
        "The failed pending request must not select a late track"
    );
}

#[test]
fn stale_subtitle_load_failures_preserve_the_new_request_and_its_original_slot() {
    let (mut s, tx) = pending_external_subtitle();
    s.player.subtitles.slot = crate::ui::player::subtitles::Slot::Secondary;
    s.load_external_subtitle(0);
    s.player.subtitles.slot = crate::ui::player::subtitles::Slot::Primary;
    for (generation, request) in [(6, 2), (7, 1)] {
        tx.try_send(Event::SubtitleLoadFailed {
            generation,
            request,
            error: "Stale failure".into(),
        })
        .unwrap();
    }
    poll(&mut s, 0.0, &mut Tx::default());
    assert!(s.player.subtitles.error.is_none());
    s.apply_subtitle_snapshot(
        7,
        Snapshot {
            tracks: vec![Track {
                id: 9,
                title: "Provider · Variant · #2".into(),
                language: "eng".into(),
                codec: "subrip".into(),
                external: true,
            }],
            ..Snapshot::default()
        },
    );
    assert_eq!(
        s.player.subtitles.selected[1],
        Some(9),
        "Stale failures must preserve the pending request's secondary slot"
    );
    tx.try_send(Event::SubtitleLoadFailed {
        generation: 7,
        request: 2,
        error: "Late failure after success".into(),
    })
    .unwrap();
    poll(&mut s, 0.0, &mut Tx::default());
    assert!(
        s.player.subtitles.error.is_none(),
        "A failure without a pending request must not show an error"
    );
    assert_eq!(s.player.subtitles.selected[1], Some(9));
}

fn tracks() -> Vec<Track> {
    vec![
        Track {
            id: 1,
            language: "eng".into(),
            title: "Full dialogue".into(),
            codec: "subrip".into(),
            external: false,
        },
        Track {
            id: 2,
            language: "spa".into(),
            title: "Español".into(),
            codec: "subrip".into(),
            external: false,
        },
    ]
}

#[test]
fn subtitle_panel_uses_flat_language_rows_aligned_steppers_and_right_aligned_icon_actions() {
    let mut state = init();
    state.navigation.page = Page::Player;
    state.player.playing = true;
    state.player.ready = true;
    state.apply_subtitle_snapshot(
        0,
        Snapshot {
            tracks: tracks(),
            ..Snapshot::default()
        },
    );
    state.show_subtitles();
    let mut rt = rune::Runtime::new(state, view, (1280.0, 820.0), rune::Env::default());
    rt.frame(0.0);
    let languages = rect(&rt, "subtitle-languages");
    super::dropdowns::assert_flat_panel(&rt, "player-subtitles-overlay");
    let first = rect(&rt, "subtitle-track-off");
    assert!(first[0] > languages[0] && first[0] + first[2] < languages[0] + languages[2]);
    let timing = rect(&rt, "subtitle-delay-control");
    for key in ["subtitle-size-control", "subtitle-position-control"] {
        let bounds = rect(&rt, key);
        assert_eq!(bounds[0], timing[0]);
        assert_eq!(bounds[2], timing[2]);
    }
    let reset = rect(&rt, "subtitle-reset");
    let load = rect(&rt, "subtitle-load-local");
    assert!(reset[0] < load[0]);
    assert!((load[0] + load[2] - (languages[0] + languages[2])).abs() < 1.0);
    assert_eq!(load[2], load[3], "Load-file action must be an icon target");
    let close = rect(&rt, "subtitle-close");
    assert_eq!(close[2], close[3], "Close action must be an icon target");
    let tabs = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("subtitle-tabs"))
        .unwrap();
    assert_eq!(tabs.role, "tablist");
    tap(&mut rt, "subtitle-delay-more");
    tap(&mut rt, "subtitle-size-more");
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some("+0.1 s"))
    );
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some("125%"))
    );
    tap(&mut rt, "subtitle-reset");
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some("+0.0 s"))
    );
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some("100%"))
    );
    assert!(!rt.state.player.sample.paused);
}

#[test]
fn subtitle_hover_panel_selects_two_tracks_without_pausing_or_resizing_video() {
    let mut state = init();
    state.navigation.page = Page::Player;
    state.player.playing = true;
    state.player.ready = true;
    state.apply_subtitle_snapshot(
        0,
        Snapshot {
            tracks: tracks(),
            ..Snapshot::default()
        },
    );
    let mut rt = rune::Runtime::new(state, view, (1280.0, 820.0), rune::Env::default());
    rt.frame(0.0);
    let before = rect(&rt, "video");
    let (x, y) = rt.find("subtitles").unwrap();
    rt.pointer_move(x, y);
    rt.frame(0.0);
    assert!(rt.find("player-subtitles-overlay").is_some());
    tap(&mut rt, "subtitle-slot-secondary");
    assert!(rt.state.player.subtitles.slot == crate::ui::player::subtitles::Slot::Secondary);
    tap(&mut rt, "subtitle-track-2");
    assert_eq!(rt.state.player.subtitles.selected[0], Some(1));
    assert_eq!(rt.state.player.subtitles.selected[1], Some(2));
    assert!(!rt.state.player.sample.paused);
    assert_eq!(rect(&rt, "video"), before);
    tap(&mut rt, "subtitle-track-off");
    assert_eq!(rt.state.player.subtitles.selected, [Some(1), None]);
}

#[test]
fn subtitle_language_click_survives_focus_moving_from_the_trigger() {
    let mut state = init();
    state.navigation.page = Page::Player;
    state.player.playing = true;
    state.player.ready = true;
    state.apply_subtitle_snapshot(
        0,
        Snapshot {
            tracks: tracks(),
            ..Snapshot::default()
        },
    );
    let mut rt = rune::Runtime::new(state, view, (1280.0, 820.0), rune::Env::default());
    rt.frame(0.0);
    tap(&mut rt, "subtitles");
    let (x, y) = rt.find("subtitle-track-2").unwrap();
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    rt.frame(1.0 / 60.0);
    assert!(rt.state.player.subtitles.overlay.is_open());
    assert_eq!(rt.state.player.subtitles.selected[0], Some(1));
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.player.subtitles.selected[0], Some(2));
    assert!(rt.state.player.subtitles.overlay.is_open());
    assert!(!rt.state.player.sample.paused);
}

#[test]
fn subtitle_tabs_switch_on_release_without_dismissing_the_panel() {
    use crate::ui::player::subtitles::Slot;

    for fullscreen in [false, true] {
        for pinned in [false, true] {
            let mut state = init();
            state.navigation.page = Page::Player;
            state.player.playing = true;
            state.player.fullscreen = fullscreen;
            let mut rt = rune::Runtime::new(state, view, (1280.0, 820.0), rune::Env::default());
            rt.frame(0.0);
            if pinned {
                tap(&mut rt, "subtitles");
            } else {
                let (x, y) = rt.find("subtitles").unwrap();
                rt.pointer_move(x, y);
                rt.frame(0.0);
            }
            for (key, before, after) in [
                ("subtitle-slot-secondary", Slot::Primary, Slot::Secondary),
                ("subtitle-slot-primary", Slot::Secondary, Slot::Primary),
            ] {
                let (x, y) = rt.find(key).unwrap();
                rt.pointer_move(x, y);
                rt.pointer_down(x, y);
                rt.frame(1.0 / 60.0);
                assert!(rt.state.player.subtitles.slot == before);
                assert!(rt.state.player.subtitles.overlay.is_open());
                rt.pointer_up(x, y);
                rt.frame(1.0 / 60.0);
                assert!(rt.state.player.subtitles.slot == after);
                assert!(rt.state.player.subtitles.overlay.is_open());
            }
        }
    }
}

#[test]
fn subtitle_panel_closes_on_window_deactivation_after_a_language_row_takes_focus() {
    for fullscreen in [false, true] {
        let mut state = init();
        state.navigation.page = Page::Player;
        state.player.playing = true;
        state.player.fullscreen = fullscreen;
        let mut rt = rune::Runtime::new(state, view, (1280.0, 820.0), rune::Env::default());
        rt.frame(0.0);
        tap(&mut rt, "subtitles");
        tap(&mut rt, "subtitle-track-off");
        assert!(rt.state.player.subtitles.overlay.is_open());
        rt.window_focus(false);
        rt.frame(1.0 / 60.0);
        assert!(!rt.state.player.subtitles.overlay.is_open());
    }
}

#[test]
fn off_is_not_overridden_by_late_tracks_and_stale_snapshots_are_ignored() {
    let mut state = init();
    state.player.playing = true;
    state.player.generation = 3;
    state.apply_subtitle_snapshot(
        3,
        Snapshot {
            tracks: tracks(),
            ..Snapshot::default()
        },
    );
    state.select_subtitle(None);
    state.apply_subtitle_snapshot(
        3,
        Snapshot {
            tracks: tracks(),
            ..Snapshot::default()
        },
    );
    assert_eq!(state.player.subtitles.selected[0], None);
    state.apply_subtitle_snapshot(2, Snapshot::default());
    assert_eq!(state.player.subtitles.tracks.len(), 2);
}

#[test]
fn subtitle_defaults_are_english_and_settings_edits_do_not_change_current_tracks() {
    let mut state = init();
    state.navigation.page = Page::Settings;
    state.settings.tab = super::super::settings::SettingsTab::Subtitles;
    assert_eq!(
        state.settings.subtitle_preferences.primary.as_deref(),
        Some("eng")
    );
    assert!(state.settings.subtitle_preferences.secondary.is_none());
    let mut rt = rune::Runtime::new(state, view, (1280.0, 820.0), rune::Env::default());
    rt.frame(0.0);
    tap(&mut rt, "subtitle-primary-language");
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let menu = rect(&rt, "subtitle-language-menu");
    super::dropdowns::assert_flat_panel(&rt, "subtitle-language-menu");
    rt.wheel(menu[0] + 20.0, menu[1] + 20.0, -1200.0);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let choice = rect(&rt, "subtitle-language-spa");
    assert!(
        choice[1] >= menu[1] && choice[1] + choice[3] <= menu[1] + menu[3],
        "menu {menu:?}, choice {choice:?}"
    );
    tap(&mut rt, "subtitle-language-spa");
    assert_eq!(
        rt.state.settings.subtitle_preferences.primary.as_deref(),
        Some("spa")
    );
    assert_eq!(rt.state.player.subtitles.selected, [None, None]);
}

#[test]
fn changing_defaults_does_not_reselect_current_playback_and_toggle_restores_both_tracks() {
    let mut state = init();
    state.player.playing = true;
    state.apply_subtitle_snapshot(
        0,
        Snapshot {
            tracks: tracks(),
            ..Snapshot::default()
        },
    );
    let mut preferences = state.settings.subtitle_preferences.clone();
    preferences.primary = Some("spa".into());
    state.save_subtitle_preferences(preferences);
    state.apply_subtitle_snapshot(
        0,
        Snapshot {
            tracks: tracks(),
            ..Snapshot::default()
        },
    );
    assert_eq!(state.player.subtitles.selected[0], Some(1));
    state.player.subtitles.slot = crate::ui::player::subtitles::Slot::Secondary;
    state.select_subtitle(Some(2));
    state.toggle_subtitles();
    assert_eq!(state.player.subtitles.selected, [None, None]);
    state.toggle_subtitles();
    assert_eq!(state.player.subtitles.selected, [Some(1), Some(2)]);
    state.select_subtitle(Some(1));
    assert_eq!(state.player.subtitles.selected, [None, Some(1)]);
}

#[test]
fn pinned_subtitle_panel_closes_before_guide_fullscreen_and_fits_small_windows() {
    for page in [Page::Player, Page::Guide] {
        let mut state = init();
        state.navigation.page = page;
        state.player.playing = true;
        state.player.fullscreen = true;
        let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
        rt.on_key = Some(on_key);
        rt.frame(0.0);
        tap(&mut rt, "subtitles");
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        let panel = rect(&rt, "player-subtitles-overlay");
        assert!(panel[0] >= 0.0 && panel[1] >= 0.0);
        assert!(panel[0] + panel[2] <= 880.0 && panel[1] + panel[3] <= 600.0);
        assert!(rt.state.player.subtitles.overlay.is_open());
        rt.key(rune::KeyEvent {
            key: rune::Key::Escape,
            mods: rune::Mods::default(),
        });
        rt.frame(0.0);
        assert!(!rt.state.player.subtitles.overlay.is_open());
        assert!(rt.state.player.fullscreen);
    }
}

#[test]
#[ignore = "Requires the installed libmpv runtime and SCRY_SUBTITLE_TEST_VIDEO pointing to a local fixture"]
fn native_subtitle_adapter_supports_the_pinned_runtime_and_two_tracks() -> anyhow::Result<()> {
    use scry_playback::{PlaybackEvent, PlaybackTarget, Player};
    let video = std::env::var("SCRY_SUBTITLE_TEST_VIDEO")?;
    let dir = tempfile::tempdir()?;
    let first = dir.path().join("one.srt");
    let second = dir.path().join("two.srt");
    std::fs::write(&first, "1\n00:00:00,000 --> 00:01:00,000\nHello\n")?;
    std::fs::write(&second, "1\n00:00:00,000 --> 00:01:00,000\nHola\n")?;
    let engine = Player::open(PlaybackTarget::DecodeOnly)?;
    engine.command(&["set", "pause", "yes"])?;
    for command in scry_playback::subtitles::Preferences::default().commands() {
        let args: Vec<_> = command.iter().map(String::as_str).collect();
        engine.command(&args)?;
    }
    engine.load(&video)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        anyhow::ensure!(std::time::Instant::now() < deadline, "Fixture did not load");
        if engine.event(0.05) == Some(PlaybackEvent::Loaded) {
            break;
        }
    }
    engine.command(&[
        "sub-add",
        &first.to_string_lossy(),
        "auto",
        "English",
        "eng",
    ])?;
    engine.command(&[
        "sub-add",
        &second.to_string_lossy(),
        "auto",
        "Spanish",
        "spa",
    ])?;
    let snapshot = Snapshot::read(&engine);
    assert_eq!(snapshot.tracks.len(), 2);
    assert_eq!(snapshot.tracks[0].language, "eng");
    assert_eq!(snapshot.tracks[1].language, "spa");
    engine.command(&["set", "sid", &snapshot.tracks[0].id.to_string()])?;
    engine.command(&["set", "secondary-sid", &snapshot.tracks[1].id.to_string()])?;
    assert_eq!(
        Snapshot::read(&engine).selected,
        [Some(snapshot.tracks[0].id), Some(snapshot.tracks[1].id)]
    );
    Ok(())
}
