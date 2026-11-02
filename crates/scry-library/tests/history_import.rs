use scry_library::{
    HistoryEntry, HistoryImport, Library, Media, MediaKey, MediaKind, Page, Progress, StatsPeriod,
};

fn movie(id: &str) -> Media {
    Media {
        key: MediaKey::new("imdb", MediaKind::Movie, id),
        title: "Imported movie".into(),
        parent: None,
    }
}

fn batch(item: Media) -> HistoryImport {
    HistoryImport {
        entries: vec![HistoryEntry {
            media: item,
            watched: true,
            progress: None,
            active: false,
        }],
        ..HistoryImport::default()
    }
}

#[test]
fn imported_history_is_idempotent_without_saved_membership_or_playback_sessions()
-> anyhow::Result<()> {
    let mut library = Library::memory()?;
    let item = movie("tt100");
    let first = library.import_history(&batch(item.clone()))?;
    assert_eq!(first.watched, 1);
    assert!(
        library.media_states(std::slice::from_ref(&item.key))?[0]
            .watched
            .watched
    );
    assert!(
        library
            .list_library(false, None, Page::default())?
            .is_empty()
    );
    let second = library.import_history(&batch(item))?;
    assert_eq!(second.watched, 0);
    assert_eq!(second.unchanged, 1);
    let stats = library.nerd_stats(StatsPeriod::AllTime)?;
    assert_eq!(stats.session_count, 0);
    assert_eq!(stats.recorded_completions, 0);
    assert!(stats.recorded_seconds.abs() < f64::EPSILON);
    Ok(())
}

#[test]
fn import_preserves_explicit_unwatched_progress_and_metadata() -> anyhow::Result<()> {
    let mut library = Library::memory()?;
    let mut item = movie("tt101");
    item.title = "Local title".into();
    library.set_media_watched(&item, None, None, false)?;
    let result = library.import_history(&batch(movie("tt101")))?;
    assert_eq!(result.preserved, 1);
    assert!(
        !library.media_states(std::slice::from_ref(&item.key))?[0]
            .watched
            .watched
    );
    assert_eq!(
        library.get_media(&item.key)?.map(|m| m.title),
        Some("Local title".into())
    );

    let playing = movie("tt102");
    library.record_media_playback(&playing, 50.0, 100.0, 5.0)?;
    library.import_history(&batch(playing.clone()))?;
    let state = library.media_states(&[playing.key])?.remove(0);
    assert_eq!(state.progress, Progress::new(50.0, 100.0));
    assert!(!state.watched.watched);
    Ok(())
}

#[test]
fn resume_import_creates_continue_watching_without_inventing_watched_time() -> anyhow::Result<()> {
    let mut library = Library::memory()?;
    let item = movie("tt103");
    let input = HistoryImport {
        entries: vec![HistoryEntry {
            media: item.clone(),
            watched: false,
            progress: Progress::new(40.0, 100.0),
            active: true,
        }],
        ..HistoryImport::default()
    };
    let result = library.import_history(&input)?;
    assert_eq!(result.progress, 1);
    let watching = library.list_currently_watching(Page::default())?;
    assert_eq!(watching.len(), 1);
    assert_eq!(watching[0].0.key, item.key);
    let state = library.media_states(&[item.key])?.remove(0);
    assert!(state.watched_seconds.abs() < f64::EPSILON);
    assert!(!state.watched.watched);
    assert_eq!(library.import_history(&input)?.unchanged, 1);
    Ok(())
}

#[test]
fn invalid_batch_rolls_back_all_history() -> anyhow::Result<()> {
    let mut library = Library::memory()?;
    let mut input = batch(movie("tt104"));
    let mut invalid = movie("tt105");
    invalid.title.clear();
    input.entries.extend(batch(invalid).entries);
    assert!(library.import_history(&input).is_err());
    assert!(library.get_media(&movie("tt104").key)?.is_none());
    Ok(())
}

