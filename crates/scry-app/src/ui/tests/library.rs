use super::*;

#[test]
fn movie_detail_overlay_handles_missing_duration_and_watched_state() {
    use crate::library_worker::{SelectedState, Snapshot};
    use scry_library::{Progress, WatchedState};
    for (position, duration, watched, expected) in [
        (30.0, 120.0, false, Some(0.25)),
        (150.0, 120.0, false, Some(1.0)),
        (0.0, 120.0, false, None),
        (30.0, 0.0, false, None),
        (30.0, 120.0, true, None),
    ] {
        let mut s = init();
        s.navigation.page = Page::Detail;
        s.detail = Some(serde_json::from_value(serde_json::json!({
            "id":"tt123", "type":"movie", "name":"Film", "poster":"https://example.com/poster.jpg"
        })).unwrap());
        s.library.snapshot = Some(Snapshot {
            selected_state: Some(SelectedState {
                key: s.library_media().unwrap().key,
                watched: WatchedState { watched },
                watchlist: false,
                progress: Some(Progress {
                    position_seconds: position,
                    duration_seconds: duration,
                }),
            }),
            episode_states: vec![],
        });
        let mut rt = rune::Runtime::new(s, view, (880.0, 600.0), rune::Env::default());
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        assert_eq!(rt.find("detail-poster-watched").is_some(), watched);
        assert_eq!(
            rt.find("detail-poster-progress-track").is_some(),
            expected.is_some()
        );
        if let Some(fraction) = expected {
            let poster = rect(&rt, "detail-poster");
            let track = rect(&rt, "detail-poster-progress-track");
            let fill = rect(&rt, "detail-poster-progress-fill");
            assert!((fill[2] / track[2] - fraction).abs() < 0.01);
            assert!((track[1] + track[3] / 2.0 - (poster[1] + poster[3] * 7.0 / 8.0)).abs() < 1.0);
        }
    }
}

fn seed_movie_overlays(worker: &crate::library_worker::Worker, movies: &[scry_library::Media]) {
    use crate::library_worker::{Command, Event};
    worker
        .send(Command::SavePlayback(movies[0].clone(), 30.0, 120.0, 0.0))
        .expect("save partial movie");
    worker
        .send(Command::SavePlayback(movies[1].clone(), 60.0, 120.0, 0.0))
        .unwrap();
    worker
        .send(Command::SetWatched(movies[1].clone(), true))
        .unwrap();
    worker.send(Command::Flush(1)).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if matches!(worker.try_recv(), Ok(Event::Flushed(1))) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Fixture writes did not finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[test]
fn movie_overlays_follow_saved_state_in_catalog_search_library_and_watching() {
    use crate::library_worker::{Command, Worker};
    use scry_library::{Media, MediaKey, MediaKind};
    for page in [Page::Discover, Page::Library, Page::Watching] {
        for search in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let worker =
                Worker::start(dir.path().join("library.sqlite3"), Arc::new(|| {})).unwrap();
            let movies: Vec<_> = (0..3)
                .map(|i| Media {
                    key: MediaKey::new("imdb", MediaKind::Movie, format!("tt100{i}")),
                    title: format!("Movie {i}"),
                    parent: None,
                })
                .collect();
            seed_movie_overlays(&worker, &movies);
            let mut s = init();
            s.navigation.page = page;
            s.library.worker = Some(worker);
            s.library.entries = Some(movies.clone());
            s.library.watching = movies
                .iter()
                .map(|m| {
                    (
                        m.clone(),
                        scry_library::Progress {
                            position_seconds: 30.0,
                            duration_seconds: 120.0,
                        },
                    )
                })
                .collect();
            s.metas = Arc::new(
                movies
                    .iter()
                    .map(|m| {
                        serde_json::from_value(serde_json::json!({
                            "id":m.key.id, "type":"movie", "name":m.title
                        }))
                        .unwrap()
                    })
                    .collect(),
            );
            if search {
                s.displayed_query = "Movie".into();
            }
            let mut rt = rune::Runtime::new(s, view, (880.0, 600.0), rune::Env::default());
            rt.poll = Some(poll);
            settle_library(&mut rt, |s| s.library.movies.states.len() == 3);
            for _ in 0..30 {
                rt.frame(1.0 / 60.0);
            }
            let prefix = match page {
                Page::Library => "library",
                Page::Watching => "watching",
                _ => "title",
            };
            let track = rect(&rt, &format!("{prefix}-poster-0-progress-track"));
            let fill = rect(&rt, &format!("{prefix}-poster-0-progress-fill"));
            assert!((fill[2] / track[2] - 0.25).abs() < 0.01);
            let poster = rect(&rt, &format!("{prefix}-poster-1"));
            let badge = rect(&rt, &format!("{prefix}-poster-1-watched"));
            assert!(badge[0] > poster[0] + poster[2] / 2.0);
            assert!(badge[0] + badge[2] <= poster[0] + poster[2]);
            assert!(
                badge[1] >= poster[1] && badge[1] + badge[3] < poster[1] + poster[3] / 2.0,
                "badge {badge:?}, poster {poster:?}"
            );
            assert!(
                rt.find(&format!("{prefix}-poster-1-progress-track"))
                    .is_none()
            );
            assert!(
                rt.find(&format!("{prefix}-poster-2-progress-track"))
                    .is_none()
            );
            assert!(rt.find(&format!("{prefix}-poster-2-watched")).is_none());
            if page != Page::Discover {
                continue;
            }
            rt.update(|s, _| {
                s.db(Command::SetWatched(movies[0].clone(), true), true);
            });
            settle_library(&mut rt, |s| {
                s.library
                    .movies
                    .states
                    .iter()
                    .any(|m| m.key == movies[0].key && m.watched)
            });
            assert!(rt.find(&format!("{prefix}-poster-0-watched")).is_some());
            assert!(
                rt.find(&format!("{prefix}-poster-0-progress-track"))
                    .is_none()
            );
        }
    }
}

