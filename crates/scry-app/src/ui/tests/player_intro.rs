use super::*;

fn runtime(fullscreen: bool, width: f32, height: f32) -> rune::Runtime<State> {
    let mut state = init();
    crate::ui::scenes::scene_player_intro(&mut state);
    state.player.fullscreen = fullscreen;
    let mut rt = rune::Runtime::new(state, view, (width, height), rune::Env::default());
    rt.frame(0.0);
    rt
}

#[test]
fn prompt_is_bottom_right_above_controls_in_both_window_modes() {
    for fullscreen in [false, true] {
        for (width, height) in [(880.0, 600.0), (1920.0, 1080.0)] {
            let rt = runtime(fullscreen, width, height);
            assert!(
                rt.elements()
                    .iter()
                    .any(|e| e.text.as_deref() == Some("Skip Intro")),
                "The skip action needs a visible text label"
            );
            let [x, y, w, h] = rect(&rt, "player-intro-prompt");
            let video = rect(&rt, "video");
            let controls = rect(&rt, "player-controls");
            assert!(w > 0.0 && h > 0.0, "Prompt must have a visible hit area");
            assert!(x > video[0] + video[2] / 2.0 && x + w < video[0] + video[2]);
            assert!(y >= video[1] && y + h <= video[1] + video[3]);
            assert!(y + h < controls[1], "Prompt overlaps playback controls");
        }
    }
}

#[test]
fn hint_is_passive_for_only_the_last_two_seconds_then_skip_seeks_to_end() {
    let mut rt = runtime(true, 880.0, 600.0);
    for (position, hint, button) in [
        (52.0, false, false),
        (52.312, true, false),
        (54.0, true, false),
        (54.312, false, true),
        (144.312, false, false),
    ] {
        rt.update(|s, tx| {
            s.player.sample.position_s = Some(position);
            tx.instant();
        });
        rt.frame(0.25);
        assert_eq!(
            rt.find("player-intro-upcoming").is_some(),
            hint,
            "position {position}"
        );
        assert_eq!(
            rt.find("player-skip-intro").is_some(),
            button,
            "position {position}"
        );
    }
    rt.update(|s, tx| {
        s.player.sample.position_s = Some(60.0);
        tx.instant();
    });
    rt.frame(0.25);
    tap(&mut rt, "player-skip-intro");
    assert_eq!(rt.state.player.sample.position_s, Some(144.312));
    assert!(
        !rt.state.player.sample.paused,
        "Skipping must not toggle playback"
    );
    rt.frame(0.25);
    rt.update(|s, tx| {
        s.player.sample.position_s = Some(60.0);
        tx.instant();
    });
    rt.frame(0.25);
    assert!(
        rt.find("player-skip-intro").is_some(),
        "Seeking back restores the action"
    );
}

#[test]
fn intro_prompt_does_not_move_or_disappear_when_fullscreen_controls_hide() {
    let mut rt = runtime(true, 880.0, 600.0);
    let prompt = rect(&rt, "player-intro-prompt");
    let video = rect(&rt, "video");
    rt.update(|s, tx| {
        s.player.interaction.visible = false;
        tx.instant();
    });
    rt.frame(0.25);
    assert!(rt.find("player-skip-intro").is_some());
    assert_eq!(rect(&rt, "player-intro-prompt"), prompt);
    assert_eq!(rect(&rt, "video"), video);
}

#[test]
fn player_menus_temporarily_replace_the_intro_prompt_without_moving_video() {
    let mut rt = runtime(true, 880.0, 600.0);
    let video = rect(&rt, "video");
    tap(&mut rt, "player-stats");
    rt.frame(0.25);
    assert!(rt.find("player-stats-overlay").is_some());
    assert_eq!(rect(&rt, "video"), video);
    rt.update(|s, tx| {
        s.player.stats_overlay.dismiss();
        tx.instant();
    });
    rt.frame(0.25);
    assert!(rt.find("player-skip-intro").is_some());
    assert_eq!(rect(&rt, "video"), video);
}

#[test]
fn stale_lookup_results_cannot_attach_to_a_different_or_stopped_episode() {
    let (mut state, tx) = state_with_events();
    crate::ui::scenes::scene_player_intro(&mut state);
    let intro = state.player.intro.take();
    state.player.generation = 4;
    tx.try_send(Event::IntroSegments(3, intro.clone())).unwrap();
    poll(&mut state, 0.0, &mut Tx::default());
    assert!(state.player.intro.is_none());
    tx.try_send(Event::IntroSegments(4, intro.clone())).unwrap();
    poll(&mut state, 0.0, &mut Tx::default());
    assert_eq!(state.player.intro.as_ref().map(|s| s.end_ms), Some(144_312));
    state.stop();
    tx.try_send(Event::IntroSegments(state.player.generation, intro))
        .unwrap();
    poll(&mut state, 0.0, &mut Tx::default());
    assert!(state.player.intro.is_none());
}
