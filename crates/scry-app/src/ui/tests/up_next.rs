use super::*;
use scry_library::{Library, Media, MediaKey, MediaKind, Progress};

fn media(kind: MediaKind, id: &str, parent: Option<MediaKey>) -> Media {
    Media {
        key: MediaKey::new("test", kind, id),
        title: id.into(),
        parent,
    }
}

fn settle_grid(rt: &mut rune::Runtime<State>) {
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
}

#[test]
fn quick_play_buttons_choose_the_first_matching_saved_video() {
    let show = MediaKey::new("test", MediaKind::Series, "show");
    for (button, expected, episode) in [
        ("play-next-movie", "movie", None),
        ("play-next-show", "show", Some("show:1:2")),
    ] {
        let mut s = init();
        s.apply_up_next_entries(vec![
            (
                media(MediaKind::Episode, "show:1:2", Some(show.clone())),
                None,
            ),
            (media(MediaKind::Movie, "movie", None), None),
            (media(MediaKind::Movie, "later-movie", None), None),
        ]);
        let mut rt = rune::Runtime::new(s, view, (1000.0, 820.0), rune::Env::default());
        rt.frame(0.0);
        let header = rect(&rt, "up-next-heading");
        let action = rect(&rt, button);
        assert!(action[0] > header[0]);
        assert!(action[1] >= header[1] && action[1] + action[3] <= header[1] + header[3]);
        tap(&mut rt, button);
        assert_eq!(rt.state.detail.as_ref().unwrap().id, expected);
        assert_eq!(rt.state.pending_episode.as_deref(), episode);
        assert_eq!(rt.state.library.quick_play, Some(rt.state.detail_id));
    }
}

#[test]
fn quick_play_buttons_are_disabled_without_matching_items() {
    for (kind, parent, key) in [
        (MediaKind::Movie, None, "play-next-show"),
        (
            MediaKind::Episode,
            Some(MediaKey::new("test", MediaKind::Series, "show")),
            "play-next-movie",
        ),
    ] {
        let mut s = init();
        s.apply_up_next_entries(vec![(media(kind, "saved", parent), None)]);
        let mut rt = rune::Runtime::new(s, view, (1000.0, 820.0), rune::Env::default());
        rt.frame(0.0);
        assert!(
            !rt.elements()
                .iter()
                .find(|e| e.key.as_deref() == Some(key))
                .unwrap()
                .focusable
        );
        tap(&mut rt, key);
        assert_eq!(rt.state.navigation.page, Page::Watching);
    }
}

#[test]
fn up_next_entries_appear_with_or_without_currently_watching_items() {
    for watching in [false, true] {
        for loaded in [false, true] {
            let mut s = init();
            s.library.up_next_loaded = loaded;
            if watching {
                s.library.watching = vec![(
                    media(MediaKind::Movie, "in-progress", None),
                    Progress {
                        position_seconds: 25.0,
                        duration_seconds: 100.0,
                    },
                )];
            }
            let mut rt = rune::Runtime::new(s, view, (1000.0, 820.0), rune::Env::default());
            rt.frame(0.0);
            if watching {
                assert!(rt.find("watching-open-0").is_some());
            } else {
                assert!(rt.find("watching-empty").is_some());
            }
            rt.update(|s, _| {
                s.apply_up_next_entries(vec![(media(MediaKind::Movie, "next", None), None)]);
            });
            settle_grid(&mut rt);
            assert!(rt.find("up-next-heading").is_some());
            assert!(rt.find("up-next-open-0").is_some());
        }
    }
}

