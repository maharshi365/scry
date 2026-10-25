use super::*;
use crate::{Library, MediaKey};

fn show(library: &mut Library, id: &str, episodes: u32) -> Media {
    let parent = Media {
        key: MediaKey::new("test", MediaKind::Series, id),
        title: id.into(),
        parent: None,
    };
    library.upsert_media(&parent).unwrap();
    library.add_to_watchlist(&parent.key).unwrap();
    let inventory: Vec<_> = (1..=episodes)
        .map(|episode| crate::EpisodeRelease {
            media: Media {
                key: MediaKey::new("test", MediaKind::Episode, format!("{id}:{episode}")),
                title: format!("{id} episode {episode}"),
                parent: Some(parent.key.clone()),
            },
            released_at: Some(0),
            season: Some(1),
            episode: Some(episode),
        })
        .collect();
    library
        .sync_episode_releases(&parent.key, &inventory)
        .unwrap();
    parent
}

#[test]
fn published_schedule_survives_refresh_membership_changes_and_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("guide.sqlite3");
    let mut library = Library::open(&path).unwrap();
    let parent = show(&mut library, "office", 8);
    let snapshot = library
        .guide_update(GuideAction::CreateCollection("Slop TV".into()), 10_000)
        .unwrap();
    let collection = snapshot.collections.first().unwrap().playlist.id;
    library
        .guide_update(
            GuideAction::Membership {
                collection,
                media: parent,
                included: true,
            },
            10_000,
        )
        .unwrap();
    let first = library
        .guide_update(GuideAction::EnableCollection(collection), 10_000)
        .unwrap();
    let channel = first.channels.iter().find(|c| c.name == "Slop TV").unwrap();
    let slots = channel.slots.clone();
    assert!(!slots.is_empty());
    let current = channel.current(10_000).unwrap();
    assert!(
        current.starts_at < 10_000,
        "initialization must join an already-running program"
    );
    assert!(current.ends_at() > 10_000);
    assert!(
        slots
            .windows(2)
            .all(|pair| pair[0].ends_at() == pair[1].starts_at)
    );
    drop(library);
    let mut reopened = Library::open(path).unwrap();
    let refreshed = reopened.guide_update(GuideAction::Refresh, 10_001).unwrap();
    assert_eq!(
        refreshed
            .channels
            .iter()
            .find(|c| c.name == "Slop TV")
            .unwrap()
            .slots,
        slots
    );
}

#[test]
fn comfort_uses_only_watched_released_regular_episodes_and_movies_are_separate() {
    let mut library = Library::memory().unwrap();
    let parent = show(&mut library, "office", 4);
    let watched = MediaKey::new("test", MediaKind::Episode, "office:1");
    library.set_watched(&watched, true).unwrap();
    let future = crate::EpisodeRelease {
        media: Media {
            key: MediaKey::new("test", MediaKind::Episode, "future"),
            title: "Future".into(),
            parent: Some(parent.key.clone()),
        },
        released_at: Some(100_000),
        season: Some(1),
        episode: Some(5),
    };
    let special = crate::EpisodeRelease {
        media: Media {
            key: MediaKey::new("test", MediaKind::Episode, "special"),
            title: "Special".into(),
            parent: Some(parent.key),
        },
        released_at: Some(0),
        season: Some(0),
        episode: Some(1),
    };
    library
        .register_release(&future.media, future.released_at)
        .unwrap();
    library
        .register_release(&special.media, special.released_at)
        .unwrap();
    let snapshot = library.guide_update(GuideAction::Refresh, 10_000).unwrap();
    let comfort = snapshot
        .channels
        .iter()
        .find(|c| c.id == "comfort")
        .unwrap();
    assert!(!comfort.slots.is_empty());
    assert!(comfort.slots.iter().all(|s| s.program.media.key == watched));
    let mix = snapshot
        .channels
        .iter()
        .find(|c| c.id == "library-tv")
        .unwrap();
    assert!(
        mix.slots
            .iter()
            .all(|s| s.program.media.key.kind == MediaKind::Episode
                && s.program.media.key.id.starts_with("office:"))
    );
}

#[test]
fn shuffle_balances_shows_and_avoids_repeats_until_pool_is_exhausted() {
    let mut library = Library::memory().unwrap();
    show(&mut library, "large", 150);
    show(&mut library, "small", 4);
    let snapshot = library.guide_update(GuideAction::Refresh, 10_000).unwrap();
    let slots = &snapshot
        .channels
        .iter()
        .find(|c| c.id == "library-tv")
        .unwrap()
        .slots;
    assert!(
        slots
            .windows(2)
            .all(|pair| pair[0].program.media.parent != pair[1].program.media.parent)
    );
    let small: Vec<_> = slots
        .iter()
        .filter(|slot| {
            slot.program
                .media
                .parent
                .as_ref()
                .is_some_and(|key| key.id == "small")
        })
        .take(4)
        .map(|slot| &slot.program.media.key)
        .collect();
    assert_eq!(small.len(), 4);
    assert_eq!(
        small.iter().collect::<std::collections::HashSet<_>>().len(),
        4
    );
}

