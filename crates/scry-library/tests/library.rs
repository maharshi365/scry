#![expect(
    clippy::unwrap_used,
    reason = "Tests assert stored records and intentionally fail if a record is missing"
)]
#![expect(
    clippy::float_cmp,
    reason = "Stored fixture values must round-trip exactly, without approximate arithmetic"
)]

use diesel::{Connection, SqliteConnection, connection::SimpleConnection};
use scry_library::{Library, Media, MediaKey, MediaKind, Page};

fn movie(namespace: &str, id: &str) -> Media {
    Media {
        key: MediaKey::new(namespace, MediaKind::Movie, id),
        title: format!("Title {id}"),
        parent: None,
    }
}

#[test]
fn persistence_namespaces_and_bound_strings() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("library.db");
    let first = movie("provider-a", "x'); DROP TABLE media;--");
    let second = movie("provider-b", &first.key.id);
    {
        let mut db = Library::open(&path)?;
        db.upsert_media(&first)?;
        db.upsert_media(&second)?;
        db.add_to_watchlist(&first.key)?;
        db.add_to_watchlist(&first.key)?;
        db.set_watched(&second.key, true)?;
        db.save_progress(&second.key, 42.0, 100.0)?;
        let playlist = db.create_playlist("Saved")?;
        db.add_playlist_item(playlist.id, &first.key)?;
    }
    let db = Library::open(path)?;
    assert_eq!(db.list_watchlist(Page::default())?, vec![first.clone()]);
    assert!(!db.watched_state(&first.key)?.watched);
    assert!(db.watched_state(&second.key)?.watched);
    assert_eq!(db.get_media(&second.key)?, Some(second));
    assert_eq!(
        db.load_progress(&MediaKey::new(
            "provider-b",
            MediaKind::Movie,
            &first.key.id
        ))?
        .unwrap()
        .position_seconds,
        42.0
    );
    let playlist = &db.list_playlists(Page::default())?[0];
    assert_eq!(
        db.list_playlist_items(playlist.id, Page::default())?,
        vec![first]
    );
    Ok(())
}

#[test]
fn watchlist_removal_and_metadata_updates_preserve_state() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let mut item = movie("test", "same-id");
    db.upsert_media(&item)?;
    db.add_to_watchlist(&item.key)?;
    db.set_watched(&item.key, true)?;
    item.title = "New title".into();
    db.upsert_media(&item)?;
    let show = Media {
        key: MediaKey::new("test", MediaKind::Series, "same-id"),
        title: "Series".into(),
        parent: None,
    };
    db.upsert_media(&show)?;
    assert_eq!(db.list_watchlist(Page::default())?, vec![item.clone()]);
    assert!(db.is_in_watchlist(&item.key)?);
    assert!(!db.is_in_watchlist(&show.key)?);
    assert!(db.watched_state(&item.key)?.watched);
    assert!(!db.watched_state(&show.key)?.watched);
    db.remove_from_watchlist(&item.key)?;
    db.remove_from_watchlist(&item.key)?;
    assert!(!db.is_in_watchlist(&item.key)?);
    assert!(db.watched_state(&item.key)?.watched);
    Ok(())
}

#[test]
fn reorder_rolls_back_on_storage_failure() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("library.db");
    let mut db = Library::open(&path)?;
    let first = movie("test", "1");
    let second = movie("test", "2");
    db.upsert_media(&first)?;
    db.upsert_media(&second)?;
    let playlist = db.create_playlist("Atomic")?;
    db.add_playlist_item(playlist.id, &first.key)?;
    db.add_playlist_item(playlist.id, &second.key)?;
    // Inject a storage failure after the replacement has already started.
    let mut raw = SqliteConnection::establish(path.to_str().unwrap())?;
    raw.batch_execute("CREATE TRIGGER fail_reorder BEFORE INSERT ON playlist_items WHEN NEW.position=1 BEGIN SELECT RAISE(ABORT, 'injected storage failure'); END")?;
    assert!(
        db.reorder_playlist(playlist.id, &[second.key.clone(), first.key.clone()])
            .is_err()
    );
    assert_eq!(
        db.list_playlist_items(playlist.id, Page::default())?,
        vec![first, second]
    );
    Ok(())
}

#[test]
fn parent_validation_does_not_write_invalid_metadata() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let parent = movie("test", "parent");
    db.upsert_media(&parent)?;
    let mut episode = Media {
        key: MediaKey::new("test", MediaKind::Episode, "e1"),
        title: "Episode".into(),
        parent: Some(parent.key.clone()),
    };
    assert!(db.upsert_media(&episode).is_err());
    episode.parent = Some(MediaKey::new("test", MediaKind::Series, "missing"));
    assert!(db.upsert_media(&episode).is_err());
    assert_eq!(db.get_media(&episode.key)?, None);
    let mut invalid = parent.clone();
    invalid.title = " ".into();
    assert!(db.upsert_media(&invalid).is_err());
    assert_eq!(db.get_media(&parent.key)?, Some(parent));
    Ok(())
}

