use super::*;
use scry_library::local_media::{LocalFile, LocalRoot};

fn local_file() -> LocalFile {
    LocalFile {
        id: 42,
        root_id: 1,
        root_path: "D:/Private root".into(),
        relative_path: "Show/episode.mkv".into(),
        size: 1_048_576,
        hints: "{}".into(),
        available: true,
        namespace: Some("imdb".into()),
        kind: Some("episode".into()),
        external_id: Some("tt123:1:1".into()),
        title: Some("Episode".into()),
        metadata: None,
        manual: false,
    }
}

#[test]
fn local_sources_are_visible_without_addon_streams_and_follow_provider_filters() {
    let mut state = init();
    scene_detail(&mut state);
    state.streams.clear();
    state.local.sources.generation = state.stream_id;
    state.local.sources.files = vec![local_file()];
    let mut rt = rune::Runtime::new(state, view, (1280.0, 820.0), rune::Env::default());
    rt.frame(0.0);
    assert!(rt.find("local-source-play-42").is_some());
    assert!(rt.find("source-empty").is_none());
    assert!(rt.elements().iter().all(|e| {
        e.text
            .as_deref()
            .is_none_or(|text| !text.contains("D:/Private root"))
    }));
    rt.update(|s, _| s.stream_provider = Some("Remote provider".into()));
    rt.frame(0.0);
    assert!(rt.find("local-source-play-42").is_none());
}

#[test]
fn quick_play_waits_for_local_lookup_and_does_not_choose_between_local_and_remote() {
    let (mut state, tx) = state_with_events();
    scene_detail(&mut state);
    state.library.quick_play = Some(state.detail_id);
    state.local.sources.pending = true;
    let generation = state.stream_id;
    tx.try_send(Event::Streams(
        generation,
        state.streams.clone(),
        Vec::new(),
        true,
    ))
    .unwrap();
    poll(&mut state, 0.0, &mut Tx::default());
    assert_eq!(state.library.quick_play, Some(state.detail_id));
    tx.try_send(Event::LocalSources(generation, Ok(vec![local_file()])))
        .unwrap();
    poll(&mut state, 0.0, &mut Tx::default());
    assert!(state.library.quick_play.is_none());
    assert!(!state.player.playing);
    assert_eq!(state.navigation.page, Page::Detail);
    assert_eq!(state.local_source_files().len(), 1);
}

#[test]
fn stale_local_source_results_cannot_replace_another_episode() {
    let (mut state, tx) = state_with_events();
    state.stream_id = 5;
    state.local.sources.pending = true;
    tx.try_send(Event::LocalSources(4, Ok(vec![local_file()])))
        .unwrap();
    poll(&mut state, 0.0, &mut Tx::default());
    assert!(state.local_source_files().is_empty());
    assert!(state.local.sources.pending);
}

#[test]
fn settings_shows_multiple_roots_and_safe_removal_actions() {
    let mut state = init();
    state.navigation.page = Page::Settings;
    state.local.data.roots = vec![
        LocalRoot {
            id: 1,
            path: "D:/Movies".into(),
            online: true,
        },
        LocalRoot {
            id: 2,
            path: "E:/Shows".into(),
            online: true,
        },
    ];
    let mut rt = rune::Runtime::new(state, view, (1280.0, 820.0), rune::Env::default());
    rt.frame(0.0);
    assert!(rt.find("local-remove-1").is_some());
    assert!(rt.find("local-remove-2").is_some());
    assert!(rt.find("local-root-add").is_some());
    assert!(rt.find("local-scan").is_some());
}