#[test]
fn watching_cards_show_posters_progress_and_resume_the_saved_episode() {
    use crate::services::LibraryPoster;
    use scry_library::{Media, MediaKey, MediaKind, Progress};
    let mut s = init();
    let movie = Media {
        key: MediaKey::new("test", MediaKind::Movie, "film"),
        title: "Film".into(),
        parent: None,
    };
    let show = MediaKey::new("test", MediaKind::Series, "show");
    let episode = Media {
        key: MediaKey::new("test", MediaKind::Episode, "show:1:2"),
        title: "Show · Chapter two".into(),
        parent: Some(show.clone()),
    };
    s.library.watching = vec![
        (
            movie.clone(),
            Progress {
                position_seconds: 60.0,
                duration_seconds: 120.0,
            },
        ),
        (
            episode.clone(),
            Progress {
                position_seconds: 0.0,
                duration_seconds: 120.0,
            },
        ),
    ];
    for (key, name) in [(movie.key, "movie"), (show, "series")] {
        s.library.posters.cache.insert(
            key,
            Some(LibraryPoster {
                poster: Some(format!("https://example.com/{name}.jpg")),
                year: "2026".into(),
            }),
        );
    }
    let mut rt = rune::Runtime::new(s, view, (880.0, 600.0), rune::Env::default());
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rt.find("watching-poster-0").is_some());
    assert!(rt.find("watching-poster-1").is_some());
    assert_eq!(
        rect(&rt, "watching-open-0")[3],
        super::super::artwork::POSTER_CARD_HEIGHT
    );
    assert_eq!(
        visible_artwork(&rt.state),
        vec![
            "https://example.com/movie.jpg",
            "https://example.com/series.jpg"
        ]
    );
    let poster = rect(&rt, "watching-poster-0");
    let track = rect(&rt, "watching-poster-0-progress-track");
    let fill = rect(&rt, "watching-poster-0-progress-fill");
    assert!(track[0] > poster[0] && track[0] + track[2] < poster[0] + poster[2]);
    assert!((track[1] + track[3] / 2.0 - (poster[1] + poster[3] * 7.0 / 8.0)).abs() < 1.0);
    assert!((fill[2] / track[2] - 0.5).abs() < 0.01);
    assert!(
        rt.elements()
            .iter()
            .all(|e| e.text.as_deref() != Some("1:00 / 2:00"))
    );
    assert!(rt.find("watching-poster-1-progress-track").is_none());
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.key.as_deref() == Some("watching-info-1")
                && e.text.as_deref() == Some("Next episode"))
    );
    let card = rect(&rt, "watching-open-1");
    rt.secondary_down(card[0] + 10.0, card[1] + 10.0);
    rt.secondary_up(card[0] + 10.0, card[1] + 10.0);
    rt.frame(1.0 / 60.0);
    let super::super::context_menu::Target::Title(item) =
        rt.state.context_menu.as_ref().unwrap().1.as_ref().unwrap()
    else {
        panic!("Expected a title menu")
    };
    assert_eq!(item, &episode);
    rt.update(|s, _| s.context_menu = None);
    rt.frame(1.0 / 60.0);
    tap(&mut rt, "watching-open-1");
    assert_eq!(rt.state.detail.as_ref().unwrap().id, "show");
    assert_eq!(rt.state.detail.as_ref().unwrap().kind, "series");
    assert_eq!(rt.state.pending_episode.as_deref(), Some("show:1:2"));
}