#[test]
fn invalid_collection_changes_leave_existing_configuration_intact() {
    let mut library = Library::memory().unwrap();
    assert!(
        library
            .guide_update(GuideAction::CreateCollection("  ".into()), 10_000)
            .is_err()
    );
    let parent = show(&mut library, "office", 2);
    let snapshot = library
        .guide_update(GuideAction::CreateCollection("TV".into()), 10_000)
        .unwrap();
    let collection = snapshot.collections[0].playlist.id;
    library
        .guide_update(
            GuideAction::Membership {
                collection,
                media: parent,
                included: true,
            },
            10_000,
        )
        .unwrap();
    let movie = Media {
        key: MediaKey::new("test", MediaKind::Movie, "film"),
        title: "Film".into(),
        parent: None,
    };
    assert!(
        library
            .guide_update(
                GuideAction::Membership {
                    collection,
                    media: movie,
                    included: true
                },
                10_000
            )
            .is_err()
    );
    let snapshot = library.guide_update(GuideAction::Refresh, 10_000).unwrap();
    assert_eq!(snapshot.collections[0].members.len(), 1);
}

#[test]
fn first_tune_has_clock_progress_and_days_offline_do_not_restart_the_channel() {
    let mut library = Library::memory().unwrap();
    show(&mut library, "office", 8);
    let initial = library.guide_update(GuideAction::Refresh, 10_123).unwrap();
    let channel = initial
        .channels
        .iter()
        .find(|c| c.id == "library-tv")
        .unwrap();
    let current = channel.current(10_123).unwrap();
    assert_eq!(current.elapsed_seconds(10_123), 1123);
    assert_eq!(current.elapsed_seconds(current.starts_at - 1), 0);
    assert_eq!(
        current.elapsed_seconds(current.ends_at() + 1),
        current.program.duration
    );
    let later = 4 * HORIZON + 10_123;
    let snapshot = library.guide_update(GuideAction::Refresh, later).unwrap();
    let channel = snapshot
        .channels
        .iter()
        .find(|c| c.id == "library-tv")
        .unwrap();
    let slot = channel.current(later).unwrap();
    assert_eq!(slot.elapsed_seconds(later), 1123);
    assert!(
        channel
            .slots
            .windows(2)
            .all(|pair| pair[0].ends_at() == pair[1].starts_at)
    );
}

#[test]
fn movies_have_their_own_channels_and_genres_require_two_matching_shows() {
    let mut library = Library::memory().unwrap();
    for id in ["office", "himym"] {
        let media = show(&mut library, id, 4);
        library
            .set_guide_metadata(
                &media,
                &GuideMetadata {
                    genres: vec!["Comedy".into()],
                    ..GuideMetadata::default()
                },
            )
            .unwrap();
    }
    let movie = Media {
        key: MediaKey::new("test", MediaKind::Movie, "film"),
        title: "Film".into(),
        parent: None,
    };
    library.register_release(&movie, Some(0)).unwrap();
    library.add_to_watchlist(&movie.key).unwrap();
    let snapshot = library.guide_update(GuideAction::Refresh, 10_000).unwrap();
    let genre = snapshot
        .channels
        .iter()
        .find(|c| c.source == ChannelSource::Genre("Comedy".into()))
        .unwrap();
    assert!(!genre.slots.is_empty());
    assert!(
        genre
            .slots
            .iter()
            .all(|slot| slot.program.media.key.kind == MediaKind::Episode)
    );
    let movies = snapshot
        .channels
        .iter()
        .find(|c| c.id == "library-movies")
        .unwrap();
    assert!(!movies.slots.is_empty());
    assert!(
        movies
            .slots
            .iter()
            .all(|slot| slot.program.media.key == movie.key)
    );
}

