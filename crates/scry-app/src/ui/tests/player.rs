use super::*;

fn player_runtime() -> rune::Runtime<State> {
    let mut state = init();
    state.navigation.page = Page::Player;
    state.player.playing = true;
    state.player.sample.duration_s = Some(200.0);
    state.player.sample.position_s = Some(50.0);
    let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt
}

fn key(rt: &mut rune::Runtime<State>, key: rune::Key) {
    rt.key(rune::KeyEvent {
        key,
        mods: rune::Mods::default(),
    });
    rt.frame(0.0);
}

#[test]
fn space_and_k_toggle_playback_and_button_matches_state() {
    let mut rt = player_runtime();
    key(&mut rt, rune::Key::Text(" ".into()));
    assert!(rt.state.player.sample.paused);
    key(&mut rt, rune::Key::Text("k".into()));
    assert!(!rt.state.player.sample.paused);
    tap(&mut rt, "pause");
    assert!(rt.state.player.sample.paused);
}

#[test]
fn shortcuts_do_not_steal_text_input_or_run_outside_active_player() {
    let mut rt = player_runtime();
    tap(&mut rt, "search");
    key(&mut rt, rune::Key::Text(" k m".into()));
    assert_eq!(rt.state.search.text, " k m");
    assert!(!rt.state.player.sample.paused);
    assert_eq!(rt.state.player.volume, 70);
    rt.update(|s, _| {
        s.navigation.page = Page::Addons;
    });
    assert!(!on_key(
        &mut rt.state,
        &rune::KeyEvent {
            key: rune::Key::Text(" ".into()),
            mods: rune::Mods::default(),
        },
        &mut Tx::default()
    ));
    rt.state.navigation.page = Page::Player;
    rt.state.player.playing = false;
    assert!(!on_key(
        &mut rt.state,
        &rune::KeyEvent {
            key: rune::Key::Text(" ".into()),
            mods: rune::Mods::default(),
        },
        &mut Tx::default()
    ));
}

#[test]
fn volume_keys_are_bounded_and_mute_restores_previous_volume() {
    let mut rt = player_runtime();
    key(&mut rt, rune::Key::Up);
    assert_eq!(rt.state.player.volume, 75);
    key(&mut rt, rune::Key::Text("m".into()));
    assert!(rt.state.player.muted);
    key(&mut rt, rune::Key::Text("m".into()));
    assert!(!rt.state.player.muted);
    assert_eq!(rt.state.player.volume, 75);
    for _ in 0..30 {
        key(&mut rt, rune::Key::Up);
    }
    assert_eq!(rt.state.player.volume, 100);
    for _ in 0..30 {
        key(&mut rt, rune::Key::Down);
    }
    assert_eq!(rt.state.player.volume, 0);
}

#[test]
fn timeline_supports_click_and_drag_and_controls_fit_minimum_window() {
    let mut rt = player_runtime();
    let [x, y, w, h] = rect(&rt, "player-timeline");
    rt.pointer_down(x + w * 0.25, y + h / 2.0);
    rt.pointer_move(x + w * 0.75, y + h / 2.0);
    rt.frame(0.0);
    assert_eq!(rt.state.player.seek_preview, Some(150.0));
    rt.pointer_up(x + w * 0.75, y + h / 2.0);
    rt.frame(0.0);
    assert!(rt.state.player.seek_preview.is_none());
    assert_eq!(rt.state.player.sample.position_s, Some(150.0));
    rt.pointer_down(x + w * 0.5, y + h / 2.0);
    rt.pointer_up(x + w * 0.5, y + h / 2.0);
    rt.frame(0.0);
    assert_eq!(rt.state.player.sample.position_s, Some(100.0));
    for name in [
        "pause",
        "mute",
        "player-volume",
        "subtitles",
        "audio",
        "fullscreen",
        "stop",
    ] {
        let [x, y, w, h] = rect(&rt, name);
        assert!(
            x >= 128.0 && y >= 0.0 && x + w <= 880.0 && y + h <= 600.0,
            "{name} is outside the window"
        );
    }
}

#[test]
fn volume_slider_unmutes_and_unknown_duration_cannot_be_scrubbed() {
    let mut rt = player_runtime();
    key(&mut rt, rune::Key::Text("m".into()));
    let [x, y, w, h] = rect(&rt, "player-volume");
    rt.pointer_down(x + w * 0.5, y + h / 2.0);
    rt.pointer_up(x + w * 0.5, y + h / 2.0);
    rt.frame(0.0);
    assert_eq!(rt.state.player.volume, 50);
    assert!(!rt.state.player.muted);
    for duration in [None, Some(0.0), Some(f64::NAN), Some(f64::INFINITY)] {
        rt.update(|s, _| s.player.sample.duration_s = duration);
        rt.frame(0.0);
        tap(&mut rt, "player-timeline");
        assert_eq!(rt.state.player.sample.position_s, Some(50.0));
        assert!(rt.state.player.seek_preview.is_none());
    }
}

