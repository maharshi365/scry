use crate::{Library, local_media::LocalLink};

#[test]
fn roots_are_independent_and_removal_does_not_remove_media_history() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let key = crate::MediaKey::new("imdb", crate::MediaKind::Movie, "tt123");
    db.upsert_media(&crate::Media {
        key: key.clone(),
        title: "Movie".into(),
        parent: None,
    })?;
    db.set_watched(&key, true)?;
    let first = db.add_local_root("D:/Movies")?;
    let second = db.add_local_root("E:/Shows")?;
    assert_ne!(first, second);
    assert_eq!(db.add_local_root("D:/Movies")?, first);
    assert_eq!(db.local_roots()?.len(), 2);
    db.record_local_file(first, "movie.mkv", 42, 1, "{}", 1)?;
    db.record_local_file(second, "show.mkv", 42, 1, "{}", 1)?;
    db.remove_local_root(first)?;
    let files = db.local_files(0, false)?;
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].root_id, second);
    assert!(db.watched_state(&key)?.watched);
    Ok(())
}

#[test]
fn local_identities_are_not_reused_after_removing_a_root() -> anyhow::Result<()> {
    let db = Library::memory()?;
    let root = db.add_local_root("D:/Old")?;
    db.record_local_file(root, "one.mkv", 1, 1, "{}", 1)?;
    let old_id = db.local_files(0, false)?[0].id;
    assert_eq!(
        db.local_sources("local", "movie", &format!("local-file:{old_id}"))?
            .len(),
        1
    );
    db.remove_local_root(root)?;
    let root = db.add_local_root("D:/New")?;
    db.record_local_file(root, "two.mkv", 1, 1, "{}", 1)?;
    assert_ne!(db.local_files(0, false)?[0].id, old_id);
    assert!(
        db.local_sources("local", "movie", &format!("local-file:{old_id}"))?
            .is_empty()
    );
    Ok(())
}

#[test]
fn group_decisions_are_scoped_to_root_folder_and_title_evidence() -> anyhow::Result<()> {
    use crate::local_media::LocalGroup;
    let db = Library::memory()?;
    let first = db.add_local_root("D:/Shows")?;
    let second = db.add_local_root("E:/Shows")?;
    db.set_local_group(
        first,
        "Mixed",
        "show one",
        &LocalGroup {
            namespace: "provider".into(),
            metadata: "confirmed inventory".into(),
        },
    )?;
    assert_eq!(
        db.local_group(first, "Mixed", "show one")?
            .map(|g| g.metadata),
        Some("confirmed inventory".into())
    );
    assert!(db.local_group(first, "Mixed", "show two")?.is_none());
    assert!(db.local_group(second, "Mixed", "show one")?.is_none());
    assert!(db.local_group(first, "Other", "show one")?.is_none());
    Ok(())
}

#[test]
fn rescans_preserve_manual_links_and_only_complete_scans_mark_missing() -> anyhow::Result<()> {
    let db = Library::memory()?;
    let root = db.add_local_root("D:/Movies")?;
    db.begin_local_scan(root)?;
    db.record_local_file(root, "movie.mkv", 42, 1, "{}", 1)?;
    let file = db.local_files(0, false)?.remove(0);
    let link = LocalLink {
        namespace: "imdb".into(),
        kind: "movie".into(),
        external_id: "tt123".into(),
        title: "Movie".into(),
        metadata: "{}".into(),
        manual: true,
    };
    db.link_local_file(file.id, &link)?;
    db.begin_local_scan(root)?;
    db.record_local_file(root, "movie.mkv", 43, 2, "{}", 2)?;
    let file = db.local_files(0, false)?.remove(0);
    assert!(file.available);
    assert!(file.manual);
    assert_eq!(file.external_id.as_deref(), Some("tt123"));
    assert_eq!(db.local_sources("imdb", "movie", "tt123")?.len(), 1);
    let generation = db.begin_local_scan(root)?;
    db.finish_local_scan(root, generation)?;
    assert!(db.local_sources("imdb", "movie", "tt123")?.is_empty());
    assert!(!db.local_files(0, false)?[0].available);
    Ok(())
}

#[test]
fn automatic_link_does_not_replace_a_confirmed_match() -> anyhow::Result<()> {
    let db = Library::memory()?;
    let root = db.add_local_root("D:/Movies")?;
    db.record_local_file(root, "movie.mkv", 42, 1, "{}", 1)?;
    let id = db.local_files(0, false)?[0].id;
    let mut link = LocalLink {
        namespace: "imdb".into(),
        kind: "movie".into(),
        external_id: "tt123".into(),
        title: "Movie".into(),
        metadata: "{}".into(),
        manual: true,
    };
    db.link_local_file(id, &link)?;
    link.external_id = "tt456".into();
    link.manual = false;
    db.link_local_file(id, &link)?;
    assert_eq!(
        db.local_files(0, false)?[0].external_id.as_deref(),
        Some("tt123")
    );
    db.clear_local_link(id)?;
    assert_eq!(db.local_files(0, true)?.len(), 1);
    Ok(())
}

#[test]
fn offline_roots_hide_sources_without_erasing_links_and_successful_scan_restores_them()
-> anyhow::Result<()> {
    let db = Library::memory()?;
    let root = db.add_local_root("D:/Movies")?;
    let generation = db.begin_local_scan(root)?;
    db.record_local_file(root, "movie.mkv", 42, 1, "{}", generation)?;
    let file = db.local_files(0, false)?.remove(0);
    db.link_local_file(
        file.id,
        &LocalLink {
            namespace: "imdb".into(),
            kind: "movie".into(),
            external_id: "tt123".into(),
            title: "Movie".into(),
            metadata: "{}".into(),
            manual: true,
        },
    )?;
    db.set_local_root_online(root, false)?;
    assert!(!db.local_roots()?[0].online);
    assert!(db.local_sources("imdb", "movie", "tt123")?.is_empty());
    let offline = db.local_file(file.id)?;
    assert!(!offline.available);
    assert!(offline.manual);
    assert_eq!(offline.external_id.as_deref(), Some("tt123"));
    db.finish_local_scan(root, generation)?;
    assert!(db.local_roots()?[0].online);
    assert_eq!(db.local_sources("imdb", "movie", "tt123")?.len(), 1);
    Ok(())
}

#[test]
fn metadata_and_root_configuration_survive_reopening_the_app_database() -> anyhow::Result<()> {
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("library.sqlite3");
    {
        let db = Library::open(&path)?;
        db.add_local_root("D:/Movies")?;
        db.add_local_root("E:/Shows")?;
        db.cache_local_metadata("provider", "series", "show", "cached inventory")?;
    }
    let db = Library::open(&path)?;
    assert_eq!(db.local_roots()?.len(), 2);
    assert_eq!(
        db.local_metadata("provider", "series", "show")?.as_deref(),
        Some("cached inventory")
    );
    assert!(
        db.local_metadata("another provider", "series", "show")?
            .is_none()
    );
    Ok(())
}
