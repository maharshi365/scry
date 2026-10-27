use super::*;

#[test]
fn quick_preview_is_wider_and_chips_share_rows_when_they_fit() {
    let mut s = init();
    scene_discover(&mut s);
    s.open_result(0);
    let meta = s.detail.as_mut().unwrap();
    meta.genres = vec!["Drama".into(), "Horror".into()];
    meta.cast = vec!["First Actor".into(), "Second Actor".into()];
    meta.director = vec!["Director A".into(), "Director B".into()];
    let mut rt = rune::Runtime::new(s, view, (1280.0, 820.0), rune::Env::default());
    rt.frame(0.0);
    assert_eq!(rect(&rt, "title-preview")[2], 400.0);
    let elements = rt.elements();
    for pair in [
        ["Drama", "Horror"],
        ["First Actor", "Second Actor"],
        ["Director A", "Director B"],
    ] {
        let a = elements
            .iter()
            .find(|e| e.text.as_deref() == Some(pair[0]))
            .unwrap()
            .rect;
        let b = elements
            .iter()
            .find(|e| e.text.as_deref() == Some(pair[1]))
            .unwrap()
            .rect;
        assert!((a[1] - b[1]).abs() < 0.5, "{pair:?} should share a row");
        assert!(b[0] >= a[0] + a[2]);
    }
}

#[test]
fn changing_preview_title_rejects_the_previous_titles_late_metadata() {
    let (mut s, tx) = state_with_events();
    scene_discover(&mut s);
    s.open_result(0);
    let stale = s.detail_id;
    let stale_streams = s.stream_id;
    let old = s.detail.clone().unwrap();
    s.open_result(1);
    let selected = s.detail.as_ref().unwrap().id.clone();
    tx.try_send(Event::Detail(stale, Ok((old, "scene.catalog".into()))))
        .unwrap();
    tx.try_send(Event::Streams(
        stale_streams,
        vec![(
            "Old provider".into(),
            serde_json::from_value(serde_json::json!({"url":"https://example.invalid/old"}))
                .unwrap(),
        )],
        vec![],
        true,
    ))
    .unwrap();
    poll(&mut s, 0.0, &mut Tx::default());
    assert_eq!(s.detail.as_ref().unwrap().id, selected);
    assert!(s.streams.is_empty());
    assert!(
        s.loading,
        "The selected title is still waiting for its own metadata"
    );
}

#[test]
fn paging_beyond_the_first_episode_page_loads_its_watched_marker() {
    use scry_library::{Library, Media, MediaKey, MediaKind};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.sqlite3");
    let parent = Media {
        key: MediaKey::new("imdb", MediaKind::Series, "tt123"),
        title: "Long show".into(),
        parent: None,
    };
    let episode = Media {
        key: MediaKey::new("imdb", MediaKind::Episode, "tt123:1:201"),
        title: "Episode 201".into(),
        parent: Some(parent.key.clone()),
    };
    {
        let mut db = Library::open(&path).unwrap();
        db.upsert_media(&parent).unwrap();
        db.register_release(&episode, Some(0)).unwrap();
        db.set_watched(&episode.key, true).unwrap();
    }
    let mut s = init();
    s.navigation.page = Page::Detail;
    s.detail_namespace = "imdb".into();
    s.detail = Some(series(250));
    s.library.worker = Some(crate::library_worker::Worker::start(path, Arc::new(|| {})).unwrap());
    s.register_library_detail();
    let mut rt = rune::Runtime::new(s, view, (1280.0, 820.0), rune::Env::default());
    rt.poll = Some(poll);
    settle_library(&mut rt, |s| s.library.snapshot.is_some());
    assert!(rt.find("episode-200").is_none());
    let viewport = rect(&rt, "episodes-scroll-0-0-Some(1)");
    for _ in 0..3 {
        rt.wheel(viewport[0] + 20.0, viewport[1] + 20.0, -100_000.0);
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
    }
    settle_library(&mut rt, |s| s.episode_watched("tt123:1:201"));
    assert!(rt.find("episode-watched-200").is_some());
}

#[test]
fn provider_metadata_drives_backdrop_logo_facts_and_episode_thumbnail() {
    let meta: Meta = serde_json::from_value(serde_json::json!({
        "id":"show", "type":"series", "name":"The Observatory", "runtime":"52 min",
        "imdbRating":7.8, "releaseInfo":"2026–", "cast":["First Actor"], "director":["A Director"],
        "background":"https://example.com/background.jpg", "logo":"https://example.com/logo.png",
        "trailerStreams":[{"ytId":"trailer-id"}],
        "videos":[{"id":"show:1:1", "title":"Pilot", "season":1, "episode":1,
            "thumbnail":"https://example.com/episode.jpg", "released":"2026-01-01T00:00:00Z"}]
    }))
    .unwrap();
    assert_eq!(meta.trailer_streams[0].yt_id, "trailer-id");
    let image = rune::Pixels::new();
    image.set(2, 2, vec![180; 16]);
    let mut s = init();
    s.navigation.page = Page::Detail;
    s.detail = Some(meta);
    s.player_artwork = Some(crate::artwork::Artwork::player_fixture(&[
        ("https://example.com/background.jpg", image.clone()),
        ("https://example.com/logo.png", image.clone()),
    ]));
    s.artwork = Some(crate::artwork::Artwork::player_fixture(&[(
        "https://example.com/episode.jpg",
        image,
    )]));
    assert!(visible_artwork(&s).contains(&"https://example.com/episode.jpg".to_owned()));
    let mut rt = rune::Runtime::new(s, view, (1280.0, 820.0), rune::Env::default());
    rt.frame(0.0);
    for key in ["detail-background", "detail-logo", "episode-thumbnail-0"] {
        assert!(rt.find(key).is_some(), "{key}");
    }
    assert!(rt.elements().iter().any(|e| {
        e.text
            .as_deref()
            .is_some_and(|text| text.contains("52 min") && text.contains("7.8"))
    }));
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some("First Actor"))
    );
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some("2026-01-01"))
    );
}

