use anyhow::Context;
use diesel::{connection::SimpleConnection, prelude::*};
use scry_library::{EpisodeRelease, Library, Media, MediaKey, MediaKind, Page, Progress};

#[test]
fn version_two_upgrade_counts_saved_undated_watches_without_rewriting_history() -> anyhow::Result<()>
{
    upgrade_saved_watch_history(2)
}

#[test]
fn version_three_upgrade_excludes_specials_without_rewriting_history() -> anyhow::Result<()> {
    upgrade_saved_watch_history(3)
}

fn upgrade_saved_watch_history(version: i64) -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("library.sqlite3");
    {
        let mut connection =
            SqliteConnection::establish(path.to_str().context("test database path is not UTF-8")?)?;
        connection.batch_execute(include_str!("../src/schema.sql"))?;
        connection.batch_execute(
            "CREATE VIEW effective_watched AS
             SELECT m.media_id, CASE WHEN m.kind='series' THEN
                 EXISTS(SELECT 1 FROM media e JOIN episode_releases r ON r.media_id=e.media_id
                     WHERE e.parent_id=m.media_id)
                 AND NOT EXISTS(SELECT 1 FROM media e JOIN episode_releases r ON r.media_id=e.media_id
                     LEFT JOIN watched v ON v.media_id=e.media_id
                     WHERE e.parent_id=m.media_id AND
                         (r.released_at IS NULL OR r.released_at>unixepoch() OR COALESCE(v.watched,0)=0))
                 ELSE COALESCE(w.watched,0) END AS watched
             FROM media m LEFT JOIN watched w ON w.media_id=m.media_id;
             INSERT INTO media VALUES (1,'test','series','show','Show',NULL),
                 (2,'test','episode','first','First',1), (3,'test','episode','special','Special',1),
                 (4,'test','episode','unknown','Unknown',1);
             INSERT INTO series_inventory VALUES (1);
             INSERT INTO episode_releases VALUES (2,0,1,1),(3,NULL,0,1),(4,NULL,1,2);
             INSERT INTO watched VALUES (2,1),(3,1),(4,1);
             INSERT INTO progress VALUES (3,80.0,100.0);
             INSERT INTO viewing VALUES (3,75.0,1,1);
             INSERT INTO watchlist (media_id) VALUES (1);",
        )?;
        connection.batch_execute(&format!("PRAGMA user_version={version};"))?;
    }
    let show = MediaKey::new("test", MediaKind::Series, "show");
    let special = MediaKey::new("test", MediaKind::Episode, "special");
    for _ in 0..2 {
        let mut db = Library::open(&path)?;
        let unknown = MediaKey::new("test", MediaKind::Episode, "unknown");
        db.record_playback(&unknown, 80.0, 100.0, 80.0)?;
        assert!(db.watched_state(&unknown)?.watched);
        assert!(db.watched_state(&special)?.watched);
        let summary = db.series_progress(&show, 100)?;
        assert_eq!(summary.total_episodes, 2);
        assert_eq!(summary.released_episodes, 1);
        assert_eq!(summary.watched_episodes, 2);
        assert_eq!(summary.fraction(), Some(1.0));
        assert!(db.watched_state(&show)?.watched);
        assert!(db.series_poster_state(&show, 100)?.0);
        assert_eq!(
            db.list_library(true, MediaKind::Series, Page::default())?
                .len(),
            1
        );
        assert_eq!(
            db.load_progress(&special)?,
            Some(Progress {
                position_seconds: 80.0,
                duration_seconds: 100.0,
            })
        );
        assert!(
            (db.media_states(std::slice::from_ref(&special))?[0].watched_seconds - 75.0).abs()
                < f64::EPSILON
        );
    }
    Ok(())
}

