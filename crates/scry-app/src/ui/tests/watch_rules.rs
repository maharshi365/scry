use super::*;
use crate::library_worker::{Command, Worker};
use scry_library::{Library, Media, MediaKey, MediaKind};

fn show_meta() -> Meta {
    serde_json::from_value(
        serde_json::json!({"id":"tt123", "type":"series", "name":"Show", "videos":[
            {"id":"tt123:1:1", "season":1,"episode":1,"released":"2000-01-01"},
            {"id":"tt123:1:2", "season":1,"episode":2,"released":"2000-01-01"},
            {"id":"tt123:1:3", "season":1,"episode":3,"released":"2100-01-01"}
        ]}),
    )
    .unwrap()
}

#[test]
fn show_mark_watched_updates_episode_icons_skips_future_and_needs_no_auto_track() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.db");
    let mut s = init();
    s.navigation.page = Page::Detail;
    s.config_busy = false;
    s.detail_namespace = "imdb".into();
    s.detail = Some(show_meta());
    s.library.worker = Some(Worker::start(path.clone(), Arc::new(|| {})).unwrap());
    s.register_library_detail();
    s.remember_series_detail();
    let mut rt = rune::Runtime::new(s, view, (1280.0, 820.0), rune::Env::default());
    rt.poll = Some(poll);
    settle_library(&mut rt, |s| {
        s.library
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.selected_state.is_some())
    });
    assert!(rt.find("library-auto").is_none());
    tap(&mut rt, "library-watched");
    settle_library(&mut rt, |s| {
        s.library.snapshot.as_ref().is_some_and(|snapshot| {
            snapshot
                .selected_state
                .as_ref()
                .is_some_and(|state| !state.watched.watched)
                && snapshot
                    .episode_states
                    .iter()
                    .filter(|(_, state)| state.watched)
                    .count()
                    == 2
        })
    });
    assert!(rt.find("episode-watched-0").is_some());
    assert!(rt.find("episode-watched-1").is_some());
    assert!(rt.find("episode-watched-2").is_none());
    assert!(rt.find("detail-poster-watched").is_none());
    let icon = rt
        .elements()
        .into_iter()
        .find(|element| !element.exiting && element.key.as_deref() == Some("episode-watched-0"))
        .unwrap();
    assert_eq!(icon.role, "img");
    assert!(icon.text.is_none());
    let episode = Media {
        key: MediaKey::new("imdb", MediaKind::Episode, "tt123:1:1"),
        title: "Pilot".into(),
        parent: Some(MediaKey::new("imdb", MediaKind::Series, "tt123")),
    };
    rt.update(|s, _| s.mark_media_watched(episode.clone(), false));
    settle_library(&mut rt, |s| {
        s.library.snapshot.as_ref().is_some_and(|snapshot| {
            snapshot
                .selected_state
                .as_ref()
                .is_some_and(|state| !state.watched.watched)
                && snapshot
                    .episode_states
                    .iter()
                    .filter(|(_, state)| state.watched)
                    .count()
                    == 1
        })
    });
    rt.update(|s, _| {
        s.db(
            Command::SavePlayback(episode.clone(), 80.0, 100.0, 80.0),
            true,
        );
    });
    settle_library(&mut rt, |s| {
        s.library.snapshot.as_ref().is_some_and(|snapshot| {
            snapshot
                .selected_state
                .as_ref()
                .is_some_and(|state| !state.watched.watched)
                && snapshot
                    .episode_states
                    .iter()
                    .filter(|(_, state)| state.watched)
                    .count()
                    == 2
        })
    });
    assert!(
        Library::open(path)
            .unwrap()
            .watched_state(&episode.key)
            .unwrap()
            .watched
    );
}

#[test]
fn next_only_home_has_no_currently_watching_heading_or_empty_placeholder() {
    let mut s = init();
    s.apply_up_next_entries(vec![(
        Media {
            key: MediaKey::new("test", MediaKind::Movie, "movie"),
            title: "Movie".into(),
            parent: None,
        },
        None,
    )]);
    let mut rt = rune::Runtime::new(s, view, (880.0, 600.0), rune::Env::default());
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rt.find("watching-heading").is_none());
    assert!(rt.find("watching-empty").is_none());
    assert!(rt.find("up-next-heading").is_some());
    assert!(rt.find("up-next-open-0").is_some());
}

#[test]
fn future_movie_watched_action_is_disabled_and_auto_track_is_absent() {
    let mut s = init();
    s.navigation.page = Page::Detail;
    s.detail = Some(
        serde_json::from_value(serde_json::json!({
            "id":"movie", "type":"movie", "name":"Future movie", "released":"2100-01-01"
        }))
        .unwrap(),
    );
    let mut rt = rune::Runtime::new(s, view, (880.0, 600.0), rune::Env::default());
    rt.frame(0.0);
    let action = rt
        .elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some("library-watched"))
        .unwrap();
    assert!(
        action
            .label
            .as_deref()
            .is_some_and(|label| label.contains("unavailable"))
    );
    assert!(rt.find("library-auto").is_none());
}

#[test]
fn release_deadline_refreshes_cached_show_inventory_without_provider_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.db");
    let show = Media {
        key: MediaKey::new("imdb", MediaKind::Series, "tt123"),
        title: "Show".into(),
        parent: None,
    };
    let mut releases: Vec<_> = (1..=2)
        .map(|index| scry_library::EpisodeRelease {
            media: Media {
                key: MediaKey::new("imdb", MediaKind::Episode, format!("tt123:1:{index}")),
                title: format!("Episode {index}"),
                parent: Some(show.key.clone()),
            },
            released_at: Some(if index == 1 { 0 } else { i64::MAX }),
            season: Some(1),
            episode: Some(index),
        })
        .collect();
    {
        let mut db = Library::open(&path).unwrap();
        db.upsert_media(&show).unwrap();
        db.add_to_watchlist(&show.key).unwrap();
        db.sync_episode_releases(&show.key, &releases).unwrap();
        db.set_watched(&show.key, true).unwrap();
    }
    let mut s = init();
    scene_discover(&mut s);
    s.metas = Arc::new(vec![
        serde_json::from_value(serde_json::json!({"id":"tt123","type":"series","name":"Show"}))
            .unwrap(),
    ]);
    s.library.worker = Some(Worker::start(path.clone(), Arc::new(|| {})).unwrap());
    let mut rt = rune::Runtime::new(s, view, (880.0, 600.0), rune::Env::default());
    rt.poll = Some(poll);
    settle_library(&mut rt, |s| {
        s.library.series.states.iter().any(|state| {
            state.key == show.key
                && !state.watched
                && state.summary.is_some_and(|summary| {
                    summary.released_episodes == 1
                        && summary.total_episodes == 2
                        && summary.watched_episodes == 1
                })
        })
    });
    // Advance the stored release boundary without sending the UI a change event.
    releases[1].released_at = Some(0);
    Library::open(path)
        .unwrap()
        .sync_episode_releases(&show.key, &releases)
        .unwrap();
    rt.update(|s, _| s.library.next_release_at = Some(0));
    settle_library(&mut rt, |s| {
        s.library.series.states.iter().any(|state| {
            state.key == show.key
                && !state.watched
                && state.summary.is_some_and(|summary| {
                    summary.released_episodes == 2 && summary.watched_episodes == 1
                })
        })
    });
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rt.find("title-poster-0-watched").is_none());
}
