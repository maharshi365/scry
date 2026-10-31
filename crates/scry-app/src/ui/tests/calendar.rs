use super::*;
use crate::ui::calendar::Release;
use scry_library::{Media, MediaKey, MediaKind};

#[test]
fn month_grid_fits_its_viewport_without_unnecessary_page_scrolling() {
    for (width, height) in [(1280.0, 820.0), (1920.0, 1080.0), (3840.0, 2160.0)] {
        let mut s = init();
        s.navigation.page = Page::Calendar;
        s.calendar.anchor = chrono::NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
        let mut rt = rune::Runtime::new(s, view, (width, height), rune::Env::default());
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        let viewport = rt.state.calendar.viewport.unwrap();
        let last = rect(&rt, "calendar-slot-41");
        assert!(
            last[1] + last[3] <= viewport[1] + viewport[3] + 1.0,
            "Month grid exceeds the viewport at {width} x {height}: {last:?}, {viewport:?}"
        );
        let first = rect(&rt, "calendar-slot-0");
        rt.wheel(viewport[0] + 1.0, viewport[1] + 1.0, -500.0);
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        assert_eq!(rect(&rt, "calendar-slot-0"), first);
    }
}

#[test]
fn calendar_headers_only_offer_counts_and_compact_expand_actions_for_releases() {
    let mut s = init();
    s.navigation.page = Page::Calendar;
    let empty_date = chrono::NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
    let date = empty_date.succ_opt().unwrap();
    s.calendar.anchor = empty_date;
    s.calendar.releases.push(Release {
        date,
        media: Media {
            key: MediaKey::new("test", MediaKind::Movie, "movie"),
            title: "Movie".into(),
            parent: None,
        },
        episode_id: "movie".into(),
        title: String::new(),
        season: None,
        episode: None,
        poster: None,
    });
    let mut rt = rune::Runtime::new(s, view, (1280.0, 820.0), rune::Env::default());
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let elements = rt.elements();
    assert!(rt.find(&format!("calendar-expand-{empty_date}")).is_none());
    assert!(
        elements
            .iter()
            .any(|e| e.text.as_deref() == Some(&empty_date.format("%a %d").to_string()))
    );
    assert!(
        elements
            .iter()
            .any(|e| e.text.as_deref() == Some(&format!("{} · 1", date.format("%a %d"))))
    );
    let expand_key = format!("calendar-expand-{date}");
    let expand = elements
        .iter()
        .find(|e| e.key.as_deref() == Some(&expand_key))
        .unwrap();
    assert_eq!(expand.fill, super::super::design::tokens::transparent());
    let index = elements
        .iter()
        .position(|e| e.key.as_deref() == Some(&expand_key))
        .unwrap();
    let glyph = elements.iter().find(|e| e.parent == Some(index)).unwrap();
    assert!(glyph.rect[3] <= super::super::design::tokens::type_::SMALL.size * 1.2);
    assert!(elements.iter().all(|e| e.text.as_deref() != Some("—")));
    assert!(
        elements
            .iter()
            .all(|e| !matches!(e.key.as_deref(), Some("calendar-week" | "calendar-month")))
    );
    tap(&mut rt, &expand_key);
    assert_eq!(rt.state.calendar.expanded_day, Some(date));
    let elements = rt.elements();
    assert!(elements.iter().all(|e| !matches!(
        e.key.as_deref(),
        Some("calendar-dialog-prev" | "calendar-dialog-next")
    )));
    assert!(
        elements
            .iter()
            .all(|e| e.text.as_deref() != Some("No releases on this day."))
    );
    tap(&mut rt, "calendar-dialog-close");
    assert_eq!(rt.state.calendar.expanded_day, None);
}

#[test]
fn dialog_posters_follow_the_dialog_scroll_viewport() {
    let mut s = init();
    s.navigation.page = Page::Calendar;
    let date = s.calendar.anchor;
    s.calendar.expanded_day = Some(date);
    s.calendar.dialog_viewport = Some([0.0, 0.0, 500.0, 100.0]);
    for i in 0..2 {
        s.calendar.releases.push(Release {
            date,
            media: Media {
                key: MediaKey::new("test", MediaKind::Movie, format!("movie{i}")),
                title: "Movie".into(),
                parent: None,
            },
            episode_id: format!("movie{i}"),
            title: String::new(),
            season: None,
            episode: None,
            poster: Some(format!("https://example.com/{i}.jpg")),
        });
        s.calendar.dialog_artwork_rects.insert(
            (date, i),
            [0.0, if i == 0 { 0.0 } else { 120.0 }, 500.0, 100.0],
        );
    }
    assert_eq!(visible_artwork(&s), vec!["https://example.com/0.jpg"]);
    s.calendar.dialog_viewport = Some([0.0, 120.0, 500.0, 100.0]);
    assert_eq!(visible_artwork(&s), vec!["https://example.com/1.jpg"]);
}

