use super::*;

#[test]
fn installing_without_services_reports_failure_without_staying_busy() {
    let mut s = init();
    s.addons.busy = false;
    s.install("https://example.invalid/manifest.json".into());
    assert!(!s.addons.busy, "No completion can arrive without services");
    assert_eq!(
        s.addons.error.as_deref(),
        Some("Add-on services are unavailable. Restart the app to retry.")
    );
}

#[test]
fn starting_a_catalog_clears_search_warnings_and_stale_status() {
    let mut s = init();
    scene_discover(&mut s);
    s.discover.request.warnings = vec!["Previous search provider failed".into()];
    s.discover.request.status = "Previous search status".into();
    s.fetch_catalog();
    assert!(s.discover.request.warnings.is_empty());
    assert_eq!(s.discover.request.status, "Loading catalog…");

    s.discover.catalog = None;
    s.fetch_catalog();
    assert!(
        s.discover.request.status.is_empty(),
        "No request is loading without a catalog"
    );
    assert!(!s.discover.request.loading);
}

#[test]
fn manifest_categories_browse_mixed_media() {
    for category in ["anime", "documentary"] {
        let mut s = init();
        s.addons.installed = vec![serde_json::from_value(serde_json::json!({
            "manifest_url":"https://example.com/manifest.json",
            "manifest":{"id":"category-provider", "name":"Category provider", "catalogs":[
                {"id":"trending", "type":category},
                {"id":"popular", "type":category, "extra":[{"name":"skip"}]},
                {"id":"filtered", "type":"hidden", "extra":[{"name":"genre", "isRequired":true}]}
            ]}
        })).unwrap()];
        let mut rt = rune::Runtime::new(s, view, (1000.0, 800.0), rune::Env::default());
        rt.frame(0.0);
        tap(&mut rt, &format!("category-{category}"));
        assert_eq!(rt.state.discover.kind, category);
        assert_eq!(rt.state.discover.catalog.as_ref().unwrap().1.id, "trending");
        rt.update(|s, _| {
            s.apply_catalog_page(
                s.discover.generation,
                0,
                Ok(vec![
                    catalog_title("movie", 1),
                    catalog_title("series", 2),
                    catalog_title("channel", 3),
                ]),
            );
        });
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        assert_eq!(
            rt.state
                .discover
                .results
                .iter()
                .map(|m| m.kind.as_str())
                .collect::<Vec<_>>(),
            ["movie", "series"]
        );
        tap(&mut rt, "title-1");
        assert_eq!(rt.state.detail.meta.as_ref().unwrap().kind, "series");
        assert_eq!(rt.state.detail.namespace, "category-provider");
    }
}

#[test]
fn kitsu_details_keep_title_and_episode_library_identity() {
    let mut s = init();
    s.detail.namespace = "community.anime.kitsu".into();
    s.detail.meta = Some(
        serde_json::from_value(serde_json::json!({
            "id":"kitsu:11", "type":"series", "name":"Naruto",
            "videos":[{"id":"kitsu:11:1", "season":1, "episode":1, "title":"Episode 1"}]
        }))
        .unwrap(),
    );
    let parent = s.parent_media().unwrap();
    assert_eq!(parent.key.id, "kitsu:11");
    assert_eq!(parent.key.namespace, "community.anime.kitsu");
    s.select_episode(0);
    assert_eq!(s.detail.episodes.selected.as_deref(), Some("kitsu:11:1"));
}

#[test]
fn completed_catalogs_clear_status_and_show_titles() {
    for kind in ["movie", "series"] {
        let (mut s, tx) = state_with_events();
        scene_discover(&mut s);
        s.discover.kind = kind.into();
        s.discover.request.status = "Loading catalog…".into();
        let metas = s.discover.results.to_vec();
        s.discover.paging.pending = Some(0);
        let mut rt = rune::Runtime::new(s, view, (880.0, 600.0), rune::Env::default());
        rt.poll = Some(poll);
        tx.try_send(Event::Catalog(rt.state.discover.generation, 0, Ok(metas)))
            .unwrap();
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        assert!(rt.state.discover.request.status.is_empty());
        assert!(rt.find("title-0").is_some());
    }
}

fn paged_catalog(kind: &str) -> State {
    let mut s = init();
    scene_discover(&mut s);
    s.discover.kind = kind.into();
    let (addon, mut catalog) = s.discover.catalog.take().unwrap();
    catalog.kind = kind.into();
    catalog.extra.push(scry_addons::Extra {
        name: "skip".into(),
        is_required: false,
    });
    s.discover.catalog = Some((addon, catalog));
    s.discover.results = crate::ui::catalog::CatalogResults::default();
    s.discover.paging.pending = Some(0);
    let titles = (0..40).map(|index| catalog_title(kind, index)).collect();
    s.apply_catalog_page(s.discover.generation, 0, Ok(titles));
    s
}

