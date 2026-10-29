use scry_library::guide::{EpisodeOrder, GuideAction, SourcePolicy};
use scry_library::{EpisodeRelease, Library, Media, MediaKey, MediaKind, Page};

fn media(namespace: &str, kind: MediaKind, id: &str) -> Media {
    Media {
        key: MediaKey::new(namespace, kind, id),
        title: id.into(),
        parent: None,
    }
}

#[test]
fn opening_rejects_guide_tables_with_missing_identity_columns() -> anyhow::Result<()> {
    use diesel::{Connection, SqliteConnection, connection::SimpleConnection};

    for (table, identity) in [("guide_state", "id"), ("guide_metadata", "media_id")] {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("library.sqlite3");
        drop(Library::open(&path)?);
        let mut connection = SqliteConnection::establish(
            path.to_str()
                .ok_or_else(|| anyhow::anyhow!("invalid test path"))?,
        )?;
        connection.batch_execute(&format!(
            "DROP TABLE {table}; CREATE TABLE {table}(payload TEXT NOT NULL);"
        ))?;
        assert!(
            Library::open(&path).is_err(),
            "missing {table}.{identity} must fail validation"
        );
    }
    Ok(())
}

#[test]
fn invalid_watchlist_metadata_cannot_change_parent_membership() -> anyhow::Result<()> {
    let mut library = Library::memory()?;
    let parent = media("test", MediaKind::Series, "parent");
    library.upsert_media(&parent)?;
    let mut invalid = media("test", MediaKind::Movie, "child");
    invalid.parent = Some(parent.key.clone());
    assert!(
        library
            .update_media_watchlist(&invalid, Some(true))
            .is_err()
    );
    assert!(!library.is_in_watchlist(&parent.key)?);
    assert!(library.get_media(&invalid.key)?.is_none());

    invalid.key.kind = MediaKind::Episode;
    invalid.title.clear();
    assert!(
        library
            .update_media_watchlist(&invalid, Some(true))
            .is_err()
    );
    assert!(!library.is_in_watchlist(&parent.key)?);
    assert!(library.list_watchlist(Page::default())?.is_empty());
    Ok(())
}

#[test]
fn guide_rejects_mixed_and_episode_playlists_without_enabling_channels() -> anyhow::Result<()> {
    let mut library = Library::memory()?;
    let show = media("test", MediaKind::Series, "show");
    let movie = media("test", MediaKind::Movie, "movie");
    let episode = media("test", MediaKind::Episode, "episode");
    library.upsert_media_batch(&[show.clone(), movie.clone(), episode.clone()])?;
    let mixed = library.create_playlist("Mixed")?;
    library.add_playlist_item(mixed.id, &show.key)?;
    library.add_playlist_item(mixed.id, &movie.key)?;
    let episodes = library.create_playlist("Episodes")?;
    library.add_playlist_item(episodes.id, &episode.key)?;
    for id in [mixed.id, episodes.id] {
        assert!(
            library
                .guide_update(GuideAction::EnableCollection(id), 10_000)
                .is_err()
        );
    }
    assert!(
        library
            .guide_update(GuideAction::Refresh, 10_000)?
            .channels
            .is_empty()
    );
    assert_eq!(
        library.list_playlist_items(mixed.id, Page::default())?,
        vec![show, movie]
    );
    Ok(())
}

#[test]
fn guide_keeps_delimiter_containing_series_identities_separate() -> anyhow::Result<()> {
    let mut library = Library::memory()?;
    for (namespace, id) in [("a", "b:c"), ("a:b", "c")] {
        let show = media(namespace, MediaKind::Series, id);
        library.upsert_media(&show)?;
        library.add_to_watchlist(&show.key)?;
        let releases: Vec<_> = (1..=2)
            .map(|number| {
                let mut episode = media(namespace, MediaKind::Episode, &format!("{id}-{number}"));
                episode.parent = Some(show.key.clone());
                EpisodeRelease {
                    media: episode,
                    released_at: Some(0),
                    season: Some(1),
                    episode: Some(number),
                }
            })
            .collect();
        library.sync_episode_releases(&show.key, &releases)?;
    }
    let snapshot = library.guide_update(
        GuideAction::Policy(SourcePolicy {
            order: EpisodeOrder::Sequential,
            ..SourcePolicy::default()
        }),
        10_000,
    )?;
    let channel = snapshot
        .channels
        .iter()
        .find(|channel| channel.id == "library-tv")
        .ok_or_else(|| anyhow::anyhow!("missing library channel"))?;
    assert!(channel.slots.len() > 2);
    assert!(
        channel
            .slots
            .windows(2)
            .all(|pair| pair[0].program.media.parent != pair[1].program.media.parent),
        "distinct shows must alternate even when their joined identity strings collide"
    );
    Ok(())
}

#[test]
fn guide_uses_known_runtime_when_next_episode_has_placeholder_progress() -> anyhow::Result<()> {
    let mut library = Library::memory()?;
    let show = media("test", MediaKind::Series, "show");
    library.upsert_media(&show)?;
    library.add_to_watchlist(&show.key)?;
    let mut first = media("test", MediaKind::Episode, "first");
    first.parent = Some(show.key.clone());
    let mut next = media("test", MediaKind::Episode, "next");
    next.parent = Some(show.key.clone());
    let releases: Vec<_> = [first.clone(), next.clone()]
        .into_iter()
        .zip(1..=2)
        .map(|(media, episode)| EpisodeRelease {
            media,
            released_at: Some(0),
            season: Some(1),
            episode: Some(episode),
        })
        .collect();
    library.sync_episode_releases(&show.key, &releases)?;
    library.set_known_runtimes(&[(next.key.clone(), 3300.0)])?;
    library.record_playback(&first.key, 3290.0, 3300.0, 3290.0)?;
    library.finish_playback(&first.key, Some(&next))?;

    let snapshot = library.guide_update(GuideAction::Refresh, 10_000)?;
    let program = snapshot
        .channels
        .iter()
        .flat_map(|channel| &channel.slots)
        .find(|slot| slot.program.media.key == next.key)
        .map(|slot| &slot.program)
        .ok_or_else(|| anyhow::anyhow!("next episode is missing from the guide"))?;
    assert_eq!(program.duration, 3300);
    assert!(
        !program.estimated,
        "provider runtime must not be treated as an estimate"
    );
    Ok(())
}
