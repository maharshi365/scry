use anyhow::Context;
use diesel::{connection::SimpleConnection, prelude::*};
use scry_library::{Library, MediaKey, MediaKind, StatsPeriod};

#[test]
fn existing_watched_history_uses_known_runtime_without_inventing_dates() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("old.sqlite3");
    {
        let mut connection =
            SqliteConnection::establish(path.to_str().context("test database path")?)?;
        connection.batch_execute(include_str!("../src/schema.sql"))?;
        connection.batch_execute("CREATE VIEW effective_watched AS SELECT m.media_id,COALESCE(w.watched,0) AS watched FROM media m LEFT JOIN watched w USING(media_id);")?;
        connection.batch_execute("INSERT INTO media VALUES (1,'test','movie','one','One',NULL); INSERT INTO watched VALUES (1,1); INSERT INTO progress VALUES (1,80,100); PRAGMA user_version=4;")?;
    }
    let mut db = Library::open(&path)?;
    assert!(
        db.watched_state(&MediaKey::new("test", MediaKind::Movie, "one"))?
            .watched
    );
    let stats = db.nerd_stats(StatsPeriod::AllTime)?;
    assert_eq!(stats.movies_watched, 1);
    assert!((stats.watch_seconds - 100.0).abs() < f64::EPSILON);
    assert!(stats.recorded_seconds.abs() < f64::EPSILON);
    assert!(db.nerd_stats(StatsPeriod::Week)?.watch_seconds.abs() < f64::EPSILON);
    assert!(db.evaluate_achievements()?.newly_unlocked.is_empty());
    Ok(())
}
