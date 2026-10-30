use super::*;
use scry_library::{Library, Media, MediaKey, MediaKind};

fn wait_for_watched_series(rt: &mut rune::Runtime<State>, key: &MediaKey) {
    settle_library(rt, |s| {
        s.library
            .series
            .states
            .iter()
            .any(|state| state.key == *key && state.watched)
    });
}

#[test]
fn large_visible_series_set_is_not_evicted_until_it_leaves_the_viewport() {
    let mut s = init();
    s.navigation.page = Page::Library;
    let media = (0..50)
        .map(|index| Media {
            key: MediaKey::new("imdb", MediaKind::Series, format!("tt{index}")),
            title: format!("Show {index}"),
            parent: None,
        })
        .collect::<Vec<_>>();
    for item in &media {
        let meta = serde_json::from_value(
            serde_json::json!({"id":item.key.id,"type":"series","name":item.title}),
        )
        .unwrap();
        s.library
            .series
            .details
            .insert(item.key.clone(), (Some(meta), std::time::Instant::now()));
    }
    s.apply_library_entries(media);
    s.posters.route = s.navigation.route_key;
    s.posters.columns = 10;
    s.posters.viewport = Some([0.0, 0.0, 2000.0, 1500.0]);
    s.posters.visible = 0..5;
    s.request_series_states();
    assert_eq!(s.library.series.details.len(), 50);
    s.posters.viewport = Some([0.0, 0.0, 2000.0, 300.0]);
    s.posters.visible = 0..1;
    s.request_series_states();
    assert_eq!(s.library.series.details.len(), 32);
    assert!(
        super::super::poster_grid::series_keys(&s)
            .iter()
            .all(|key| s.library.series.details.contains_key(key))
    );
}

#[test]
fn undated_watched_specials_share_the_counter_and_poster_completion() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite3");
    let show = Media {
        key: MediaKey::new("imdb", MediaKind::Series, "tt123"),
        title: "Show".into(),
        parent: None,
    };
    {
        let mut db = Library::open(&path).unwrap();
        let releases = [Some(0), None]
            .into_iter()
            .enumerate()
            .map(|(index, released_at)| scry_library::EpisodeRelease {
                media: Media {
                    key: MediaKey::new("imdb", MediaKind::Episode, format!("tt123:{index}:1")),
                    title: format!("Episode {index}"),
                    parent: Some(show.key.clone()),
                },
                released_at,
                season: Some(u32::from(index == 0)),
                episode: None,
            })
            .collect::<Vec<_>>();
        db.set_media_watched(&show, None, Some(&releases), true)
            .unwrap();
    }
    let mut s = init();
    scene_discover(&mut s);
    s.discover.results = crate::ui::catalog::CatalogResults::from(vec![
        serde_json::from_value(serde_json::json!({"id":"tt123","type":"series","name":"Show"}))
            .unwrap(),
    ]);
    s.library.worker = Some(crate::library_worker::Worker::start(path, Arc::new(|| {})).unwrap());
    let mut rt = rune::Runtime::new(s, view, (880.0, 600.0), rune::Env::default());
    rt.poll = Some(poll);
    wait_for_watched_series(&mut rt, &show.key);
    assert_eq!(
        rt.state.series_progress_label(&show.key).as_deref(),
        Some("1 / 1 episodes watched · 100%")
    );
    assert!(rt.find("title-poster-0-watched").is_some());
}

