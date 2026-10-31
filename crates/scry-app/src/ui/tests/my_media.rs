use super::*;
use crate::local_media::{Index, Match};

fn local_index() -> (tempfile::TempDir, Index) {
    let root = tempfile::tempdir().unwrap();
    for name in [
        "Arrival.2016.mkv",
        "Example.S01E01.mkv",
        "Example.S01E02.mkv",
    ] {
        std::fs::write(root.path().join(name), []).unwrap();
    }
    let mut index = Index::default();
    index.add_root(root.path()).unwrap();
    index.scan();
    (root, index)
}

#[test]
fn sidebar_opens_my_media_and_filters_movies_from_shows() {
    let (_root, index) = local_index();
    let mut s = init();
    s.my_media.index = index;
    let mut rt = rune::Runtime::new(s, view, (1280.0, 820.0), rune::Env::default());
    rt.frame(0.0);
    tap(&mut rt, "nav-my-media");
    rt.frame(1.0);
    assert_eq!(rt.state.navigation.page, Page::MyMedia);
    assert!(rt.find("media-root").is_some());
    tap(&mut rt, "media-movies");
    rt.frame(1.0);
    let movie = &rt.state.my_media.index.items[0];
    let show = &rt.state.my_media.index.items[1];
    assert!(rt.find(&format!("media-open-{}", movie.id)).is_some());
    assert!(
        rt.find(&format!("media-open-{}", show.id)).is_none(),
        "Movie filter at {:?}; show at {:?}",
        rect(&rt, "media-movies"),
        rect(&rt, &format!("media-open-{}", show.id))
    );
    tap(&mut rt, "media-shows");
    rt.frame(1.0);
    assert!(
        rt.find(&format!(
            "media-open-{}",
            rt.state.my_media.index.items[0].id
        ))
        .is_none()
    );
    assert!(
        rt.find(&format!(
            "media-open-{}",
            rt.state.my_media.index.items[1].id
        ))
        .is_some()
    );
}

#[test]
fn unmatched_movie_opens_with_a_local_source_without_any_addons() {
    let (_root, index) = local_index();
    let id = index.items[0].id.clone();
    let mut s = init();
    s.navigation.page = Page::MyMedia;
    s.my_media.index = index;
    let mut rt = rune::Runtime::new(s, view, (1280.0, 820.0), rune::Env::default());
    rt.frame(0.0);
    tap(&mut rt, &format!("media-open-{id}"));
    assert_eq!(rt.state.navigation.page, Page::Detail);
    assert_eq!(rt.state.detail.meta.as_ref().unwrap().name, "Arrival");
    assert_eq!(rt.state.local_sources().len(), 1);
    assert!(rt.find("local-source-0").is_some());
    assert!(
        !rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some("No streams found"))
    );
}

#[test]
fn matched_files_are_sources_on_normal_metadata_details_and_episode_selection() {
    let (_root, mut index) = local_index();
    let meta: Meta = serde_json::from_value(serde_json::json!({
        "id":"ttshow", "type":"series", "name":"Example", "videos":[
            {"id":"ttshow:1:1", "title":"One", "season":1, "episode":1},
            {"id":"ttshow:1:2", "title":"Two", "season":1, "episode":2}
        ]
    }))
    .unwrap();
    index.items[1].matched = Some(Match::from_meta(&meta, true));
    let mut s = init();
    s.my_media.index = index;
    s.detail.meta = Some(meta);
    s.detail.episodes.selected = Some("ttshow:1:2".into());
    let sources = s.local_sources();
    assert_eq!(sources.len(), 1);
    assert!(sources[0].ends_with("Example.S01E02.mkv"));
    s.detail.episodes.selected = Some("ttshow:1:3".into());
    assert!(s.local_sources().is_empty());
}

#[test]
fn stale_metadata_search_cannot_replace_a_new_review() {
    let (mut s, tx) = state_with_events();
    s.my_media.search_generation = 2;
    tx.try_send(Event::MediaCandidates(1, Ok(vec![series(1)])))
        .unwrap();
    let mut rt = rune::Runtime::new(s, view, (1280.0, 820.0), rune::Env::default());
    rt.frame(0.0);
    poll(&mut rt.state, 0.0, &mut Tx::default());
    assert!(rt.state.my_media.candidates.is_empty());
}

