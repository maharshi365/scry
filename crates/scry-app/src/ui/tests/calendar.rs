use super::*;
use crate::ui::calendar::{Mode, Release};
use scry_library::{Media, MediaKey, MediaKind};

#[test]
fn calendar_switches_modes_and_scrolls_a_busy_day_without_moving_its_neighbour() {
    let mut s = init();
    s.navigation.page = Page::Calendar;
    s.config_busy = false;
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
    rt.frame(0.0);
    let day = rect(&rt, "calendar-day-scroll-2026-10-19");
    let neighbour = rect(&rt, "calendar-day-2026-10-20");
    let before = rect(&rt, "calendar-episode-2026-10-19-3");
    rt.wheel(day[0] + day[2] / 2.0, day[1] + 40.0, -150.0);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rect(&rt, "calendar-episode-2026-10-19-3")[1] < before[1] - 80.0);
    assert_eq!(rect(&rt, "calendar-day-2026-10-20"), neighbour);
    tap(&mut rt, "calendar-month");
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    assert_eq!(rt.state.calendar.mode, Mode::Month);
    assert_eq!(
        rt.elements()
            .iter()
            .filter(|e| !e.exiting
                && e.key
                    .as_deref()
                    .is_some_and(|key| key.starts_with("calendar-day-")
                        && key.len() == "calendar-day-2026-10-19".len()))
            .count(),
        42
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
    tap(&mut rt, "calendar-week");
    assert_eq!(rt.state.calendar.mode, Mode::Week);
}

#[test]
fn startup_and_addon_install_have_no_discover_or_playlist_controls() {
    let mut rt = rune::Runtime::new(init(), view, (1280.0, 820.0), rune::Env::default());
    rt.frame(0.0);
    assert_eq!(rt.state.navigation.page, Page::Watching);
    assert!(rt.find("nav-discover").is_none());
    tap(&mut rt, "nav-library");
    assert!(rt.find("playlist-create").is_none());
    tap(&mut rt, "nav-addons");
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rt.find("manifest").is_none());
    tap(&mut rt, "supported-addon-0");
    assert!(
        rt.state.addon_install_open,
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
    assert!(rt.state.install.text.contains("cinemeta"));
    tap(&mut rt, "addon-dialog-close");
    assert!(rt.find("manifest").is_none());
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
    s.detail_namespace = "provider".into();
    s.detail=Some(serde_json::from_value(serde_json::json!({"id":"tt123","type":"series","name":"Show","videos":[{"id":"custom-episode","season":1,"episode":1,"title":"Pilot"}]})).unwrap());
    let expected = s.visible_episode_keys()[0].clone();
    let mut rt = rune::Runtime::new(s, view, (1280.0, 820.0), rune::Env::default());
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let r = rect(&rt, "episode-0");
    rt.secondary_down(r[0] + 10.0, r[1] + 10.0);
    rt.secondary_up(r[0] + 10.0, r[1] + 10.0);
    rt.frame(1.0 / 60.0);
    let item = rt.state.context_menu.as_ref().unwrap().1.as_ref().unwrap();
    assert_eq!(item.key, expected);
    assert_eq!(item.key.namespace, "imdb");
}
