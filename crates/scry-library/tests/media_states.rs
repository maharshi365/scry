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