#[test]
fn context_refresh_reloads_calendar_without_changing_day_geometry() {
    let mut s = init();
    s.navigation.page = Page::Calendar;
    let date = s.calendar.anchor;
    let mut rt = rune::Runtime::new(s, view, (1280.0, 820.0), rune::Env::default());
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let day = rect(&rt, &format!("calendar-day-{date}"));
    assert!(rt.find("calendar-refresh").is_none());
    rt.secondary_down(1250.0, 790.0);
    rt.secondary_up(1250.0, 790.0);
    rt.frame(1.0 / 60.0);
    // Allow the pointer-anchored menu to fit inside the window before clicking.
    rt.frame(1.0 / 60.0);
    tap(&mut rt, "context-refresh");
    assert!(
        rt.state.shell.context_menu.is_none(),
        "refresh was not activated"
    );
    assert!(rt.state.calendar.loading);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    assert_eq!(rect(&rt, &format!("calendar-day-{date}")), day);
    rt.update(|s, _| s.apply_calendar(s.calendar.generation, vec![], 0));
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    assert_eq!(rect(&rt, &format!("calendar-day-{date}")), day);
}

#[test]
fn expanded_day_loads_posters_outside_the_grid_and_renders_them() {
    let mut s = init();
    s.navigation.page = Page::Calendar;
    let date = s.calendar.anchor + chrono::Duration::days(60);
    s.calendar.expanded_day = Some(date);
    s.calendar.releases.push(Release {
        date,
        media: Media {
            key: MediaKey::new("test", MediaKind::Movie, "movie"),
            title: "Movie".into(),
            parent: None,
        },
        episode_id: "movie".into(),
        title: String::new(),
        season: None,
        episode: None,
        poster: Some("https://example.com/dialog.jpg".into()),
    });
    assert_eq!(visible_artwork(&s), vec!["https://example.com/dialog.jpg"]);
    let image = rune::Pixels::new();
    image.set(2, 3, vec![180; 24]);
    s.artwork.posters = Some(crate::artwork::Artwork::player_fixture(&[(
        "https://example.com/dialog.jpg",
        image,
    )]));
    let mut rt = rune::Runtime::new(s, view, (1280.0, 820.0), rune::Env::default());
    rt.frame(0.0);
    assert!(rt.find("calendar-dialog-poster-0-image").is_some());
    let poster = rect(&rt, "calendar-dialog-poster-0");
    assert!(poster[2] >= 48.0 && poster[3] >= 72.0);
}

#[test]
fn month_calendar_scrolls_a_busy_day_without_moving_its_neighbour() {
    let mut s = init();
    s.navigation.page = Page::Calendar;
    s.addons.busy = false;
    s.calendar.anchor = chrono::NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
    let date = s.calendar.anchor;
    for i in 0..12 {
        s.calendar.releases.push(Release {
            date,
            media: Media {
                key: MediaKey::new("imdb", MediaKind::Series, format!("show{i}")),
                title: format!("Series {i}"),
                parent: None,
            },
            episode_id: format!("ep{i}"),
            title: format!("Episode {i}"),
            season: Some(1),
            episode: Some(1),
            poster: None,
        });
    }
    let mut rt = rune::Runtime::new(s, view, (1280.0, 820.0), rune::Env::default());
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    assert_eq!(
        rt.elements()
            .iter()
            .filter(|e| !e.exiting
                && e.key
                    .as_deref()
                    .is_some_and(|key| key.starts_with("calendar-day-")
                        && key.len() == "calendar-day-2026-10-19".len()))
            .count(),
        31
    );
    let day = rect(&rt, "calendar-day-scroll-2026-10-19");
    let neighbour = rect(&rt, "calendar-day-2026-10-20");
    let before = rect(&rt, "calendar-episode-2026-10-19-3");
    rt.wheel(day[0] + day[2] / 2.0, day[1] + day[3] / 2.0, -150.0);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rect(&rt, "calendar-episode-2026-10-19-3")[1] < before[1] - 80.0);
    assert_eq!(rect(&rt, "calendar-day-2026-10-20"), neighbour);
}

#[test]
fn startup_opens_watching_and_addon_install_uses_a_dialog() {
    let mut s = init();
    s.addons.busy = false;
    let mut rt = rune::Runtime::new(s, view, (1280.0, 820.0), rune::Env::default());
    rt.frame(0.0);
    assert_eq!(rt.state.navigation.page, Page::Watching);
    tap(&mut rt, "nav-library");
    tap(&mut rt, "nav-addons");
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    tap(&mut rt, "add-addon");
    assert!(
        rt.state.addons.install_open,
        "{:?}",
        rt.elements()
            .iter()
            .filter(|e| e.key.as_deref().is_some_and(|k| k.contains("addon")))
            .map(|e| (&e.key, e.rect))
            .collect::<Vec<_>>()
    );
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rt.find("manifest").is_some());
    assert_eq!(
        rect(&rt, "manifest-frame")[3],
        super::super::design::tokens::control::DIALOG_HEIGHT
    );
    assert!(rt.state.addons.install.text.is_empty());
    tap(&mut rt, "addon-dialog-close");
}