#[test]
fn modified_player_keys_are_left_to_the_app() {
    let mut rt = player_runtime();
    assert!(!on_key(
        &mut rt.state,
        &rune::KeyEvent {
            key: rune::Key::Text("k".into()),
            mods: rune::Mods {
                ctrl: true,
                ..rune::Mods::default()
            },
        },
        &mut Tx::default()
    ));
    assert!(!rt.state.player.sample.paused);
}

#[test]
fn focused_editor_keeps_player_arrows_and_escape_local() {
    let mut rt = player_runtime();
    tap(&mut rt, "search");
    for key_value in [rune::Key::Up, rune::Key::Down, rune::Key::Escape] {
        key(&mut rt, key_value);
    }
    assert_eq!(rt.state.player.volume, 70);
    assert_eq!(rt.state.navigation.page, Page::Player);
    assert!(rt.state.player.playing);
}

#[test]
fn other_screens_can_use_player_actions_without_enabling_player_shortcuts() {
    use crate::ui::player::actions::PlayerAction;

    let mut state = init();
    state.navigation.page = Page::Library;
    state.player.playing = true;
    state.player_action(PlayerAction::TogglePause);
    state.player_action(PlayerAction::ToggleMute);
    assert!(state.player.sample.paused);
    assert!(state.player.muted);
    state.player_action(PlayerAction::SetVolume(200));
    assert_eq!(state.player.volume, 100);
    assert!(!state.player.muted);
    state.player_action(PlayerAction::SeekTo(35.0));
    assert_eq!(state.player.sample.position_s, Some(35.0));
    assert!(!on_key(
        &mut state,
        &rune::KeyEvent {
            key: rune::Key::Text("k".into()),
            mods: rune::Mods::default(),
        },
        &mut Tx::default()
    ));
    assert!(state.player.sample.paused);
    assert_eq!(state.navigation.page, Page::Library);
}

#[test]
fn player_actions_do_not_change_an_inactive_player() {
    use crate::ui::player::actions::PlayerAction;

    let mut state = init();
    for action in [
        PlayerAction::TogglePause,
        PlayerAction::ToggleMute,
        PlayerAction::SetVolume(12),
        PlayerAction::ChangeVolume(5),
        PlayerAction::SeekTo(35.0),
        PlayerAction::Stop,
    ] {
        state.player_action(action);
    }
    assert!(!state.player.sample.paused);
    assert!(!state.player.muted);
    assert_eq!(state.player.volume, 70);
    assert_eq!(state.player.sample.position_s, None);
    assert_eq!(state.player.generation, 0);
}

#[test]
fn fullscreen_shortcut_hides_chrome_and_escape_exits_without_leaving_player() {
    let mut rt = player_runtime();
    key(&mut rt, rune::Key::Text("f".into()));
    assert!(rt.find("sidebar").is_none());
    assert!(rt.find("topbar").is_none());
    assert!(rt.find("player-controls").is_some());
    key(&mut rt, rune::Key::Escape);
    assert_eq!(rt.state.navigation.page, Page::Player);
    assert!(rt.state.player.playing);
    assert!(rt.find("sidebar").is_some());
    key(&mut rt, rune::Key::Text("F".into()));
    key(&mut rt, rune::Key::Text("f".into()));
    assert!(rt.find("sidebar").is_some());
}

#[test]
fn fullscreen_button_matches_shortcut_and_does_not_steal_typed_f() {
    let mut rt = player_runtime();
    tap(&mut rt, "search");
    key(&mut rt, rune::Key::Text("f".into()));
    assert_eq!(rt.state.search.text, "f");
    assert!(rt.find("sidebar").is_some());
    tap(&mut rt, "fullscreen");
    assert!(rt.find("sidebar").is_none());
    tap(&mut rt, "fullscreen");
    assert!(rt.find("sidebar").is_some());
}

#[test]
fn opening_player_shows_loading_instead_of_an_empty_video_surface() {
    let rt = player_runtime();
    assert!(rt.find("player-loading").is_some());
    assert!(rt.find("player-loading-title").is_some());
    assert!(rt.find("player-back").is_some());
    assert_eq!(rect(&rt, "player-loading"), rect(&rt, "video"));
}

