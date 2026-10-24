use scry_library::{EpisodeRelease, Library, Media, MediaKey, MediaKind};

fn item(kind: MediaKind, id: &str, parent: Option<MediaKey>) -> Media {
    Media {
        key: MediaKey::new("test", kind, id),
        title: id.into(),
        parent,
    }
}

#[test]
fn future_movies_and_episodes_are_silent_no_ops_including_bulk_actions() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let movie = item(MediaKind::Movie, "future-movie", None);
    db.register_release(&movie, Some(i64::MAX))?;
    db.save_progress(&movie.key, 10.0, 100.0)?;
    db.set_watched(&movie.key, true)?;
    assert!(!db.watched_state(&movie.key)?.watched);
    assert!(db.load_progress(&movie.key)?.is_some());
    let show = item(MediaKind::Series, "show", None);
    db.upsert_media(&show)?;
    let released = item(MediaKind::Episode, "released", Some(show.key.clone()));
    let future = item(MediaKind::Episode, "future", Some(show.key.clone()));
    db.sync_episode_releases(
        &show.key,
        &[
            EpisodeRelease {
                media: released.clone(),
                released_at: 0,
                season: Some(1),
                episode: Some(1),
            },
            EpisodeRelease {
                media: future.clone(),
                released_at: i64::MAX,
                season: Some(1),
                episode: Some(2),
            },
        ],
    )?;
    db.set_watched(&future.key, true)?;
    db.set_episodes_watched(&show, &[released.clone(), future.clone()], true)?;
    assert!(db.watched_state(&released.key)?.watched);
    assert!(!db.watched_state(&future.key)?.watched);
    db.record_playback(&future.key, 80.0, 100.0, 80.0)?;
    assert!(!db.watched_state(&future.key)?.watched);
    Ok(())
}

#[test]
fn show_actions_cascade_to_released_episodes_and_new_releases_clear_completion()
-> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let show = item(MediaKind::Series, "show", None);
    db.upsert_media(&show)?;
    let episodes: Vec<_> = (1..=2)
        .map(|index| EpisodeRelease {
            media: item(
                MediaKind::Episode,
                &format!("episode-{index}"),
                Some(show.key.clone()),
            ),
            released_at: if index == 1 { 0 } else { i64::MAX },
            season: Some(1),
            episode: Some(index),
        })
        .collect();
    db.sync_episode_releases(&show.key, &episodes)?;
    db.set_watched(&show.key, true)?;
    assert!(db.watched_state(&show.key)?.watched);
    assert!(db.watched_state(&episodes[0].media.key)?.watched);
    assert!(!db.watched_state(&episodes[1].media.key)?.watched);
    let mut released = episodes;
    released[1].released_at = 0;
    db.sync_episode_releases(&show.key, &released)?;
    assert!(!db.watched_state(&show.key)?.watched);
    db.set_watched(&show.key, true)?;
    assert!(db.watched_state(&released[1].media.key)?.watched);
    db.set_watched(&show.key, false)?;
    assert!(!db.watched_state(&released[0].media.key)?.watched);
    assert!(!db.watched_state(&released[1].media.key)?.watched);
    db.record_playback(&released[0].media.key, 80.0, 100.0, 80.0)?;
    assert!(db.watched_state(&released[0].media.key)?.watched);
    Ok(())
}

#[test]
fn new_future_dates_clear_invalid_watched_flags_but_keep_resume_history() -> anyhow::Result<()> {
    let mut db = Library::memory()?;
    let movie = item(MediaKind::Movie, "movie", None);
    db.upsert_media(&movie)?;
    db.record_playback(&movie.key, 80.0, 100.0, 80.0)?;
    assert!(db.watched_state(&movie.key)?.watched);
    db.register_release(&movie, Some(i64::MAX))?;
    assert!(!db.watched_state(&movie.key)?.watched);
    assert!(db.load_progress(&movie.key)?.is_some());
    let show = item(MediaKind::Series, "show", None);
    let episode = item(MediaKind::Episode, "episode", Some(show.key.clone()));
    db.upsert_media_batch(&[show.clone(), episode.clone()])?;
    db.set_watched(&episode.key, true)?;
    db.sync_episode_releases(
        &show.key,
        &[EpisodeRelease {
            media: episode.clone(),
            released_at: i64::MAX,
            season: Some(1),
            episode: Some(1),
        }],
    )?;
    assert!(!db.watched_state(&episode.key)?.watched);
    Ok(())
}
