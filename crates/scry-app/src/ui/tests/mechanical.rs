use super::*;

#[test]
fn compact_shell_gives_space_to_content_and_keeps_navigation_usable() {
    for width in [880.0, 1280.0] {
        let mut rt = rune::Runtime::new(init(), view, (width, 600.0), rune::Env::default());
        rt.frame(0.0);
        for title in ["YOUR COLLECTION", "DISCOVER", "SCHEDULE", "SYSTEM"] {
            assert!(
                rt.elements()
                    .iter()
                    .any(|element| element.text.as_deref() == Some(title)),
                "Sidebar heading {title} must not have a number prefix"
            );
        }
        assert!(rect(&rt, "topbar")[3] <= 52.0);
        assert!(rect(&rt, "sidebar")[2] <= 200.0);
        let pages = rect(&rt, "sidebar-pages");
        let settings = rect(&rt, "nav-settings");
        rt.update(|_, tx| tx.scroll_into_view("nav-calendar"));
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        let calendar = rect(&rt, "nav-calendar");
        assert!(
            calendar[1] >= pages[1] && calendar[1] + calendar[3] <= pages[1] + pages[3] + 0.5,
            "Calendar must remain fully reachable at minimum height"
        );
        assert_eq!(rect(&rt, "nav-settings"), settings);
        tap(&mut rt, "nav-calendar");
        assert_eq!(rt.state.navigation.page, Page::Calendar);
        let nav = rect(&rt, "nav-settings");
        assert!(nav[3] >= 32.0 && nav[3] <= 36.0);
        assert!(nav[1] + nav[3] <= 600.0);
        tap(&mut rt, "nav-settings");
        assert_eq!(rt.state.navigation.page, Page::Settings);
        assert!(rt.find("streaming-settings").is_some());
    }
}

#[test]
fn sidebar_groups_collection_discovery_and_schedule_links_in_order() {
    for width in [880.0, 1280.0] {
        let mut state = init();
        state.addons.installed = vec![
            serde_json::from_value(serde_json::json!({
                "manifest_url":"https://example.invalid/manifest.json",
                "manifest":{"id":"example", "name":"Example", "catalogs":[
                    {"id":"anime", "type":"anime"}
                ]}
            }))
            .unwrap(),
        ];
        let mut rt = rune::Runtime::new(state, view, (width, 820.0), rune::Env::default());
        rt.frame(0.0);
        for (last_link, heading) in [
            ("nav-my-media", "sidebar-discover-heading"),
            ("category-anime", "sidebar-schedule-heading"),
        ] {
            let link = rect(&rt, last_link);
            let heading = rect(&rt, heading);
            assert!(
                heading[1] - link[1] - link[3]
                    >= super::super::design::tokens::layout::PAGE_GAP - 0.5,
                "Navigation groups need more spacing than links within a group"
            );
        }
        let keys = [
            "sidebar-collection-heading",
            "nav-watching",
            "nav-library",
            "nav-my-media",
            "sidebar-discover-heading",
            "movies",
            "series",
            "category-anime",
            "sidebar-schedule-heading",
            "nav-guide",
            "nav-calendar",
        ];
        for pair in keys.windows(2) {
            let previous = rect(&rt, pair[0]);
            let next = rect(&rt, pair[1]);
            assert!(
                next[1] >= previous[1] + previous[3],
                "{} must follow {}",
                pair[1],
                pair[0]
            );
        }
        assert!(
            rt.elements()
                .iter()
                .any(|element| element.text.as_deref() == Some("Series"))
        );
        for (key, page) in [
            ("nav-library", Page::Library),
            ("nav-my-media", Page::MyMedia),
            ("nav-watching", Page::Watching),
            ("nav-guide", Page::Guide),
            ("nav-calendar", Page::Calendar),
        ] {
            tap(&mut rt, key);
            assert_eq!(rt.state.navigation.page, page);
        }
        for (key, kind) in [
            ("movies", "movie"),
            ("series", "series"),
            ("category-anime", "anime"),
        ] {
            tap(&mut rt, key);
            assert_eq!(rt.state.navigation.page, Page::Discover);
            assert_eq!(rt.state.discover.kind, kind);
        }
    }
}

#[test]
fn compact_catalog_exposes_more_titles_without_clipping_text_or_posters() {
    let mut s = init();
    scene_discover(&mut s);
    let mut rt = rune::Runtime::new(s, view, (1280.0, 820.0), rune::Env::default());
    for _ in 0..4 {
        rt.frame(0.0);
    }
    assert!(
        rt.state.posters.columns >= 6,
        "A desktop catalog must use compact columns"
    );
    let first = rect(&rt, "title-0");
    let poster = rect(&rt, "title-poster-0");
    let info = rect(&rt, "title-info-0");
    assert!(poster[3] <= 200.0);
    assert!(info[1] >= poster[1] + poster[3]);
    assert!(info[1] + info[3] <= first[1] + first[3] + 0.5);
    tap(&mut rt, "title-0");
    assert_eq!(
        rt.state.detail.meta.as_ref().map(|m| m.id.as_str()),
        Some("scene-0")
    );
}

#[test]
fn screens_fit_with_the_fonts_shipped_in_the_app() {
    use super::super::{design::tokens, scenes};
    for width in [880.0, 1280.0] {
        for fixture in [
            scenes::scene_discover,
            scenes::scene_library,
            scenes::scene_detail,
            scenes::scene_settings,
            scenes::scene_addons,
            scenes::scene_calendar_month,
            super::super::stats::scene,
        ] {
            let mut state = init();
            fixture(&mut state);
            let mut rt = rune::Runtime::new(state, view, (width, 820.0), rune::Env::default());
            rt.text = rune::text::TextSystem::new(tokens::fonts::FILES, tokens::fonts::FAMILIES);
            for _ in 0..30 {
                rt.frame(1.0 / 60.0);
            }
            for element in rt
                .elements()
                .into_iter()
                .filter(|e| !e.exiting && e.visible[2] > 0.0 && e.visible[3] > 0.0)
            {
                assert!(
                    element.rect[0] + element.rect[2] <= width + 0.5,
                    "{:?} overflows {:?} at {width}: {:?}",
                    element.key,
                    rt.state.navigation.page,
                    element.rect
                );
            }
            for key in ["nav-settings", "nav-back", "search"] {
                let target = rect(&rt, key);
                assert!(target[2] > 0.0 && target[3] >= tokens::control::HEIGHT);
            }
        }
    }
}
