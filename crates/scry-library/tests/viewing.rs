use scry_library::{Library, Media, MediaKey, MediaKind, Page};

fn movie(id: &str) -> Media {
    Media {
        key: MediaKey::new("test", MediaKind::Movie, id),
        title: id.into(),
        parent: None,
    }
}

#[test]
fn watched_titles_leave_to_watch_and_unwatched_saved_titles_return() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    for kind in [MediaKind::Movie, MediaKind::Series] {
        let item = Media {
            key: MediaKey::new("test", kind, "saved"),
            title: "Saved".into(),
            parent: None,
        };
        db.upsert_media(&item)?;
        db.add_to_watchlist(&item.key)?;
        if kind == MediaKind::Series {
            db.sync_episode_releases(
                &item.key,
                &[scry_library::EpisodeRelease {
                    media: Media {
                        key: MediaKey::new("test", MediaKind::Episode, "pilot"),
                        title: "Pilot".into(),
                        parent: Some(item.key.clone()),
                    },
                    released_at: Some(0),
                    season: Some(1),
                    episode: Some(1),
                }],
            )?;
        }
        db.set_watched(&item.key, true)?;
        assert!(db.list_library(false, kind, Page::default())?.is_empty());
        assert_eq!(
            db.list_library(true, kind, Page::default())?,
            vec![item.clone()]
        );
        db.set_watched(&item.key, false)?;
        assert_eq!(db.list_library(false, kind, Page::default())?, vec![item]);
        assert!(db.list_library(true, kind, Page::default())?.is_empty());
    }
    Ok(())
}

#[test]
fn automatically_watched_titles_are_excluded_before_to_watch_pagination() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let remaining = movie("remaining");
    let finished = movie("finished");
    for item in [&remaining, &finished] {
        db.upsert_media(item)?;
        db.add_to_watchlist(&item.key)?;
    }
    db.record_playback(&finished.key, 80.0, 100.0, 80.0)?;
    assert_eq!(
        db.list_library(false, MediaKind::Movie, Page::new(1, 0)?)?,
        vec![remaining]
    );
    assert_eq!(
        db.list_library(true, MediaKind::Movie, Page::default())?,
        vec![finished]
    );
    Ok(())
}

#[test]
fn calendar_includes_only_saved_movies_and_series() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let saved = movie("saved");
    let playing = movie("playing");
    let watched = movie("watched");
    let unrelated = movie("browsed");
    let show = Media {
        key: MediaKey::new("test", MediaKind::Series, "show"),
        title: "Show".into(),
        parent: None,
    };
    for item in [&saved, &playing, &watched, &unrelated, &show] {
        db.upsert_media(item)?;
    }
    db.add_to_watchlist(&saved.key)?;
    db.add_to_watchlist(&show.key)?;
    db.record_playback(&playing.key, 10.0, 100.0, 10.0)?;
    db.set_watched(&watched.key, true)?;
    let titles = db.list_calendar_titles()?;
    assert_eq!(titles, vec![show, saved]);
    Ok(())
}

#[test]
fn watched_time_is_independent_of_position_and_survives_reopen() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("library.db");
    let item = movie("one");
    {
        let mut db = Library::open(&path)?;
        db.upsert_media(&item)?;
        db.record_playback(&item.key, 95.0, 100.0, 0.0)?;
        assert!(!db.watched_state(&item.key)?.watched);
        db.record_playback(&item.key, 70.0, 100.0, 70.0)?;
        assert!(!db.watched_state(&item.key)?.watched);
    }
    let mut db = Library::open(path)?;
    db.record_playback(&item.key, 71.0, 100.0, 1.0)?;
    assert!(db.watched_state(&item.key)?.watched);
    assert_eq!(db.list_currently_watching(Page::default())?[0].0, item);
    db.finish_playback(&item.key, None)?;
    assert_eq!(db.list_currently_watching(Page::default())?.len(), 1);
    db.record_playback(&item.key, 91.0, 100.0, 20.0)?;
    db.finish_playback(&item.key, None)?;
    assert!(db.list_currently_watching(Page::default())?.is_empty());
    assert!(
        db.load_progress(&item.key)?
            .is_some_and(|p| p.position_seconds == 0.0)
    );
    Ok(())
}

