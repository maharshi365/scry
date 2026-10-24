use anyhow::Context;
use scry_library::{Library, Media, MediaKey, MediaKind, Page};

fn show() -> Media {
    Media {
        key: MediaKey::new("test", MediaKind::Series, "show"),
        title: "Show".into(),
        parent: None,
    }
}

fn episode(parent: &Media, index: usize) -> Media {
    Media {
        key: MediaKey::new("test", MediaKind::Episode, format!("episode-{index}")),
        title: format!("Episode {index}"),
        parent: Some(parent.key.clone()),
    }
}

#[test]
fn series_progress_uses_the_latest_unfinished_episode_not_episode_order() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let parent = show();
    db.upsert_media(&parent)?;
    let first = episode(&parent, 1);
    let later = episode(&parent, 200);
    for item in [&first, &later] {
        db.upsert_media(item)?;
    }
    db.record_playback(&later.key, 40.0, 100.0, 0.0)?;
    db.record_playback(&first.key, 25.0, 100.0, 0.0)?;
    let progress = db
        .series_poster_state(&parent.key, &[])?
        .1
        .context("missing first progress")?;
    assert!((progress.position_seconds - 25.0).abs() < 0.01);
    db.record_playback(&first.key, 95.0, 100.0, 0.0)?;
    let progress = db
        .series_poster_state(&parent.key, &[])?
        .1
        .context("missing earlier unfinished progress")?;
    assert!((progress.position_seconds - 40.0).abs() < 0.01);
    db.set_watched(&later.key, true)?;
    assert!(db.series_poster_state(&parent.key, &[])?.1.is_none());
    Ok(())
}

#[test]
fn series_check_requires_every_released_episode_across_all_pages() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let parent = show();
    db.upsert_media(&parent)?;
    let mut released = Vec::new();
    for i in 0..250 {
        let item = episode(&parent, i);
        db.upsert_media(&item)?;
        db.set_watched(&item.key, true)?;
        released.push(item.key);
    }
    assert!(db.series_poster_state(&parent.key, &released)?.0);
    assert!(!db.series_poster_state(&parent.key, &[])?.0);
    let unknown = episode(&parent, 251).key;
    released.push(unknown);
    assert!(!db.series_poster_state(&parent.key, &released)?.0);
    released.pop();
    db.set_watched(&released[249], false)?;
    db.set_watched(&parent.key, true)?;
    assert!(db.series_poster_state(&parent.key, &released)?.0);
    Ok(())
}

#[test]
fn episode_completion_moves_a_series_between_library_lists_without_creating_an_override()
-> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let parent = show();
    db.upsert_media(&parent)?;
    db.add_to_watchlist(&parent.key)?;
    let first = episode(&parent, 1);
    db.upsert_media(&first)?;
    db.set_watched(&first.key, true)?;
    assert!(db.reconcile_series(&parent.key, std::slice::from_ref(&first.key))?);
    assert_eq!(
        db.list_library(true, MediaKind::Series, Page::default())?,
        vec![parent.clone()]
    );
    assert!(
        db.list_library(false, MediaKind::Series, Page::default())?
            .is_empty()
    );
    assert_eq!(db.watched_state(&parent.key)?.manual_override, None);
    assert!(!db.reconcile_series(&parent.key, std::slice::from_ref(&first.key))?);
    let new_release = episode(&parent, 2).key;
    assert!(db.reconcile_series(&parent.key, &[first.key.clone(), new_release])?);
    assert!(
        db.list_library(true, MediaKind::Series, Page::default())?
            .is_empty()
    );
    assert_eq!(
        db.list_library(false, MediaKind::Series, Page::default())?,
        vec![parent.clone()]
    );
    db.set_watched(&parent.key, false)?;
    assert!(!db.reconcile_series(&parent.key, &[first.key])?);
    assert!(!db.watched_state(&parent.key)?.watched);
    Ok(())
}

#[test]
fn bulk_episode_changes_are_atomic_and_do_not_touch_other_seasons() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let parent = show();
    db.upsert_media(&parent)?;
    let items: Vec<_> = (0..250).map(|i| episode(&parent, i)).collect();
    db.set_episodes_watched(&parent, &items, true)?;
    assert!(db.watched_state(&items[249].key)?.watched);
    let other = episode(&parent, 999);
    db.upsert_media(&other)?;
    db.record_playback(&other.key, 30.0, 100.0, 0.0)?;
    db.set_episodes_watched(&parent, &items, false)?;
    assert!(!db.watched_state(&items[249].key)?.watched);
    assert!(db.load_progress(&other.key)?.is_some());
    let mut invalid = items[1].clone();
    invalid.parent = Some(MediaKey::new("test", MediaKind::Series, "another-show"));
    assert!(
        db.set_episodes_watched(&parent, &[items[0].clone(), invalid], true)
            .is_err()
    );
    assert!(!db.watched_state(&items[0].key)?.watched);
    Ok(())
}
