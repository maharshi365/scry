use scry_library::{EpisodeRelease, Library, Media, MediaKey, MediaKind, Page};

fn item(kind: MediaKind, id: &str, parent: Option<MediaKey>) -> Media {
    Media {
        key: MediaKey::new("test", kind, id),
        title: id.into(),
        parent,
    }
}

#[test]
fn next_items_use_saved_membership_release_dates_watched_flags_and_item_progress()
-> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let show = item(MediaKind::Series, "show", None);
    let movie = item(MediaKind::Movie, "movie", None);
    let unsaved = item(MediaKind::Movie, "unsaved", None);
    for media in [&show, &movie, &unsaved] {
        db.upsert_media(media)?;
    }
    db.add_to_watchlist(&show.key)?;
    db.add_to_watchlist(&movie.key)?;
    let releases: Vec<_> = (1..=4)
        .map(|index| EpisodeRelease {
            media: item(
                MediaKind::Episode,
                &format!("episode-{index}"),
                Some(show.key.clone()),
            ),
            season: Some(1),
            episode: Some(index),
            released_at: Some(if index == 4 { 200 } else { 100 }),
        })
        .collect();
    db.sync_episode_releases(&show.key, &releases)?;
    db.set_watched(&releases[0].media.key, true)?;
    db.record_playback(&releases[1].media.key, 25.0, 100.0, 0.0)?;
    db.record_playback(&releases[2].media.key, 50.0, 100.0, 0.0)?;
    db.record_playback(&movie.key, 20.0, 200.0, 0.0)?;
    let next = db.list_up_next(150, Page::default())?;
    assert_eq!(
        next.iter()
            .map(|(media, _)| media.key.clone())
            .collect::<Vec<_>>(),
        vec![movie.key.clone(), releases[1].media.key.clone()]
    );
    assert_eq!(
        next[1].1,
        Some(scry_library::Progress {
            position_seconds: 25.0,
            duration_seconds: 100.0
        })
    );
    db.set_watched(&show.key, true)?;
    assert_eq!(db.list_up_next(250, Page::default())?.len(), 1);
    db.remove_from_watchlist(&movie.key)?;
    assert!(db.list_up_next(250, Page::default())?.is_empty());
    Ok(())
}

#[test]
fn next_items_cover_all_episode_pages_and_survive_reopening() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("library.sqlite3");
    let show = item(MediaKind::Series, "show", None);
    let releases: Vec<_> = (1..=250)
        .map(|index| EpisodeRelease {
            media: item(
                MediaKind::Episode,
                &format!("episode-{index}"),
                Some(show.key.clone()),
            ),
            season: Some(1),
            episode: Some(index),
            released_at: Some(100),
        })
        .collect();
    {
        let mut db = Library::open(&path)?;
        db.upsert_media(&show)?;
        db.add_to_watchlist(&show.key)?;
        assert!(db.sync_episode_releases(&show.key, &releases)?);
        assert!(!db.sync_episode_releases(&show.key, &releases)?);
        db.set_watched(&releases[0].media.key, true)?;
    }
    let db = Library::open(&path)?;
    let first = db.list_up_next(150, Page::new(200, 0)?)?;
    let second = db.list_up_next(150, Page::new(200, 200)?)?;
    assert_eq!(first.len(), 1);
    assert!(second.is_empty());
    assert_eq!(first[0].0.key, releases[1].media.key);
    let mut db = db;
    db.set_watched(&releases[1].media.key, true)?;
    assert_eq!(
        db.list_up_next(150, Page::default())?[0].0.key,
        releases[2].media.key
    );
    Ok(())
}

