use super::*;

#[test]
fn loading_action_reserves_its_label_space_and_restores_the_same_button() {
    let mut rt = runtime(scene_controls, (1280.0, 820.0));
    let before = rect(&rt, "playground-loading-action");
    let neighbor = rect(&rt, "playground-loading-finish");
    tap(&mut rt, "playground-loading-action");
    assert_eq!(stage(&rt).loading, Loading::Busy);
    assert!(
        rt.playground_element("playground-loading-action-spinner")
            .is_some()
    );
    assert!(
        rect(&rt, "playground-loading-action")
            .into_iter()
            .zip(before)
            .all(|(actual, expected)| (actual - expected).abs() < 0.01)
    );
    assert!(
        rect(&rt, "playground-loading-finish")
            .into_iter()
            .zip(neighbor)
            .all(|(actual, expected)| (actual - expected).abs() < 0.01)
    );
    tap(&mut rt, "playground-loading-finish");
    assert_eq!(stage(&rt).loading, Loading::Idle);
    assert!(
        rt.playground_element("playground-loading-action-spinner")
            .is_none()
    );
    assert!(
        rect(&rt, "playground-loading-action")
            .into_iter()
            .zip(before)
            .all(|(actual, expected)| (actual - expected).abs() < 0.01)
    );
}

#[test]
fn titlebar_fixture_keeps_actions_interactive() {
    let mut rt = runtime(scene_titlebar, (1280.0, 820.0));
    tap(&mut rt, "playground-titlebar-back");
    assert_eq!(stage(&rt).presses, 1);
    assert_eq!(rt.state.presses, 0);
}

fn runtime(scene: fn(&mut State), size: (f32, f32)) -> rune::Runtime<State> {
    let mut state = init();
    scene(&mut state);
    let name = if state.dialog_open {
        "Feedback/Dialog"
    } else {
        state.category.story()
    };
    let mut rt = app().headless_runtime(size, rune::Env::default());
    rt.frame(0.0);
    rt.open_playground(name, None);
    rt
}

#[expect(clippy::panic, reason = "A missing fixture state must fail the test")]
fn stage(rt: &rune::Runtime<State>) -> &State {
    rt.playground_state::<State>()
        .unwrap_or_else(|| panic!("Missing playground fixture state"))
}

#[expect(clippy::panic, reason = "A missing fixture action must fail the test")]
fn tap(rt: &mut rune::Runtime<State>, key: &str) {
    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
    }
    let element = rt
        .playground_element(key)
        .unwrap_or_else(|| panic!("Missing {key}"));
    let [x, y, w, h] = element.rect;
    let (x, y) = (x + w * 0.5, y + h * 0.5);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
}

#[expect(clippy::panic, reason = "A missing fixture element must fail the test")]
fn rect(rt: &rune::Runtime<State>, key: &str) -> [f32; 4] {
    rt.elements()
        .iter()
        .find(|element| !element.exiting && element.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("Missing {key}"))
        .rect
}

#[test]
fn controls_run_local_actions_and_replay_resets_only_the_selected_scene() {
    let mut rt = runtime(scene_controls, (1280.0, 820.0));
    tap(&mut rt, "playground-primary");
    assert_eq!(stage(&rt).presses, 1);
    tap(&mut rt, "playground-secondary");
    assert_eq!(stage(&rt).presses, 2);
    tap(&mut rt, "playground-disabled");
    tap(&mut rt, "playground-busy");
    assert_eq!(
        stage(&rt).presses,
        2,
        "Unavailable examples must not run actions"
    );
    rt.open_playground("Examples/Settings", None);
    assert_eq!(stage(&rt).category, Category::Settings);
    tap(&mut rt, "playground-volume-more");
    assert_eq!(stage(&rt).volume, 60);
    rt.playground_trigger('r');
    assert_eq!(stage(&rt).category, Category::Settings);
    assert_eq!(stage(&rt).volume, 50);
    assert_eq!(stage(&rt).presses, 0);
    assert_eq!(rt.state.category, Category::Controls);
    assert_eq!(rt.state.volume, 50);
}