#[test]
fn provider_filter_keeps_local_and_remote_sources_separate() {
    let (_root, index) = local_index();
    let mut s = init();
    s.navigation.page = Page::Detail;
    let meta = index.items[0].meta().unwrap();
    s.detail.episodes.selected = Some(meta.id.clone());
    s.detail.meta = Some(meta);
    s.my_media.index = index;
    s.detail.sources.streams = vec![(
        "Remote".into(),
        serde_json::from_value(serde_json::json!({"url":"https://example.com/video.mp4"})).unwrap(),
    )];
    let mut rt = rune::Runtime::new(s, view, (1280.0, 820.0), rune::Env::default());
    rt.frame(0.0);
    assert!(rt.find("local-source-0").is_some());
    assert!(rt.find("stream-0").is_some());
    rt.update(|s, _| s.detail.sources.provider = Some("Remote".into()));
    rt.frame(1.0);
    assert!(rt.find("local-source-0").is_none());
    rt.update(|s, _| s.detail.sources.provider = Some("My Media".into()));
    rt.frame(1.0);
    assert!(rt.find("local-source-0").is_some());
    assert!(rt.find("stream-0").is_none());
}

#[test]
fn saved_unmatched_series_uses_the_local_episode_inventory() {
    let (_root, index) = local_index();
    let seed: Meta = serde_json::from_value(serde_json::json!({
        "id":index.items[1].id, "type":"series", "name":"Example"
    }))
    .unwrap();
    let mut s = init();
    s.detail.namespace = "org.scry.local".into();
    s.my_media.index = index;
    s.open_detail_from(seed);
    assert_eq!(s.detail.meta.as_ref().unwrap().videos.len(), 2);
}

#[test]
fn episode_order_controls_show_the_selected_mode_and_leave_existing_order_on_lookup_failure() {
    let mut s = init();
    super::super::my_media::scene_ordering(&mut s);
    let id = s
        .my_media
        .index
        .items
        .iter()
        .find(|item| item.ordering.is_some())
        .unwrap()
        .id
        .clone();
    let mut rt = rune::Runtime::new(s, view, (1280.0, 820.0), rune::Env::default());
    rt.frame(0.0);
    for key in [
        "media-order-aired",
        "media-order-absolute",
        "media-order-dvd",
    ] {
        assert!(rt.find(key).is_some(), "{key}");
    }
    rt.state.my_media.busy = true;
    rt.state
        .apply_media_orders(1, &id, Err("No alternate DVD order is available".into()));
    assert!(!rt.state.my_media.busy);
    assert_eq!(
        rt.state
            .my_media
            .index
            .items
            .iter()
            .find(|item| item.id == id)
            .unwrap()
            .ordering
            .as_ref()
            .unwrap()
            .mode,
        crate::local_media::EpisodeOrder::Dvd
    );
    assert_eq!(
        rt.state.my_media.error.as_deref(),
        Some("No alternate DVD order is available")
    );
}

#[test]
fn previewing_order_choices_does_not_change_the_saved_mapping() {
    let mut s = init();
    super::super::my_media::scene_ordering(&mut s);
    let item = s
        .my_media
        .index
        .items
        .iter()
        .find(|item| item.ordering.is_some())
        .unwrap();
    let id = item.id.clone();
    let mut choice = item.ordering.clone().unwrap();
    choice.name = "Another DVD edition".into();
    s.apply_media_orders(1, &id, Ok(vec![choice]));
    let mut rt = rune::Runtime::new(s, view, (1280.0, 820.0), rune::Env::default());
    rt.frame(0.0);
    assert!(rt.find("media-order-choice-0").is_some());
    assert_eq!(
        rt.state
            .my_media
            .index
            .items
            .iter()
            .find(|item| item.id == id)
            .unwrap()
            .ordering
            .as_ref()
            .unwrap()
            .name,
        "Firefly DVD release"
    );
}