fn catalog_title(kind: &str, index: usize) -> Meta {
    serde_json::from_value(serde_json::json!({"id":format!("title-{index}"), "type":kind, "name":format!("Title {index}")})).unwrap()
}

#[test]
fn discover_catalog_selector_uses_manifest_catalogs_and_restores_history() {
    for kind in ["anime", "documentary"] {
        let mut s = init();
        s.addons.installed =
            vec![serde_json::from_value(serde_json::json!({
            "manifest_url":"https://example.com/manifest.json",
            "manifest":{"id":"catalog-provider", "name":"Catalog provider", "catalogs":[
                {"id":"trending", "name":"Trending", "type":kind},
                {"id":"popular", "name":"Most Popular", "type":kind, "extra":[{"name":"skip"}]},
                {"id":"search", "type":kind, "extra":[{"name":"search", "isRequired":true}]},
                {"id":"filtered", "type":kind, "extra":[{"name":"genre", "isRequired":true}]}
            ]}
        })).unwrap()];
        s.choose_kind(kind);
        s.apply_catalog_page(
            s.discover.generation,
            0,
            Ok(vec![catalog_title("series", 0)]),
        );
        let mut rt = rune::Runtime::new(s, view, (880.0, 600.0), rune::Env::default());
        rt.on_key = Some(on_key);
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        let before = rect(&rt, "title-0");
        tap(&mut rt, "catalog-select");
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        assert_eq!(rect(&rt, "title-0"), before);
        super::dropdowns::assert_flat_panel(&rt, "catalog-menu");
        assert!(rt.find("catalog-option-0-selected").is_some());
        assert!(rt.find("catalog-option-1").is_some());
        assert!(
            rt.elements()
                .iter()
                .any(|element| { element.text.as_deref() == Some("Trending (limited list)") })
        );
        tap(&mut rt, "catalog-option-1");
        assert_eq!(rt.state.discover.catalog.as_ref().unwrap().1.id, "popular");
        assert!(rt.state.discover.results.is_empty());
        assert_eq!(rt.state.discover.paging.pending, Some(0));
        rt.update(|s, _| {
            s.apply_catalog_page(
                s.discover.generation - 1,
                0,
                Ok(vec![catalog_title("series", 999)]),
            );
            s.apply_catalog_page(
                s.discover.generation,
                0,
                Ok(vec![catalog_title("series", 1)]),
            );
        });
        assert_eq!(rt.state.discover.results[0].id, "title-1");
        tap(&mut rt, "nav-back");
        assert_eq!(rt.state.discover.kind, kind);
        assert_eq!(rt.state.discover.catalog.as_ref().unwrap().1.id, "trending");
        assert_eq!(rt.state.discover.results[0].id, "title-0");
    }
}

#[test]
fn catalog_selector_dismisses_without_loading_and_distinguishes_providers() {
    for (width, height) in [(880.0, 600.0), (1280.0, 820.0)] {
        let mut s = init();
        s.addons.installed = ["First", "Second"]
            .into_iter()
            .map(|name| {
                serde_json::from_value(serde_json::json!({
                    "manifest_url":format!("https://example.com/{name}/manifest.json"),
                    "manifest":{"id":name, "name":name, "catalogs":[
                        {"id":"popular", "name":"Popular", "type":"movie", "extra":[{"name":"skip"}]}
                    ]}
                })).unwrap()
            })
            .collect();
        s.choose_kind("movie");
        s.apply_catalog_page(
            s.discover.generation,
            0,
            Ok(vec![catalog_title("movie", 0)]),
        );
        let mut rt = rune::Runtime::new(s, view, (width, height), rune::Env::default());
        rt.on_key = Some(on_key);
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        let generation = rt.state.discover.generation;
        let history = rt.state.navigation.back.len();
        for dismiss in 0..3 {
            tap(&mut rt, "catalog-select");
            for _ in 0..30 {
                rt.frame(1.0 / 60.0);
            }
            let popup = rect(&rt, "catalog-menu");
            assert!(popup[0] >= 0.0 && popup[1] >= 0.0);
            assert!(popup[0] + popup[2] <= width && popup[1] + popup[3] <= height);
            match dismiss {
                0 => rt.key(rune::KeyEvent {
                    key: rune::Key::Escape,
                    mods: rune::Mods::default(),
                }),
                1 => {
                    rt.pointer_down(10.0, 10.0);
                    rt.pointer_up(10.0, 10.0);
                }
                _ => tap(&mut rt, "catalog-option-0"),
            }
            rt.frame(0.0);
            assert!(!rt.state.discover.menu_open);
            assert_eq!(rt.state.discover.generation, generation);
            assert_eq!(rt.state.navigation.back.len(), history);
        }
        tap(&mut rt, "catalog-select");
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        tap(&mut rt, "catalog-option-1");
        assert_eq!(
            rt.state.discover.catalog.as_ref().unwrap().0.manifest.id,
            "Second"
        );
        assert_eq!(rt.state.discover.paging.pending, Some(0));
    }
}

