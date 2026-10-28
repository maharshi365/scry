use super::*;
use crate::library_worker::{SelectedState, Snapshot};
use chrono::Datelike;
use scry_library::WatchedState;

#[test]
fn watching_empty_state_has_catalog_actions() {
    let mut rt = rune::Runtime::new(init(), view, (1000.0, 800.0), rune::Env::default());
    rt.frame(0.0);
    assert!(rt.find("watching-empty").is_some());
    tap(&mut rt, "watching-browse-movies");
    assert_eq!(rt.state.navigation.page, Page::Discover);
    assert_eq!(rt.state.kind, "movie");
}

#[test]
fn library_empty_states_keep_filters_and_browse_the_selected_type() {
    for watched in [false, true] {
        let mut s = init();
        s.navigation.page = Page::Library;
        s.library.entries = Some(vec![]);
        let mut rt = rune::Runtime::new(s, view, (1000.0, 800.0), rune::Env::default());
        rt.frame(0.0);
        if watched {
            tap(&mut rt, "library-watched-tab");
        }
        tap(&mut rt, "library-series-filter");
        rt.update(|s, _| s.apply_library_entries(vec![]));
        rt.frame(1.0 / 60.0);
        let list_tab = rect(&rt, "library-watchlist-tab");
        let kind_tab = rect(&rt, "library-series-filter");
        assert!(
            (list_tab[1] - kind_tab[1]).abs() < 1.0,
            "Library filters share one toolbar"
        );
        assert!(list_tab[3] >= 36.0);
        assert!(rt.find("library-empty").is_some());
        let empty = rect(&rt, "library-empty");
        let filters = rect(&rt, "library-series-filter");
        assert!(
            empty[1] >= filters[1] + filters[3],
            "empty content must stay below the filters"
        );
        assert!(empty[1] < filters[1] + filters[3] + 40.0);
        assert!(
            empty[3] < 350.0,
            "empty state should not stretch across the screen"
        );
        assert!(rt.elements().iter().any(|e| e.text.as_deref()
            == Some(if watched {
                "No watched TV series"
            } else {
                "No TV series saved"
            })));
        tap(&mut rt, "library-browse");
        assert_eq!(rt.state.navigation.page, Page::Discover);
        assert_eq!(rt.state.kind, "series");
    }
}

#[test]
fn month_columns_align_across_rows_at_small_window_sizes() {
    use crate::ui::calendar::{Release, dates};
    for width in [880.0, 1000.0, 1280.0] {
        let mut s = init();
        s.navigation.page = Page::Calendar;
        let date = s.calendar.anchor;
        let days = dates(date);
        s.calendar.releases.push(Release {
            date,
            media: scry_library::Media {
                key: scry_library::MediaKey::new("test", scry_library::MediaKind::Series, "show"),
                title: "A very long series title that must not widen its calendar column".into(),
                parent: None,
            },
            episode_id: "e1".into(),
            title: "An equally long episode title".into(),
            season: Some(1),
            episode: Some(1),
            poster: None,
        });
        let mut rt = rune::Runtime::new(s, view, (width, 650.0), rune::Env::default());
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        let first = rect(&rt, "calendar-slot-0");
        for (i, date) in days.iter().enumerate() {
            if date.month() != rt.state.calendar.anchor.month() {
                continue;
            }
            let r = rect(&rt, &format!("calendar-day-{date}"));
            let col = rect(&rt, &format!("calendar-slot-{}", i % 7));
            assert!(
                (r[0] - col[0]).abs() < 0.5,
                "column alignment at width {width}"
            );
            assert!(
                (r[2] - first[2]).abs() < 0.5,
                "column width at {width}: {} vs {}",
                r[2],
                first[2]
            );
            assert!(
                r[0] + r[2] <= width - 19.0,
                "day overflows the content at {width}"
            );
        }
    }
}

#[test]
fn calendar_has_date_navigation_for_large_libraries() {
    let mut s = init();
    s.navigation.page = Page::Calendar;
    s.calendar.series = (0..20)
        .map(|i| scry_library::Media {
            key: scry_library::MediaKey::new(
                "test",
                scry_library::MediaKind::Series,
                i.to_string(),
            ),
            title: format!("Show {i}"),
            parent: None,
        })
        .collect();
    let mut rt = rune::Runtime::new(s, view, (880.0, 650.0), rune::Env::default());
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rt.find("calendar-prev").is_some());
    assert!(rt.find("calendar-next").is_some());
}

#[test]
fn episode_names_and_watched_badge_are_shown() {
    let mut s = init();
    scene_detail(&mut s);
    s.detail.as_mut().unwrap().videos[0].title = "First Period".into();
    let media = s.library_media().unwrap();
    s.library.snapshot = Some(Snapshot {
        selected_state: Some(SelectedState {
            key: media.key.clone(),
            watched: WatchedState { watched: true },
            watchlist: false,
            progress: None,
        }),
        episode_states: vec![(media.key, WatchedState { watched: true })],
    });
    let mut rt = rune::Runtime::new(s, view, (1280.0, 820.0), rune::Env::default());
    rt.frame(0.0);
    assert!(rt.elements().iter().any(|e| {
        e.key.as_deref() == Some("source-episode")
            && e.text
                .as_deref()
                .is_some_and(|text| text.contains("First Period"))
    }));
    tap(&mut rt, "detail-back");
    assert!(rt.find("episode-watched-0").is_some());
    let elements = rt.elements();
    assert!(
        elements
            .iter()
            .any(|e| e.text.as_deref() == Some("First Period"))
    );
}