#[test]
fn catalog_preview_opens_the_picker_and_history_restores_the_preview() {
    for width in [880.0, 1280.0] {
        let mut s = init();
        scene_discover(&mut s);
        let mut rt = rune::Runtime::new(s, view, (width, 820.0), rune::Env::default());
        rt.frame(0.0);
        tap(&mut rt, "title-0");
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        assert!(rt.state.detail_preview);
        assert!(rt.find("title-preview").is_some());
        assert!(rt.find("title-0").is_some());
        let panel = rect(&rt, "title-preview");
        assert!(
            panel[0] + panel[2] <= width + 1.0,
            "Preview fits window: {panel:?}"
        );
        for key in [
            "library-watchlist",
            "library-watched",
            "title-trailer",
            "title-play",
        ] {
            let bounds = rect(&rt, key);
            assert!(
                bounds[2] > 30.0 && bounds[1] + bounds[3] <= panel[1] + panel[3],
                "{key} has a usable target: {bounds:?}"
            );
            assert!(
                bounds[0] >= panel[0] && bounds[0] + bounds[2] <= panel[0] + panel[2] + 1.0,
                "{key} fits preview: {bounds:?} in {panel:?}"
            );
            assert!((bounds[3] - 52.0).abs() < 0.5);
        }
        tap(&mut rt, "title-play");
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        assert!(!rt.state.detail_preview);
        assert!(rt.find("source-picker").is_some());
        assert!(rect(&rt, "source-picker")[2] > 250.0);
        tap(&mut rt, "detail-back");
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        assert!(rt.state.detail_preview);
        assert!(rt.find("title-preview").is_some());
    }
}

#[test]
fn tv_picker_returns_from_sources_without_changing_season() {
    let mut s = init();
    s.navigation.page = Page::Detail;
    s.detail = Some(multi_season_series());
    let mut rt = rune::Runtime::new(s, view, (880.0, 600.0), rune::Env::default());
    rt.frame(0.0);
    assert!(rt.find("episodes").is_some());
    assert!(rt.find("source-picker").is_none());
    assert!(rt.find("episode-search").is_none());
    assert!(rt.find("series-progress-summary").is_none());
    assert!(
        rt.elements()
            .iter()
            .all(|element| element.text.as_deref() != Some("EPISODES"))
    );
    assert_eq!(rt.state.visible_episode_indices(), [4, 2]);
    tap(&mut rt, "episode-2");
    assert_eq!(rt.state.selected_video_id.as_deref(), Some("show:1:2"));
    assert!(rt.find("source-picker").is_some());
    let stale = rt.state.stream_id;
    tap(&mut rt, "detail-back");
    assert!(rt.state.stream_id > stale);
    assert_eq!(rt.state.active_season(), Some(1));
    assert!(rt.state.selected_video_id.is_none());
    assert!(rt.find("episode-2").is_some());
}

#[test]
fn app_fullscreen_hides_topbar_on_every_route_and_escape_keeps_the_route() {
    for page in [
        Page::Watching,
        Page::Discover,
        Page::Library,
        Page::Addons,
        Page::Calendar,
        Page::Detail,
    ] {
        let mut s = init();
        s.navigation.page = page;
        s.detail = Some(series(3));
        let mut rt = rune::Runtime::new(s, view, (1280.0, 820.0), rune::Env::default());
        rt.on_key = Some(on_key);
        rt.frame(0.0);
        tap(&mut rt, "app-fullscreen");
        assert!(rt.state.app_fullscreen);
        assert!(rt.find("topbar").is_none());
        assert!(rt.find("app-exit-fullscreen").is_some());
        rt.key(rune::KeyEvent {
            key: rune::Key::Escape,
            mods: rune::Mods::default(),
        });
        rt.frame(0.0);
        assert!(!rt.state.app_fullscreen);
        assert_eq!(rt.state.navigation.page, page);
        assert!(rt.find("topbar").is_some());
    }
}

#[test]
fn f11_works_in_search_and_app_fullscreen_survives_stopping_playback() {
    let mut rt = rune::Runtime::new(init(), view, (1280.0, 820.0), rune::Env::default());
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    tap(&mut rt, "search");
    rt.key(rune::KeyEvent {
        key: rune::Key::Function(11),
        mods: rune::Mods::default(),
    });
    rt.frame(0.0);
    assert!(rt.state.app_fullscreen);
    assert!(rt.find("topbar").is_none());
    rt.update(|s, _| {
        s.player.playing = true;
        s.stop();
    });
    assert!(rt.state.app_fullscreen);
    tap(&mut rt, "app-exit-fullscreen");
    assert!(!rt.state.app_fullscreen);
}
