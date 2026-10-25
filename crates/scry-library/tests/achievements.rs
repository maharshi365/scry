use scry_library::{Library, Media, MediaKey, MediaKind, StatsPeriod};

fn movie() -> Media {
    Media {
        key: MediaKey::new("test", MediaKind::Movie, "one"),
        title: "One".into(),
        parent: None,
    }
}

#[test]
fn watched_titles_count_known_runtime_without_double_counting_playback() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let item = movie();
    db.upsert_media(&item)?;
    db.set_known_runtimes(&[(item.key.clone(), 100.0)])?;
    db.set_watched(&item.key, true)?;
    db.save_progress(&item.key, 90.0, 100.0)?;
    let stats = db.nerd_stats(StatsPeriod::AllTime)?;
    assert_eq!(stats.movies_watched, 1);
    assert!((stats.watch_seconds - 100.0).abs() < f64::EPSILON);
    db.record_playback(&item.key, 95.0, 100.0, 5.0)?;
    db.set_watched(&item.key, false)?;
    let stats = db.nerd_stats(StatsPeriod::AllTime)?;
    assert!((stats.watch_seconds - 5.0).abs() < f64::EPSILON);
    assert_eq!(stats.movies_watched, 0);
    assert_eq!(stats.top_titles.len(), 1);
    Ok(())
}

#[test]
fn unknown_runtime_is_counted_when_it_becomes_known_and_manual_hours_unlock_achievements()
-> anyhow::Result<()> {
    let mut db = Library::memory()?;
    db.evaluate_achievements()?;
    let item = movie();
    db.upsert_media(&item)?;
    db.set_watched(&item.key, true)?;
    assert!(db.nerd_stats(StatsPeriod::AllTime)?.watch_seconds.abs() < f64::EPSILON);
    db.set_known_runtimes(&[(item.key.clone(), 90_000.0)])?;
    assert!((db.nerd_stats(StatsPeriod::AllTime)?.watch_seconds - 90_000.0).abs() < f64::EPSILON);
    assert!(
        db.evaluate_achievements()?
            .newly_unlocked
            .iter()
            .any(|entry| entry.id == "a-day-well-spent")
    );
    db.set_watched(&item.key, true)?;
    assert!((db.nerd_stats(StatsPeriod::AllTime)?.watch_seconds - 90_000.0).abs() < f64::EPSILON);
    Ok(())
}

#[test]
fn catch_up_is_grouped_once_and_unlocks_survive_reopen_and_unwatch() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("stats.sqlite3");
    let item = movie();
    {
        let mut db = Library::open(&path)?;
        db.upsert_media(&item)?;
        db.record_playback(&item.key, 90_000.0, 100_000.0, 90_000.0)?;
        let result = db.evaluate_achievements()?;
        let unlocked = result.newly_unlocked;
        assert!(
            unlocked
                .iter()
                .any(|entry| entry.id == "a-day-well-spent" && entry.historical)
        );
        assert!(db.evaluate_achievements()?.newly_unlocked.is_empty());
        db.set_watched(&item.key, false)?;
    }
    let mut db = Library::open(&path)?;
    let result = db.evaluate_achievements()?;
    assert!(result.newly_unlocked.is_empty());
    assert!(
        result
            .entries
            .iter()
            .any(|entry| entry.id == "a-day-well-spent" && entry.unlocked)
    );
    assert!((db.nerd_stats(StatsPeriod::AllTime)?.watch_seconds - 90_000.0).abs() < f64::EPSILON);
    Ok(())
}

#[test]
fn future_unlocks_are_live_and_rewatches_require_new_playback() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    db.evaluate_achievements()?;
    let item = movie();
    db.upsert_media(&item)?;
    for _ in 0..3 {
        db.record_playback(&item.key, 95.0, 100.0, 75.0)?;
        // Repeated samples within a run must not create more completions.
        db.record_playback(&item.key, 96.0, 100.0, 1.0)?;
        db.finish_playback(&item.key, None)?;
    }
    let result = db.evaluate_achievements()?;
    assert!(
        result
            .newly_unlocked
            .iter()
            .any(|entry| entry.id == "comfort-zone" && !entry.historical)
    );
    assert_eq!(db.nerd_stats(StatsPeriod::AllTime)?.recorded_completions, 3);
    Ok(())
}

#[test]
fn failed_samples_do_not_change_stats() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let item = movie();
    db.upsert_media(&item)?;
    for delta in [-1.0, f64::NAN, f64::INFINITY] {
        assert!(db.record_playback(&item.key, 50.0, 100.0, delta).is_err());
    }
    assert!(db.nerd_stats(StatsPeriod::AllTime)?.top_titles.is_empty());
    Ok(())
}

#[test]
fn played_and_manually_watched_titles_use_the_same_runtime_credit() -> anyhow::Result<()> {
    let mut manual = Library::memory()?;
    let mut played = Library::memory()?;
    let item = movie();
    for db in [&mut manual, &mut played] {
        db.upsert_media(&item)?;
        db.set_known_runtimes(&[(item.key.clone(), 100.0)])?;
    }
    manual.set_watched(&item.key, true)?;
    played.record_playback(&item.key, 80.0, 100.0, 80.0)?;
    let manual = manual.nerd_stats(StatsPeriod::AllTime)?;
    let played = played.nerd_stats(StatsPeriod::AllTime)?;
    assert!((manual.watch_seconds - played.watch_seconds).abs() < f64::EPSILON);
    assert_eq!(manual.movies_watched, played.movies_watched);
    Ok(())
}

#[test]
fn invalid_runtime_batch_cannot_partially_change_watch_hours() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let item = movie();
    db.upsert_media(&item)?;
    db.set_watched(&item.key, true)?;
    for bad in [-1.0, 0.0, f64::NAN, f64::INFINITY] {
        assert!(
            db.set_known_runtimes(&[(item.key.clone(), 100.0), (item.key.clone(), bad)])
                .is_err()
        );
        assert!(db.nerd_stats(StatsPeriod::AllTime)?.watch_seconds.abs() < f64::EPSILON);
    }
    Ok(())
}

#[test]
fn dashboard_library_completion_counts_only_saved_titles() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let watched = movie();
    db.upsert_media(&watched)?;
    db.set_watched(&watched.key, true)?;
    db.add_to_watchlist(&watched.key)?;
    for (id, saved) in [("waiting", true), ("outside-library", false)] {
        let mut item = movie();
        item.key.id = id.into();
        db.upsert_media(&item)?;
        if saved {
            db.add_to_watchlist(&item.key)?;
        } else {
            db.set_watched(&item.key, true)?;
        }
    }
    let stats = db.nerd_stats(StatsPeriod::AllTime)?;
    assert_eq!(stats.saved_titles, 2);
    assert_eq!(stats.saved_watched, 1);
    assert_eq!(stats.movies_watched, 2);
    Ok(())
}
