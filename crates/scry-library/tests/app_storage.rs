use scry_library::{AddonRecord, Library, Media, MediaKey, MediaKind, Page};

#[test]
fn all_app_state_survives_reopening_one_database() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("scry.sqlite3");
    let addon = AddonRecord {
        id: "example".into(),
        manifest_url: "https://example.com/private/manifest.json".into(),
        manifest_json: r#"{"id":"example","name":"Example"}"#.into(),
    };
    let media = Media {
        key: MediaKey::new("example", MediaKind::Movie, "one"),
        title: "One".into(),
        parent: None,
    };
    {
        let mut db = Library::open(&path)?;
        db.replace_addons(std::slice::from_ref(&addon))?;
        db.upsert_media(&media)?;
        db.add_to_watchlist(&media.key)?;
    }
    let mut db = Library::open(&path)?;
    assert_eq!(db.load_addons()?, vec![addon]);
    assert_eq!(db.list_watchlist(Page::default())?, vec![media]);
    db.replace_addons(&[])?;
    drop(db);
    assert!(Library::open(path)?.load_addons()?.is_empty());
    assert!(std::fs::read_dir(directory.path())?.all(|entry| {
        entry.is_ok_and(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("scry.sqlite3")
        })
    }));
    Ok(())
}

#[test]
fn invalid_addon_replacement_keeps_the_previous_configuration() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let addon = AddonRecord {
        id: "example".into(),
        manifest_url: "https://example.com/manifest.json".into(),
        manifest_json: "{}".into(),
    };
    db.replace_addons(std::slice::from_ref(&addon))?;
    assert!(db.replace_addons(&[addon.clone(), addon.clone()]).is_err());
    assert_eq!(db.load_addons()?, vec![addon.clone()]);
    let invalid = AddonRecord {
        manifest_json: "not JSON".into(),
        ..addon.clone()
    };
    assert!(db.replace_addons(&[invalid]).is_err());
    assert_eq!(db.load_addons()?, vec![addon]);
    Ok(())
}

#[test]
fn addon_order_replacement_and_limit_are_persistent() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let records: Vec<_> = (0..32)
        .map(|index| AddonRecord {
            id: index.to_string(),
            manifest_url: format!("https://example.com/{index}/manifest.json"),
            manifest_json: "{}".into(),
        })
        .collect();
    db.replace_addons(&records)?;
    assert_eq!(db.load_addons()?, records);
    let mut too_many = records.clone();
    too_many.push(AddonRecord {
        id: "extra".into(),
        ..records[0].clone()
    });
    assert!(db.replace_addons(&too_many).is_err());
    assert_eq!(db.load_addons()?, records);
    let reordered = vec![records[2].clone(), records[0].clone()];
    db.replace_addons(&reordered)?;
    assert_eq!(db.load_addons()?, reordered);
    Ok(())
}

#[test]
#[expect(
    clippy::float_cmp,
    reason = "Stored fixture values must round-trip exactly"
)]
fn concurrent_startup_and_independent_workers_share_one_database() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("scry.sqlite3");
    let barrier = std::sync::Barrier::new(4);
    std::thread::scope(|scope| -> anyhow::Result<()> {
        let handles: Vec<_> = (0..4)
            .map(|index| {
                let path = &path;
                let barrier = &barrier;
                scope.spawn(move || -> anyhow::Result<()> {
                    barrier.wait();
                    let mut db = Library::open(path)?;
                    if index == 0 {
                        db.replace_addons(&[AddonRecord {
                            id: "example".into(),
                            manifest_url: "https://example.com/manifest.json".into(),
                            manifest_json: "{}".into(),
                        }])?;
                    } else {
                        let media = Media {
                            key: MediaKey::new("example", MediaKind::Movie, index.to_string()),
                            title: format!("Movie {index}"),
                            parent: None,
                        };
                        db.upsert_media(&media)?;
                        db.add_to_watchlist(&media.key)?;
                        db.record_playback(&media.key, 20.0, 100.0, 20.0)?;
                    }
                    Ok(())
                })
            })
            .collect();
        for handle in handles {
            handle
                .join()
                .map_err(|_| anyhow::anyhow!("storage worker panicked"))??;
        }
        Ok(())
    })?;
    let db = Library::open(path)?;
    assert_eq!(
        db.load_addons()?
            .iter()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>(),
        vec!["example"]
    );
    let items = db.list_watchlist(Page::default())?;
    assert_eq!(items.len(), 3);
    for item in items {
        assert!(
            db.load_progress(&item.key)?
                .is_some_and(|sample| sample.position_seconds == 20.0)
        );
    }
    Ok(())
}
