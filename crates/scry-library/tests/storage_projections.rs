use anyhow::{Context, Result};
use diesel::{connection::SimpleConnection, prelude::*, sql_types::Double};
use scry_library::{EpisodeRelease, Library, Media, MediaKey, MediaKind, StatsPeriod};

fn movie() -> Media {
    Media {
        key: MediaKey::new("test", MediaKind::Movie, "movie"),
        title: "Movie".into(),
        parent: None,
    }
}

#[derive(QueryableByName)]
struct Seconds {
    #[diesel(sql_type = Double)]
    seconds: f64,
}

#[test]
fn persisted_totals_follow_watches_runtime_and_recorded_time_across_connections() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("app.sqlite3");
    let mut writer = Library::open(&path)?;
    let reader = Library::open(&path)?;
    let mut raw = SqliteConnection::establish(path.to_str().context("fixture path")?)?;
    let item = movie();
    writer.upsert_media(&item)?;
    writer.set_known_runtimes(&[(item.key.clone(), 100.0)])?;
    for (watched, expected) in [(true, 100.0), (true, 100.0), (false, 0.0)] {
        writer.set_watched(&item.key, watched)?;
        let stored =
            diesel::sql_query("SELECT movie_seconds AS seconds FROM watch_totals WHERE id=1")
                .get_result::<Seconds>(&mut raw)?;
        assert!((stored.seconds - expected).abs() < f64::EPSILON);
        assert!(
            (reader.nerd_stats(StatsPeriod::AllTime)?.watch_seconds - expected).abs()
                < f64::EPSILON
        );
    }
    writer.record_playback(&item.key, 80.0, 100.0, 80.0)?;
    writer.set_watched(&item.key, false)?;
    drop(writer);
    let reader = Library::open(path)?;
    assert!((reader.nerd_stats(StatsPeriod::AllTime)?.watch_seconds - 80.0).abs() < f64::EPSILON);
    Ok(())
}

#[test]
fn failed_bulk_write_rolls_back_sources_summaries_and_achievement_invalidation() -> Result<()> {
    let mut library = Library::memory()?;
    let item = movie();
    library.upsert_media(&item)?;
    library.set_known_runtimes(&[(item.key.clone(), 100.0)])?;
    library.set_watched(&item.key, true)?;
    let mut invalid = item.clone();
    invalid.key.id = "invalid".into();
    invalid.title.clear();
    let added = Media {
        key: MediaKey::new("test", MediaKind::Movie, "added"),
        ..item.clone()
    };
    assert!(
        library
            .upsert_media_batch(&[
                added.clone(),
                Media {
                    title: "Changed".into(),
                    ..item.clone()
                },
                invalid
            ])
            .is_err()
    );
    assert_eq!(library.get_media(&item.key)?.as_ref(), Some(&item));
    assert!(library.get_media(&added.key)?.is_none());
    assert!((library.nerd_stats(StatsPeriod::AllTime)?.watch_seconds - 100.0).abs() < f64::EPSILON);
    Ok(())
}

#[test]
fn version_eight_upgrade_rebuilds_summaries_without_changing_history() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("app.sqlite3");
    {
        let mut raw = SqliteConnection::establish(path.to_str().context("fixture path")?)?;
        for sql in [
            include_str!("../src/storage/schema.sql"),
            "CREATE VIEW effective_watched AS SELECT m.media_id,COALESCE(w.watched,0) AS watched FROM media m LEFT JOIN watched w USING(media_id);",
            include_str!("../src/stats/schema.sql"),
            include_str!("../src/guide/schema.sql"),
            include_str!("../src/playback/skip_segments.sql"),
            include_str!("../src/playback/saved_streams.sql"),
            "INSERT INTO media VALUES (1,'test','movie','movie','Movie',NULL); INSERT INTO watched VALUES(1,1); INSERT INTO media_runtime VALUES(1,100,NULL); INSERT INTO stats_titles(media_id,seconds) VALUES(1,30); PRAGMA user_version=8;",
        ] {
            raw.batch_execute(sql)?;
        }
    }
    for _ in 0..2 {
        let library = Library::open(&path)?;
        let stats = library.nerd_stats(StatsPeriod::AllTime)?;
        assert!((stats.watch_seconds - 100.0).abs() < f64::EPSILON);
        assert!((stats.recorded_seconds - 30.0).abs() < f64::EPSILON);
        assert!(
            stats
                .top_titles
                .first()
                .is_some_and(|title| (title.seconds - 100.0).abs() < f64::EPSILON)
        );
        assert!(library.nerd_stats(StatsPeriod::Week)?.watch_seconds.abs() < f64::EPSILON);
        assert!(library.watched_state(&movie().key)?.watched);
    }
    let mut raw = SqliteConnection::establish(path.to_str().context("fixture path")?)?;
    let stored = diesel::sql_query("SELECT movie_seconds AS seconds FROM watch_totals WHERE id=1")
        .get_result::<Seconds>(&mut raw)?;
    assert!((stored.seconds - 100.0).abs() < f64::EPSILON);
    Ok(())
}

