use super::*;
use crate::ui::source_picker::SourceSort;

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
    s.detail.sources.sort = SourceSort::Provider;
    s.detail.sources.streams = vec![
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
            "source-provider-sort"
        },
    );
}

#[test]
fn best_available_menu_ranks_sources_and_switches_back_to_provider_order() {
    let mut rt = runtime(1000.0);
    assert_order(&mut rt, &[0, 1, 2]);
    choose_sort(&mut rt, true);
    assert_order(&mut rt, &[2, 1, 0]);
    assert_eq!(rt.state.detail.sources.streams[0].1.title, "Weak");
    assert_eq!(rt.state.detail.sources.streams[1].1.title, "Healthy");
    assert!(!rt.state.player.playing);
    assert_eq!(rt.state.player.generation, 0);
    choose_sort(&mut rt, false);
    assert_order(&mut rt, &[0, 1, 2]);
}

#[test]
fn best_available_respects_provider_filter_and_ranks_late_sources() {
    let mut rt = runtime(1000.0);
    rt.update(|s, _| s.detail.sources.provider = Some("Provider A".into()));
    choose_sort(&mut rt, true);
    assert_order(&mut rt, &[1, 0]);
    rt.update(|s, _| {
        s.detail
            .sources
            .streams
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
        s.detail.sources.streams = vec![
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
            s.detail.sources.streams = vec![
                ("Provider A".into(), torrent("Strong", 100, 300)),
                ("Provider A".into(), torrent("Compact", 60, 100)),
                ("Provider B".into(), torrent("Huge swarm", 1000, 200)),
            ];
        });
        choose_sort(&mut rt, true);
        assert_order(&mut rt, &[2, 0, 1]);
        let header = rect(&rt, "source-picker-header");
        let back = rect(&rt, "episode-back");
        let provider = rect(&rt, "source-select");
        let sort = rect(&rt, "source-sort");
        assert!((back[1] - sort[1]).abs() < 0.5);
        assert!(back[0] + back[2] <= sort[0]);
        assert!(rt.find("source-heading").is_none());
        assert!(sort[0] + sort[2] <= header[0] + header[2] + 1.0);
        assert!(provider[1] >= sort[1] + sort[3]);
        tap(&mut rt, "source-select");
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        tap(&mut rt, "source-provider-0");
        assert_order(&mut rt, &[1, 0]);
        assert_eq!(rt.state.detail.sources.sort, SourceSort::Best);
    }
}

#[test]
fn single_provider_has_a_compact_header_sort_without_an_empty_toolbar() {
    let mut rt = runtime(880.0);
    rt.update(|s, tx| {
        s.detail
            .sources
            .streams
            .retain(|(provider, _)| provider == "Provider A");
        tx.instant();
    });
    rt.frame(0.0);
    let before = rect(&rt, "stream-0");
    let header = rect(&rt, "source-picker-header");
    let sort = rect(&rt, "source-sort");
    let back = rect(&rt, "episode-back");
    assert!(sort[2] < header[2] * 0.65);
    assert!((back[1] - sort[1]).abs() < 0.5);
    assert!(back[0] + back[2] <= sort[0]);
    assert!(rt.find("source-heading").is_none());
    tap(&mut rt, "source-sort");
    assert!(rt.find("source-sort-menu").is_some());
    assert_eq!(rect(&rt, "stream-0"), before);
    tap(&mut rt, "source-best");
    assert_eq!(rt.state.detail.sources.sort, SourceSort::Best);
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some("Best available"))
    );
}

#[test]
fn movie_sources_keep_sorting_without_an_episode_back_button_or_heading() {
    let mut state = init();
    super::super::scenes::scene_movie(&mut state);
    let mut rt = rune::Runtime::new(state, view, (880.0, 650.0), rune::Env::default());
    rt.frame(0.0);
    assert!(rt.find("episode-back").is_none());
    assert!(rt.find("source-heading").is_none());
    let sort = rect(&rt, "source-sort");
    let header = rect(&rt, "source-picker-header");
    assert!((sort[1] - header[1]).abs() < 0.5);
    assert!(rect(&rt, "stream-0")[1] >= sort[1] + sort[3]);
    tap(&mut rt, "source-sort");
    tap(&mut rt, "source-provider-sort");
    assert_eq!(rt.state.detail.sources.sort, SourceSort::Provider);
}

