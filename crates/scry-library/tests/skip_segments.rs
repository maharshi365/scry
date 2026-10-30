use scry_library::{Library, SkipSegmentsRecord};

#[test]
fn saved_segments_survive_reopening_and_updates_are_episode_and_provider_scoped()
-> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("segments.sqlite3");
    let original = SkipSegmentsRecord {
        payload: "{\"intro\":null}".into(),
        etag: Some("\"v1\"".into()),
        fetched_at: 100,
    };
    {
        let db = Library::open(&path)?;
        db.save_skip_segments("introdb", "tt2560140", 1, 2, &original)?;
    }
    let db = Library::open(&path)?;
    assert_eq!(
        db.skip_segments("introdb", "tt2560140", 1, 2)?,
        Some(original.clone())
    );
    assert_eq!(db.skip_segments("introdb", "tt2560140", 1, 3)?, None);
    assert_eq!(db.skip_segments("other", "tt2560140", 1, 2)?, None);
    let updated = SkipSegmentsRecord {
        fetched_at: 200,
        etag: None,
        payload: "{\"no_intro\":true}".into(),
    };
    db.save_skip_segments("introdb", "tt2560140", 1, 3, &original)?;
    db.save_skip_segments("introdb", "tt2560140", 1, 2, &updated)?;
    assert_eq!(
        db.skip_segments("introdb", "tt2560140", 1, 2)?,
        Some(updated)
    );
    assert_eq!(
        db.skip_segments("introdb", "tt2560140", 1, 3)?,
        Some(original)
    );
    Ok(())
}

#[test]
fn invalid_records_do_not_replace_saved_data() -> anyhow::Result<()> {
    let db = Library::memory()?;
    let record = SkipSegmentsRecord {
        payload: "{}".into(),
        etag: None,
        fetched_at: 100,
    };
    db.save_skip_segments("introdb", "tt2560140", 1, 2, &record)?;
    let invalid = SkipSegmentsRecord {
        fetched_at: -1,
        ..record.clone()
    };
    assert!(
        db.save_skip_segments("introdb", "tt2560140", 1, 2, &invalid)
            .is_err()
    );
    assert!(
        db.save_skip_segments("introdb", "tt2560140", 1, 0, &record)
            .is_err()
    );
    assert_eq!(
        db.skip_segments("introdb", "tt2560140", 1, 2)?,
        Some(record)
    );
    Ok(())
}

#[test]
fn adding_the_cache_to_an_existing_database_preserves_library_data() -> anyhow::Result<()> {
    use diesel::{Connection, connection::SimpleConnection};
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("existing.sqlite3");
    drop(Library::open(&path)?);
    {
        let mut raw = diesel::SqliteConnection::establish(
            path.to_str()
                .ok_or_else(|| anyhow::anyhow!("invalid fixture path"))?,
        )?;
        raw.batch_execute("DROP TABLE skip_segments; INSERT INTO addons VALUES ('fixture', 0, 'https://example.invalid/manifest.json', '{}'); PRAGMA user_version=6;")?;
    }
    let db = Library::open(&path)?;
    assert_eq!(
        db.load_addons()?.first().map(|a| a.id.as_str()),
        Some("fixture")
    );
    let record = SkipSegmentsRecord {
        payload: "{}".into(),
        etag: None,
        fetched_at: 100,
    };
    db.save_skip_segments("introdb", "tt2560140", 1, 2, &record)?;
    assert_eq!(
        db.skip_segments("introdb", "tt2560140", 1, 2)?,
        Some(record)
    );
    Ok(())
}
