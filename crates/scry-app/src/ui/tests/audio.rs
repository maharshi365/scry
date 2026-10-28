use super::*;
use scry_playback::audio::{Snapshot, Track};

fn snapshot(selected: i64) -> Snapshot {
    Snapshot {
        tracks: vec![
            Track {
                id: 3,
                language: "eng".into(),
                title: "Main".into(),
            },
            Track {
                id: 9,
                language: "jpn".into(),
                title: "Original".into(),
            },
        ],
        selected: Some(selected),
    }
}

#[test]
fn audio_button_opens_track_overlay_without_changing_playback() {
    let mut state = init();
    state.navigation.page = Page::Player;
    state.player.playing = true;
    state.player.ready = true;
    let mut rt = rune::Runtime::new(state, view, (1280.0, 820.0), rune::Env::default());
    rt.frame(0.0);
    let before = rect(&rt, "video");
    tap(&mut rt, "audio");
    assert!(rt.find("player-audio-overlay").is_some());
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some("No audio tracks found."))
    );
    assert!(!rt.state.player.sample.paused);
    assert!(!rt.state.player.muted);
    assert_eq!(rect(&rt, "video"), before);
}

#[test]
fn audio_hover_lists_tracks_and_selection_follows_engine_confirmation() {
    let mut state = init();
    state.navigation.page = Page::Player;
    state.player.playing = true;
    state.player.ready = true;
    state.apply_audio_snapshot(0, snapshot(3));
    let mut rt = rune::Runtime::new(state, view, (1280.0, 820.0), rune::Env::default());
    rt.frame(0.0);
    let before = rect(&rt, "video");
    let (x, y) = rt.find("audio").unwrap();
    rt.pointer_move(x, y);
    rt.frame(0.0);
    assert!(rt.find("player-audio-overlay").is_some());
    assert!(rt.find("audio-track-3").is_some());
    assert!(rt.find("audio-track-9").is_some());
    super::dropdowns::assert_flat_panel(&rt, "player-audio-overlay");
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.label.as_deref() == Some("English · Main, selected"))
    );
    tap(&mut rt, "audio");
    tap(&mut rt, "audio-track-9");
    assert_eq!(
        rt.state.player.audio.selected,
        Some(3),
        "Wait for the engine, rather than claim an unconfirmed selection"
    );
    rt.update(|s, _| s.apply_audio_snapshot(0, snapshot(9)));
    rt.frame(0.0);
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.label.as_deref() == Some("Japanese · Original, selected"))
    );
    assert!(rt.state.player.audio.overlay.is_open());
    assert!(!rt.state.player.sample.paused);
    assert_eq!(rect(&rt, "video"), before);
    tap(&mut rt, "audio-close");
    assert!(!rt.state.player.audio.overlay.is_open());
}

#[test]
fn audio_panel_closes_before_fullscreen_and_on_deactivation() {
    for page in [Page::Player, Page::Guide] {
        let mut state = init();
        state.navigation.page = page;
        state.player.playing = true;
        state.player.fullscreen = true;
        state.apply_audio_snapshot(0, snapshot(3));
        let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
        rt.on_key = Some(on_key);
        rt.frame(0.0);
        tap(&mut rt, "audio");
        tap(&mut rt, "audio-track-9");
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        let panel = rect(&rt, "player-audio-overlay");
        assert!(panel[0] >= 0.0 && panel[1] >= 0.0);
        assert!(panel[0] + panel[2] <= 880.0 && panel[1] + panel[3] <= 600.0);
        rt.key(rune::KeyEvent {
            key: rune::Key::Escape,
            mods: rune::Mods::default(),
        });
        rt.frame(0.0);
        assert!(!rt.state.player.audio.overlay.is_open());
        assert!(rt.state.player.fullscreen);
        tap(&mut rt, "audio");
        tap(&mut rt, "audio-track-3");
        rt.window_focus(false);
        rt.frame(0.0);
        assert!(!rt.state.player.audio.overlay.is_open());
    }
}

#[test]
fn audio_and_subtitles_do_not_overlap_and_stopped_playback_rejects_late_tracks() {
    let mut state = init();
    state.player.playing = true;
    state.player.generation = 3;
    state.apply_audio_snapshot(3, snapshot(9));
    state.apply_audio_snapshot(2, Snapshot::default());
    assert_eq!(state.player.audio.selected, Some(9));
    assert_eq!(state.player.audio.tracks.len(), 2);
    state.show_subtitles();
    state.show_audio();
    assert!(!state.player.subtitles.overlay.is_open());
    assert!(state.player.audio.overlay.is_open());
    state.show_subtitles();
    assert!(!state.player.audio.overlay.is_open());
    state.stop();
    state.apply_audio_snapshot(state.player.generation, snapshot(3));
    assert!(state.player.audio.tracks.is_empty());
    assert_eq!(state.player.audio.selected, None);
    assert!(!state.player.audio.overlay.is_open());
}
