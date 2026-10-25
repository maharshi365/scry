use super::*;

#[test]
fn topbar_controls_share_height_and_alignment_while_searching() {
    for width in [880.0, 1280.0] {
        let mut rt = rune::Runtime::new(init(), view, (width, 600.0), rune::Env::default());
        rt.frame(0.0);
        let search = rect(&rt, "search-frame");
        let button = rect(&rt, "search-go");
        for key in ["nav-back", "nav-forward", "search-go", "page-refresh"] {
            let bounds = rect(&rt, key);
            assert!((bounds[3] - search[3]).abs() < 0.5, "{key} height");
            assert!((bounds[1] - search[1]).abs() < 0.5, "{key} alignment");
        }
        rt.update(|s, _| {
            s.navigation.page = Page::Discover;
            s.loading = true;
            s.displayed_query = "movie".into();
        });
        rt.frame(0.0);
        assert_eq!(rect(&rt, "search-go"), button);
        assert_eq!(rect(&rt, "search-frame"), search);
    }
}

#[test]
fn watched_episode_badges_do_not_move_episode_cards_or_titles() {
    let mut s = init();
    s.navigation.page = Page::Detail;
    s.detail = Some(series(3));
    s.detail.as_mut().unwrap().videos[0].title =
        "A long episode name that wraps onto multiple lines in the episode list".into();
    let key = s.visible_episode_keys()[0].clone();
    s.selected_video_id = Some(key.id.clone());
    s.library.snapshot = Some(crate::library_worker::Snapshot {
        selected_state: Some(crate::library_worker::SelectedState {
            key: key.clone(),
            watched: scry_library::WatchedState { watched: false },
            watchlist: true,
            progress: Some(scry_library::Progress {
                position_seconds: 30.0,
                duration_seconds: 100.0,
            }),
        }),
        episode_states: vec![],
    });
    let mut rt = rune::Runtime::new(s, view, (880.0, 600.0), rune::Env::default());
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let before: Vec<_> = (0..3)
        .map(|index| rect(&rt, &format!("episode-{index}")))
        .collect();
    let title = rect(&rt, "episode-title-0");
    let sources = rect(&rt, "sources-scroll-0");
    rt.update(|s, _| {
        s.library.snapshot = Some(crate::library_worker::Snapshot {
            selected_state: Some(crate::library_worker::SelectedState {
                key: key.clone(),
                watched: scry_library::WatchedState { watched: true },
                watchlist: true,
                progress: None,
            }),
            episode_states: vec![(key, scry_library::WatchedState { watched: true })],
        });
    });
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rt.find("episode-watched-0").is_some());
    assert_eq!(rect(&rt, "episode-title-0"), title);
    assert_eq!(rect(&rt, "sources-scroll-0"), sources);
    for (index, bounds) in before.into_iter().enumerate() {
        assert_eq!(rect(&rt, &format!("episode-{index}")), bounds);
    }
}

#[test]
fn sidebar_keeps_only_a_compact_logo_at_minimum_window_size() {
    let mut rt = rune::Runtime::new(init(), view, (880.0, 600.0), rune::Env::default());
    rt.frame(0.0);
    let elements = rt.elements();
    let header = rect(&rt, "topbar");
    let logo = rect(&rt, "brand-logo");
    assert!(
        elements
            .iter()
            .all(|element| element.text.as_deref() != Some("Scry"))
    );
    assert_eq!([logo[2], logo[3]], [28.0, 28.0]);
    assert!(logo[0] >= header[0] && logo[1] >= header[1]);
    assert!(logo[1] + logo[3] <= header[1] + header[3]);
    let back = rect(&rt, "nav-back");
    assert!(back[0] - (logo[0] + logo[2]) <= 24.0);
    assert!((logo[1] + logo[3] / 2.0 - (back[1] + back[3] / 2.0)).abs() < 1.0);
    assert!(rect(&rt, "nav-watching")[1] >= header[1] + header[3] + 14.0);
    assert_eq!(rect(&rt, "sidebar")[1], header[1] + header[3]);
}

#[test]
fn episode_wheel_is_bounded_and_sources_and_toolbar_stay_visible() {
    let mut state = init();
    state.navigation.page = Page::Detail;
    state.detail = Some(series(250));
    let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
    rt.frame(0.0);
    let toolbar = rect(&rt, "topbar");
    let viewport = rect(&rt, "episodes-scroll-0-0-Some(1)");
    let sources = rect(&rt, "sources-scroll-0");
    let first = rect(&rt, "episode-0");
    assert!(viewport[3] > 100.0 && viewport[1] + viewport[3] <= 600.0);
    assert!(sources[0] >= viewport[0] + viewport[2]);
    rt.wheel(viewport[0] + 20.0, viewport[1] + 20.0, -100_000.0);
    for _ in 0..120 {
        rt.frame(1.0 / 60.0);
    }
    let last = rect(&rt, "episode-99");
    assert!(
        rect(&rt, "episode-0")[1] < first[1] - 100.0,
        "first {first:?}, after {:?}, viewport {viewport:?}, last {last:?}",
        rect(&rt, "episode-0")
    );
    assert!(
        last[1] >= viewport[1] && last[1] + last[3] <= viewport[1] + viewport[3] + 1.0,
        "Last episode must be reachable: {last:?} in {viewport:?}"
    );
    assert_eq!(rect(&rt, "topbar"), toolbar);
    assert_eq!(rect(&rt, "sources-scroll-0"), sources);
}
