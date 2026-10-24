use super::*;

#[test]
fn explorer_shows_titles_and_years_without_provider_controls_or_repeated_type_labels() {
    let mut state = init();
    scene_discover(&mut state);
    let mut rt = rune::Runtime::new(state, view, (1000.0, 800.0), rune::Env::default());
    rt.frame(0.0);
    let elements = rt.elements();
    let texts: Vec<_> = elements
        .iter()
        .filter_map(|element| element.text.as_deref())
        .collect();
    assert!(texts.contains(&"Movies"));
    assert!(!texts.contains(&"Cinema collection"));
    assert!(!texts.contains(&"Popular movies"));
    assert!(!texts.contains(&"Modern classics"));
    assert_eq!(
        elements
            .iter()
            .find(|element| element.key.as_deref() == Some("title-info-0"))
            .unwrap()
            .text
            .as_deref(),
        Some("2024")
    );
    rt.update(|s, _| {
        s.displayed_query = "Arrival".into();
    });
    rt.frame(0.0);
    assert_eq!(
        rt.elements()
            .iter()
            .find(|element| element.key.as_deref() == Some("title-info-0"))
            .unwrap()
            .text
            .as_deref(),
        Some("2024 · Movie")
    );
}

#[test]
#[expect(
    clippy::print_stderr,
    reason = "Print measured layout statistics only in this performance test"
)]
fn large_catalog_only_lays_out_nearby_cards() {
    let mut state = init();
    scene_discover(&mut state);
    state.metas = Arc::new(
        (0..1000)
            .map(|index| {
                let mut meta = state.metas[index % state.metas.len()].clone();
                meta.id = format!("title-{index}");
                meta.name = format!("Catalog title {index}");
                meta
            })
            .collect(),
    );
    let started = std::time::Instant::now();
    let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
    rt.poll = Some(poll);
    for _ in 0..5 {
        rt.frame(1.0 / 60.0);
    }
    let mounted = rt
        .elements()
        .iter()
        .filter(|element| {
            !element.exiting
                && element.key.as_deref().is_some_and(|key| {
                    key.strip_prefix("title-")
                        .is_some_and(|index| index.parse::<usize>().is_ok())
                })
        })
        .count();
    eprintln!(
        "catalog: mounted={mounted}, settle={:?}, memory={:?}",
        started.elapsed(),
        scry_playback::telemetry::memory()
    );
    assert!(
        mounted < 100,
        "Only nearby cards should be laid out, got {mounted}"
    );
    let viewport = rect(&rt, "titles-scroll-0");
    rt.wheel(viewport[0] + 30.0, viewport[1] + 30.0, -1_000_000.0);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let last = rect(&rt, "title-999");
    assert!(last[1] >= viewport[1] && last[1] + last[3] <= viewport[1] + viewport[3] + 1.0);
    tap(&mut rt, "title-999");
    tap(&mut rt, "nav-back");
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let restored = rect(&rt, "title-999");
    assert!(
        restored[1] >= viewport[1] && restored[1] + restored[3] <= viewport[1] + viewport[3] + 1.0
    );
}

#[test]
fn idle_scroll_queues_newly_visible_posters_without_an_extra_wake() {
    let mut state = init();
    scene_discover(&mut state);
    state.metas = Arc::new(
        (0..12)
            .map(|index| {
                let mut meta = state.metas[index % state.metas.len()].clone();
                meta.poster = Some(format!("https://example.invalid/poster-{index}.jpg"));
                meta
            })
            .collect(),
    );
    let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
    rt.poll = Some(poll);
    for _ in 0..120 {
        rt.frame(1.0 / 60.0);
    }
    let url = "https://example.invalid/poster-11.jpg".to_owned();
    assert!(!rt.state.artwork_urls.contains(&url));
    let viewport = rect(&rt, "titles-scroll-0");
    rt.wheel(viewport[0] + 30.0, viewport[1] + 30.0, -1_000_000.0);
    for _ in 0..20 {
        let moving = rt.frame(1.0 / 60.0);
        if !moving && !rt.has_pending_input() {
            break;
        }
    }
    assert!(
        visible_artwork(&rt.state).contains(&url),
        "Fixture must reveal the last row"
    );
    assert!(
        rt.state.artwork_urls.contains(&url),
        "Visible posters must be queued before the window goes idle"
    );
}

#[test]
fn poster_grid_scrolls_to_last_title_and_back_reveals_the_opened_card() {
    let mut state = init();
    scene_discover(&mut state);
    state.metas = Arc::new(
        (0..40)
            .map(|index| {
                let mut meta = state.metas[index % state.metas.len()].clone();
                meta.id = format!("title-{index}");
                meta.poster = Some(format!("https://example.invalid/poster-{index}.jpg"));
                meta
            })
            .collect(),
    );
    let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
    rt.poll = Some(poll);
    for _ in 0..5 {
        rt.frame(1.0 / 60.0);
    }
    let toolbar = rect(&rt, "topbar");
    let viewport = rect(&rt, "titles-scroll-0");
    assert!(
        visible_artwork(&rt.state).len() < 12,
        "Do not request every result's artwork"
    );
    assert!(rect(&rt, "title-2")[0] + rect(&rt, "title-2")[2] <= viewport[0] + viewport[2] + 1.0);
    rt.wheel(viewport[0] + 30.0, viewport[1] + 30.0, -100_000.0);
    for _ in 0..20 {
        rt.frame(1.0 / 60.0);
    }
    let last = rect(&rt, "title-39");
    let artwork = visible_artwork(&rt.state);
    assert!(artwork.contains(&"https://example.invalid/poster-39.jpg".to_owned()));
    assert!(!artwork.contains(&"https://example.invalid/poster-0.jpg".to_owned()));
    assert!(last[1] >= viewport[1] && last[1] + last[3] <= viewport[1] + viewport[3] + 1.0);
    assert!(
        !rt.elements()
            .iter()
            .any(|element| !element.exiting && element.key.as_deref() == Some("title-0")),
        "Distant cards should be unmounted"
    );
    assert_eq!(rect(&rt, "topbar"), toolbar);
    tap(&mut rt, "title-39");
    assert_eq!(rt.state.detail.as_ref().unwrap().id, "title-39");
    tap(&mut rt, "nav-back");
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let restored = rect(&rt, "title-39");
    assert!(
        restored[1] >= viewport[1] && restored[1] + restored[3] <= viewport[1] + viewport[3] + 1.0,
        "Back must reveal the clicked card: {restored:?}"
    );
}
