use super::*;

fn subtitles() -> rune::Runtime<State> {
    let mut state = init();
    crate::ui::player::subtitles::scene(&mut state);
    let mut rt = rune::Runtime::new(state, view, (1280.0, 820.0), rune::Env::default());
    rt.frame(0.0);
    rt
}

#[test]
fn subtitle_picker_cancellation_and_invalid_files_do_not_load_subtitles() {
    let mut rt = subtitles();
    tap(&mut rt, "subtitle-load-local");
    assert!(rt.file_dialog().is_some());
    rt.resolve_file_dialog(None).unwrap();
    rt.frame(0.0);
    assert!(rt.state.player.subtitles.error.is_none());
    tap(&mut rt, "subtitle-load-local");
    rt.resolve_file_dialog(Some("not-subtitles.json".into()))
        .unwrap();
    rt.frame(0.0);
    assert_eq!(
        rt.state.player.subtitles.error.as_deref(),
        Some("Choose an SRT, VTT, ASS, or SSA file")
    );
}

#[test]
fn subtitle_picker_answer_cannot_attach_to_replacement_playback() {
    let mut rt = subtitles();
    tap(&mut rt, "subtitle-load-local");
    rt.state.player.generation += 1;
    rt.resolve_file_dialog(Some("previous-video.srt".into()))
        .unwrap();
    rt.frame(0.0);
    assert!(
        rt.state.player.subtitles.error.is_none(),
        "A stale picker result reached the closed player"
    );
}

#[test]
fn history_picker_cancellation_preserves_idle_state_and_selection_dispatches() {
    let mut state = init();
    state.navigation.page = Page::Settings;
    state.settings.tab = crate::ui::settings::SettingsTab::Data;
    let mut rt = rune::Runtime::new(state, view, (1280.0, 820.0), rune::Env::default());
    rt.frame(0.0);
    tap(&mut rt, "history-select");
    rt.resolve_file_dialog(None).unwrap();
    rt.frame(0.0);
    assert!(rt.state.settings.history.error.is_none());
    assert!(rt.state.settings.history.phase == crate::ui::settings::history::Phase::Idle);
    tap(&mut rt, "history-select");
    rt.resolve_file_dialog(Some("history.json".into())).unwrap();
    rt.frame(0.0);
    assert_eq!(
        rt.state.settings.history.error.as_deref(),
        Some("History import is unavailable")
    );
}

#[test]
fn live_video_pixels_do_not_push_controls_below_a_small_window() {
    use scry_app::platform::{VideoHost, VideoPresentation};
    for viewport in [(1000.0, 700.0), (880.0, 600.0)] {
        let mut state = init();
        state.navigation.page = Page::Player;
        state.player.playing = true;
        state.player.ready = true;
        let VideoPresentation::Pixels(pixels) = state.shell.host.presentation() else {
            panic!("Expected a composited Linux surface");
        };
        let pixels = pixels.clone();
        pixels.set(1280, 720, vec![128; 1280 * 720 * 4]);
        let mut rt = rune::Runtime::new(state, view, (1280.0, 820.0), rune::Env::default());
        rt.frame(0.0);
        rt.state.set_fullscreen(true);
        rt.set_viewport((1440.0, 1000.0), 1.0);
        rt.frame(0.0);
        rt.state.set_fullscreen(false);
        rt.frame(0.0);
        rt.set_viewport(viewport, 1.0);
        rt.frame(0.0);
        pixels.set(780, 542, vec![128; 780 * 542 * 4]);
        rt.frame(1.0 / 60.0);
        let controls = rect(&rt, "player-controls");
        assert!(
            controls[1] + controls[3] <= viewport.1,
            "Live video pushed controls outside {viewport:?}: {controls:?}"
        );
    }
}
