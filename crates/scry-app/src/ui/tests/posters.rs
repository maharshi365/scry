use super::*;

fn gallery() -> rune::Runtime<State> {
    let mut state = init();
    scene_discover(&mut state);
    let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
    for _ in 0..4 {
        rt.frame(0.0);
    }
    rt
}

fn advance(rt: &mut rune::Runtime<State>) {
    for _ in 0..90 {
        rt.frame(1.0 / 60.0);
    }
}

#[test]
fn gallery_and_search_show_present_for_ongoing_series() {
    let mut rt = gallery();
    let mut metas = rt.state.discover.results.to_vec();
    metas[0].kind = "series".into();
    metas[0].year = "2022–".into();
    rt.state.discover.results = metas.into();
    for (query, expected) in [
        ("", "2022 to Present"),
        ("show", "2022 to Present · TV show"),
    ] {
        rt.update(|s, _| s.discover.displayed_query = query.into());
        rt.frame(0.0);
        let subtitle = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("title-info-0"))
            .unwrap();
        assert_eq!(subtitle.text.as_deref(), Some(expected));
    }
}

#[test]
fn tracked_series_keep_dates_and_status_overlays_on_posters() {
    let mut rt = gallery();
    let meta = serde_json::from_value(serde_json::json!({
        "id":"tt123", "type":"series", "name":"Show", "year":"2022–"
    }))
    .unwrap();
    rt.state.discover.results = vec![meta].into();
    let media = scry_library::Media {
        key: scry_library::MediaKey::new("imdb", scry_library::MediaKind::Series, "tt123"),
        title: "Show".into(),
        parent: None,
    };
    let progress = scry_library::Progress {
        position_seconds: 1.0,
        duration_seconds: 2.0,
    };
    rt.state.library.entries = Some(vec![media.clone()]);
    rt.state.library.watching = vec![(media.clone(), progress)];
    rt.state.library.posters.cache.insert(
        media.key.clone(),
        Some(crate::services::LibraryPoster {
            poster: None,
            year: "2022–".into(),
        }),
    );
    rt.state.library.series.states = vec![scry_library::SeriesState {
        key: media.key,
        tracked: true,
        watched: false,
        progress: Some(progress),
        summary: Some(scry_library::SeriesProgress {
            total_episodes: 2,
            released_episodes: 2,
            watched_episodes: 1,
            completed_equivalents: 1.0,
            resume_equivalents: 1.0,
        }),
    }];
    for (page, query, expected) in [
        (Page::Library, "", "2022 to Present"),
        (Page::Watching, "", "2022 to Present"),
        (Page::Discover, "", "2022 to Present"),
        (Page::Discover, "show", "2022 to Present · TV show"),
    ] {
        for watched in [false, true] {
            rt.update(|s, _| {
                s.navigation.page = page;
                s.discover.displayed_query = query.into();
                s.library.series.states[0].watched = watched;
                s.library.grid_generation += 1;
                s.discover.generation += 1;
            });
            advance(&mut rt);
            let prefix = match page {
                Page::Library => "library",
                Page::Watching => "watching",
                _ => "title",
            };
            let subtitle_key = format!("{prefix}-info-0");
            let subtitle = rt
                .elements()
                .into_iter()
                .find(|e| e.key.as_deref() == Some(&subtitle_key))
                .unwrap();
            assert_eq!(subtitle.text.as_deref(), Some(expected));
            if watched {
                assert!(rt.find(&format!("{prefix}-poster-0-watched")).is_some());
            } else {
                assert!(
                    rt.find(&format!("{prefix}-poster-0-progress-track"))
                        .is_some()
                );
            }
        }
    }
}

#[test]
fn gallery_cards_are_unframed_and_reserve_two_title_lines() {
    let rt = gallery();
    let elements = rt.elements();
    let card = elements
        .iter()
        .find(|e| e.key.as_deref() == Some("title-0"))
        .unwrap();
    assert_eq!(
        card.fill.a, 0.0,
        "The poster must not sit inside a filled card"
    );
    let title = rect(&rt, "poster-title-title-0");
    let subtitle = rect(&rt, "title-info-0");
    assert!(
        title[3] >= 30.0,
        "Reserve room for a wrapped title: {title:?}"
    );
    assert!(subtitle[1] >= title[1] + title[3]);
    assert!(subtitle[1] + subtitle[3] <= card.rect[1] + card.rect[3] + 0.5);
}

#[test]
fn gallery_zoom_is_centered_clipped_and_does_not_follow_the_pointer() {
    let mut rt = gallery();
    let frame = rect(&rt, "title-poster-0");
    let title = rect(&rt, "poster-title-title-0");
    let neighbor = rect(&rt, "title-poster-1");
    let neighbor_artwork = rect(&rt, "title-poster-1-artwork");
    rt.pointer_move(frame[0] + frame[2] * 0.2, frame[1] + frame[3] * 0.2);
    advance(&mut rt);
    let zoomed = rect(&rt, "title-poster-0-artwork");
    assert!(
        zoomed[2] > frame[2] && zoomed[3] > frame[3],
        "Only the artwork zooms"
    );
    assert!(zoomed[2] <= frame[2] * 1.041, "The zoom must stay subtle");
    for axis in 0..2 {
        assert!(
            (zoomed[axis] + zoomed[axis + 2] / 2.0 - frame[axis] - frame[axis + 2] / 2.0).abs()
                < 0.5
        );
    }
    let artwork = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("title-poster-0-artwork"))
        .unwrap();
    assert!(artwork.visible[2] <= frame[2] + 0.5 && artwork.visible[3] <= frame[3] + 0.5);
    rt.pointer_move(frame[0] + frame[2] * 0.8, frame[1] + frame[3] * 0.8);
    advance(&mut rt);
    assert_eq!(rect(&rt, "title-poster-0-artwork"), zoomed);
    assert_eq!(rect(&rt, "title-poster-0"), frame);
    assert_eq!(rect(&rt, "poster-title-title-0"), title);
    assert_eq!(rect(&rt, "title-poster-1"), neighbor);
    assert_eq!(rect(&rt, "title-poster-1-artwork"), neighbor_artwork);
    rt.pointer_move(0.0, 599.0);
    advance(&mut rt);
    let resting = rect(&rt, "title-poster-0-artwork");
    assert!((resting[2] - frame[2]).abs() < 0.5 && (resting[3] - frame[3]).abs() < 0.5);
}

#[test]
fn gallery_does_not_zoom_with_reduced_motion() {
    let mut rt = gallery();
    let resting = rect(&rt, "title-poster-0-artwork");
    let frame = rect(&rt, "title-poster-0");
    rt.pointer_move(frame[0] + frame[2] / 2.0, frame[1] + frame[3] / 2.0);
    advance(&mut rt);
    assert!(rect(&rt, "title-poster-0-artwork")[2] > resting[2]);
    rt.set_system_reduced_motion(true);
    rt.frame(0.0);
    assert_eq!(rect(&rt, "title-poster-0-artwork"), resting);
    rt.pointer_move(0.0, 599.0);
    rt.frame(0.0);
    rt.pointer_move(frame[0] + frame[2] / 2.0, frame[1] + frame[3] / 2.0);
    advance(&mut rt);
    assert_eq!(rect(&rt, "title-poster-0-artwork"), resting);
}