#[test]
fn settings_and_list_examples_keep_state_in_their_own_window() {
    let mut first = runtime(scene_settings, (1280.0, 820.0));
    let second = runtime(scene_settings, (1280.0, 820.0));
    tap(&mut first, "playground-quality-High quality");
    tap(&mut first, "playground-save");
    assert_eq!(stage(&first).saved, Some(Quality::High));
    assert_eq!(stage(&second).quality, Quality::Automatic);
    assert_eq!(stage(&second).saved, None);
    first.open_playground("Examples/List detail", None);
    tap(&mut first, "playground-item-1");
    assert_eq!(stage(&first).selected, 1);
    assert!(first.elements().iter().any(|element| {
        element.text.as_deref() == Some(ITEMS[1].0)
            && element.key.as_deref() == Some("playground-detail-title")
    }));
    first.open_playground("Examples/Settings", None);
    assert_eq!(stage(&first).saved, None);
    assert_eq!(first.state.saved, None);
}

#[test]
fn dialog_cancel_and_confirm_have_distinct_local_results() {
    let mut rt = runtime(scene_dialog, (1280.0, 820.0));
    tap(&mut rt, "playground-dialog-cancel");
    assert!(!stage(&rt).dialog_open);
    assert!(stage(&rt).notice);
    tap(&mut rt, "playground-open-dialog");
    assert!(stage(&rt).dialog_open);
    tap(&mut rt, "playground-dialog-confirm");
    assert!(!stage(&rt).dialog_open);
    assert!(!stage(&rt).notice);
    assert!(rt.state.notice);
}

#[test]
fn scene_fixtures_reset_previous_interactions() {
    let mut state = init();
    state.presses = 8;
    state.quality = Quality::High;
    state.dialog_open = true;
    scene_settings(&mut state);
    assert_eq!(state.category, Category::Settings);
    assert_eq!(state.presses, 0);
    assert_eq!(state.quality, Quality::Automatic);
    assert!(!state.dialog_open);
    scene_dialog(&mut state);
    assert_eq!(state.category, Category::Feedback);
    assert!(state.dialog_open);
    scene_motion(&mut state);
    assert_eq!(state.category, Category::Motion);
    assert!(state.expanded);
    assert!(!state.dialog_open);
}

#[test]
fn motion_example_can_be_revealed_and_hidden() {
    let mut rt = runtime(scene_motion, (1280.0, 820.0));
    assert!(rt.find("playground-motion-surface").is_some());
    tap(&mut rt, "playground-motion-toggle");
    assert!(!stage(&rt).expanded);
    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rt.find("playground-motion-surface").is_none());
    tap(&mut rt, "playground-motion-toggle");
    assert!(stage(&rt).expanded);
    assert!(rt.find("playground-motion-surface").is_some());
}

#[test]
fn built_in_host_shows_fixture_scenes_without_custom_navigation() {
    for size in [(1106.0, 480.0), (1346.0, 600.0), (1746.0, 820.0)] {
        for scene in [
            scene_controls,
            scene_settings,
            scene_list_detail,
            scene_feedback,
            scene_dialog,
            scene_motion,
        ] {
            let rt = runtime(scene, size);
            let root = rect(&rt, "playground-root");
            assert!(root[2] >= 640.0 && root[3] > 0.0, "{size:?}: {root:?}");
            if stage(&rt).dialog_open {
                let dialog = rect(&rt, "playground-dialog");
                assert!(dialog[0] >= root[0]);
                assert!(dialog[0] + dialog[2] <= root[0] + root[2] + 0.5);
            }
            assert!(rt.find("playground-nav-controls").is_none());
            for element in rt.elements().iter().filter(|element| {
                !element.exiting
                    && element
                        .key
                        .as_deref()
                        .is_some_and(|key| key.starts_with("playground-"))
            }) {
                assert!(element.rect[2].is_finite(), "{:?}", element.key);
                assert!(
                    element.rect[0] + element.rect[2] <= size.0 + 0.5,
                    "{:?} overflows {size:?}: {:?}",
                    element.key,
                    element.rect
                );
            }
        }
    }
}
