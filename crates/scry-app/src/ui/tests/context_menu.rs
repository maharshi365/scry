use super::*;
use scry_library::{Library, Media, MediaKey, MediaKind};

fn right_click(rt: &mut rune::Runtime<State>, key: &str) {
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let bounds = rect(rt, key);
    rt.secondary_down(bounds[0] + 10.0, bounds[1] + 10.0);
    rt.secondary_up(bounds[0] + 10.0, bounds[1] + 10.0);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
}

#[test]
fn season_actions_cover_every_episode_on_all_pages_but_not_other_seasons() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite3");
    let parent = Media {
        key: MediaKey::new("imdb", MediaKind::Series, "tt123"),
        title: "Show".into(),
        parent: None,
    };
    let other = Media {
        key: MediaKey::new("imdb", MediaKind::Episode, "tt123:2:1"),
        title: "Other season".into(),
        parent: Some(parent.key.clone()),
    };
    {
        let mut db = Library::open(&path).unwrap();
        db.upsert_media(&parent).unwrap();
        db.upsert_media(&other).unwrap();
        db.record_playback(&other.key, 30.0, 120.0, 0.0).unwrap();
    }
    let mut s = init();
    s.navigation.page = Page::Detail;
    s.detail_namespace = "imdb".into();
    let mut videos: Vec<_> = (1..=205).map(|index| serde_json::json!({
        "id":format!("tt123:1:{index}"),"title":format!("Episode {index}"),"season":1,"episode":index,"released":"2000-01-01"
    })).collect();
    videos.push(serde_json::json!({"id":"tt123:2:1","title":"Other season","season":2,"episode":1,"released":"2000-01-01"}));
    s.detail = Some(
        serde_json::from_value(
            serde_json::json!({"id":"tt123","type":"series","name":"Show","videos":videos}),
        )
        .unwrap(),
    );
    s.remember_series_detail();
    s.library.worker =
        Some(crate::library_worker::Worker::start(path.clone(), Arc::new(|| {})).unwrap());
    let mut rt = rune::Runtime::new(s, view, (1280.0, 820.0), rune::Env::default());
    rt.poll = Some(poll);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    right_click(&mut rt, "season-select");
    assert!(rt.find("context-season-watched").is_some());
    assert!(rt.find("context-watched").is_none());
    tap(&mut rt, "context-season-watched");
    settle_library(&mut rt, |s| {
        s.library.snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.episode_states.len() == 100
                && snapshot
                    .episode_states
                    .iter()
                    .all(|(_, state)| state.watched)
        })
    });
    let db = Library::open(&path).unwrap();
    let last = MediaKey::new("imdb", MediaKind::Episode, "tt123:1:205");
    assert!(db.watched_state(&last).unwrap().watched);
    assert!(db.load_progress(&other.key).unwrap().is_some());
    // The new achievement notification occupies the top-right controls until
    // dismissed, just as it does in the app.
    if rt.find("achievement-dismiss").is_some() {
        tap(&mut rt, "achievement-dismiss");
    }
    right_click(&mut rt, "season-select");
    tap(&mut rt, "context-season-unwatched");
    settle_library(&mut rt, |s| {
        s.library.snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.episode_states.len() == 100
                && snapshot
                    .episode_states
                    .iter()
                    .all(|(_, state)| !state.watched)
        })
    });
    assert!(!db.watched_state(&last).unwrap().watched);
    assert!(db.load_progress(&other.key).unwrap().is_some());
    tap(&mut rt, "season-select");
    right_click(&mut rt, "season-option-1");
    let super::super::context_menu::Target::Season {
        season, episodes, ..
    } = rt.state.context_menu.as_ref().unwrap().1.as_ref().unwrap()
    else {
        panic!("Expected a season target")
    };
    assert_eq!(*season, Some(2));
    assert_eq!(
        episodes,
        &vec![Media {
            title: "Show · Other season".into(),
            ..other
        }]
    );
}

#[test]
fn library_actions_use_add_and_remove_library_labels() {
    let mut s = init();
    scene_discover(&mut s);
    let mut rt = rune::Runtime::new(s, view, (880.0, 600.0), rune::Env::default());
    rt.frame(0.0);
    right_click(&mut rt, "title-0");
    let elements = rt.elements();
    assert!(
        elements
            .iter()
            .any(|e| e.key.as_deref() == Some("context-later")
                && e.label.as_deref() == Some("Add to library"))
    );
    assert!(
        elements
            .iter()
            .any(|e| e.key.as_deref() == Some("context-remove")
                && e.label.as_deref() == Some("Remove from library"))
    );
    assert!(
        elements
            .iter()
            .all(|e| e.text.as_deref() != Some("Watch later"))
    );
}
