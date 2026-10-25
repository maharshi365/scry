use super::*;

#[test]
fn filename_sources_wrap_before_the_play_icon_without_redundant_captions() {
    let release = "Carrie.1976.REMASTERED.1080p.BluRay.DTS-HD.x264-VeryLongReleaseGroup.mkv";
    for width in [880.0, 1280.0] {
        let mut s = init();
        scene_detail(&mut s);
        s.streams = vec![("Torrentio".into(), serde_json::from_value(serde_json::json!({
            "name":"Torrentio\n1080p", "infoHash":"0123456789012345678901234567890123456789",
            "title":format!("{release}\n15 peers · 1.53 GB\nRARBG")
        })).unwrap()), ("Direct provider".into(), serde_json::from_value(serde_json::json!({
            "name":"HTTPS", "url":"https://example.invalid/video", "title":"Direct release"
        })).unwrap())];
        let mut rt = rune::Runtime::new(s, view, (width, 820.0), rune::Env::default());
        rt.frame(0.0);
        let title = rect(&rt, "stream-release-0");
        let play = rect(&rt, "stream-play-0");
        assert!(
            title[3] > 32.0,
            "A filename must wrap within the text column: {title:?}"
        );
        assert!(title[0] + title[2] <= play[0] - 5.0);
        assert!(
            rt.elements().iter().all(|e| e
                .text
                .as_deref()
                .is_none_or(|text| !text.contains("Provided by")
                    && !text.contains("connects to peers")
                    && text != "Direct stream"))
        );
        assert_eq!(rt.state.streams[0].1.title.lines().next(), Some(release));
    }
}

#[test]
fn stale_catalog_and_playback_messages_do_not_replace_current_state() {
    let (mut s, tx) = state_with_events();
    s.catalog_id = 2;
    s.player.generation = 4;
    s.status = "current".into();
    tx.try_send(Event::Catalog(1, 0, Err("old catalog failed".into())))
        .unwrap();
    tx.try_send(Event::Stopped(3, Some("old player failed".into())))
        .unwrap();
    poll(&mut s, 0.0, &mut Tx::default());
    assert!(s.error.is_none());
    assert_eq!(s.status, "current");
}

#[test]
fn stream_failure_keeps_successful_sources_visible() {
    let (mut s, tx) = state_with_events();
    s.navigation.page = Page::Detail;
    s.stream_id = 7;
    let stream: Stream =
        serde_json::from_value(serde_json::json!({"url":"https://example.com/video"})).unwrap();
    tx.try_send(Event::Streams(
        7,
        vec![("Working add-on".into(), stream)],
        vec!["Other add-on failed".into()],
        true,
    ))
    .unwrap();
    poll(&mut s, 0.0, &mut Tx::default());
    assert_eq!(s.streams.len(), 1);
    assert_eq!(s.streams[0].0, "Working add-on");
    assert_eq!(s.warnings, ["Other add-on failed"]);
    assert!(!s.loading);
}

#[test]
fn source_filter_shows_only_the_selected_provider_and_resets_for_another_episode() {
    let mut state = init();
    scene_detail(&mut state);
    let stream = state.streams[0].1.clone();
    state.streams = vec![
        ("Provider A".into(), stream.clone()),
        ("Provider B".into(), stream),
    ];
    let mut rt = rune::Runtime::new(state, view, (1000.0, 800.0), rune::Env::default());
    rt.frame(0.0);
    tap(&mut rt, "source-select");
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    tap(&mut rt, "source-provider-1");
    assert!(rt.find("stream-0").is_none());
    assert!(rt.find("stream-1").is_some());
    tap(&mut rt, "source-select");
    tap(&mut rt, "source-all");
    assert!(rt.find("stream-0").is_some());
    assert!(rt.find("stream-1").is_some());
    tap(&mut rt, "source-select");
    tap(&mut rt, "source-provider-0");
    rt.update(|s, _| s.select_episode(1));
    assert!(rt.state.stream_provider.is_none());
    assert!(rt.state.streams.is_empty());
    assert_eq!(
        rt.state.selected_video_id.as_deref(),
        Some("scene-series:1:2")
    );
}

#[test]
fn unsupported_source_card_cannot_start_playback() {
    let mut state = init();
    scene_detail(&mut state);
    state.streams = vec![(
        "Header provider".into(),
        serde_json::from_value(serde_json::json!({
            "url":"https://example.invalid/video.mp4",
            "behaviorHints":{"proxyHeaders":{"request":{"Authorization":"private"}}}
        }))
        .unwrap(),
    )];
    let mut rt = rune::Runtime::new(state, view, (1000.0, 800.0), rune::Env::default());
    rt.frame(0.0);
    tap(&mut rt, "stream-0");
    assert_eq!(rt.state.navigation.page, Page::Detail);
    assert!(!rt.state.player.playing);
    assert_eq!(rt.state.player.generation, 0);
    assert!(rt.state.error.is_none());
}
