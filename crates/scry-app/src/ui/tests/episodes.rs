use super::*;

#[test]
fn episode_201_is_reachable_and_selection_uses_its_absolute_identity() {
    let mut s = init();
    s.detail = Some(series(250));
    assert_eq!(s.episode_range(), 0..100);
    s.move_episode_page(true);
    assert_eq!(s.episode_range(), 100..200);
    s.move_episode_page(true);
    assert_eq!(s.episode_range(), 200..250);
    s.select_episode(s.episode_range().start);
    assert_eq!(s.library_media().unwrap().key.id, "tt123:1:201");
    s.move_episode_page(true);
    assert_eq!(s.episode_range(), 200..250);
    for _ in 0..3 {
        s.move_episode_page(false);
    }
    assert_eq!(s.episode_range(), 0..100);
}

#[test]
fn season_selector_filters_and_sorts_episodes_without_changing_protocol_identity() {
    let mut state = init();
    state.navigation.page = Page::Detail;
    state.detail = Some(multi_season_series());
    assert_eq!(state.seasons(), [Some(1), Some(2), Some(0), None]);
    assert_eq!(state.visible_episode_indices(), [4, 2]);
    let mut rt = rune::Runtime::new(state, view, (1000.0, 800.0), rune::Env::default());
    rt.frame(0.0);
    assert!(rt.find("episode-4").is_some());
    tap(&mut rt, "season-next");
    assert_eq!(rt.state.visible_episode_indices(), [3, 0]);
    tap(&mut rt, "episode-3");
    assert_eq!(rt.state.selected_video_id.as_deref(), Some("show:2:1"));
    assert_eq!(
        rt.state
            .visible_episode_keys()
            .iter()
            .map(|key| key.id.as_str())
            .collect::<Vec<_>>(),
        ["show:2:1", "show:2:2"]
    );
    tap(&mut rt, "detail-back");
    assert!(rt.state.selected_video_id.is_none());
    assert!(rt.find("episode-3").is_some());
    tap(&mut rt, "season-select");
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    tap(&mut rt, "season-option-2");
    assert_eq!(rt.state.visible_episode_indices(), [1]);
    assert!(rt.state.selected_video_id.is_none());
    tap(&mut rt, "season-next");
    assert_eq!(rt.state.visible_episode_indices(), [5]);
}

#[test]
fn changing_season_rejects_old_streams_and_history_restores_the_season() {
    let (mut state, tx) = state_with_events();
    state.navigation.page = Page::Detail;
    state.detail = Some(multi_season_series());
    state.select_episode(4);
    let old_id = state.stream_id;
    state.select_season(Some(2));
    tx.try_send(Event::Streams(
        old_id,
        vec![(
            "Old provider".into(),
            serde_json::from_value(serde_json::json!({"url":"https://example.invalid/video"}))
                .unwrap(),
        )],
        vec![],
        true,
    ))
    .unwrap();
    poll(&mut state, 0.0, &mut Tx::default());
    assert!(state.streams.is_empty());
    assert!(!state.loading);
    state.navigate(Page::Addons);
    state.travel(false);
    assert_eq!(state.active_season(), Some(2));
    assert_eq!(state.visible_episode_indices(), [3, 0]);
}

#[test]
fn library_episode_opens_the_correct_season_with_unsorted_metadata() {
    let (mut state, tx) = state_with_events();
    state.pending_episode = Some("show:2:2".into());
    state.open_detail_from(series(0));
    tx.try_send(Event::Detail(
        state.detail_id,
        Ok((multi_season_series(), "provider".into())),
    ))
    .unwrap();
    poll(&mut state, 0.0, &mut Tx::default());
    assert_eq!(state.active_season(), Some(2));
    assert_eq!(state.selected_video_id.as_deref(), Some("show:2:2"));
    assert_eq!(state.visible_episode_indices(), [3, 0]);
}

#[test]
fn pending_library_episode_opens_its_page_before_registration() {
    let (mut s, tx) = state_with_events();
    s.episode_offset = 200;
    s.pending_episode = Some("tt123:1:201".into());
    s.open_detail_from(series(0));
    assert_eq!(s.episode_offset, 0);
    tx.try_send(Event::Detail(
        s.detail_id,
        Ok((series(250), "provider".into())),
    ))
    .unwrap();
    poll(&mut s, 0.0, &mut Tx::default());
    assert_eq!(s.episode_range(), 200..250);
    assert_eq!(s.selected_video_id.as_deref(), Some("tt123:1:201"));
    assert_eq!(s.visible_episode_keys().first().unwrap().id, "tt123:1:201");
    assert!(s.pending_episode.is_none());
}

#[test]
fn episode_page_bounds_handle_empty_and_out_of_range_offsets() {
    let mut s = init();
    s.detail = Some(series(0));
    s.episode_offset = usize::MAX;
    assert_eq!(s.episode_range(), 0..0);
    s.detail = Some(series(201));
    assert_eq!(s.episode_range(), 200..201);
    s.move_episode_page(true);
    assert_eq!(s.episode_offset, 200);
}