#[test]
fn watching_requests_parent_series_artwork_and_keeps_it_after_library_refreshes() {
    use crate::services::LibraryPoster;
    use scry_library::{Media, MediaKey, MediaKind, Progress};
    let mut s = init();
    let parent = MediaKey::new("test", MediaKind::Series, "show");
    s.library.watching = vec![(
        Media {
            key: MediaKey::new("test", MediaKind::Episode, "show:2:3"),
            title: "Show · Episode".into(),
            parent: Some(parent.clone()),
        },
        Progress {
            position_seconds: 30.0,
            duration_seconds: 120.0,
        },
    )];
    let missing = s.missing_library_posters();
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].key, parent);
    assert!(missing[0].parent.is_none());
    s.apply_library_posters(
        0,
        vec![(
            parent.clone(),
            Some(LibraryPoster {
                poster: Some("https://example.com/show.jpg".into()),
                year: "2026".into(),
            }),
        )],
        true,
    );
    s.apply_library_entries(vec![]);
    assert!(s.library.posters.cache.contains_key(&parent));
    assert!(s.missing_library_posters().is_empty());
    assert_eq!(visible_artwork(&s), vec!["https://example.com/show.jpg"]);
    s.apply_watching_entries(vec![]);
    assert!(!s.library.posters.cache.contains_key(&parent));
    assert!(visible_artwork(&s).is_empty());
}

#[test]
fn watching_grid_history_restores_the_scrolled_episode_card() {
    use scry_library::{Media, MediaKey, MediaKind, Progress};
    let mut s = init();
    s.apply_watching_entries(
        (0..40)
            .map(|i| {
                (
                    Media {
                        key: MediaKey::new("test", MediaKind::Episode, format!("show{i}:1:2")),
                        title: format!("Show {i} · Episode two"),
                        parent: Some(MediaKey::new("test", MediaKind::Series, format!("show{i}"))),
                    },
                    Progress {
                        position_seconds: 60.0,
                        duration_seconds: 120.0,
                    },
                )
            })
            .collect(),
    );
    let mut rt = rune::Runtime::new(s, view, (880.0, 600.0), rune::Env::default());
    rt.poll = Some(poll);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let viewport = rect(&rt, "watching-scroll");
    rt.wheel(viewport[0] + 30.0, viewport[1] + 30.0, -1_000_000.0);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let last = rect(&rt, "watching-open-39");
    assert!(last[1] >= viewport[1] && last[1] + last[3] <= viewport[1] + viewport[3] + 1.0);
    assert!(rt.find("watching-open-0").is_none());
    assert!(
        rt.state
            .missing_library_posters()
            .iter()
            .any(|m| m.key.id == "show39")
    );
    tap(&mut rt, "watching-open-39");
    assert_eq!(rt.state.detail.as_ref().unwrap().id, "show39");
    assert_eq!(rt.state.pending_episode.as_deref(), Some("show39:1:2"));
    tap(&mut rt, "nav-back");
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let restored = rect(&rt, "watching-open-39");
    assert!(
        restored[1] >= viewport[1] && restored[1] + restored[3] <= viewport[1] + viewport[3] + 1.0
    );
}