#[test]
fn duplicate_catalog_pages_advance_automatically_but_have_a_retry_bound() {
    let s = paged_catalog("series");
    let mut rt = rune::Runtime::new(s, view, (880.0, 600.0), rune::Env::default());
    rt.poll = Some(poll);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let viewport = rect(&rt, "titles-scroll-0");
    rt.wheel(viewport[0] + 20.0, viewport[1] + 20.0, -100_000.0);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    rt.update(|s, _| {
        s.apply_catalog_page(
            s.discover.generation,
            40,
            Ok((0..40)
                .map(|index| catalog_title("series", index))
                .collect()),
        );
    });
    assert_eq!(rt.state.discover.results.len(), 40);
    assert_eq!(rt.state.discover.paging.pending, Some(80));
    rt.update(|s, _| {
        s.apply_catalog_page(
            s.discover.generation,
            80,
            Ok(vec![catalog_title("series", 40)]),
        );
    });
    assert_eq!(rt.state.discover.results.len(), 41);
    assert!(rt.state.discover.paging.has_more);
    rt.update(|s, _| {
        s.request_more_catalog();
        for offset in 81..84 {
            assert_eq!(s.discover.paging.pending, Some(offset));
            s.apply_catalog_page(
                s.discover.generation,
                offset,
                Ok(vec![catalog_title("series", 0)]),
            );
        }
    });
    assert!(!rt.state.discover.paging.has_more);
    assert_eq!(rt.state.discover.paging.pending, None);
    assert_eq!(rt.state.discover.results.len(), 41);
}

#[test]
fn movie_and_tv_catalogs_load_on_scroll_append_without_jumping_and_stop_on_repeats() {
    for kind in ["movie", "series"] {
        let s = paged_catalog(kind);
        let mut rt = rune::Runtime::new(s, view, (880.0, 600.0), rune::Env::default());
        rt.poll = Some(poll);
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        assert_eq!(rt.state.discover.paging.pending, None);
        let viewport = rect(&rt, "titles-scroll-0");
        rt.wheel(viewport[0] + 20.0, viewport[1] + 20.0, -100_000.0);
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        assert_eq!(rt.state.discover.paging.pending, Some(40));
        let last_before = rect(&rt, "title-39");
        rt.update(|s, _| {
            s.apply_catalog_page(
                s.discover.generation + 1,
                40,
                Ok(vec![catalog_title(kind, 999)]),
            );
            s.apply_catalog_page(s.discover.generation, 0, Ok(vec![catalog_title(kind, 998)]));
            let mut more = vec![catalog_title(kind, 0)];
            more.extend((40..50).map(|index| catalog_title(kind, index)));
            s.apply_catalog_page(s.discover.generation, 40, Ok(more));
        });
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        assert_eq!(rt.state.discover.results.len(), 50);
        assert_eq!(rt.state.discover.paging.next_offset, 51);
        assert!((rect(&rt, "title-39")[1] - last_before[1]).abs() < 0.1);
        rt.wheel(viewport[0] + 20.0, viewport[1] + 20.0, -100_000.0);
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        assert_eq!(rt.state.discover.paging.pending, Some(51));
        rt.update(|s, _| {
            for offset in 51..54 {
                s.apply_catalog_page(
                    s.discover.generation,
                    offset,
                    Ok(vec![catalog_title(kind, 0)]),
                );
            }
        });
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        assert!(!rt.state.discover.paging.has_more);
        assert_eq!(rt.state.discover.results.len(), 50);
        tap(&mut rt, "title-49");
        tap(&mut rt, "nav-back");
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        assert_eq!(rt.state.discover.results.len(), 50);
        assert!(!rt.state.discover.paging.has_more);
        assert!(rt.find("title-49").is_some());
        rt.update(|s, _| {
            s.fetch_catalog();
            s.apply_catalog_page(
                s.discover.generation,
                0,
                Ok((0..10).map(|index| catalog_title(kind, index)).collect()),
            );
        });
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        assert_eq!(rt.state.discover.results.len(), 10);
        assert!(rt.state.posters.visible.end <= 10usize.div_ceil(rt.state.posters.columns));
    }
}