#[test]
fn failed_schema_upgrade_keeps_the_old_schema_and_source_data() -> Result<()> {
    #[derive(QueryableByName)]
    struct Value {
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        value: i64,
    }
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("app.sqlite3");
    let mut raw = SqliteConnection::establish(path.to_str().context("fixture path")?)?;
    for sql in [
        include_str!("../src/storage/schema.sql"),
        "CREATE VIEW effective_watched AS SELECT m.media_id,COALESCE(w.watched,0) AS watched FROM media m LEFT JOIN watched w USING(media_id);",
        include_str!("../src/stats/schema.sql"),
        include_str!("../src/guide/schema.sql"),
        include_str!("../src/playback/skip_segments.sql"),
        include_str!("../src/playback/saved_streams.sql"),
        "PRAGMA foreign_keys=OFF; INSERT INTO media_runtime VALUES(999,100,NULL); PRAGMA user_version=8;",
    ] {
        raw.batch_execute(sql)?;
    }
    assert!(Library::open(&path).is_err());
    for (query, expected) in [
        ("SELECT user_version AS value FROM pragma_user_version", 8),
        (
            "SELECT COUNT(*) AS value FROM sqlite_schema WHERE name='watch_totals'",
            0,
        ),
        (
            "SELECT COUNT(*) AS value FROM media_runtime WHERE media_id=999",
            1,
        ),
    ] {
        assert_eq!(
            diesel::sql_query(query)
                .get_result::<Value>(&mut raw)?
                .value,
            expected
        );
    }
    Ok(())
}

#[test]
fn completion_projection_tracks_inventory_changes_and_specials() -> Result<()> {
    let mut library = Library::memory()?;
    let show = Media {
        key: MediaKey::new("test", MediaKind::Series, "show"),
        title: "Show".into(),
        parent: None,
    };
    library.upsert_media(&show)?;
    let episode = |id: &str, season, released_at| EpisodeRelease {
        media: Media {
            key: MediaKey::new("test", MediaKind::Episode, id),
            title: id.into(),
            parent: Some(show.key.clone()),
        },
        released_at,
        season: Some(season),
        episode: Some(1),
    };
    let regular = episode("regular", 1, Some(0));
    let special = episode("special", 0, None);
    library.sync_episode_releases(&show.key, &[regular.clone(), special.clone()])?;
    library.set_watched(&regular.media.key, true)?;
    assert!(library.watched_state(&show.key)?.watched);
    let future = episode("future", 2, Some(i64::MAX));
    library.sync_episode_releases(&show.key, &[regular.clone(), special.clone(), future])?;
    assert!(!library.watched_state(&show.key)?.watched);
    library.sync_episode_releases(&show.key, &[regular, special])?;
    assert!(library.watched_state(&show.key)?.watched);
    Ok(())
}

#[test]
fn achievement_cache_observes_commits_from_another_connection() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("app.sqlite3");
    let mut reader = Library::open(&path)?;
    let mut writer = Library::open(path)?;
    reader.evaluate_achievements()?;
    let item = movie();
    writer.upsert_media(&item)?;
    writer.set_known_runtimes(&[(item.key.clone(), 90_000.0)])?;
    writer.set_watched(&item.key, true)?;
    let result = reader.evaluate_achievements()?;
    assert!(
        result
            .newly_unlocked
            .iter()
            .any(|entry| entry.id == "a-day-well-spent" && !entry.historical)
    );
    writer.set_watched(&item.key, false)?;
    let result = reader.evaluate_achievements()?;
    assert!(result.newly_unlocked.is_empty());
    assert!(
        result
            .entries
            .iter()
            .any(|entry| entry.id == "a-day-well-spent" && entry.unlocked)
    );
    assert!(result.entries.iter().any(|entry| entry.id == "centurion"
        && entry.progress
            == scry_library::Evaluation::Progress {
                current: 0.0,
                target: 100.0
            }));
    Ok(())
}

#[test]
fn concurrent_evaluators_announce_each_unlock_once() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("app.sqlite3");
    let mut library = Library::open(&path)?;
    library.evaluate_achievements()?;
    let item = movie();
    library.upsert_media(&item)?;
    library.set_known_runtimes(&[(item.key.clone(), 90_000.0)])?;
    library.set_watched(&item.key, true)?;
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || -> Result<_> {
                let mut library = Library::open(path)?;
                barrier.wait();
                library.evaluate_achievements()
            })
        })
        .collect();
    let mut announcements = 0;
    for handle in handles {
        let result = handle
            .join()
            .map_err(|_| anyhow::anyhow!("evaluator thread failed"))??;
        assert!(
            result
                .entries
                .iter()
                .any(|entry| entry.id == "a-day-well-spent" && entry.unlocked)
        );
        announcements += result
            .newly_unlocked
            .iter()
            .filter(|entry| entry.id == "a-day-well-spent")
            .count();
    }
    assert_eq!(announcements, 1);
    Ok(())
}