#[test]
fn quick_play_starts_a_single_source_but_keeps_multiple_sources_in_the_picker() {
    for count in [1, 2] {
        let (mut s, tx) = state_with_events();
        s.navigation.page = Page::Detail;
        s.detail = Some(
            serde_json::from_value(
                serde_json::json!({"id":"movie", "type":"movie", "name":"Movie"}),
            )
            .unwrap(),
        );
        s.detail_namespace = "test".into();
        s.selected_video_id = Some("movie".into());
        s.library.quick_play = Some(s.detail_id);
        let stream: Stream =
            serde_json::from_value(serde_json::json!({"url":"https://example.com/video.mp4"}))
                .unwrap();
        tx.try_send(Event::Streams(
            s.stream_id,
            vec![("Provider".into(), stream); count],
            vec![],
            true,
        ))
        .unwrap();
        poll(&mut s, 0.0, &mut Tx::default());
        assert!(s.library.quick_play.is_none());
        if count == 1 {
            assert_eq!(s.library.playing.as_ref().unwrap().key.id, "movie");
        } else {
            assert!(s.library.playing.is_none());
            assert_eq!(s.navigation.page, Page::Detail);
            assert_eq!(s.streams.len(), 2);
        }
    }
}

#[test]
fn next_cards_use_each_items_progress_and_resume_the_exact_episode() {
    let show = MediaKey::new("test", MediaKind::Series, "show");
    let first = media(MediaKind::Episode, "show:1:1", Some(show.clone()));
    let second = media(MediaKind::Episode, "show:1:2", Some(show.clone()));
    let movie = media(MediaKind::Movie, "movie", None);
    let mut s = init();
    s.library.watching = vec![(
        second.clone(),
        Progress {
            position_seconds: 75.0,
            duration_seconds: 100.0,
        },
    )];
    s.apply_up_next_entries(vec![
        (
            first,
            Some(Progress {
                position_seconds: 25.0,
                duration_seconds: 100.0,
            }),
        ),
        (
            second,
            Some(Progress {
                position_seconds: 75.0,
                duration_seconds: 100.0,
            }),
        ),
        (
            movie,
            Some(Progress {
                position_seconds: 20.0,
                duration_seconds: 200.0,
            }),
        ),
    ]);
    s.library.posters.cache.insert(
        show,
        Some(crate::services::LibraryPoster {
            poster: Some("https://example.com/show.jpg".into()),
            year: "2026".into(),
        }),
    );
    let mut rt = rune::Runtime::new(s, view, (1000.0, 820.0), rune::Env::default());
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let current = rect(&rt, "watching-poster-0-progress-track");
    let current_fill = rect(&rt, "watching-poster-0-progress-fill");
    assert!((current_fill[2] / current[2] - 0.75).abs() < 0.01);
    for (index, fraction) in [0.25, 0.75, 0.1].into_iter().enumerate() {
        let track = rect(&rt, &format!("up-next-poster-{index}-progress-track"));
        let fill = rect(&rt, &format!("up-next-poster-{index}-progress-fill"));
        assert!((fill[2] / track[2] - fraction).abs() < 0.01);
    }
    let current_card = rect(&rt, "watching-open-0");
    assert!(rect(&rt, "up-next-heading")[1] >= current_card[1] + current_card[3]);
    assert!(visible_artwork(&rt.state).contains(&"https://example.com/show.jpg".into()));
    tap(&mut rt, "up-next-open-0");
    assert_eq!(rt.state.detail.as_ref().unwrap().id, "show");
    assert_eq!(rt.state.pending_episode.as_deref(), Some("show:1:1"));
}