#[test]
fn catalog_page_failures_keep_loaded_titles_and_require_explicit_retry() {
    let mut s = paged_catalog("movie");
    s.discover.paging.pending = Some(40);
    s.apply_catalog_page(
        s.discover.generation,
        40,
        Err("Provider unavailable".into()),
    );
    assert_eq!(s.discover.results.len(), 40);
    assert!(s.discover.paging.failed);
    s.posters.viewport = Some([0.0, 0.0, 600.0, 400.0]);
    let rows = s.discover.results.len().div_ceil(s.posters.columns);
    s.posters.visible = rows - 1..rows;
    s.request_more_catalog();
    assert_eq!(s.discover.paging.pending, None);
    s.discover.paging.failed = false;
    s.request_more_catalog();
    assert_eq!(s.discover.paging.pending, Some(40));
    s.apply_catalog_page(s.discover.generation, 40, Ok(Vec::new()));
    assert!(!s.discover.paging.has_more);
    assert_eq!(s.discover.results.len(), 40);
}

#[test]
fn discover_title_count_is_not_zero_padded() {
    for count in [0, 5, 50, 100] {
        let mut s = init();
        scene_discover(&mut s);
        s.discover.request.loading = false;
        s.discover.results = crate::ui::catalog::CatalogResults::from(
            (0..count)
                .map(|index| catalog_title("series", index))
                .collect::<Vec<_>>(),
        );
        let mut rt = rune::Runtime::new(s, view, (1000.0, 800.0), rune::Env::default());
        rt.frame(0.0);
        let expected = format!("{count} titles loaded");
        assert!(
            rt.elements()
                .iter()
                .any(|element| element.text.as_deref() == Some(expected.as_str())),
            "Discover must display {expected:?}"
        );
    }
}