#[test]
fn release_guard_skips_only_watched_flag_and_preserves_valid_resume() -> anyhow::Result<()> {
    let mut library = Library::memory()?;
    let parent = Media {
        key: MediaKey::new("imdb", MediaKind::Series, "tt106"),
        title: "Sample show".into(),
        parent: None,
    };
    let episode = Media {
        key: MediaKey::new("imdb", MediaKind::Episode, "tt106:1:1"),
        title: "Episode one".into(),
        parent: Some(parent.key.clone()),
    };
    let input = HistoryImport {
        parents: vec![parent],
        inventory: vec![scry_library::EpisodeRelease {
            media: episode.clone(),
            season: Some(1),
            episode: Some(1),
            released_at: Some(4_102_444_800),
        }],
        entries: vec![HistoryEntry {
            media: episode.clone(),
            watched: true,
            progress: Progress::new(10.0, 100.0),
            active: true,
        }],
    };
    let result = library.import_history(&input)?;
    assert_eq!(result.ineligible, 1);
    assert_eq!(result.watched, 0);
    assert_eq!(result.progress, 1);
    assert_eq!(result.notices.len(), 1);
    let state = library.media_states(&[episode.key])?.remove(0);
    assert!(!state.watched.watched);
    assert_eq!(state.progress, Progress::new(10.0, 100.0));
    Ok(())
}

#[test]
fn episode_history_survives_reopen_without_marking_unseen_episodes_watched() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("history.sqlite3");
    let parent = Media {
        key: MediaKey::new("imdb", MediaKind::Series, "tt107"),
        title: "Sample show".into(),
        parent: None,
    };
    let episodes: Vec<_> = (1..=3)
        .map(|n| scry_library::EpisodeRelease {
            media: Media {
                key: MediaKey::new("imdb", MediaKind::Episode, format!("tt107:1:{n}")),
                title: format!("Episode {n}"),
                parent: Some(parent.key.clone()),
            },
            season: Some(1),
            episode: Some(n),
            released_at: Some(1_577_836_800),
        })
        .collect();
    {
        let mut library = Library::open(&path)?;
        library.import_history(&HistoryImport {
            parents: vec![parent.clone()],
            inventory: episodes.clone(),
            entries: vec![HistoryEntry {
                media: episodes[0].media.clone(),
                watched: true,
                progress: None,
                active: false,
            }],
        })?;
    }
    let library = Library::open(path)?;
    let summary = library.series_progress(&parent.key, 1_700_000_000)?;
    assert_eq!(summary.total_episodes, 3);
    assert_eq!(summary.watched_episodes, 1);
    assert!(!library.series_poster_state(&parent.key, 1_700_000_000)?.0);
    assert_eq!(library.nerd_stats(StatsPeriod::AllTime)?.session_count, 0);
    Ok(())
}

#[test]
fn importing_another_episode_does_not_replace_or_duplicate_the_local_resume_choice()
-> anyhow::Result<()> {
    let mut library = Library::memory()?;
    let parent = Media {
        key: MediaKey::new("imdb", MediaKind::Series, "tt108"),
        title: "Sample show".into(),
        parent: None,
    };
    let make = |number| HistoryImport {
        parents: vec![parent.clone()],
        entries: vec![HistoryEntry {
            media: Media {
                key: MediaKey::new("imdb", MediaKind::Episode, format!("tt108:1:{number}")),
                title: format!("Episode {number}"),
                parent: Some(parent.key.clone()),
            },
            watched: false,
            progress: Progress::new(10.0, 100.0),
            active: true,
        }],
        ..HistoryImport::default()
    };
    library.import_history(&make(1))?;
    let result = library.import_history(&make(2))?;
    let watching = library.list_currently_watching(Page::default())?;
    assert_eq!(watching.len(), 1);
    assert_eq!(watching[0].0.key.id, "tt108:1:1");
    assert_eq!(
        result.progress, 1,
        "The other episode's resume point can still be stored"
    );
    assert_eq!(result.preserved, 1);
    assert!(result.notices[0].reason.contains("Continue Watching"));
    library.dismiss_currently_watching(&watching[0].0.key)?;
    library.import_history(&make(3))?;
    assert!(
        library.list_currently_watching(Page::default())?.is_empty(),
        "An import must not undo the local dismissal"
    );
    Ok(())
}
