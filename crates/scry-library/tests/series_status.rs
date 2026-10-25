use anyhow::Context;
use scry_library::{EpisodeRelease, Library, Media, MediaKey, MediaKind, Page};

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

fn sync(db: &mut Library, parent: &Media, keys: &[MediaKey]) -> anyhow::Result<bool> {
    db.sync_episode_releases(
        &parent.key,
        &keys
            .iter()
            .map(|key| EpisodeRelease {
                media: Media {
                    key: key.clone(),
                    title: key.id.clone(),
                    parent: Some(parent.key.clone()),
                },
                released_at: Some(0),
                season: Some(1),
                episode: None,
            })
            .collect::<Vec<_>>(),
    )
}

#[test]
fn caught_up_is_not_complete_until_all_known_episodes_are_released_and_watched()
-> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let parent = show();
    db.upsert_media(&parent)?;
    db.add_to_watchlist(&parent.key)?;
    let mut episodes: Vec<_> = (1..=8)
        .map(|index| {
            Ok(EpisodeRelease {
                media: episode(&parent, index),
                released_at: Some(if index <= 4 { 0 } else { i64::MAX }),
                season: Some(1),
                episode: Some(u32::try_from(index)?),
            })
        })
        .collect::<anyhow::Result<_>>()?;
    db.sync_episode_releases(&parent.key, &episodes)?;
    db.set_watched(&parent.key, true)?;
    assert_eq!(db.series_progress(&parent.key, 100)?.fraction(), Some(0.5));
    assert!(!db.series_poster_state(&parent.key, 100)?.0);
    assert!(!db.watched_state(&parent.key)?.watched);
    assert!(
        db.list_library(true, MediaKind::Series, Page::default())?
            .is_empty()
    );
    assert_eq!(
        db.list_library(false, MediaKind::Series, Page::default())?,
        vec![parent.clone()]
    );
    assert!(db.list_up_next(100, Page::default())?.is_empty());
    for release in &episodes[4..] {
        assert!(!db.watched_state(&release.media.key)?.watched);
    }
    for release in &mut episodes[4..] {
        release.released_at = Some(0);
    }
    db.sync_episode_releases(&parent.key, &episodes)?;
    assert!(!db.watched_state(&parent.key)?.watched);
    assert_eq!(
        db.list_up_next(100, Page::default())?[0].0.key,
        episodes[4].media.key
    );
    db.set_watched(&parent.key, true)?;
    assert_eq!(db.series_progress(&parent.key, 100)?.fraction(), Some(1.0));
    assert!(db.series_poster_state(&parent.key, 100)?.0);
    assert!(db.watched_state(&parent.key)?.watched);
    assert_eq!(
        db.list_library(true, MediaKind::Series, Page::default())?,
        vec![parent]
    );
    Ok(())
}

#[test]
fn undated_episodes_count_toward_completion_but_cannot_be_marked_or_offered() -> anyhow::Result<()>
{
    let mut db = Library::memory()?;
    let parent = show();
    db.upsert_media(&parent)?;
    db.add_to_watchlist(&parent.key)?;
    let first = episode(&parent, 1);
    let unknown = episode(&parent, 2);
    let mut releases = vec![
        EpisodeRelease {
            media: first,
            released_at: Some(0),
            season: Some(1),
            episode: Some(1),
        },
        EpisodeRelease {
            media: unknown.clone(),
            released_at: None,
            season: Some(1),
            episode: Some(2),
        },
    ];
    db.sync_episode_releases(&parent.key, &releases)?;
    db.set_watched(&parent.key, true)?;
    db.set_watched(&unknown.key, true)?;
    db.record_playback(&unknown.key, 80.0, 100.0, 80.0)?;
    let summary = db.series_progress(&parent.key, 100)?;
    assert_eq!(summary.total_episodes, 2);
    assert_eq!(summary.released_episodes, 1);
    assert_eq!(summary.fraction(), Some(0.5));
    assert!(!db.watched_state(&unknown.key)?.watched);
    assert!(!db.watched_state(&parent.key)?.watched);
    assert!(!db.series_poster_state(&parent.key, 100)?.0);
    assert!(db.list_up_next(100, Page::default())?.is_empty());
    assert_eq!(db.next_release_at(100)?, None);
    releases[1].released_at = Some(0);
    db.sync_episode_releases(&parent.key, &releases)?;
    assert_eq!(db.list_up_next(100, Page::default())?[0].0.key, unknown.key);
    db.set_watched(&unknown.key, true)?;
    assert!(db.watched_state(&parent.key)?.watched);
    assert_eq!(db.series_progress(&parent.key, 100)?.fraction(), Some(1.0));
    Ok(())
}