#[test]
fn expanded_day_scrolls_all_releases_and_opens_movies() {
    let mut s = init();
    s.navigation.page = Page::Calendar;
    let date = s.calendar.anchor;
    for i in 0..20 {
        s.calendar.releases.push(Release {
            date,
            media: Media {
                key: MediaKey::new("test", MediaKind::Movie, format!("movie{i}")),
                title: format!("Movie {i}"),
                parent: None,
            },
            episode_id: format!("movie{i}"),
            title: String::new(),
            season: None,
            episode: None,
            poster: None,
        });
    }
    let mut rt = rune::Runtime::new(s, view, (1280.0, 820.0), rune::Env::default());
    rt.frame(0.0);
    tap(&mut rt, &format!("calendar-expand-{date}"));
    assert!(rt.find("calendar-day-dialog").is_some());
    let before = rect(&rt, "calendar-dialog-release-10");
    let list = rect(&rt, "calendar-dialog-scroll");
    rt.wheel(list[0] + 50.0, list[1] + 50.0, -400.0);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rect(&rt, "calendar-dialog-release-10")[1] < before[1] - 100.0);
    tap(&mut rt, "calendar-dialog-close");
    tap(&mut rt, &format!("calendar-expand-{date}"));
    let list = rect(&rt, "calendar-dialog-scroll");
    rt.wheel(list[0] + 50.0, list[1] + 50.0, 2000.0);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    tap(&mut rt, "calendar-dialog-release-0");
    assert_eq!(rt.state.navigation.page, Page::Detail);
    assert_eq!(rt.state.detail.meta.as_ref().unwrap().kind, "movie");
    assert!(rt.state.detail.pending_episode.is_none());
}

#[test]
fn right_click_actions_persist_and_page_refresh_is_available() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.sqlite3");
    let mut s = init();
    scene_discover(&mut s);
    s.library.worker =
        Some(crate::library_worker::Worker::start(path.clone(), Arc::new(|| {})).unwrap());
    let mut rt = rune::Runtime::new(s, view, (1280.0, 820.0), rune::Env::default());
    rt.poll = Some(poll);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let r = rect(&rt, "title-0");
    rt.secondary_down(r[0] + 10.0, r[1] + 10.0);
    rt.secondary_up(r[0] + 10.0, r[1] + 10.0);
    rt.frame(1.0 / 60.0);
    assert!(
        rt.state
            .shell
            .context_menu
            .as_ref()
            .is_some_and(|(_, item)| item.is_some())
    );
    for _ in 0..10 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rt.find("context-watched").is_some());
    tap(&mut rt, "context-later");
    settle_library(&mut rt, |s| {
        s.library
            .entries
            .as_ref()
            .is_some_and(|data| !data.is_empty())
    });
    let db = scry_library::Library::open(&path).unwrap();
    let saved = db.list_watchlist(scry_library::Page::default()).unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].title, "The Grand Budapest Hotel");
    rt.secondary_down(1250.0, 790.0);
    rt.secondary_up(1250.0, 790.0);
    rt.frame(1.0 / 60.0);
    assert!(rt.find("context-refresh").is_some());
}

#[test]
fn episode_context_menu_uses_its_parent_canonical_namespace() {
    let mut s = init();
    s.navigation.page = Page::Detail;
    s.detail.namespace = "provider".into();
    s.detail.meta=Some(serde_json::from_value(serde_json::json!({"id":"tt123","type":"series","name":"Show","videos":[{"id":"custom-episode","season":1,"episode":1,"title":"Pilot"}]})).unwrap());
    let expected = s.visible_episode_keys()[0].clone();
    let mut rt = rune::Runtime::new(s, view, (1280.0, 820.0), rune::Env::default());
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let r = rect(&rt, "episode-0");
    rt.secondary_down(r[0] + 10.0, r[1] + 10.0);
    rt.secondary_up(r[0] + 10.0, r[1] + 10.0);
    rt.frame(1.0 / 60.0);
    let super::super::context_menu::Target::Title(item) = rt
        .state
        .shell
        .context_menu
        .as_ref()
        .unwrap()
        .1
        .as_ref()
        .unwrap()
    else {
        panic!("Expected a title menu")
    };
    assert_eq!(item.key, expected);
    assert_eq!(item.key.namespace, "imdb");
}
