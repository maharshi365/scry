use super::*;
use crate::ui::design::tokens::player::{MINI_HEIGHT, MINI_INSET, MINI_WIDTH};

const WINDOW: (f32, f32) = (880.0, 600.0);

/// Ready playback opened from the Library page.
fn watching() -> rune::Runtime<State> {
    let mut state = init();
    state.navigation.page = Page::Library;
    state.remember();
    state.navigation.page = Page::Player;
    state.player.playing = true;
    state.player.ready = true;
    state.player.title = "Long show · S1 E1".into();
    state.player.sample.duration_s = Some(200.0);
    state.player.sample.position_s = Some(50.0);
    let mut rt = rune::Runtime::new(state, view, WINDOW, rune::Env::default());
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    rt
}

fn key(rt: &mut rune::Runtime<State>, key: rune::Key) -> bool {
    let handled = on_key(
        &mut rt.state,
        &rune::KeyEvent {
            key,
            mods: rune::Mods::default(),
        },
        &mut Tx::default(),
    );
    rt.frame(0.0);
    handled
}

#[test]
fn miniplayer_button_keeps_playback_in_the_bottom_right_corner() {
    let mut rt = watching();
    let generation = rt.state.player.generation;
    tap(&mut rt, "miniplayer");
    assert_eq!(rt.state.navigation.page, Page::Library);
    assert!(rt.state.player.mini);
    assert!(rt.state.player.playing, "Playback must continue");
    assert_eq!(rt.state.player.generation, generation);

    let [x, y, w, h] = rect(&rt, "miniplayer-panel");
    assert!((w - MINI_WIDTH).abs() < 0.5);
    assert!((x + w - (WINDOW.0 - MINI_INSET)).abs() < 0.5);
    assert!((y + h - (WINDOW.1 - MINI_INSET)).abs() < 0.5);
    assert!(
        (h - MINI_HEIGHT).abs() < 0.5,
        "The miniplayer has no footer"
    );
    let video = rect(&rt, "mini-video");
    assert_eq!(video, [x, y, w, h], "The video fills the miniplayer");
    assert!((video[2] / video[3] - 16.0 / 9.0).abs() < 0.05);
    rt.frame(0.0);
    assert_eq!(rt.state.player.interaction.video_rect, Some(video));
    let bar = rect(&rt, "mini-controls");
    assert_eq!(
        rt.state.player.interaction.controls_rect,
        Some(bar),
        "The native video must leave a hole for the overlaid controls"
    );
    assert!(
        bar[1] > video[1] + video[3] / 2.0,
        "Controls sit at the bottom"
    );
    for control in [
        "mini-seek-back",
        "mini-pause",
        "mini-seek-forward",
        "mini-expand",
        "mini-close",
    ] {
        let [cx, cy, cw, ch] = rect(&rt, control);
        assert!(
            cx >= video[0]
                && cy >= video[1]
                && cx + cw <= video[0] + video[2]
                && cy + ch <= video[1] + video[3],
            "{control} must sit on top of the video"
        );
    }
}

#[test]
fn miniplayer_controls_show_on_hover_and_hide_when_the_pointer_leaves() {
    let mut rt = watching();
    tap(&mut rt, "miniplayer");
    assert!(
        rt.find("mini-controls").is_some(),
        "Controls show when the miniplayer opens"
    );
    rt.update(|s, tx| {
        s.player
            .interaction
            .pointer(None, std::time::Instant::now());
        tx.instant();
    });
    rt.frame(0.0);
    assert!(rt.find("mini-controls").is_none());
    assert!(
        rt.find("mini-video").is_some(),
        "The video stays visible without controls"
    );

    let [x, y, w, h] = rect(&rt, "mini-video");
    rt.pointer_move(x + w / 2.0, y + h / 2.0);
    rt.frame(0.0);
    assert!(rt.find("mini-controls").is_some());
}

