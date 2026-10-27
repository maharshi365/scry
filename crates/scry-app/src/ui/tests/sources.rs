use super::*;

fn torrent(title: &str, seeders: u64, bytes: u64) -> Stream {
    serde_json::from_value(serde_json::json!({
        "name":"1080p", "title":title,
        "infoHash":"0123456789abcdef0123456789abcdef01234567", "fileIdx":0,
        "behaviorHints":{"seeders":seeders,"videoSize":bytes}
    }))
    .unwrap()
}

fn runtime(width: f32) -> rune::Runtime<State> {
    let mut s = init();
    scene_detail(&mut s);
    s.streams = vec![
        ("Provider A".into(), torrent("Weak", 1, 10)),
        ("Provider A".into(), torrent("Healthy", 100, 300)),
        ("Provider B".into(), torrent("Other", 200, 200)),
    ];
    let mut rt = rune::Runtime::new(s, view, (width, 820.0), rune::Env::default());
    rt.frame(0.0);
    rt
}

fn assert_order(rt: &mut rune::Runtime<State>, indices: &[usize]) {
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let positions: Vec<_> = indices
        .iter()
        .map(|index| rect(rt, &format!("stream-{index}"))[1])
        .collect();
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "{positions:?}"
    );
}

fn choose_sort(rt: &mut rune::Runtime<State>, best: bool) {
    tap(rt, "source-sort");
    tap(
        rt,
        if best {
            "source-best"
        } else {
            "source-original"
        },
    );
}

#[test]
fn best_available_menu_ranks_sources_and_restores_original_order() {
    let mut rt = runtime(1000.0);
    assert_order(&mut rt, &[0, 1, 2]);
    choose_sort(&mut rt, true);
    assert_order(&mut rt, &[2, 1, 0]);
    assert_eq!(rt.state.streams[0].1.title, "Weak");
    assert_eq!(rt.state.streams[1].1.title, "Healthy");
    assert!(!rt.state.player.playing);
    assert_eq!(rt.state.player.generation, 0);
    choose_sort(&mut rt, false);
    assert_order(&mut rt, &[0, 1, 2]);
}

#[test]
fn best_available_respects_provider_filter_and_ranks_late_sources() {
    let mut rt = runtime(1000.0);
    rt.update(|s, _| s.stream_provider = Some("Provider A".into()));
    choose_sort(&mut rt, true);
    assert_order(&mut rt, &[1, 0]);
    assert!(rt.find("stream-2").is_none());
    rt.update(|s, _| {
        s.streams
            .push(("Provider A".into(), torrent("Late winner", 100, 100)));
    });
    assert_order(&mut rt, &[3, 1, 0]);
    choose_sort(&mut rt, false);
    assert_order(&mut rt, &[0, 1, 3]);
}

#[test]
fn best_available_prefers_direct_links_and_keeps_unsupported_sources_last() {
    let mut rt = runtime(1000.0);
    rt.update(|s, _| {
        s.streams = vec![
            (
                "Provider".into(),
                serde_json::from_value(
                    serde_json::json!({"externalUrl":"https://example.invalid/external"}),
                )
                .unwrap(),
            ),
            ("Provider".into(), torrent("Torrent", 100, 100)),
            (
                "Provider".into(),
                serde_json::from_value(serde_json::json!({"url":"https://example.invalid/direct"}))
                    .unwrap(),
            ),
        ];
    });
    choose_sort(&mut rt, true);
    assert_order(&mut rt, &[2, 1, 0]);
    tap(&mut rt, "stream-0");
    assert!(!rt.state.player.playing);
    assert_eq!(rt.state.player.generation, 0);
}

#[test]
fn header_sort_fits_and_provider_selection_ranks_only_visible_swarms() {
    for width in [880.0, 1280.0] {
        let mut rt = runtime(width);
        rt.update(|s, _| {
            s.streams = vec![
                ("Provider A".into(), torrent("Strong", 100, 300)),
                ("Provider A".into(), torrent("Compact", 60, 100)),
                ("Provider B".into(), torrent("Huge swarm", 1000, 200)),
            ];
        });
        choose_sort(&mut rt, true);
        assert_order(&mut rt, &[2, 0, 1]);
        let header = rect(&rt, "source-picker-header");
        let heading = rect(&rt, "season-select");
        let provider = rect(&rt, "source-select");
        let sort = rect(&rt, "source-sort");
        assert!(heading[0] + heading[2] <= sort[0]);
        assert!(sort[0] + sort[2] <= header[0] + header[2] + 1.0);
        assert!(provider[1] >= sort[1] + sort[3]);
        tap(&mut rt, "source-select");
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        tap(&mut rt, "source-provider-0");
        assert_order(&mut rt, &[1, 0]);
        assert!(rt.find("stream-2").is_none());
        assert!(rt.state.stream_best_first);
    }
}

#[test]
fn single_provider_has_a_compact_header_sort_without_an_empty_toolbar() {
    let mut rt = runtime(880.0);
    rt.update(|s, tx| {
        s.streams.retain(|(provider, _)| provider == "Provider A");
        tx.instant();
    });
    rt.frame(0.0);
    assert!(rt.find("source-controls").is_none());
    assert!(rt.find("source-select").is_none());
    let before = rect(&rt, "stream-0");
    let header = rect(&rt, "source-picker-header");
    let sort = rect(&rt, "source-sort");
    let heading = rect(&rt, "season-select");
    assert!(sort[2] < header[2] * 0.65);
    assert!(sort[1] < heading[1] + heading[3] && sort[1] + sort[3] > heading[1]);
    assert!(heading[0] + heading[2] <= sort[0]);
    tap(&mut rt, "source-sort");
    assert!(rt.find("source-sort-menu").is_some());
    assert_eq!(rect(&rt, "stream-0"), before);
    tap(&mut rt, "source-best");
    assert!(rt.state.stream_best_first);
    assert!(rt.find("source-sort-menu").is_none());
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some("Best available"))
    );
}

#[test]
fn season_and_sort_selectors_use_pointer_cursors() {
    let mut rt = runtime(880.0);
    for key in ["season-select", "source-sort"] {
        let (x, y) = rt.find(key).unwrap();
        rt.pointer_move(x, y);
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.cursor(), rune::Cursor::Pointer, "{key}");
    }
}

#[test]
fn sort_menu_marks_only_the_current_order() {
    let mut rt = runtime(880.0);
    tap(&mut rt, "source-sort");
    assert!(rt.find("source-original-selected").is_some());
    assert!(rt.find("source-best-selected").is_none());
    tap(&mut rt, "source-best");
    tap(&mut rt, "source-sort");
    assert!(rt.find("source-best-selected").is_some());
    assert!(rt.find("source-original-selected").is_none());
}
