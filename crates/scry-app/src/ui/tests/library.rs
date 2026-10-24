use super::*;

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