#[test]
fn season_and_sort_selectors_use_pointer_cursors_on_their_own_screens() {
    let mut rt = runtime(880.0);
    for key in ["source-sort", "season-select"] {
        if key == "season-select" {
            tap(&mut rt, "episode-back");
        }
        let (x, y) = rt.find(key).unwrap();
        rt.pointer_move(x, y);
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.cursor(), rune::Cursor::Pointer, "{key}");
    }
}

#[test]
fn episode_stream_list_has_no_season_navigation() {
    let mut rt = runtime(880.0);
    assert!(rt.find("season-select").is_none());
    assert!(rt.find("season-prev").is_none());
    assert!(rt.find("season-next").is_none());
    assert!(rt.find("source-episode").is_some());
    assert!(rt.find("source-episode-number").is_some());
    tap(&mut rt, "episode-back");
    assert!(rt.find("season-select").is_some());
    assert!(rt.state.detail.episodes.selected.is_none());
}

#[test]
fn sort_menu_marks_only_the_current_order() {
    let mut rt = runtime(880.0);
    tap(&mut rt, "source-sort");
    assert!(rt.find("source-provider-sort-selected").is_some());
    assert!(rt.find("source-original").is_none());
    assert!(
        !rt.elements()
            .iter()
            .any(|element| element.text.as_deref() == Some("Original order"))
    );
    tap(&mut rt, "source-best");
    tap(&mut rt, "source-sort");
    assert!(rt.find("source-best-selected").is_some());
}

#[test]
fn provider_sort_is_ascending_and_preserves_order_within_providers() {
    let mut rt = runtime(1000.0);
    rt.update(|s, _| {
        s.detail.sources.streams = vec![
            ("Zulu".into(), torrent("First Zulu", 1, 10)),
            ("alpha".into(), torrent("First alpha", 100, 300)),
            ("Zulu".into(), torrent("Second Zulu", 200, 200)),
            ("Beta".into(), torrent("First Beta", 10, 100)),
        ];
    });
    tap(&mut rt, "source-sort");
    tap(&mut rt, "source-provider-sort");
    assert_order(&mut rt, &[1, 3, 0, 2]);
    tap(&mut rt, "source-sort");
    assert!(rt.find("source-provider-sort-selected").is_some());
    tap(&mut rt, "source-provider-sort");
    assert_order(&mut rt, &[1, 3, 0, 2]);
    assert_eq!(rt.state.detail.sources.streams[0].1.title, "First Zulu");
}

#[test]
fn best_available_is_the_default_regardless_of_provider_name() {
    let mut s = init();
    scene_detail(&mut s);
    s.detail.sources.streams = vec![
        ("Zulu".into(), torrent("Healthy", 100, 300)),
        ("alpha".into(), torrent("Weak", 1, 10)),
    ];
    let mut rt = rune::Runtime::new(s, view, (1000.0, 820.0), rune::Env::default());
    rt.frame(0.0);
    assert_order(&mut rt, &[0, 1]);
    tap(&mut rt, "source-sort");
    assert!(rt.find("source-best-selected").is_some());
}

#[test]
fn provider_sort_respects_filters_and_sorts_late_sources() {
    let mut rt = runtime(1000.0);
    tap(&mut rt, "source-sort");
    tap(&mut rt, "source-provider-sort");
    rt.update(|s, _| {
        s.detail
            .sources
            .streams
            .push(("A new provider".into(), torrent("Late", 1, 10)));
    });
    assert_order(&mut rt, &[3, 0, 1, 2]);
    rt.update(|s, _| s.detail.sources.provider = Some("Provider A".into()));
    assert_order(&mut rt, &[0, 1]);
}