#[test]
fn rendered_detail_actions_persist_and_appear_in_the_library() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.sqlite3");
    let mut s = init();
    s.navigation.page = Page::Detail;
    s.config_busy = false;
    s.detail_namespace = "provider".into();
    s.detail = Some(
        serde_json::from_value(serde_json::json!({
            "id":"tt123", "type":"movie", "name":"A test movie"
        }))
        .unwrap(),
    );
    s.library.worker =
        Some(crate::library_worker::Worker::start(path.clone(), Arc::new(|| {})).unwrap());
    s.register_library_detail();
    let mut rt = rune::Runtime::new(s, view, (1000.0, 800.0), rune::Env::default());
    rt.poll = Some(poll);
    settle_library(&mut rt, |s| {
        s.library
            .snapshot
            .as_ref()
            .is_some_and(|v| v.selected_state.is_some())
    });
    tap(&mut rt, "library-watchlist");
    settle_library(&mut rt, |s| {
        s.library
            .snapshot
            .as_ref()
            .is_some_and(|v| v.selected_state.as_ref().is_some_and(|v| v.watchlist))
    });
    tap(&mut rt, "library-watched");
    settle_library(&mut rt, |s| {
        s.library
            .snapshot
            .as_ref()
            .is_some_and(|v| v.selected_state.as_ref().is_some_and(|v| v.watched.watched))
    });
    tap(&mut rt, "nav-library");
    settle_library(&mut rt, |s| {
        s.library.entries.as_ref().is_some_and(Vec::is_empty)
    });
    assert!(rt.find("library-open-0").is_none());
    tap(&mut rt, "library-watched-tab");
    settle_library(&mut rt, |s| {
        s.library
            .entries
            .as_ref()
            .is_some_and(|items| items.len() == 1)
    });
    assert!(rt.find("library-open-0").is_some());
    let library = scry_library::Library::open(path).unwrap();
    let key = scry_library::MediaKey::new("imdb", scry_library::MediaKind::Movie, "tt123");
    assert!(library.is_in_watchlist(&key).unwrap());
    assert!(library.watched_state(&key).unwrap().watched);
    assert_eq!(
        library.get_media(&key).unwrap().unwrap().title,
        "A test movie"
    );
}

#[test]
fn library_and_catalog_use_the_same_poster_card_geometry() {
    use crate::services::LibraryPoster;
    use scry_library::{Media, MediaKey, MediaKind};
    for width in [880.0, 1280.0] {
        let mut reference = None;
        for library in [false, true] {
            let mut s = init();
            scene_discover(&mut s);
            if library {
                s.navigation.page = Page::Library;
                let media: Vec<_> = (0..12)
                    .map(|i| Media {
                        key: MediaKey::new("test", MediaKind::Movie, format!("film{i}")),
                        title: format!("Film {i}"),
                        parent: None,
                    })
                    .collect();
                let first = media[0].key.clone();
                s.apply_library_entries(media);
                s.apply_library_posters(
                    0,
                    vec![(
                        first,
                        Some(LibraryPoster {
                            poster: Some("https://example.com/poster.jpg".into()),
                            year: "2026".into(),
                        }),
                    )],
                    true,
                );
                let image = rune::Pixels::new();
                image.set(2, 3, vec![180; 24]);
                s.artwork = Some(crate::artwork::Artwork::player_fixture(&[(
                    "https://example.com/poster.jpg",
                    image,
                )]));
            }
            let mut rt = rune::Runtime::new(s, view, (width, 820.0), rune::Env::default());
            for _ in 0..30 {
                rt.frame(1.0 / 60.0);
            }
            let key = if library { "library-open-0" } else { "title-0" };
            let card = rect(&rt, key);
            let poster = rect(
                &rt,
                if library {
                    "library-poster-0"
                } else {
                    "title-poster-0"
                },
            );
            let sizes = [card[2], card[3], poster[2], poster[3]];
            if let Some(expected) = reference {
                assert_eq!(sizes, expected);
            } else {
                reference = Some(sizes);
            }
            if library {
                assert!(
                    rt.find("poster-fallback-0").is_none(),
                    "cached posters must replace placeholders"
                );
                assert!(
                    rt.find("poster-fallback-1").is_some(),
                    "missing artwork keeps a placeholder"
                );
                assert_eq!(
                    visible_artwork(&rt.state),
                    vec!["https://example.com/poster.jpg"]
                );
                tap(&mut rt, "library-open-0");
                assert_eq!(rt.state.detail.as_ref().unwrap().id, "film0");
            }
        }
    }
}