#[test]
fn invalid_release_inventories_leave_existing_metadata_and_progress_unchanged() -> anyhow::Result<()>
{
    let mut db = Library::memory()?;
    let show = item(MediaKind::Series, "show", None);
    db.upsert_media(&show)?;
    db.add_to_watchlist(&show.key)?;
    let release = EpisodeRelease {
        media: item(MediaKind::Episode, "episode", Some(show.key.clone())),
        season: Some(1),
        episode: Some(1),
        released_at: Some(100),
    };
    db.sync_episode_releases(&show.key, std::slice::from_ref(&release))?;
    db.record_playback(&release.media.key, 25.0, 100.0, 0.0)?;
    let mut invalid = release.clone();
    invalid.media.parent = Some(MediaKey::new("test", MediaKind::Series, "different-show"));
    assert!(db.sync_episode_releases(&show.key, &[invalid]).is_err());
    assert!(
        db.sync_episode_releases(&show.key, &[release.clone(), release.clone()])
            .is_err()
    );
    let before = db.list_up_next(150, Page::default())?;
    assert_eq!(before.len(), 1);
    assert_eq!(
        before[0].1,
        Some(scry_library::Progress {
            position_seconds: 25.0,
            duration_seconds: 100.0
        })
    );
    let mut invalid_title = release.clone();
    invalid_title.media.title.clear();
    assert!(
        db.sync_episode_releases(&show.key, &[invalid_title])
            .is_err()
    );
    assert_eq!(db.list_up_next(150, Page::default())?, before);
    assert!(db.sync_episode_releases(&show.key, &[])?);
    assert!(db.list_up_next(150, Page::default())?.is_empty());
    assert!(db.load_progress(&release.media.key)?.is_some());
    Ok(())
}
#[test]
fn next_episodes_require_a_passed_known_date_even_for_specials() -> anyhow::Result<()> {
    for season in [Some(0), Some(1), None] {
        let mut db = Library::memory()?;
        let show = Media {
            key: MediaKey::new("test", MediaKind::Series, "show"),
            title: "Show".into(),
            parent: None,
        };
        db.upsert_media(&show)?;
        db.add_to_watchlist(&show.key)?;
        let releases = [None, Some(200), Some(100)]
            .into_iter()
            .enumerate()
            .map(|(index, released_at)| EpisodeRelease {
                media: Media {
                    key: MediaKey::new("test", MediaKind::Episode, format!("episode-{index}")),
                    title: format!("Episode {index}"),
                    parent: Some(show.key.clone()),
                },
                released_at,
                season,
                episode: None,
            })
            .collect::<Vec<_>>();
        db.sync_episode_releases(&show.key, &releases)?;
        db.record_playback(&releases[0].media.key, 25.0, 100.0, 25.0)?;
        let next = db.list_up_next(100, Page::default())?;
        assert_eq!(next.len(), 1);
        assert_eq!(next[0].0.key, releases[2].media.key);
        db.set_watched(&releases[2].media.key, true)?;
        assert!(db.list_up_next(100, Page::default())?.is_empty());
        let next = db.list_up_next(200, Page::default())?;
        assert_eq!(next.len(), 1);
        assert_eq!(next[0].0.key, releases[1].media.key);
        db.set_watched(&releases[1].media.key, true)?;
        assert!(db.list_up_next(200, Page::default())?.is_empty());
    }
    Ok(())
}

#[test]
fn removing_a_large_inventory_clears_every_release_but_retains_episode_metadata()
-> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let show = item(MediaKind::Series, "large-removal", None);
    db.upsert_media(&show)?;
    let releases: Vec<_> = (0..1_101)
        .map(|index| EpisodeRelease {
            media: item(
                MediaKind::Episode,
                &format!("large-{index}"),
                Some(show.key.clone()),
            ),
            season: Some(1),
            episode: Some(index),
            released_at: Some(100),
        })
        .collect();
    db.sync_episode_releases(&show.key, &releases)?;
    db.record_playback(&releases[1_100].media.key, 25.0, 100.0, 0.0)?;
    assert!(db.sync_episode_releases(&show.key, &[])?);
    assert!(!db.sync_episode_releases(&show.key, &[])?);
    assert_eq!(db.series_progress(&show.key, 150)?.total_episodes, 0);
    assert!(db.get_media(&releases[1_100].media.key)?.is_some());
    assert_eq!(
        db.load_progress(&releases[1_100].media.key)?
            .map(|p| p.position_seconds),
        Some(25.0)
    );
    Ok(())
}

#[test]
fn refresh_applies_mixed_changes_without_losing_identity_or_resume_state() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let show = item(MediaKind::Series, "refresh", None);
    db.upsert_media(&show)?;
    db.add_to_watchlist(&show.key)?;
    let mut releases: Vec<_> = (0..4)
        .map(|index| EpisodeRelease {
            media: item(
                MediaKind::Episode,
                &format!("refresh-{index}"),
                Some(show.key.clone()),
            ),
            season: Some(1),
            episode: Some(index),
            released_at: Some(100),
        })
        .collect();
    // The same external ID in another namespace is a distinct episode.
    releases[3].media.key.id = releases[0].media.key.id.clone();
    releases[3].media.key.namespace = "other".into();
    db.sync_episode_releases(&show.key, &releases)?;
    db.record_playback(&releases[0].media.key, 25.0, 100.0, 0.0)?;
    let removed = releases.remove(1);
    releases[1].media.title = "Renamed".into();
    releases[1].released_at = Some(200);
    let mut added = releases[0].clone();
    added.media.key.id = "new".into();
    added.episode = Some(4);
    releases.push(added.clone());
    assert!(db.sync_episode_releases(&show.key, &releases)?);
    assert!(!db.sync_episode_releases(&show.key, &releases)?);
    assert_eq!(
        db.get_media(&releases[1].media.key)?,
        Some(releases[1].media.clone())
    );
    assert_eq!(db.get_media(&added.media.key)?, Some(added.media.clone()));
    assert_eq!(
        db.load_progress(&releases[0].media.key)?
            .map(|p| p.position_seconds),
        Some(25.0)
    );
    db.set_watched(&releases[0].media.key, true)?;
    assert_eq!(
        db.list_up_next(150, Page::default())?[0].0.key,
        releases[2].media.key
    );
    db.set_watched(&releases[2].media.key, true)?;
    assert_eq!(
        db.list_up_next(150, Page::default())?[0].0.key,
        added.media.key
    );
    assert!(
        db.get_media(&removed.media.key)?.is_some(),
        "removed inventory retains metadata"
    );
    Ok(())
}
