use diesel::{connection::SimpleConnection, prelude::*};
use scry_library::{Library, Media, MediaKey, MediaKind};

#[test]
fn version_seven_upgrade_keeps_existing_watch_data() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("library.sqlite3");
    let item = Media {
        key: MediaKey::new("provider", MediaKind::Movie, "one"),
        title: "One".into(),
        parent: None,
    };
    {
        let mut db = Library::open(&path)?;
        db.upsert_media(&item)?;
        db.record_media_playback(&item, 25.0, 100.0, 25.0)?;
    }
    {
        let mut connection = SqliteConnection::establish(
            path.to_str()
                .ok_or_else(|| anyhow::anyhow!("invalid test path"))?,
        )?;
        connection.batch_execute("DROP TABLE saved_streams; PRAGMA user_version=7;")?;
    }
    let mut db = Library::open(path)?;
    assert_eq!(
        db.list_currently_watching(scry_library::Page::default())?[0].0,
        item
    );
    db.save_stream(&item.key, "{}")?;
    assert_eq!(db.saved_stream(&item.key)?.as_deref(), Some("{}"));
    Ok(())
}

#[test]
fn saved_streams_survive_restarts_and_stay_scoped_to_the_exact_media() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("library.sqlite3");
    let item = Media {
        key: MediaKey::new("provider", MediaKind::Movie, "one"),
        title: "One".into(),
        parent: None,
    };
    {
        let mut db = Library::open(&path)?;
        db.upsert_media(&item)?;
        db.save_stream(&item.key, r#"{"url":"https://example.com/video"}"#)?;
        assert!(db.save_stream(&item.key, &"x".repeat(65_537)).is_err());
    }
    let db = Library::open(path)?;
    assert_eq!(
        db.saved_stream(&item.key)?.as_deref(),
        Some(r#"{"url":"https://example.com/video"}"#)
    );
    assert!(
        db.saved_stream(&MediaKey::new("other", MediaKind::Movie, "one"))?
            .is_none()
    );
    assert!(
        db.saved_stream(&MediaKey::new("provider", MediaKind::Episode, "one"))?
            .is_none()
    );
    Ok(())
}