#[test]
fn next_episode_replaces_finished_episode_but_not_other_series() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let show = Media {
        key: MediaKey::new("test", MediaKind::Series, "show"),
        title: "Show".into(),
        parent: None,
    };
    db.upsert_media(&show)?;
    let ep = |id: &str| Media {
        key: MediaKey::new("test", MediaKind::Episode, id),
        title: id.into(),
        parent: Some(show.key.clone()),
    };
    let first = ep("s1e1");
    let second = ep("s1e2");
    let other_show = Media {
        key: MediaKey::new("test", MediaKind::Series, "other-show"),
        title: "Other show".into(),
        parent: None,
    };
    let other = Media {
        key: MediaKey::new("test", MediaKind::Episode, "other-e1"),
        title: "Other episode".into(),
        parent: Some(other_show.key.clone()),
    };
    db.upsert_media(&other_show)?;
    db.upsert_media(&other)?;
    db.record_playback(&other.key, 5.0, 100.0, 5.0)?;
    db.register_release(&first, Some(0))?;
    db.record_playback(&first.key, 95.0, 100.0, 95.0)?;
    db.finish_playback(&first.key, Some(&second))?;
    let watching = db.list_currently_watching(Page::default())?;
    assert_eq!(watching.len(), 2);
    assert_eq!(watching[0].0, second);
    assert_eq!(watching[1].0, other);
    assert!(db.watched_state(&first.key)?.watched);
    db.set_watched(&second.key, false)?;
    assert_eq!(
        db.list_currently_watching(Page::default())?
            .into_iter()
            .map(|(m, _)| m)
            .collect::<Vec<_>>(),
        vec![other]
    );
    Ok(())
}

#[test]
fn type_filter_is_applied_before_pagination() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let film = movie("film");
    db.upsert_media(&film)?;
    db.add_to_watchlist(&film.key)?;
    let show = Media {
        key: MediaKey::new("test", MediaKind::Series, "show"),
        title: "Show".into(),
        parent: None,
    };
    db.upsert_media(&show)?;
    db.add_to_watchlist(&show.key)?;
    assert_eq!(
        db.list_library(false, MediaKind::Movie, Page::new(1, 0)?)?,
        vec![film]
    );
    assert_eq!(
        db.list_library(false, MediaKind::Series, Page::new(1, 0)?)?,
        vec![show]
    );
    Ok(())
}

#[test]
fn schema_initialization_preserves_saved_state_on_reopen() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("library.db");
    let item = movie("saved");
    {
        let mut db = Library::open(&path)?;
        db.upsert_media(&item)?;
        db.add_to_watchlist(&item.key)?;
        db.record_playback(&item.key, 50.0, 100.0, 50.0)?;
    }
    let mut db = Library::open(path)?;
    assert!(db.is_in_watchlist(&item.key)?);
    assert_eq!(db.list_currently_watching(Page::default())?[0].0, item);
    db.record_playback(&item.key, 80.0, 100.0, 30.0)?;
    assert!(db.watched_state(&item.key)?.watched);
    Ok(())
}

#[test]
fn invalid_playback_does_not_write_resume_state() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let item = movie("bad");
    db.upsert_media(&item)?;
    for (position, duration, delta) in [
        (f64::NAN, 100.0, 1.0),
        (10.0, 0.0, 1.0),
        (10.0, 100.0, -1.0),
    ] {
        assert!(
            db.record_playback(&item.key, position, duration, delta)
                .is_err()
        );
    }
    assert!(db.load_progress(&item.key)?.is_none());
    assert!(db.list_currently_watching(Page::default())?.is_empty());
    Ok(())
}

#[test]
fn advancing_preserves_next_episode_resume_and_does_not_reactivate_watched_episodes()
-> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let show = Media {
        key: MediaKey::new("test", MediaKind::Series, "show"),
        title: "Show".into(),
        parent: None,
    };
    db.upsert_media(&show)?;
    let first = Media {
        key: MediaKey::new("test", MediaKind::Episode, "first"),
        title: "First".into(),
        parent: Some(show.key.clone()),
    };
    let next = Media {
        key: MediaKey::new("test", MediaKind::Episode, "next"),
        title: "Next".into(),
        parent: Some(show.key.clone()),
    };
    db.register_release(&first, Some(0))?;
    db.register_release(&next, Some(0))?;
    db.record_playback(&next.key, 35.0, 100.0, 35.0)?;
    db.record_playback(&first.key, 95.0, 100.0, 80.0)?;
    db.finish_playback(&first.key, Some(&next))?;
    assert!(
        db.load_progress(&next.key)?
            .is_some_and(|progress| (progress.position_seconds - 35.0).abs() < 0.001)
    );
    assert_eq!(
        db.list_currently_watching(Page::default())?[0].0.key,
        next.key
    );
    db.set_watched(&next.key, true)?;
    db.record_playback(&first.key, 95.0, 100.0, 0.0)?;
    db.finish_playback(&first.key, Some(&next))?;
    assert!(db.list_currently_watching(Page::default())?.is_empty());
    assert!(db.load_progress(&next.key)?.is_none());
    Ok(())
}

