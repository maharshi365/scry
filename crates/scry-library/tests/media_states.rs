use scry_library::{Library, Media, MediaKey, MediaKind};

#[test]
fn batched_states_preserve_order_namespaces_missing_keys_and_playback_semantics()
-> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let movie = Media {
        key: MediaKey::new("test", MediaKind::Movie, "same"),
        title: "Movie".into(),
        parent: None,
    };
    db.upsert_media(&movie)?;
    db.record_playback(&movie.key, 95.0, 100.0, 10.0)?;
    let missing = MediaKey::new("other", MediaKind::Movie, "same");
    let keys = vec![missing.clone(), movie.key.clone(), movie.key.clone()];
    let states = db.media_states(&keys)?;
    assert_eq!(
        states
            .iter()
            .map(|state| state.key.clone())
            .collect::<Vec<_>>(),
        keys
    );
    assert!(!states[0].stored);
    assert!(!states[0].watched.watched);
    assert!(states[0].progress.is_none());
    assert!(states[1].stored);
    assert!(!states[1].watched.watched);
    assert!((states[1].watched_seconds - 10.0).abs() < 0.001);
    assert!(
        states[1]
            .progress
            .is_some_and(|progress| (progress.position_seconds - 95.0).abs() < 0.001)
    );
    assert_eq!(states[1], states[2]);
    assert!(db.media_states(&[])?.is_empty());
    assert!(db.media_states(&vec![missing; 201]).is_err());
    Ok(())
}

#[test]
fn metadata_batches_roll_back_when_an_episode_is_invalid() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let parent = Media {
        key: MediaKey::new("test", MediaKind::Series, "show"),
        title: "Show".into(),
        parent: None,
    };
    let invalid = Media {
        key: MediaKey::new("test", MediaKind::Episode, "episode"),
        title: String::new(),
        parent: Some(parent.key.clone()),
    };
    assert!(db.upsert_media_batch(&[parent.clone(), invalid]).is_err());
    assert!(db.get_media(&parent.key)?.is_none());
    Ok(())
}

#[test]
fn invalid_samples_roll_back_metadata_registration() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let movie = Media {
        key: MediaKey::new("test", MediaKind::Movie, "new"),
        title: "New".into(),
        parent: None,
    };
    assert!(db.record_media_playback(&movie, 10.0, 0.0, 10.0).is_err());
    assert!(db.get_media(&movie.key)?.is_none());
    Ok(())
}

#[test]
fn detail_snapshot_combines_selected_state_parent_watchlist_and_episode_states()
-> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let parent = Media {
        key: MediaKey::new("test", MediaKind::Series, "show"),
        title: "Show".into(),
        parent: None,
    };
    let first = Media {
        key: MediaKey::new("test", MediaKind::Episode, "e1"),
        title: "Episode 1".into(),
        parent: Some(parent.key.clone()),
    };
    let second = Media {
        key: MediaKey::new("test", MediaKind::Episode, "e2"),
        title: "Episode 2".into(),
        parent: Some(parent.key.clone()),
    };
    db.upsert_media_batch(&[parent.clone(), first.clone(), second.clone()])?;
    db.add_to_watchlist(&parent.key)?;
    db.register_release(&first, Some(0))?;
    db.set_watched(&first.key, true)?;
    db.save_progress(&second.key, 30.0, 120.0)?;

    let snapshot = db.detail_snapshot(Some(&second.key), &[])?;
    let selected = snapshot
        .selected_state
        .ok_or_else(|| anyhow::anyhow!("missing selected episode state"))?;
    assert_eq!(selected.key, second.key);
    assert!(!selected.watched.watched);
    assert!(selected.watchlist);
    assert_eq!(
        selected
            .progress
            .map(|p| (p.position_seconds, p.duration_seconds)),
        Some((30.0, 120.0))
    );
    assert_eq!(
        snapshot.episode_states,
        vec![
            (
                first.key.clone(),
                scry_library::WatchedState { watched: true }
            ),
            (
                second.key.clone(),
                scry_library::WatchedState { watched: false }
            ),
        ]
    );

    let unknown = MediaKey::new("test", MediaKind::Episode, "e3");
    let custom = db.detail_snapshot(None, &[unknown.clone(), first.key.clone()])?;
    assert!(custom.selected_state.is_none());
    assert_eq!(
        custom.episode_states,
        vec![
            (
                unknown.clone(),
                scry_library::WatchedState { watched: false }
            ),
            (first.key, scry_library::WatchedState { watched: true }),
        ]
    );
    assert!(db.detail_snapshot(None, &vec![unknown; 201]).is_err());
    Ok(())
}

#[test]
fn failed_finish_playback_next_episode_rolls_back_current_completion() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let parent = Media {
        key: MediaKey::new("test", MediaKind::Series, "show"),
        title: "Show".into(),
        parent: None,
    };
    let first = Media {
        key: MediaKey::new("test", MediaKind::Episode, "e1"),
        title: "Episode 1".into(),
        parent: Some(parent.key.clone()),
    };
    db.upsert_media_batch(&[parent.clone(), first.clone()])?;
    db.register_release(&first, Some(0))?;
    db.record_playback(&first.key, 95.0, 100.0, 80.0)?;

    let invalid_next = Media {
        key: MediaKey::new("test", MediaKind::Episode, "e2"),
        title: "   ".into(),
        parent: Some(parent.key),
    };
    assert!(db.finish_playback(&first.key, Some(&invalid_next)).is_err());
    assert_eq!(
        db.load_progress(&first.key)?.map(|p| p.position_seconds),
        Some(95.0)
    );
    assert_eq!(
        db.list_currently_watching(scry_library::Page::default())?
            .into_iter()
            .map(|(media, _)| media.key)
            .collect::<Vec<_>>(),
        vec![first.key]
    );
    assert!(db.get_media(&invalid_next.key)?.is_none());
    Ok(())
}