#[test]
fn explorer_shows_titles_and_years() {
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
        s.discover.displayed_query = "Arrival".into();
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
    state.discover.results = crate::ui::catalog::CatalogResults::from(
        (0..1000)
            .map(|index| {
                let mut meta = state.discover.results[index % state.discover.results.len()].clone();
                meta.id = format!("title-{index}");
                meta.name = format!("Catalog title {index}");
                meta
            })
            .collect::<Vec<_>>(),
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
fn poster_fetching_tracks_real_layout_across_large_screen_resizes() {
    let mut s = init();
    scene_discover(&mut s);
    s.discover.results = crate::ui::catalog::CatalogResults::from(
        (0..200)
            .map(|index| {
                let mut meta = s.discover.results[0].clone();
                meta.poster = Some(format!("https://example.com/{index}.jpg"));
                meta
            })
            .collect::<Vec<_>>(),
    );
    let mut rt = rune::Runtime::new(s, view, (3840.0, 2160.0), rune::Env::default());
    for (width, height) in [(3840.0, 2160.0), (880.0, 600.0), (3840.0, 2160.0)] {
        rt.set_viewport((width, height), 1.0);
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        let viewport = rect(&rt, "titles-scroll-0");
        let urls = visible_artwork(&rt.state);
        if width > 3000.0 {
            assert!(urls.len() > 32);
        } else {
            assert!(urls.len() < 12);
        }
        for element in rt.elements().into_iter().filter(|element| !element.exiting) {
            let Some(index) = element
                .key
                .as_deref()
                .and_then(|key| key.strip_prefix("title-"))
                .and_then(|index| index.parse::<usize>().ok())
            else {
                continue;
            };
            if element.rect[1] < viewport[1] + viewport[3]
                && element.rect[1] + element.rect[3] > viewport[1]
            {
                assert!(
                    urls.contains(&format!("https://example.com/{index}.jpg")),
                    "Visible card {index} must load at {width} x {height}"
                );
            }
        }
    }
}

#[test]
fn keyed_reveal_reaches_an_unmounted_poster_row_and_loads_only_visible_artwork() {
    let mut s = paged_catalog("movie");
    s.discover.results = crate::ui::catalog::CatalogResults::from(
        (0..1000)
            .map(|index| {
                let mut meta = catalog_title("movie", index);
                meta.poster = Some(format!("https://example.invalid/{index}.jpg"));
                meta
            })
            .collect::<Vec<_>>(),
    );
    let mut rt = rune::Runtime::new(s, view, (880.0, 600.0), rune::Env::default());
    rt.poll = Some(poll);
    for _ in 0..120 {
        rt.frame(1.0 / 60.0);
    }
    let row = 900 / rt.state.posters.columns;
    rt.update(move |_, tx| {
        tx.scroll_into_view_in("titles-scroll-0", format!("title-row-{row}"));
    });
    for _ in 0..120 {
        rt.frame(1.0 / 60.0);
    }
    let viewport = rect(&rt, "titles-scroll-0");
    let card = rect(&rt, "title-900");
    assert!(card[1] < viewport[1] + viewport[3] && card[1] + card[3] > viewport[1]);
    let urls = visible_artwork(&rt.state);
    assert!(urls.contains(&"https://example.invalid/900.jpg".into()));
    assert!(!urls.contains(&"https://example.invalid/0.jpg".into()));
    assert!(urls.len() <= rt.state.posters.columns * 3);
}

#[test]
fn large_viewport_includes_partial_rows_without_fetching_offscreen_posters() {
    let mut s = init();
    scene_discover(&mut s);
    s.discover.results = crate::ui::catalog::CatalogResults::from(
        (0..80)
            .map(|index| {
                let mut meta = s.discover.results[0].clone();
                meta.poster = Some(format!("https://example.com/{index}.jpg"));
                meta
            })
            .collect::<Vec<_>>(),
    );
    s.posters.route = s.navigation.route_key;
    s.posters.columns = 10;
    s.posters.viewport = Some([0.0, 0.0, 2000.0, 1201.0]);
    s.posters.visible = 0..5;
    assert_eq!(
        visible_artwork(&s),
        (0..50)
            .map(|index| format!("https://example.com/{index}.jpg"))
            .collect::<Vec<_>>()
    );
    s.posters.viewport = Some([0.0, 0.0, 2000.0, 600.0]);
    s.posters.visible = 0..2;
    assert_eq!(visible_artwork(&s).len(), 20);
}

#[test]
fn idle_scroll_queues_newly_visible_posters_without_an_extra_wake() {
    let mut state = init();
    scene_discover(&mut state);
    state.discover.results = crate::ui::catalog::CatalogResults::from(
        (0..12)
            .map(|index| {
                let mut meta = state.discover.results[index % state.discover.results.len()].clone();
                meta.poster = Some(format!("https://example.invalid/poster-{index}.jpg"));
                meta
            })
            .collect::<Vec<_>>(),
    );
    let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
    rt.poll = Some(poll);
    for _ in 0..120 {
        rt.frame(1.0 / 60.0);
    }
    let url = "https://example.invalid/poster-11.jpg".to_owned();
    assert!(!rt.state.artwork.poster_urls.contains(&url));
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
        rt.state.artwork.poster_urls.contains(&url),
        "Visible posters must be queued before the window goes idle"
    );
}

#[test]
fn poster_grid_scrolls_to_last_title_and_back_reveals_the_opened_card() {
    let mut state = init();
    scene_discover(&mut state);
    state.discover.results = crate::ui::catalog::CatalogResults::from(
        (0..40)
            .map(|index| {
                let mut meta = state.discover.results[index % state.discover.results.len()].clone();
                meta.id = format!("title-{index}");
                meta.poster = Some(format!("https://example.invalid/poster-{index}.jpg"));
                meta
            })
            .collect::<Vec<_>>(),
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
    assert_eq!(rect(&rt, "topbar"), toolbar);
    tap(&mut rt, "title-39");
    assert_eq!(rt.state.detail.meta.as_ref().unwrap().id, "title-39");
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
#[test]
fn search_input_shows_pending_requests_until_the_last_provider_finishes() {
    let (mut s, tx) = state_with_events();
    s.discover.search = TextEdit::new("Avatar");
    s.submit_search();
    let mut rt = rune::Runtime::new(s, view, (1280.0, 820.0), rune::Env::default());
    rt.frame(0.0);
    assert!(rt.find("search-spinner").is_some());
    assert!(rt.find("search").is_some());
    rt.poll = Some(poll);
    tx.try_send(Event::Search(
        rt.state.discover.generation,
        vec![],
        vec![],
        false,
    ))
    .unwrap();
    rt.frame(1.0 / 60.0);
    assert!(rt.state.discover.request.status.is_empty());
    assert!(rt.find("search-spinner").is_some());
    tx.try_send(Event::Search(
        rt.state.discover.generation,
        vec![],
        vec![],
        true,
    ))
    .unwrap();
    rt.frame(1.0 / 60.0);
    assert!(rt.state.discover.request.status.is_empty());
    assert!(rt.find("search").is_some());
    drop(tx);
}