#[test]
fn next_pagination_reaches_all_saved_titles_and_updates_after_marking_watched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite3");
    let movies: Vec<_> = (0..201)
        .map(|index| media(MediaKind::Movie, &format!("movie-{index}"), None))
        .collect();
    {
        let mut db = Library::open(&path).unwrap();
        for movie in &movies {
            db.upsert_media(movie).unwrap();
            db.add_to_watchlist(&movie.key).unwrap();
        }
    }
    let mut s = init();
    s.library.worker = Some(crate::library_worker::Worker::start(path, Arc::new(|| {})).unwrap());
    let mut rt = rune::Runtime::new(s, view, (880.0, 600.0), rune::Env::default());
    rt.poll = Some(poll);
    settle_library(&mut rt, |s| {
        s.library.up_next_loaded && s.library.up_next.len() == 200
    });
    assert!(rt.state.library.up_next_has_more);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let mounted = rt
        .elements()
        .iter()
        .filter(|element| {
            !element.exiting
                && element
                    .key
                    .as_deref()
                    .is_some_and(|key| key.starts_with("up-next-open-"))
        })
        .count();
    assert!(
        mounted < 30,
        "Distant next cards should stay virtualized: {mounted}"
    );
    let viewport = rect(&rt, "watching-scroll");
    rt.wheel(viewport[0] + 20.0, viewport[1] + 20.0, -100_000.0);
    for _ in 0..120 {
        rt.frame(1.0 / 60.0);
    }
    tap(&mut rt, "up-next-next");
    settle_library(&mut rt, |s| {
        s.library.up_next_offset == 200 && s.library.up_next.len() == 1
    });
    assert_eq!(rt.state.library.up_next[0].0.key, movies[0].key);
    assert!(!rt.state.library.up_next_has_more);
    rt.update(|s, _| {
        s.db(
            crate::library_worker::Command::SetWatched(movies[0].clone(), true),
            true,
        );
    });
    settle_library(&mut rt, |s| {
        s.library.up_next_offset == 0 && s.library.up_next.len() == 200
    });
    assert!(!rt.state.library.up_next_has_more);
    assert!(
        rt.state
            .library
            .up_next
            .iter()
            .all(|(item, _)| item.key != movies[0].key)
    );
}

#[test]
fn rapid_home_scrolling_keeps_layout_bounded_and_restores_visible_artwork() {
    let mut s = init();
    let items: Vec<_> = (0..200)
        .map(|index| {
            (
                media(MediaKind::Movie, &format!("movie-{index}"), None),
                None,
            )
        })
        .collect();
    s.apply_up_next_entries(items.clone());
    for (index, (item, _)) in items.iter().enumerate() {
        s.library.posters.cache.insert(
            item.key.clone(),
            Some(crate::services::LibraryPoster {
                poster: Some(format!("https://example.invalid/{index}.jpg")),
                year: String::new(),
            }),
        );
    }
    let pixels = rune::Pixels::new();
    pixels.set(2, 3, vec![180; 24]);
    s.artwork = Some(crate::artwork::Artwork::player_fixture(&[
        ("https://example.invalid/0.jpg", pixels.clone()),
        ("https://example.invalid/199.jpg", pixels),
    ]));
    let mut rt = rune::Runtime::new(s, view, (880.0, 600.0), rune::Env::default());
    rt.poll = Some(poll);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let viewport = rect(&rt, "watching-scroll");
    // No settling between inputs: exercise work replacement during a fast fling.
    for _ in 0..80 {
        rt.wheel(viewport[0] + 20.0, viewport[1] + 20.0, -650.0);
        rt.frame(1.0 / 60.0);
    }
    for direction in [-1.0, 1.0].into_iter().cycle().take(12) {
        rt.wheel(
            viewport[0] + 20.0,
            viewport[1] + 20.0,
            direction * 100_000.0,
        );
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        let rows = rt
            .elements()
            .iter()
            .filter(|el| {
                !el.exiting
                    && el
                        .key
                        .as_deref()
                        .is_some_and(|key| key.starts_with("up-next-row-"))
            })
            .count();
        assert!(
            rows < 16,
            "Offscreen rows must have no layout nodes: {rows}"
        );
    }
    rt.wheel(viewport[0] + 20.0, viewport[1] + 20.0, -100_000.0);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    assert!(
        rt.state
            .artwork_urls
            .contains(&"https://example.invalid/199.jpg".into())
    );
    assert!(
        !rt.state
            .artwork_urls
            .contains(&"https://example.invalid/0.jpg".into())
    );
    rt.wheel(viewport[0] + 20.0, viewport[1] + 20.0, 100_000.0);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rt.find("up-next-open-0").is_some());
    assert!(
        rt.state
            .artwork_urls
            .contains(&"https://example.invalid/0.jpg".into())
    );
    rt.wheel(viewport[0] + 20.0, viewport[1] + 20.0, -100_000.0);
    settle_grid(&mut rt);
    // A same-sized database refresh must not leave the viewport/geometry empty.
    rt.update(|s, _| s.apply_up_next_entries(s.library.up_next.clone()));
    settle_grid(&mut rt);
    assert!(
        rt.state
            .artwork_urls
            .contains(&"https://example.invalid/199.jpg".into())
    );
    tap(&mut rt, "up-next-open-199");
    assert_eq!(rt.state.detail.as_ref().unwrap().id, "movie-199");
}