#[test]
fn series_progress_aggregates_episode_completion_independently_of_resume_order()
-> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let parent = show();
    db.upsert_media(&parent)?;
    let first = episode(&parent, 1);
    let later = episode(&parent, 200);
    for item in [&first, &later] {
        db.upsert_media(item)?;
    }
    db.record_playback(&later.key, 40.0, 100.0, 40.0)?;
    db.record_playback(&first.key, 25.0, 100.0, 25.0)?;
    let released = vec![
        first.key.clone(),
        later.key.clone(),
        episode(&parent, 201).key,
    ];
    sync(&mut db, &parent, &released)?;
    let progress = db.series_progress(&parent.key, 100)?;
    assert_eq!(progress.released_episodes, 3);
    assert_eq!(progress.watched_episodes, 0);
    assert!((progress.fraction().context("missing fraction")? - 0.65 / 3.0).abs() < 0.001);
    db.record_playback(&first.key, 95.0, 100.0, 80.0)?;
    db.finish_playback(&first.key, None)?;
    let progress = db.series_progress(&parent.key, 100)?;
    assert_eq!(progress.watched_episodes, 1);
    assert!((progress.completed_equivalents - 1.4).abs() < 0.001);
    db.set_watched(&later.key, true)?;
    assert_eq!(db.series_progress(&parent.key, 100)?.watched_episodes, 2);
    db.set_watched(&first.key, false)?;
    let progress = db.series_progress(&parent.key, 100)?;
    assert_eq!(progress.watched_episodes, 1);
    assert!((progress.completed_equivalents - 1.0).abs() < 0.001);
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
    sync(&mut db, &parent, &released)?;
    assert!(db.series_poster_state(&parent.key, 100)?.0);
    assert!(!db.series_poster_state(&parent.key, -1)?.0);
    let unknown = episode(&parent, 251).key;
    released.push(unknown);
    sync(&mut db, &parent, &released)?;
    assert!(!db.series_poster_state(&parent.key, 100)?.0);
    released.pop();
    db.set_watched(&released[249], false)?;
    db.set_watched(&parent.key, true)?;
    assert!(db.series_poster_state(&parent.key, 100)?.0);
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
    assert!(sync(&mut db, &parent, std::slice::from_ref(&first.key))?);
    assert_eq!(
        db.list_library(true, MediaKind::Series, Page::default())?,
        vec![parent.clone()]
    );
    assert!(
        db.list_library(false, MediaKind::Series, Page::default())?
            .is_empty()
    );
    assert!(!sync(&mut db, &parent, std::slice::from_ref(&first.key))?);
    let new_release = episode(&parent, 2).key;
    assert!(sync(&mut db, &parent, &[first.key.clone(), new_release])?);
    assert!(
        db.list_library(true, MediaKind::Series, Page::default())?
            .is_empty()
    );
    assert_eq!(
        db.list_library(false, MediaKind::Series, Page::default())?,
        vec![parent.clone()]
    );
    db.set_watched(&parent.key, false)?;
    sync(&mut db, &parent, &[first.key])?;
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

#[test]
fn seeking_and_future_releases_do_not_inflate_show_watch_progress() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let parent = show();
    db.upsert_media(&parent)?;
    let first = episode(&parent, 1);
    let future = episode(&parent, 2);
    db.sync_episode_releases(
        &parent.key,
        &[
            EpisodeRelease {
                media: first.clone(),
                released_at: Some(100),
                season: Some(1),
                episode: Some(1),
            },
            EpisodeRelease {
                media: future.clone(),
                released_at: Some(200),
                season: Some(1),
                episode: Some(2),
            },
        ],
    )?;
    db.record_playback(&first.key, 95.0, 100.0, 10.0)?;
    let summary = db.series_progress(&parent.key, 150)?;
    assert_eq!(summary.released_episodes, 1);
    assert_eq!(summary.watched_episodes, 0);
    assert!((summary.completed_equivalents - 0.1).abs() < 0.001);
    assert!((summary.resume_equivalents - 0.95).abs() < 0.001);
    db.finish_playback(&first.key, None)?;
    assert!((db.series_progress(&parent.key, 150)?.completed_equivalents - 0.1).abs() < 0.001);
    db.set_watched(&first.key, true)?;
    assert!(!db.series_poster_state(&parent.key, 150)?.0);
    assert_eq!(db.series_progress(&parent.key, 150)?.fraction(), Some(0.5));
    let after_release = db.series_progress(&parent.key, 250)?;
    assert_eq!(after_release.released_episodes, 2);
    assert_eq!(after_release.watched_episodes, 1);
    assert!(
        (after_release
            .fraction()
            .context("missing released progress")?
            - 0.5)
            .abs()
            < 0.001
    );
    assert!(!db.series_poster_state(&parent.key, 250)?.0);
    db.set_watched(&parent.key, true)?;
    assert!(db.series_poster_state(&parent.key, 250)?.0);
    assert_eq!(db.series_progress(&parent.key, 250)?.watched_episodes, 2);
    db.set_watched(&future.key, false)?;
    assert!(!db.series_poster_state(&parent.key, 250)?.0);
    Ok(())
}

#[test]
fn stored_inventory_and_episode_actions_update_all_library_views_without_reconciliation()
-> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let parent = show();
    db.upsert_media(&parent)?;
    db.add_to_watchlist(&parent.key)?;
    let first = episode(&parent, 1);
    sync(&mut db, &parent, std::slice::from_ref(&first.key))?;
    db.set_watched(&first.key, true)?;
    assert!(db.watched_state(&parent.key)?.watched);
    assert_eq!(
        db.list_library(true, MediaKind::Series, Page::default())?,
        vec![parent.clone()]
    );
    assert!(db.list_up_next(100, Page::default())?.is_empty());
    db.set_watched(&first.key, false)?;
    assert!(!db.watched_state(&parent.key)?.watched);
    assert_eq!(
        db.list_library(false, MediaKind::Series, Page::default())?,
        vec![parent.clone()]
    );
    assert_eq!(db.list_up_next(100, Page::default())?[0].0.key, first.key);
    sync(&mut db, &parent, &[])?;
    assert!(!db.watched_state(&parent.key)?.watched);
    assert_eq!(db.series_progress(&parent.key, 100)?.fraction(), None);
    Ok(())
}
