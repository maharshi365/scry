use super::*;

pub(super) fn assert_flat_panel(rt: &rune::Runtime<State>, key: &str) {
    let elements = rt.elements();
    let panel = elements
        .iter()
        .position(|e| e.key.as_deref() == Some(key))
        .unwrap();
    for element in &elements {
        let mut parent = element.parent;
        let mut inside = false;
        while let Some(index) = parent {
            if index == panel {
                inside = true;
                break;
            }
            parent = elements[index].parent;
        }
        if inside && !matches!(element.role, "button" | "tab") {
            assert_eq!(
                element.radius, 0.0,
                "{key} contains an inner rounded panel: {:?}",
                element.key
            );
        }
    }
}

fn runtime(width: f32, height: f32) -> rune::Runtime<State> {
    let mut s = init();
    s.navigate(Page::Detail);
    s.config_busy = false;
    s.detail = Some(multi_season_series());
    let source = serde_json::from_value(
        serde_json::json!({"url":"https://example.invalid/video", "title":"Test source"}),
    )
    .unwrap();
    s.streams = vec![
        ("First provider".into(), source),
        (
            "Second provider".into(),
            serde_json::from_value(serde_json::json!({"url":"https://example.invalid/other"}))
                .unwrap(),
        ),
    ];
    let mut rt = rune::Runtime::new(s, view, (width, height), rune::Env::default());
    rt.on_key = Some(on_key);
    settle(&mut rt);
    rt
}

fn settle(rt: &mut rune::Runtime<State>) {
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
}

#[test]
fn dropdowns_overlay_without_moving_content_and_dismiss_without_navigation() {
    for (width, height) in [(880.0, 600.0), (1280.0, 820.0)] {
        let mut rt = runtime(width, height);
        for (trigger, menu) in [
            ("season-select", "season-menu"),
            ("source-select", "source-menu"),
            ("source-sort", "source-sort-menu"),
        ] {
            if trigger != "season-select" {
                rt.update(|s, _| s.selected_video_id = Some("show:1:1".into()));
                settle(&mut rt);
            }
            let keys = if trigger == "season-select" {
                vec!["topbar", "season-select", "episode-4"]
            } else {
                vec!["topbar", "detail-back", "source-picker-header", "stream-0"]
            };
            let before: Vec<_> = keys.into_iter().map(|key| (key, rect(&rt, key))).collect();
            tap(&mut rt, trigger);
            settle(&mut rt);
            let popup = rect(&rt, menu);
            assert_flat_panel(&rt, menu);
            assert!(popup[0] >= 8.0 && popup[1] >= 8.0);
            assert!(popup[0] + popup[2] <= width - 7.0 && popup[1] + popup[3] <= height - 7.0);
            for (key, bounds) in &before {
                assert_eq!(rect(&rt, key), *bounds, "Opening {menu} moved {key}");
            }
            rt.key(rune::KeyEvent {
                key: rune::Key::Escape,
                mods: rune::Mods::default(),
            });
            settle(&mut rt);
            assert_eq!(rt.state.navigation.page, Page::Detail);
            assert!(
                !rt.state.season_menu_open
                    && !rt.state.source_menu_open
                    && !rt.state.source_sort_open
            );
            tap(&mut rt, trigger);
            settle(&mut rt);
            rt.pointer_down(10.0, 10.0);
            rt.pointer_up(10.0, 10.0);
            settle(&mut rt);
            assert!(
                !rt.state.season_menu_open
                    && !rt.state.source_menu_open
                    && !rt.state.source_sort_open
            );
            for (key, bounds) in before {
                assert_eq!(rect(&rt, key), bounds);
            }
        }
    }
}

#[test]
fn overlay_choices_keep_season_selection_and_provider_filtering_working() {
    let mut rt = runtime(880.0, 600.0);
    tap(&mut rt, "season-select");
    settle(&mut rt);
    assert!(rt.find("season-option-0-selected").is_some());
    rt.key(rune::KeyEvent {
        key: rune::Key::Down,
        mods: rune::Mods::default(),
    });
    rt.key(rune::KeyEvent {
        key: rune::Key::Enter,
        mods: rune::Mods::default(),
    });
    settle(&mut rt);
    assert_eq!(rt.state.active_season(), Some(2));
    assert!(!rt.state.season_menu_open);
    rt.update(|s, _| {
        s.selected_video_id = Some("show:2:1".into());
        let source =
            serde_json::from_value(serde_json::json!({"url":"https://example.invalid/video"}))
                .unwrap();
        s.streams = vec![
            ("First provider".into(), source),
            (
                "Second provider".into(),
                serde_json::from_value(serde_json::json!({"url":"https://example.invalid/other"}))
                    .unwrap(),
            ),
        ];
    });
    settle(&mut rt);
    tap(&mut rt, "source-select");
    settle(&mut rt);
    assert!(rt.find("source-all-selected").is_some());
    tap(&mut rt, "source-provider-1");
    settle(&mut rt);
    assert_eq!(rt.state.stream_provider.as_deref(), Some("Second provider"));
    assert!(!rt.state.source_menu_open);
    assert!(rt.find("stream-1").is_some());
    tap(&mut rt, "source-select");
    settle(&mut rt);
    assert!(rt.find("source-provider-1-selected").is_some());
}