#[test]
fn library_grid_scrolls_and_history_restores_the_opened_poster_card() {
    use crate::services::LibraryPoster;
    use scry_library::{Media, MediaKey, MediaKind};
    let mut s = init();
    s.navigation.page = Page::Library;
    let items: Vec<_> = (0..200)
        .map(|i| Media {
            key: MediaKey::new("test", MediaKind::Movie, format!("film{i}")),
            title: format!("Film {i}"),
            parent: None,
        })
        .collect();
    let posters = items
        .iter()
        .enumerate()
        .map(|(i, m)| {
            (
                m.key.clone(),
                Some(LibraryPoster {
                    poster: Some(format!("https://example.com/{i}.jpg")),
                    year: "2026".into(),
                }),
            )
        })
        .collect();
    s.apply_library_entries(items);
    s.apply_library_posters(0, posters, true);
    let mut rt = rune::Runtime::new(s, view, (880.0, 600.0), rune::Env::default());
    rt.poll = Some(poll);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let filters = rect(&rt, "library-watchlist-tab");
    let viewport = rect(&rt, "library-scroll-0-0-Movie");
    assert!(!visible_artwork(&rt.state).contains(&"https://example.com/199.jpg".into()));
    rt.wheel(viewport[0] + 30.0, viewport[1] + 30.0, -1_000_000.0);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let last = rect(&rt, "library-open-199");
    assert!(last[1] >= viewport[1] && last[1] + last[3] <= viewport[1] + viewport[3] + 1.0);
    assert_eq!(rect(&rt, "library-watchlist-tab"), filters);
    assert!(
        rt.state
            .artwork_urls
            .contains(&"https://example.com/199.jpg".into())
    );
    assert!(
        !rt.state
            .artwork_urls
            .contains(&"https://example.com/0.jpg".into())
    );
    assert!(rt.find("library-open-0").is_none());
    tap(&mut rt, "library-open-199");
    assert_eq!(rt.state.detail.as_ref().unwrap().id, "film199");
    tap(&mut rt, "nav-back");
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let restored = rect(&rt, "library-open-199");
    assert!(
        restored[1] >= viewport[1] && restored[1] + restored[3] <= viewport[1] + viewport[3] + 1.0
    );
}

#[test]
fn library_posters_are_bounded_visible_and_ignore_stale_or_unrelated_results() {
    use crate::services::LibraryPoster;
    use scry_library::{Media, MediaKey, MediaKind};
    let mut s = init();
    s.navigation.page = Page::Library;
    s.apply_library_entries(
        (0..200)
            .map(|i| Media {
                key: MediaKey::new("test", MediaKind::Movie, i.to_string()),
                title: format!("Movie {i}"),
                parent: None,
            })
            .collect(),
    );
    assert_eq!(s.missing_library_posters().len(), 12);
    s.poster_route = s.navigation.route_key;
    s.poster_columns = 3;
    s.poster_viewport = Some([0.0, 0.0, 700.0, 300.0]);
    s.poster_rects.insert(0, [0.0, -320.0, 700.0, 302.0]);
    s.poster_rects.insert(1, [0.0, 0.0, 700.0, 302.0]);
    let missing = s.missing_library_posters();
    assert_eq!(
        missing
            .iter()
            .map(|m| m.key.id.as_str())
            .collect::<Vec<_>>(),
        vec!["3", "4", "5"]
    );
    let key = missing[0].key.clone();
    let poster = Some(LibraryPoster {
        poster: Some("https://example.com/3.jpg".into()),
        year: "2026".into(),
    });
    s.library.posters.generation = 1;
    s.apply_library_posters(0, vec![(key.clone(), poster.clone())], true);
    assert!(s.library.posters.cache.is_empty());
    s.apply_library_posters(
        1,
        vec![
            (key.clone(), poster),
            (MediaKey::new("other", MediaKind::Movie, "3"), None),
        ],
        true,
    );
    assert_eq!(s.library.posters.cache.len(), 1);
    assert_eq!(visible_artwork(&s), vec!["https://example.com/3.jpg"]);
    s.apply_library_posters(1, vec![(missing[1].key.clone(), None)], true);
    assert_eq!(
        s.missing_library_posters()
            .iter()
            .map(|m| m.key.id.as_str())
            .collect::<Vec<_>>(),
        vec!["5"]
    );
    s.retry_library_posters();
    assert_eq!(
        s.missing_library_posters()
            .iter()
            .map(|m| m.key.id.as_str())
            .collect::<Vec<_>>(),
        vec!["4", "5"]
    );
}
