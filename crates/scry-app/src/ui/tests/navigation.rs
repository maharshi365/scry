use super::*;

#[test]
fn history_restores_preview_but_discards_transient_detail_ui() {
    let mut state = init();
    scene_discover(&mut state);
    let titles = state.discover.results.clone();
    state.open_result(0);
    state.detail.namespace = "preview.provider".into();
    state.detail.episodes.search = TextEdit::new("temporary filter");
    state.detail.episodes.viewport = Some([0.0, 0.0, 100.0, 100.0]);
    state.detail.sources.menu_open = true;
    state.detail.episodes.menu_open = true;
    state.detail.request.error = Some("Temporary metadata failure".into());
    state.detail.sources.request.error = Some("Temporary stream failure".into());
    state.navigate(Page::Library);
    state.travel(false);
    assert_eq!(state.navigation.page, Page::Detail);
    assert!(state.detail.preview);
    assert_eq!(state.detail.namespace, "preview.provider");
    assert_eq!(state.discover.results[0].id, titles[0].id);
    assert!(state.detail.episodes.search.text.is_empty());
    assert!(state.detail.episodes.viewport.is_none());
    assert!(!state.detail.sources.menu_open);
    assert!(!state.detail.episodes.menu_open);
    assert!(state.detail.request.error.is_none());
    assert!(state.detail.sources.request.error.is_none());
}

#[test]
fn leaving_detail_rejects_late_metadata_and_stream_results() {
    let (mut state, tx) = state_with_events();
    state.open_detail_from(series(3));
    let detail = state.detail.generation;
    state.find_streams("series".into(), "tt123:1:1".into());
    let streams = state.runtime.streams.generation;
    state.navigate(Page::Library);
    tx.try_send(Event::Detail(detail, Err("Late metadata error".into())))
        .unwrap();
    tx.try_send(Event::Streams(
        streams,
        vec![],
        vec!["Late stream warning".into()],
        true,
    ))
    .unwrap();
    poll(&mut state, 0.0, &mut Tx::default());
    assert_eq!(state.navigation.page, Page::Library);
    assert!(state.detail.request.error.is_none());
    assert!(state.detail.sources.request.warnings.is_empty());
    assert!(!state.detail.request.loading);
    assert!(!state.detail.sources.request.loading);
}

#[test]
fn history_restarts_metadata_with_a_new_generation_and_keeps_source_order() {
    let mut state = init();
    state.open_detail_from(series(3));
    state.detail.sources.sort = crate::ui::source_picker::SourceSort::Best;
    let generation = state.detail.generation;
    state.navigate(Page::Library);
    let (services, _) = crate::services::Services::install_fixture().unwrap();
    state.runtime.services = Some(services);
    state.travel(false);
    assert!(state.detail.generation > generation);
    assert!(state.detail.request.loading);
    assert_eq!(
        state.detail.sources.sort,
        crate::ui::source_picker::SourceSort::Best
    );
    assert_eq!(state.detail.meta.as_ref().unwrap().id, "tt123");
}

#[test]
fn enter_in_search_submits_from_other_pages_and_keeps_browse_history() {
    let mut state = init();
    state.navigation.page = Page::Library;
    state.addons.busy = false;
    state.discover.search = TextEdit::new("Arrival");
    let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
    rt.frame(0.0);
    tap(&mut rt, "search");
    rt.key(rune::KeyEvent {
        key: rune::Key::Enter,
        mods: rune::Mods::default(),
    });
    rt.frame(0.0);
    assert_eq!(rt.state.navigation.page, Page::Discover);
    assert_eq!(rt.state.discover.displayed_query, "Arrival");
    assert!(rt.state.discover.request.loading);
    rt.update(|s, _| s.travel(false));
    assert_eq!(rt.state.navigation.page, Page::Library);
    assert_eq!(rt.state.discover.search.text, "");
    rt.update(|s, _| s.travel(true));
    assert_eq!(rt.state.discover.search.text, "Arrival");
    assert!(
        rt.state.discover.request.loading,
        "Canceled searches must restart on Forward"
    );
}

#[test]
fn focused_search_forwards_alt_history_keys_but_keeps_escape_local() {
    let mut state = init();
    state.navigate(Page::Addons);
    let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
    rt.on_key = Some(on_key);
    rt.frame(0.0);
    tap(&mut rt, "search");
    for (key, expected) in [
        (rune::Key::Left, Page::Watching),
        (rune::Key::Right, Page::Addons),
    ] {
        rt.key(rune::KeyEvent {
            key,
            mods: rune::Mods {
                alt: true,
                ..rune::Mods::default()
            },
        });
        rt.frame(0.0);
        assert_eq!(rt.state.navigation.page, expected);
    }
    rt.update(|s, _| s.navigate(Page::Detail));
    tap(&mut rt, "search");
    rt.key(rune::KeyEvent {
        key: rune::Key::Escape,
        mods: rune::Mods::default(),
    });
    rt.frame(0.0);
    assert_eq!(rt.state.navigation.page, Page::Detail);
}

#[test]
fn history_restores_episode_page_and_never_restores_player() {
    let mut state = init();
    state.navigation.page = Page::Detail;
    state.detail.meta = Some(series(250));
    state.detail.episodes.selected = Some("tt123:1:201".into());
    state.detail.episodes.offset = 200;
    state.navigate(Page::Library);
    state.travel(false);
    assert_eq!(state.episode_range(), 200..250);
    assert_eq!(
        state.detail.episodes.selected.as_deref(),
        Some("tt123:1:201")
    );
    state.navigation.page = Page::Player;
    state.navigate(Page::Addons);
    state.travel(false);
    assert_eq!(state.navigation.page, Page::Detail);
    assert!(!state.player.playing);
    assert!(state.player.generation > 0);
}

#[test]
fn search_history_keeps_result_provider_and_rejects_old_search_updates() {
    let (mut state, tx) = state_with_events();
    scene_discover(&mut state);
    let mut provider = state.addons.installed[0].clone();
    provider.manifest.id = "search.provider".into();
    let meta: Meta = serde_json::from_value(
        serde_json::json!({"id":"custom-title", "type":"series", "name":"A search result"}),
    )
    .unwrap();
    state.discover.search = TextEdit::new("A search result");
    state.submit_search();
    let generation = state.discover.generation;
    tx.try_send(Event::Search(
        generation,
        vec![crate::search::SearchResult {
            meta,
            addon: provider,
        }],
        vec![],
        true,
    ))
    .unwrap();
    poll(&mut state, 0.0, &mut Tx::default());
    state.choose_kind("series");
    state.travel(false);
    assert_eq!(state.discover.search.text, "A search result");
    tx.try_send(Event::Search(
        generation,
        vec![],
        vec!["Old search failed".into()],
        true,
    ))
    .unwrap();
    poll(&mut state, 0.0, &mut Tx::default());
    assert_eq!(state.discover.results.len(), 1);
    assert!(state.discover.request.warnings.is_empty());
    state.open_result(0);
    assert_eq!(state.detail.namespace, "search.provider");
    assert_eq!(state.detail.meta.as_ref().unwrap().id, "custom-title");
}
