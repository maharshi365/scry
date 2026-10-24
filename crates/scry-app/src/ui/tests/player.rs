use super::*;

fn player_runtime() -> rune::Runtime<State> {
    let mut state = init();
    state.page = Page::Player;
    state.playing = true;
    state.sample.duration_s = Some(200.0);
    state.sample.position_s = Some(50.0);
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
    assert!(rt.state.sample.paused);
    key(&mut rt, rune::Key::Text("k".into()));
    assert!(!rt.state.sample.paused);
    tap(&mut rt, "pause");
    assert!(rt.state.sample.paused);
}

#[test]
fn shortcuts_do_not_steal_text_input_or_run_outside_active_player() {
    let mut rt = player_runtime();
    tap(&mut rt, "search");
    key(&mut rt, rune::Key::Text(" k m".into()));
    assert_eq!(rt.state.search.text, " k m");
    assert!(!rt.state.sample.paused);
    assert_eq!(rt.state.volume, 70);
    rt.update(|s, _| {
        s.page = Page::Addons;
    });
    assert!(!on_key(
        &mut rt.state,
        &rune::KeyEvent {
            key: rune::Key::Text(" ".into()),
            mods: rune::Mods::default(),
        },
        &mut Tx::default()
    ));
    rt.state.page = Page::Player;
    rt.state.playing = false;
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
    assert_eq!(rt.state.volume, 75);
    key(&mut rt, rune::Key::Text("m".into()));
    assert!(rt.state.muted);
    key(&mut rt, rune::Key::Text("m".into()));
    assert!(!rt.state.muted);
    assert_eq!(rt.state.volume, 75);
    for _ in 0..30 {
        key(&mut rt, rune::Key::Up);
    }
    assert_eq!(rt.state.volume, 100);
    for _ in 0..30 {
        key(&mut rt, rune::Key::Down);
    }
    assert_eq!(rt.state.volume, 0);
}

#[test]
fn timeline_supports_click_and_drag_and_controls_fit_minimum_window() {
    let mut rt = player_runtime();
    let [x, y, w, h] = rect(&rt, "player-timeline");
    rt.pointer_down(x + w * 0.25, y + h / 2.0);
    rt.pointer_move(x + w * 0.75, y + h / 2.0);
    rt.frame(0.0);
    assert_eq!(rt.state.seek_preview, Some(150.0));
    rt.pointer_up(x + w * 0.75, y + h / 2.0);
    rt.frame(0.0);
    assert!(rt.state.seek_preview.is_none());
    assert_eq!(rt.state.sample.position_s, Some(150.0));
    rt.pointer_down(x + w * 0.5, y + h / 2.0);
    rt.pointer_up(x + w * 0.5, y + h / 2.0);
    rt.frame(0.0);
    assert_eq!(rt.state.sample.position_s, Some(100.0));
    for name in [
        "pause",
        "mute",
        "player-volume",
        "subtitles",
        "audio",
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
    assert_eq!(rt.state.volume, 50);
    assert!(!rt.state.muted);
    for duration in [None, Some(0.0), Some(f64::NAN), Some(f64::INFINITY)] {
        rt.update(|s, _| s.sample.duration_s = duration);
        rt.frame(0.0);
        tap(&mut rt, "player-timeline");
        assert_eq!(rt.state.sample.position_s, Some(50.0));
        assert!(rt.state.seek_preview.is_none());
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
    assert!(!rt.state.sample.paused);
}
