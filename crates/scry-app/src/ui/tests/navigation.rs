use super::*;

#[test]
fn enter_in_search_submits_from_other_pages_and_keeps_browse_history() {
    let mut state = init();
    state.navigation.page = Page::Library;
    state.config_busy = false;
    state.search = TextEdit::new("Arrival");
    let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
    rt.frame(0.0);
    assert!(
        rt.find("search-go").is_none(),
        "Search submits with Enter, not a separate button"
    );
    tap(&mut rt, "search");
    rt.key(rune::KeyEvent {
        key: rune::Key::Enter,
        mods: rune::Mods::default(),
    });
    rt.frame(0.0);
    assert_eq!(rt.state.navigation.page, Page::Discover);
    assert_eq!(rt.state.displayed_query, "Arrival");
    assert!(rt.state.loading);
    rt.update(|s, _| s.travel(false));
    assert_eq!(rt.state.navigation.page, Page::Library);
    assert_eq!(rt.state.search.text, "");
    rt.update(|s, _| s.travel(true));
    assert_eq!(rt.state.search.text, "Arrival");
    assert!(
        rt.state.loading,
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
    state.detail = Some(series(250));
    state.selected_video_id = Some("tt123:1:201".into());
    state.episode_offset = 200;
    state.navigate(Page::Library);
    state.travel(false);
    assert_eq!(state.episode_range(), 200..250);
    assert_eq!(state.selected_video_id.as_deref(), Some("tt123:1:201"));
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
    let mut provider = state.addons[0].clone();
    provider.manifest.id = "search.provider".into();
    let meta: Meta = serde_json::from_value(
        serde_json::json!({"id":"custom-title", "type":"series", "name":"A search result"}),
    )
    .unwrap();
    state.search = TextEdit::new("A search result");
    state.submit_search();
    let generation = state.catalog_id;
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
    assert_eq!(state.search.text, "A search result");
    tx.try_send(Event::Search(
        generation,
        vec![],
        vec!["Old search failed".into()],
        true,
    ))
    .unwrap();
    poll(&mut state, 0.0, &mut Tx::default());
    assert_eq!(state.metas.len(), 1);
    assert!(state.warnings.is_empty());
    state.open_result(0);
    assert_eq!(state.detail_namespace, "search.provider");
    assert_eq!(state.detail.as_ref().unwrap().id, "custom-title");
}