#[test]
fn version_one_upgrade_corrects_completion_and_preserves_watch_history() -> anyhow::Result<()> {
    #[derive(QueryableByName)]
    struct Flag {
        #[diesel(sql_type = diesel::sql_types::Bool)]
        watched: bool,
    }

    let dir = tempfile::tempdir()?;
    let path = dir.path().join("library.sqlite3");
    {
        let mut connection =
            SqliteConnection::establish(path.to_str().context("test database path is not UTF-8")?)?;
        // Version one required dates and treated every released episode watched as complete.
        connection.batch_execute(
            &include_str!("../src/schema.sql")
                .replace("released_at BIGINT,", "released_at BIGINT NOT NULL,"),
        )?;
        connection.batch_execute(
            "CREATE VIEW effective_watched AS
             SELECT m.media_id, CASE WHEN m.kind='series' THEN
                 EXISTS(SELECT 1 FROM media e JOIN episode_releases r ON r.media_id=e.media_id
                     WHERE e.parent_id=m.media_id AND r.released_at<=unixepoch())
                 AND NOT EXISTS(SELECT 1 FROM media e JOIN episode_releases r ON r.media_id=e.media_id
                     LEFT JOIN watched v ON v.media_id=e.media_id
                     WHERE e.parent_id=m.media_id AND r.released_at<=unixepoch() AND COALESCE(v.watched,0)=0)
                 ELSE COALESCE(w.watched,0) END AS watched
             FROM media m LEFT JOIN watched w ON w.media_id=m.media_id;
             INSERT INTO media VALUES (1,'test','series','show','Show',NULL),
                 (2,'test','episode','first','First',1), (3,'test','episode','future','Future',1);
             INSERT INTO series_inventory VALUES (1);
             INSERT INTO episode_releases VALUES (2,0,1,1),(3,9223372036854775807,1,2);
             INSERT INTO watched VALUES (2,1);
             INSERT INTO progress VALUES (2,80.0,100.0);
             INSERT INTO viewing VALUES (2,75.0,1,1);
             INSERT INTO watchlist (media_id) VALUES (1);
             PRAGMA user_version=1;",
        )?;
        assert!(
            diesel::sql_query("SELECT watched FROM effective_watched WHERE media_id=1")
                .get_result::<Flag>(&mut connection)?
                .watched
        );
    }
    let show = MediaKey::new("test", MediaKind::Series, "show");
    let first = MediaKey::new("test", MediaKind::Episode, "first");
    {
        let mut db = Library::open(&path)?;
        assert!(!db.watched_state(&show)?.watched);
        assert!(db.watched_state(&first)?.watched);
        assert_eq!(
            db.load_progress(&first)?,
            Some(Progress {
                position_seconds: 80.0,
                duration_seconds: 100.0
            })
        );
        assert!(
            (db.media_states(std::slice::from_ref(&first))?[0].watched_seconds - 75.0).abs()
                < f64::EPSILON
        );
        assert_eq!(db.series_progress(&show, 100)?.fraction(), Some(0.5));
        assert_eq!(
            db.list_library(false, MediaKind::Series, Page::default())?[0].key,
            show
        );
        assert!(
            db.list_library(true, MediaKind::Series, Page::default())?
                .is_empty()
        );
        // The upgraded table must also accept undated inventory.
        db.sync_episode_releases(
            &show,
            &[
                EpisodeRelease {
                    media: Media {
                        key: first.clone(),
                        title: "First".into(),
                        parent: Some(show.clone()),
                    },
                    released_at: Some(0),
                    season: Some(1),
                    episode: Some(1),
                },
                EpisodeRelease {
                    media: Media {
                        key: MediaKey::new("test", MediaKind::Episode, "unknown"),
                        title: "Unknown".into(),
                        parent: Some(show.clone()),
                    },
                    released_at: None,
                    season: Some(1),
                    episode: Some(2),
                },
            ],
        )?;
    }
    let db = Library::open(&path)?;
    assert!(db.watched_state(&first)?.watched);
    assert!(!db.watched_state(&show)?.watched);
    assert_eq!(db.series_progress(&show, 100)?.fraction(), Some(0.5));
    Ok(())
}