#[test]
fn cached_artwork_draws_a_backdrop_and_original_logo_in_both_window_modes() {
    let background = rune::Pixels::new();
    background.set(2, 1, vec![50; 8]);
    let logo = rune::Pixels::new();
    logo.set(2, 1, vec![255; 8]);
    let mut state = init();
    state.navigation.page = Page::Player;
    state.player.playing = true;
    state.player.background = Some("https://example.com/background".into());
    state.player.logo = Some("https://example.com/logo".into());
    state.player_artwork = Some(crate::artwork::Artwork::player_fixture(&[
        ("https://example.com/background", background),
        ("https://example.com/logo", logo),
    ]));
    let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    assert!(rt.find("player-loading-title").is_none());
    assert!(rt.find("player-loading-logo").is_some());
    assert_eq!(rect(&rt, "player-loading-background"), rect(&rt, "video"));
    key(&mut rt, rune::Key::Text("f".into()));
    assert!(rt.find("player-loading-logo").is_some());
    assert!(rt.find("player-back").is_some());
    let [x, y, w, h] = rect(&rt, "player-loading-logo");
    assert!(x >= 0.0 && y >= 0.0 && x + w <= 880.0 && y + h <= 600.0);
    assert_eq!(rect(&rt, "player-loading-background"), rect(&rt, "video"));
}

#[test]
fn only_current_playback_ready_event_replaces_loading_artwork() {
    let (mut s, events) = state_with_events();
    s.navigation.page = Page::Player;
    s.player.playing = true;
    s.player.generation = 4;
    events.try_send(Event::Ready(3)).unwrap();
    poll(&mut s, 0.0, &mut Tx::default());
    assert!(!s.player.ready);
    events
        .try_send(Event::Metrics(
            4,
            scry_playback::telemetry::Sample {
                position_s: Some(0.0),
                ..Default::default()
            },
        ))
        .unwrap();
    poll(&mut s, 0.0, &mut Tx::default());
    assert!(
        !s.player.ready,
        "a timestamp alone does not mean video is ready"
    );
    events.try_send(Event::Ready(4)).unwrap();
    poll(&mut s, 0.0, &mut Tx::default());
    assert!(s.player.ready);
    let mut rt = rune::Runtime::new(s, view, (880.0, 600.0), rune::Env::default());
    rt.frame(0.0);
    assert!(rt.find("player-loading").is_none());
    assert!(rt.find("video").is_some());
}

#[test]
fn stop_and_failure_leave_fullscreen_and_reset_loading() {
    let mut rt = player_runtime();
    key(&mut rt, rune::Key::Text("f".into()));
    rt.state.player.ready = true;
    rt.state.stop();
    assert!(!rt.state.player.fullscreen);
    assert!(!rt.state.player.ready);
    assert!(!rt.state.player.playing);

    let (mut s, events) = state_with_events();
    s.player.playing = true;
    s.player.fullscreen = true;
    s.player.ready = true;
    events
        .try_send(Event::Stopped(0, Some("Failed".into())))
        .unwrap();
    poll(&mut s, 0.0, &mut Tx::default());
    assert!(!s.player.fullscreen);
    assert!(!s.player.ready);
    assert!(!s.player.playing);
    assert_eq!(s.error.as_deref(), Some("Failed"));
    events.try_send(Event::Ready(0)).unwrap();
    poll(&mut s, 0.0, &mut Tx::default());
    assert!(
        !s.player.ready,
        "late ready events must not reveal stopped video"
    );
}

#[test]
fn loading_artwork_is_prefetched_and_owned_by_the_playing_title() {
    use crate::ui::artwork::player_artwork;

    let mut s = init();
    s.navigation.page = Page::Detail;
    s.detail = Some(
        serde_json::from_value(serde_json::json!({
            "id":"tt123", "type":"movie", "name":"A movie",
            "background":"https://example.com/backdrop.jpg",
            "logo":"https://example.com/logo.png"
        }))
        .unwrap(),
    );
    assert_eq!(
        player_artwork(&s),
        [
            "https://example.com/backdrop.jpg",
            "https://example.com/logo.png"
        ]
    );
    s.navigation.page = Page::Player;
    s.player.playing = true;
    s.player.background = Some("https://example.com/playing.jpg".into());
    s.player.logo = Some("https://example.com/playing.png".into());
    assert_eq!(
        player_artwork(&s),
        [
            "https://example.com/playing.jpg",
            "https://example.com/playing.png"
        ]
    );
    s.player.ready = true;
    assert!(player_artwork(&s).is_empty());
}
