use super::*;

fn runtime(scene: fn(&mut State), size: (f32, f32)) -> rune::Runtime<State> {
    let mut state = init();
    scene(&mut state);
    let mut rt = rune::Runtime::new(state, view, size, rune::Env::default());
    rt.frame(0.0);
    rt
}

#[expect(clippy::panic, reason = "A missing fixture action must fail the test")]
fn tap(rt: &mut rune::Runtime<State>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("Missing {key}"));
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
fn controls_run_local_actions_and_reset_keeps_the_active_category() {
    let mut rt = runtime(scene_controls, (1280.0, 820.0));
    tap(&mut rt, "playground-primary");
    assert_eq!(rt.state.presses, 1);
    tap(&mut rt, "playground-secondary");
    assert_eq!(rt.state.presses, 2);
    tap(&mut rt, "playground-disabled");
    tap(&mut rt, "playground-busy");
    assert_eq!(
        rt.state.presses, 2,
        "Unavailable examples must not run actions"
    );
    tap(&mut rt, "playground-nav-settings");
    assert_eq!(rt.state.category, Category::Settings);
    tap(&mut rt, "playground-volume-more");
    assert_eq!(rt.state.volume, 60);
    tap(&mut rt, "playground-reset");
    assert_eq!(rt.state.category, Category::Settings);
    assert_eq!(rt.state.volume, 50);
    assert_eq!(rt.state.presses, 0);
}

#[test]
fn settings_and_list_examples_keep_state_in_their_own_window() {
    let mut first = runtime(scene_settings, (1280.0, 820.0));
    let second = runtime(scene_settings, (1280.0, 820.0));
    tap(&mut first, "playground-quality-High quality");
    tap(&mut first, "playground-save");
    assert_eq!(first.state.saved, Some(Quality::High));
    assert_eq!(second.state.quality, Quality::Automatic);
    assert_eq!(second.state.saved, None);
    tap(&mut first, "playground-nav-list-detail");
    tap(&mut first, "playground-item-1");
    assert_eq!(first.state.selected, 1);
    assert!(first.elements().iter().any(|element| {
        element.text.as_deref() == Some(ITEMS[1].0)
            && element.key.as_deref() == Some("playground-detail-title")
    }));
    tap(&mut first, "playground-nav-settings");
    assert_eq!(first.state.saved, Some(Quality::High));
}

#[test]
fn dialog_cancel_and_confirm_have_distinct_local_results() {
    let mut rt = runtime(scene_dialog, (1280.0, 820.0));
    tap(&mut rt, "playground-dialog-cancel");
    assert!(!rt.state.dialog_open);
    assert!(rt.state.notice);
    tap(&mut rt, "playground-open-dialog");
    assert!(rt.state.dialog_open);
    tap(&mut rt, "playground-dialog-confirm");
    assert!(!rt.state.dialog_open);
    assert!(!rt.state.notice);
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
    assert!(!rt.state.expanded);
    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rt.find("playground-motion-surface").is_none());
    tap(&mut rt, "playground-motion-toggle");
    assert!(rt.state.expanded);
    assert!(rt.find("playground-motion-surface").is_some());
}

#[test]
fn examples_fit_supported_widths_and_keep_navigation_reachable() {
    for size in [(640.0, 480.0), (880.0, 600.0), (1280.0, 820.0)] {
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
            for category in Category::ALL {
                let key = format!("playground-nav-{}", category.key());
                let bounds = rect(&rt, &key);
                assert!(
                    bounds[0] >= root[0] && bounds[1] >= root[1],
                    "{key}: {bounds:?}"
                );
                assert!(bounds[0] + bounds[2] <= size.0 + 0.5, "{key}: {bounds:?}");
                assert!(bounds[1] + bounds[3] <= size.1 + 0.5, "{key}: {bounds:?}");
            }
            for element in rt.elements().iter().filter(|element| !element.exiting) {
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