#[test]
fn back_navigation_moves_ready_playback_to_the_miniplayer() {
    let mut rt = watching();
    key(&mut rt, rune::Key::Escape);
    assert_eq!(rt.state.navigation.page, Page::Library);
    assert!(rt.state.player.mini && rt.state.player.playing);

    rt.update(|s, _| s.navigate(Page::Settings));
    assert_eq!(rt.state.navigation.page, Page::Settings);
    assert!(
        rt.state.player.mini && rt.state.player.playing,
        "Browsing must not stop miniplayer playback"
    );
}

#[test]
fn miniplayer_controls_pause_seek_and_expand_the_same_playback() {
    let mut rt = watching();
    tap(&mut rt, "miniplayer");
    let generation = rt.state.player.generation;
    tap(&mut rt, "mini-pause");
    assert!(rt.state.player.sample.paused);
    tap(&mut rt, "mini-pause");
    assert!(!rt.state.player.sample.paused);

    tap(&mut rt, "mini-expand");
    assert_eq!(rt.state.navigation.page, Page::Player);
    assert!(!rt.state.player.mini);
    assert!(rt.state.player.playing);
    assert_eq!(rt.state.player.generation, generation);
    assert!(rt.find("miniplayer-panel").is_none());
    assert!(rt.find("video").is_some());
    rt.frame(0.0);
    assert_eq!(
        rt.state.player.interaction.video_rect,
        Some(rect(&rt, "video"))
    );
}

#[test]
fn closing_the_miniplayer_stops_playback_and_keeps_the_page() {
    let mut rt = watching();
    tap(&mut rt, "miniplayer");
    tap(&mut rt, "mini-close");
    assert_eq!(rt.state.navigation.page, Page::Library);
    assert!(!rt.state.player.playing);
    assert!(!rt.state.player.mini);
    assert!(rt.find("mini-video").is_none());
}

#[test]
fn sources_and_loading_playback_still_stop_instead_of_shrinking() {
    let mut rt = watching();
    tap(&mut rt, "player-back");
    assert!(!rt.state.player.playing && !rt.state.player.mini);

    let mut rt = watching();
    rt.state.player.ready = false;
    rt.frame(0.0);
    key(&mut rt, rune::Key::Escape);
    assert_eq!(rt.state.navigation.page, Page::Library);
    assert!(
        !rt.state.player.playing && !rt.state.player.mini,
        "Unready playback has nothing to show in the corner"
    );
}

#[test]
fn guide_and_new_sources_replace_miniplayer_playback() {
    let mut rt = watching();
    tap(&mut rt, "miniplayer");
    rt.update(|s, _| s.navigate(Page::Guide));
    assert!(!rt.state.player.playing && !rt.state.player.mini);

    let mut rt = watching();
    tap(&mut rt, "miniplayer");
    let generation = rt.state.player.generation;
    rt.update(|s, _| {
        s.play_media_source(
            crate::services::Source::Local("next.mkv".into()),
            "Next".into(),
        );
    });
    assert!(!rt.state.player.mini);
    assert!(rt.state.player.generation > generation);
}

#[test]
fn miniplayer_shortcuts_control_playback_but_leave_arrows_to_the_page() {
    let mut rt = watching();
    assert!(key(&mut rt, rune::Key::Text("i".into())));
    assert!(rt.state.player.mini);
    assert!(key(&mut rt, rune::Key::Text(" ".into())));
    assert!(rt.state.player.sample.paused);
    assert!(!key(&mut rt, rune::Key::Up));
    assert_eq!(rt.state.player.volume, 70);
    assert!(!key(&mut rt, rune::Key::Text("f".into())));
    assert!(!rt.state.player.fullscreen);
    assert!(key(&mut rt, rune::Key::Text("i".into())));
    assert_eq!(rt.state.navigation.page, Page::Player);
    assert!(!rt.state.player.mini && rt.state.player.playing);
}