#[test]
fn background_tracking_finds_completed_shows_hidden_by_the_library_filter() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite3");
    let mut s = init();
    s.navigation.page = Page::Library;
    {
        let mut db = Library::open(&path).unwrap();
        for index in 0..13 {
            let id = format!("tt100{index}");
            let key = MediaKey::new("imdb", MediaKind::Series, &id);
            let episode = Media {
                key: MediaKey::new("imdb", MediaKind::Episode, format!("{id}:1:1")),
                title: "Pilot".into(),
                parent: Some(key.clone()),
            };
            db.upsert_media(&Media {
                key: key.clone(),
                title: format!("Show {index}"),
                parent: None,
            })
            .unwrap();
            db.add_to_watchlist(&key).unwrap();
            db.register_release(&episode, Some(0)).unwrap();
            db.set_watched(&episode.key, true).unwrap();
            db.sync_episode_releases(&key, &[]).unwrap();
            let meta = serde_json::from_value(serde_json::json!({
                "id":id,"type":"series","name":"Show","videos":[
                    {"id":episode.key.id,"season":1,"episode":1,"released":"2000-01-01"}
                ]
            }))
            .unwrap();
            s.library
                .series
                .details
                .insert(key, (Some(meta), std::time::Instant::now()));
        }
    }
    s.library.worker = Some(crate::library_worker::Worker::start(path, Arc::new(|| {})).unwrap());
    let mut rt = rune::Runtime::new(s, view, (880.0, 600.0), rune::Env::default());
    rt.frame(0.0);
    tap(&mut rt, "library-series-filter");
    tap(&mut rt, "library-watched-tab");
    rt.poll = Some(poll);
    settle_library(&mut rt, |s| {
        s.library
            .entries
            .as_ref()
            .is_some_and(|items| items.len() == 13)
    });
    let first = rt.state.library.entries.as_ref().unwrap()[0].key.clone();
    wait_for_watched_series(&mut rt, &first);
    assert!(rt.find("library-poster-0-watched").is_some());
    tap(&mut rt, "library-watchlist-tab");
    settle_library(&mut rt, |s| {
        s.library.entries.as_ref().is_some_and(Vec::is_empty)
    });
    assert!(rt.find("library-empty").is_some());
}