#[test]
fn playlist_order_and_failed_reorder_are_atomic() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let items: Vec<_> = (0..3).map(|id| movie("test", &id.to_string())).collect();
    let playlist = db.create_playlist("Robert'); DROP TABLE media;--")?;
    for item in &items {
        db.upsert_media(item)?;
        db.add_playlist_item(playlist.id, &item.key)?;
        db.add_playlist_item(playlist.id, &item.key)?;
    }
    let reversed: Vec<_> = items.iter().rev().map(|m| m.key.clone()).collect();
    db.reorder_playlist(playlist.id, &reversed)?;
    assert!(
        db.reorder_playlist(
            playlist.id,
            &[
                reversed[0].clone(),
                reversed[0].clone(),
                reversed[2].clone()
            ]
        )
        .is_err()
    );
    assert!(db.reorder_playlist(playlist.id, &reversed[..2]).is_err());
    assert_eq!(
        db.list_playlist_items(playlist.id, Page::default())?,
        items.iter().rev().cloned().collect::<Vec<_>>()
    );
    db.remove_playlist_item(playlist.id, &items[1].key)?;
    db.add_playlist_item(playlist.id, &items[1].key)?;
    assert_eq!(
        db.list_playlist_items(playlist.id, Page::new(1, 1)?)?,
        vec![items[0].clone()]
    );
    db.rename_playlist(playlist.id, "Renamed")?;
    assert_eq!(db.list_playlists(Page::default())?[0].name, "Renamed");
    db.delete_playlist(playlist.id)?;
    assert!(db.list_playlists(Page::default())?.is_empty());
    Ok(())
}

#[test]
fn episodes_and_manual_overrides_are_independent() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let show = Media {
        key: MediaKey::new("test", MediaKind::Series, "show"),
        title: "Show".into(),
        parent: None,
    };
    db.upsert_media(&show)?;
    let episode = Media {
        key: MediaKey::new("test", MediaKind::Episode, "e1"),
        title: "Pilot".into(),
        parent: Some(show.key.clone()),
    };
    db.upsert_media(&episode)?;
    db.record_playback(&episode.key, 90.0, 100.0, 90.0)?;
    assert!(db.watched_state(&episode.key)?.watched);
    assert!(!db.watched_state(&show.key)?.watched);
    assert_eq!(
        db.list_episodes(&show.key, Page::default())?,
        vec![episode.clone()]
    );
    db.set_watched(&episode.key, false)?;
    assert_eq!(db.load_progress(&episode.key)?, None);
    db.record_playback(&episode.key, 100.0, 100.0, 100.0)?;
    assert!(!db.watched_state(&episode.key)?.watched);
    db.clear_watched_override(&episode.key)?;
    assert!(db.watched_state(&episode.key)?.watched);
    db.set_watched(&episode.key, true)?;
    db.record_playback(&episode.key, 1.0, 100.0, 1.0)?;
    assert!(db.watched_state(&episode.key)?.watched);
    assert_eq!(db.list_watched(Page::default())?, vec![episode]);
    Ok(())
}

#[test]
fn progress_validation_and_pagination() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let item = movie("test", "1");
    db.upsert_media(&item)?;
    db.save_progress(&item.key, -10.0, 100.0)?;
    assert!(!db.watched_state(&item.key)?.watched);
    assert_eq!(db.load_progress(&item.key)?.unwrap().position_seconds, 0.0);
    for (position, duration) in [
        (f64::NAN, 100.0),
        (0.0, f64::INFINITY),
        (0.0, 0.0),
        (0.0, -1.0),
    ] {
        assert!(db.save_progress(&item.key, position, duration).is_err());
    }
    assert_eq!(db.load_progress(&item.key)?.unwrap().position_seconds, 0.0);
    db.save_progress(&item.key, 200.0, 100.0)?;
    assert_eq!(
        db.load_progress(&item.key)?.unwrap().position_seconds,
        100.0
    );
    assert!(db.watched_state(&item.key)?.watched);
    db.save_progress(&item.key, 1.0, 100.0)?;
    assert!(db.watched_state(&item.key)?.watched);
    assert!(Page::new(0, 0).is_err());
    assert!(Page::new(201, 0).is_err());
    Ok(())
}

#[test]
fn rejects_unknown_and_damaged_schema() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("library.db");
    drop(Library::open(&path)?);
    let mut raw = SqliteConnection::establish(path.to_str().unwrap())?;
    raw.batch_execute("INSERT INTO __diesel_schema_migrations(version) VALUES('99999999999999')")?;
    assert!(Library::open(&path).is_err());
    raw.batch_execute("DELETE FROM __diesel_schema_migrations WHERE version='99999999999999'; DROP TABLE progress;")?;
    assert!(Library::open(&path).is_err());
    let unversioned = dir.path().join("unversioned.db");
    let mut raw = SqliteConnection::establish(unversioned.to_str().unwrap())?;
    raw.batch_execute("CREATE TABLE unrelated(value TEXT)")?;
    assert!(Library::open(unversioned).is_err());
    Ok(())
}