#[test]
fn library_pagination_does_not_page_currently_watching() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite3");
    let current = media(MediaKind::Movie, "current", None);
    {
        let mut db = Library::open(&path).unwrap();
        for index in 0..201 {
            let movie = media(MediaKind::Movie, &format!("movie-{index}"), None);
            db.upsert_media(&movie).unwrap();
            db.add_to_watchlist(&movie.key).unwrap();
        }
        db.upsert_media(&current).unwrap();
        db.record_playback(&current.key, 10.0, 100.0, 10.0).unwrap();
    }
    let mut s = init();
    s.library.worker = Some(crate::library_worker::Worker::start(path, Arc::new(|| {})).unwrap());
    let mut rt = rune::Runtime::new(s, view, (880.0, 600.0), rune::Env::default());
    rt.poll = Some(poll);
    settle_library(&mut rt, |s| s.library.watching.len() == 1);
    tap(&mut rt, "nav-library");
    settle_library(&mut rt, |s| {
        s.library
            .entries
            .as_ref()
            .is_some_and(|items| items.len() == 200)
    });
    tap(&mut rt, "library-next");
    settle_library(&mut rt, |s| {
        s.library
            .entries
            .as_ref()
            .is_some_and(|items| items.len() == 1)
    });
    tap(&mut rt, "nav-watching");
    settle_library(&mut rt, |s| s.library.watching.len() == 1);
    assert_eq!(rt.state.library.watching[0].0.key, current.key);
    assert!(rt.find("watching-open-0").is_some());
}

#[test]
fn currently_watching_episode_badge_does_not_use_the_whole_shows_status() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite3");
    let show = media(MediaKind::Series, "show", None);
    let episode = media(MediaKind::Episode, "episode", Some(show.key.clone()));
    {
        let mut db = Library::open(&path).unwrap();
        db.upsert_media_batch(&[show.clone(), episode.clone()])
            .unwrap();
        db.register_release(&episode, Some(0)).unwrap();
        db.record_playback(&episode.key, 80.0, 100.0, 80.0).unwrap();
        db.sync_episode_releases(&show.key, &[]).unwrap();
        assert!(!db.watched_state(&show.key).unwrap().watched);
    }
    let mut s = init();
    s.library.worker = Some(crate::library_worker::Worker::start(path, Arc::new(|| {})).unwrap());
    let mut rt = rune::Runtime::new(s, view, (880.0, 600.0), rune::Env::default());
    rt.poll = Some(poll);
    settle_library(&mut rt, |s| {
        s.library.watching_watched.contains(&episode.key)
    });
    assert!(rt.find("watching-poster-0-watched").is_some());
}

#[test]
fn series_metadata_supplies_saved_poster_artwork_without_another_lookup() {
    let mut s = init();
    let show = MediaKey::new("test", MediaKind::Series, "show");
    let episode = media(MediaKind::Episode, "episode", Some(show.clone()));
    s.apply_up_next_entries(vec![(episode.clone(), None)]);
    let meta = serde_json::from_value(serde_json::json!({
        "id":"show", "type":"series", "name":"Show", "year":"2026",
        "poster":"https://example.invalid/show.jpg", "videos":[
            {"id":"episode", "season":1, "episode":1, "released":"2000-01-01"}
        ]
    }))
    .unwrap();
    s.apply_series_details(0, vec![(show, Some(meta))], true);
    assert_eq!(
        s.library_poster(&episode).unwrap().poster.as_deref(),
        Some("https://example.invalid/show.jpg")
    );
    assert!(s.missing_library_posters().is_empty());
    assert!(
        s.artwork_urls
            .contains(&"https://example.invalid/show.jpg".into())
    );
}