#[test]
fn tv_completion_waits_for_matching_metadata_and_reacts_to_new_releases() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite3");
    let key = MediaKey::new("imdb", MediaKind::Series, "tt123");
    {
        let mut db = Library::open(&path).unwrap();
        db.upsert_media(&Media {
            key: key.clone(),
            title: "Show".into(),
            parent: None,
        })
        .unwrap();
        let episode = Media {
            key: MediaKey::new("imdb", MediaKind::Episode, "tt123:1:1"),
            title: "Pilot".into(),
            parent: Some(key.clone()),
        };
        db.register_release(&episode, Some(0)).unwrap();
        db.set_watched(&episode.key, true).unwrap();
        // A saved watch can outlive the provider's episode inventory.
        db.sync_episode_releases(&key, &[]).unwrap();
    }
    let (mut s, tx) = state_with_events();
    s.navigation.page = Page::Discover;
    s.discover.results = crate::ui::catalog::CatalogResults::from(vec![
        serde_json::from_value(serde_json::json!({"id":"tt123","type":"series","name":"Show"}))
            .unwrap(),
    ]);
    s.library.worker = Some(crate::library_worker::Worker::start(path, Arc::new(|| {})).unwrap());
    let mut rt = rune::Runtime::new(s, view, (880.0, 600.0), rune::Env::default());
    rt.poll = Some(poll);
    settle_library(&mut rt, |s| !s.library.series.states.is_empty());
    let meta: Meta = serde_json::from_value(serde_json::json!({
        "id":"tt123","type":"series","name":"Show","videos":[
            {"id":"tt123:1:1","season":1,"episode":1,"released":"2000-01-01"}
        ]
    }))
    .unwrap();
    let generation = rt.state.library.series.metadata_generation;
    tx.try_send(Event::SeriesDetails(
        generation + 1,
        vec![(key.clone(), Some(meta.clone()))],
        true,
    ))
    .unwrap();
    rt.frame(1.0 / 60.0);
    assert!(rt.state.library.series.details.is_empty());
    tx.try_send(Event::SeriesDetails(
        generation,
        vec![(key.clone(), Some(meta.clone()))],
        true,
    ))
    .unwrap();
    settle_library(&mut rt, |s| {
        s.library.series.states.iter().any(|state| state.watched)
    });
    assert!(rt.find("title-poster-0-watched").is_some());
    let mut new_release = meta;
    new_release.videos.push(
        serde_json::from_value(serde_json::json!({
            "id":"tt123:2:1","season":2,"episode":1,"released":"2001-01-01"
        }))
        .unwrap(),
    );
    tx.try_send(Event::SeriesDetails(
        generation,
        vec![(key, Some(new_release))],
        true,
    ))
    .unwrap();
    settle_library(&mut rt, |s| {
        s.library.series.states.first().is_some_and(|state| {
            !state.watched
                && state
                    .progress
                    .is_some_and(|p| (p.position_seconds - 0.5).abs() < 0.001)
        })
    });
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "One regression scenario verifies consistent show progress and completion across all four routes"
)]
fn tv_posters_share_episode_progress_and_require_the_full_inventory_across_routes() {
    for page in [Page::Discover, Page::Library, Page::Watching, Page::Detail] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.sqlite3");
        let parent = Media {
            key: MediaKey::new("imdb", MediaKind::Series, "tt123"),
            title: "Show".into(),
            parent: None,
        };
        let first = Media {
            key: MediaKey::new("imdb", MediaKind::Episode, "tt123:1:1"),
            title: "Show · Pilot".into(),
            parent: Some(parent.key.clone()),
        };
        let second = Media {
            key: MediaKey::new("imdb", MediaKind::Episode, "tt123:2:1"),
            title: "Show · Return".into(),
            parent: Some(parent.key.clone()),
        };
        {
            let mut db = Library::open(&path).unwrap();
            for media in [&parent, &first, &second] {
                db.upsert_media(media).unwrap();
            }
            db.add_to_watchlist(&parent.key).unwrap();
            db.register_release(&first, Some(0)).unwrap();
            db.register_release(&second, Some(0)).unwrap();
            db.set_watched(&first.key, true).unwrap();
            db.record_playback(&second.key, 30.0, 120.0, 30.0).unwrap();
        }
        let meta: Meta = serde_json::from_value(serde_json::json!({
            "id":"tt123", "type":"series", "name":"Show", "poster":"https://example.com/poster.jpg", "videos":[
                {"id":"tt123:1:1", "season":1, "episode":1, "released":"2000-01-01"},
                {"id":"tt123:2:1", "season":2, "episode":1, "released":"2001-01-01"},
                {"id":"tt123:2:2", "season":2, "episode":2, "released":"2100-01-01"}
            ]
        })).unwrap();
        let mut s = init();
        s.navigation.page = page;
        s.discover.kind = "series".into();
        s.detail.meta = Some(meta.clone());
        s.detail.namespace = "imdb".into();
        s.remember_series_detail();
        s.discover.results = crate::ui::catalog::CatalogResults::from(vec![meta]);
        s.library.worker =
            Some(crate::library_worker::Worker::start(path, Arc::new(|| {})).unwrap());
        let mut rt = rune::Runtime::new(s, view, (880.0, 600.0), rune::Env::default());
        rt.poll = Some(poll);
        rt.frame(0.0);
        if page == Page::Library {
            tap(&mut rt, "library-series-filter");
        }
        settle_library(&mut rt, |s| {
            s.library.series.states.iter().any(|state| {
                state.key == parent.key
                    && state.progress.is_some()
                    && state
                        .summary
                        .is_some_and(|summary| summary.total_episodes == 3)
            })
        });
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        let poster = match page {
            Page::Library => "library-poster-0",
            Page::Watching => "watching-poster-0",
            Page::Detail => "detail-poster",
            _ => "title-poster-0",
        };
        let track = rect(&rt, &format!("{poster}-progress-track"));
        let fill = rect(&rt, &format!("{poster}-progress-fill"));
        let expected = if page == Page::Watching {
            0.25
        } else {
            1.25 / 3.0
        };
        assert!((fill[2] / track[2] - expected).abs() < 0.01);
        rt.update(|s, _| {
            s.db(
                crate::library_worker::Command::SetWatched(second.clone(), true),
                true,
            );
        });
        if page == Page::Watching {
            settle_library(&mut rt, |s| s.library.watching.is_empty());
            continue;
        }
        settle_library(&mut rt, |s| {
            s.library.series.states.iter().any(|state| {
                state.key == parent.key
                    && !state.watched
                    && state.summary.is_some_and(|summary| {
                        summary.total_episodes == 3 && summary.watched_episodes == 2
                    })
            })
        });
        rt.update(|s, _| {
            let mut meta = s.detail.meta.clone().unwrap();
            meta.videos[2].released = Some("2002-01-01".into());
            s.detail.meta = Some(meta);
            s.remember_series_detail();
            s.db(
                crate::library_worker::Command::SetWatched(parent.clone(), true),
                true,
            );
        });
        if page == Page::Library {
            settle_library(&mut rt, |s| {
                s.library.entries.as_ref().is_some_and(Vec::is_empty)
            });
            tap(&mut rt, "library-watched-tab");
            settle_library(&mut rt, |s| {
                s.library
                    .entries
                    .as_ref()
                    .is_some_and(|items| items.iter().any(|media| media.key == parent.key))
            });
        }
        wait_for_watched_series(&mut rt, &parent.key);
        assert!(
            rt.find(&format!("{poster}-watched")).is_some(),
            "Missing badge on {page:?}"
        );
    }
}