#[test]
fn sequential_programming_and_duration_updates_do_not_change_published_slots() {
    let mut library = Library::memory().unwrap();
    let parent = show(&mut library, "office", 4);
    let snapshot = library
        .guide_update(GuideAction::CreateCollection("Office".into()), 10_000)
        .unwrap();
    let collection = snapshot.collections[0].playlist.id;
    library
        .guide_update(
            GuideAction::Membership {
                collection,
                media: parent,
                included: true,
            },
            10_000,
        )
        .unwrap();
    let initial = library
        .guide_update(GuideAction::EnableCollection(collection), 10_000)
        .unwrap();
    let id = format!("collection-{collection}");
    let slots = initial
        .channels
        .iter()
        .find(|c| c.id == id)
        .unwrap()
        .slots
        .clone();
    library
        .set_known_runtimes(&[(
            MediaKey::new("test", MediaKind::Episode, "office:1"),
            1200.0,
        )])
        .unwrap();
    let snapshot = library.guide_update(GuideAction::Refresh, 10_000).unwrap();
    assert_eq!(
        snapshot.channels.iter().find(|c| c.id == id).unwrap().slots,
        slots
    );
    let after = slots.last().unwrap().ends_at() + 4 * HORIZON;
    let snapshot = library
        .guide_update(
            GuideAction::Policy(SourcePolicy {
                order: EpisodeOrder::Sequential,
                ..SourcePolicy::default()
            }),
            after,
        )
        .unwrap();
    let channel = snapshot.channels.iter().find(|c| c.id == id).unwrap();
    assert!(
        channel
            .slots
            .windows(2)
            .all(|pair| pair[1].program.episode == Some(pair[0].program.episode.unwrap() % 4 + 1))
    );
    assert!(
        channel
            .slots
            .iter()
            .filter(|s| s.program.episode == Some(1))
            .all(|s| s.program.duration == 1200)
    );
}

#[test]
fn shared_programming_updates_all_channels_and_new_channels_inherit_it() {
    let mut library = Library::memory().unwrap();
    let office = show(&mut library, "office", 4);
    show(&mut library, "himym", 4);
    library
        .set_watched(&MediaKey::new("test", MediaKind::Episode, "office:2"), true)
        .unwrap();
    library
        .set_watched(&MediaKey::new("test", MediaKind::Episode, "himym:3"), true)
        .unwrap();
    let initial = library.guide_update(GuideAction::Refresh, 10_000).unwrap();
    let current = initial
        .channels
        .iter()
        .find(|c| c.id == "library-tv")
        .unwrap()
        .current(10_000)
        .unwrap()
        .clone();
    let snapshot = library
        .guide_update(
            GuideAction::Policy(SourcePolicy {
                order: EpisodeOrder::Sequential,
                watched_only: true,
                ..SourcePolicy::default()
            }),
            10_000,
        )
        .unwrap();
    assert_eq!(
        snapshot
            .channels
            .iter()
            .find(|c| c.id == "library-tv")
            .unwrap()
            .current(10_000),
        Some(&current)
    );
    for channel in &snapshot.channels {
        assert!(
            channel
                .slots
                .iter()
                .filter(|s| s.starts_at > 10_000)
                .all(|s| matches!(s.program.media.key.id.as_str(), "office:2" | "himym:3"))
        );
    }
    let snapshot = library
        .guide_update(GuideAction::CreateCollection("Office".into()), 10_000)
        .unwrap();
    let collection = snapshot.collections[0].playlist.id;
    library
        .guide_update(
            GuideAction::Membership {
                collection,
                media: office,
                included: true,
            },
            10_000,
        )
        .unwrap();
    let snapshot = library
        .guide_update(GuideAction::EnableCollection(collection), 10_000)
        .unwrap();
    let channel = snapshot
        .channels
        .iter()
        .find(|c| c.source == ChannelSource::Collection(collection))
        .unwrap();
    assert!(!channel.slots.is_empty());
    assert!(
        channel
            .slots
            .iter()
            .all(|s| s.program.media.key.id == "office:2")
    );
    assert!(snapshot.policy.watched_only);
    assert_eq!(snapshot.policy.order, EpisodeOrder::Sequential);
}

#[test]
fn undated_future_and_special_episodes_never_enter_new_schedules() {
    let mut library = Library::memory().unwrap();
    let parent = show(&mut library, "office", 1);
    let inventory: Vec<_> = [
        ("released", Some(0), 1),
        ("future", Some(100_000), 1),
        ("undated", None, 1),
        ("special", Some(0), 0),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (id, released_at, season))| crate::EpisodeRelease {
        media: Media {
            key: MediaKey::new("test", MediaKind::Episode, id),
            title: id.into(),
            parent: Some(parent.key.clone()),
        },
        released_at,
        season: Some(season),
        episode: Some(u32::try_from(index + 1).unwrap()),
    })
    .collect();
    library
        .sync_episode_releases(&parent.key, &inventory)
        .unwrap();
    let snapshot = library.guide_update(GuideAction::Refresh, 10_000).unwrap();
    let slots = &snapshot
        .channels
        .iter()
        .find(|c| c.id == "library-tv")
        .unwrap()
        .slots;
    assert!(!slots.is_empty());
    assert!(
        slots
            .iter()
            .all(|slot| slot.program.media.key.id == "released")
    );
}