#[test]
fn calendar_excludes_unsaved_shows_with_episode_watch_or_playback_state() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let show = Media {
        key: MediaKey::new("test", MediaKind::Series, "show"),
        title: "Show".into(),
        parent: None,
    };
    let episode = Media {
        key: MediaKey::new("test", MediaKind::Episode, "episode"),
        title: "Pilot".into(),
        parent: Some(show.key.clone()),
    };
    db.upsert_media_batch(&[show.clone(), episode.clone()])?;
    db.register_release(&episode, Some(0))?;
    db.record_playback(&episode.key, 10.0, 100.0, 10.0)?;
    assert!(db.list_calendar_titles()?.is_empty());
    db.set_watched(&episode.key, true)?;
    assert!(db.list_calendar_titles()?.is_empty());
    assert!(
        db.list_library(true, MediaKind::Series, Page::default())?
            .is_empty()
    );
    assert!(!db.is_in_watchlist(&show.key)?);
    Ok(())
}

#[test]
fn calendar_keeps_watched_saved_titles_until_removed() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    for kind in [MediaKind::Movie, MediaKind::Series] {
        let item = Media {
            key: MediaKey::new("test", kind, "saved"),
            title: "Saved".into(),
            parent: None,
        };
        db.upsert_media(&item)?;
        db.add_to_watchlist(&item.key)?;
        if kind == MediaKind::Series {
            db.register_release(
                &Media {
                    key: MediaKey::new("test", MediaKind::Episode, "saved-pilot"),
                    title: "Pilot".into(),
                    parent: Some(item.key.clone()),
                },
                Some(0),
            )?;
        }
        db.set_watched(&item.key, true)?;
        assert!(db.watched_state(&item.key)?.watched);
        assert_eq!(db.list_calendar_titles()?, vec![item.clone()]);
        db.remove_from_watchlist(&item.key)?;
        assert!(db.list_calendar_titles()?.is_empty());
    }
    Ok(())
}

#[test]
fn calendar_includes_all_saved_titles_without_a_page_limit() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let saved: Vec<_> = (0..205).map(|i| movie(&i.to_string())).collect();
    db.upsert_media_batch(&saved)?;
    for item in &saved {
        db.add_to_watchlist(&item.key)?;
    }
    let unsaved = movie("unsaved");
    db.upsert_media(&unsaved)?;
    db.set_watched(&unsaved.key, true)?;
    assert_eq!(
        db.list_calendar_titles()?,
        saved.into_iter().rev().collect::<Vec<_>>()
    );
    Ok(())
}

#[test]
fn watching_unsaved_titles_does_not_add_them_to_library() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let saved = movie("saved");
    let manual = movie("manual");
    let played = movie("played");
    for item in [&saved, &manual, &played] {
        db.upsert_media(item)?;
    }
    db.add_to_watchlist(&saved.key)?;
    db.set_watched(&saved.key, true)?;
    db.set_watched(&manual.key, true)?;
    db.record_playback(&played.key, 80.0, 100.0, 80.0)?;
    for item in [&manual, &played] {
        assert!(db.watched_state(&item.key)?.watched);
        assert!(!db.is_in_watchlist(&item.key)?);
    }
    assert_eq!(
        db.list_library(true, MediaKind::Movie, Page::new(1, 0)?)?,
        vec![saved.clone()]
    );
    assert!(
        db.list_library(true, MediaKind::Movie, Page::new(1, 1)?)?
            .is_empty()
    );
    assert!(
        db.list_library(false, MediaKind::Movie, Page::default())?
            .is_empty()
    );
    db.remove_from_watchlist(&saved.key)?;
    assert!(db.watched_state(&saved.key)?.watched);
    assert!(
        db.list_library(true, MediaKind::Movie, Page::default())?
            .is_empty()
    );
    Ok(())
}
