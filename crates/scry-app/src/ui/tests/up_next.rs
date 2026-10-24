use super::*;
use scry_library::{Library, Media, MediaKey, MediaKind, Progress};

fn media(kind: MediaKind, id: &str, parent: Option<MediaKey>) -> Media {
    Media {
        key: MediaKey::new("test", kind, id),
        title: id.into(),
        parent,
    }
}

#[test]
fn next_cards_use_each_items_progress_and_resume_the_exact_episode() {
    let show = MediaKey::new("test", MediaKind::Series, "show");
    let first = media(MediaKind::Episode, "show:1:1", Some(show.clone()));
    let second = media(MediaKind::Episode, "show:1:2", Some(show.clone()));
    let movie = media(MediaKind::Movie, "movie", None);
    let mut s = init();
    s.library.watching = vec![(
        second.clone(),
        Progress {
            position_seconds: 75.0,
            duration_seconds: 100.0,
        },
    )];
    s.apply_up_next_entries(vec![
        (
            first.clone(),
            Some(Progress {
                position_seconds: 25.0,
                duration_seconds: 100.0,
            }),
        ),
        (
            second,
            Some(Progress {
                position_seconds: 75.0,
                duration_seconds: 100.0,
            }),
        ),
        (
            movie,
            Some(Progress {
                position_seconds: 20.0,
                duration_seconds: 200.0,
            }),
        ),
    ]);
    s.library.posters.cache.insert(
        show,
        Some(crate::services::LibraryPoster {
            poster: Some("https://example.com/show.jpg".into()),
            year: "2026".into(),
        }),
    );
    let mut rt = rune::Runtime::new(s, view, (1000.0, 820.0), rune::Env::default());
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let current = rect(&rt, "watching-poster-0-progress-track");
    let current_fill = rect(&rt, "watching-poster-0-progress-fill");
    assert!((current_fill[2] / current[2] - 0.75).abs() < 0.01);
    for (index, fraction) in [0.25, 0.75, 0.1].into_iter().enumerate() {
        let track = rect(&rt, &format!("up-next-poster-{index}-progress-track"));
        let fill = rect(&rt, &format!("up-next-poster-{index}-progress-fill"));
        assert!((fill[2] / track[2] - fraction).abs() < 0.01);
    }
    let current_card = rect(&rt, "watching-open-0");
    assert!(rect(&rt, "up-next-heading")[1] >= current_card[1] + current_card[3]);
    assert!(visible_artwork(&rt.state).contains(&"https://example.com/show.jpg".into()));
    tap(&mut rt, "up-next-open-0");
    assert_eq!(rt.state.detail.as_ref().unwrap().id, "show");
    assert_eq!(rt.state.pending_episode.as_deref(), Some("show:1:1"));
}

#[test]
fn next_pagination_reaches_all_saved_titles_and_updates_after_marking_watched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite3");
    let movies: Vec<_> = (0..201)
        .map(|index| media(MediaKind::Movie, &format!("movie-{index}"), None))
        .collect();
    {
        let mut db = Library::open(&path).unwrap();
        for movie in &movies {
            db.upsert_media(movie).unwrap();
            db.add_to_watchlist(&movie.key).unwrap();
        }
    }
    let mut s = init();
    s.library.worker = Some(crate::library_worker::Worker::start(path, Arc::new(|| {})).unwrap());
    let mut rt = rune::Runtime::new(s, view, (880.0, 600.0), rune::Env::default());
    rt.poll = Some(poll);
    settle_library(&mut rt, |s| {
        s.library.up_next_loaded && s.library.up_next.len() == 200
    });
    assert!(rt.find("watching-empty").is_some());
    assert!(rt.state.library.up_next_has_more);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let mounted = rt
        .elements()
        .iter()
        .filter(|element| {
            !element.exiting
                && element
                    .key
                    .as_deref()
                    .is_some_and(|key| key.starts_with("up-next-open-"))
        })
        .count();
    assert!(
        mounted < 30,
        "Distant next cards should stay virtualized: {mounted}"
    );
    let viewport = rect(&rt, "watching-scroll");
    rt.wheel(viewport[0] + 20.0, viewport[1] + 20.0, -100_000.0);
    for _ in 0..120 {
        rt.frame(1.0 / 60.0);
    }
    tap(&mut rt, "up-next-next");
    settle_library(&mut rt, |s| {
        s.library.up_next_offset == 200 && s.library.up_next.len() == 1
    });
    assert_eq!(rt.state.library.up_next[0].0.key, movies[0].key);
    assert!(!rt.state.library.up_next_has_more);
    assert!(rt.find("up-next-next").is_none());
    rt.update(|s, _| {
        s.db(
            crate::library_worker::Command::SetWatched(movies[0].clone(), true),
            true,
        );
    });
    settle_library(&mut rt, |s| {
        s.library.up_next_offset == 0 && s.library.up_next.len() == 200
    });
    assert!(!rt.state.library.up_next_has_more);
    assert!(
        rt.state
            .library
            .up_next
            .iter()
            .all(|(item, _)| item.key != movies[0].key)
    );
}