#[test]
fn compact_source_header_keeps_controls_and_episode_on_two_rows() {
    let mut s = init();
    scene_detail(&mut s);
    s.detail = Some(multi_season_series());
    s.selected_video_id = Some("show:1:1".into());
    let mut rt = rune::Runtime::new(s, view, (880.0, 650.0), rune::Env::default());
    rt.frame(0.0);
    let season = rect(&rt, "season-select");
    let sort = rect(&rt, "source-sort");
    let title = rect(&rt, "source-episode");
    let episode = rect(&rt, "source-episode-number");
    assert!((season[1] - sort[1]).abs() < 1.0);
    assert!(title[1] >= season[1] + season[3]);
    assert!(episode[0] >= title[0] + title[2]);
    assert!(episode[1] < title[1] + title[3]);
    tap(&mut rt, "source-sort");
    tap(&mut rt, "source-original");
    assert_eq!(
        rt.state.stream_sort,
        crate::ui::source_picker::SourceSort::Original
    );
    tap(&mut rt, "season-select");
    tap(&mut rt, "season-option-1");
    assert_eq!(rt.state.active_season(), Some(2));
    assert!(rt.state.selected_video_id.is_none());
    assert!(rt.find("episodes").is_some());
    assert!(rt.state.streams.is_empty());
}

#[test]
fn calendar_retains_provider_poster_and_renders_cached_image_in_month_view() {
    let mut s = init();
    s.navigation.page = Page::Calendar;
    let date = s.calendar.anchor;
    let media = scry_library::Media {
        key: scry_library::MediaKey::new("test", scry_library::MediaKind::Series, "show"),
        title: "A show".into(),
        parent: None,
    };
    s.calendar.series = vec![media.clone()];
    let meta:Meta=serde_json::from_value(serde_json::json!({
        "id":"show","type":"series","name":"A show","poster":"https://example.com/poster.jpg",
        "videos":[{"id":"show:1:1","name":"Pilot","season":1,"episode":1,"released":date.to_string()}]
    })).unwrap();
    s.apply_calendar(0, vec![(media, meta)], 0);
    assert_eq!(
        s.calendar.releases[0].poster.as_deref(),
        Some("https://example.com/poster.jpg")
    );
    let image = rune::Pixels::new();
    image.set(2, 3, vec![180; 24]);
    s.artwork = Some(crate::artwork::Artwork::player_fixture(&[(
        "https://example.com/poster.jpg",
        image,
    )]));
    let mut rt = rune::Runtime::new(s, view, (1280.0, 820.0), rune::Env::default());
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    assert!(
        rt.find(&format!("calendar-poster-{date}-0-image"))
            .is_some()
    );
    let r = rect(&rt, &format!("calendar-poster-{date}-0"));
    assert!(r[2] > 20.0 && r[3] > 35.0);
    assert!(
        rt.state
            .artwork_urls
            .iter()
            .any(|url| url == "https://example.com/poster.jpg")
    );
}

#[test]
fn calendar_only_queues_posters_visible_inside_the_day_and_calendar() {
    let mut s = init();
    s.navigation.page = Page::Calendar;
    let date = s.calendar.anchor;
    let media = scry_library::Media {
        key: scry_library::MediaKey::new("test", scry_library::MediaKind::Series, "show"),
        title: "Show".into(),
        parent: None,
    };
    for (i, url) in [
        "https://example.com/visible.jpg",
        "https://example.com/clipped.jpg",
    ]
    .into_iter()
    .enumerate()
    {
        s.calendar.releases.push(crate::ui::calendar::Release {
            date,
            media: media.clone(),
            episode_id: format!("e{i}"),
            title: "Pilot".into(),
            season: Some(1),
            episode: Some(1),
            poster: Some(url.into()),
        });
        s.calendar.artwork_rects.insert(
            format!("calendar-episode-{date}-{i}"),
            [0.0, if i == 0 { 0.0 } else { 120.0 }, 100.0, 60.0],
        );
    }
    s.calendar.viewport = Some([0.0, 0.0, 200.0, 100.0]);
    s.calendar
        .day_viewports
        .insert(date, [0.0, 0.0, 100.0, 100.0]);
    assert_eq!(visible_artwork(&s), vec!["https://example.com/visible.jpg"]);
    s.calendar.viewport = Some([0.0, 200.0, 200.0, 100.0]);
    assert!(visible_artwork(&s).is_empty());
    s.calendar.viewport = Some([0.0, 0.0, 200.0, 100.0]);
    s.calendar
        .day_viewports
        .insert(date, [120.0, 0.0, 100.0, 100.0]);
    assert!(visible_artwork(&s).is_empty());
}
