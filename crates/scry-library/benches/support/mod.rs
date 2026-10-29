use scry_library::{EpisodeRelease, Library, Media, MediaKey, MediaKind, Page};

pub const NOW: i64 = 1_700_000_000;

pub fn series(index: u32) -> Media {
    Media {
        key: MediaKey::new("benchmark", MediaKind::Series, format!("show-{index}")),
        title: format!("Series {index}"),
        parent: None,
    }
}

pub fn releases(parent: &MediaKey, count: u32) -> Vec<EpisodeRelease> {
    (0..count)
        .map(|index| EpisodeRelease {
            media: Media {
                key: MediaKey::new(
                    "benchmark",
                    MediaKind::Episode,
                    format!("{}-{index}", parent.id),
                ),
                title: format!("Episode {index}"),
                parent: Some(parent.clone()),
            },
            released_at: Some(if index % 10 == 9 {
                NOW + 86_400
            } else {
                NOW - 86_400
            }),
            season: Some(index / 20 + 1),
            episode: Some(index % 20 + 1),
        })
        .collect()
}

pub fn inventory(count: u32) -> anyhow::Result<(Library, MediaKey, Vec<EpisodeRelease>)> {
    let mut db = Library::memory()?;
    let show = series(0);
    db.upsert_media(&show)?;
    let episodes = releases(&show.key, count);
    db.sync_episode_releases(&show.key, &episodes)?;
    Ok((db, show.key, episodes))
}

pub fn library() -> anyhow::Result<Library> {
    let mut db = Library::memory()?;
    for index in 0..1_000 {
        let item = if index % 10 == 0 {
            series(index)
        } else {
            Media {
                key: MediaKey::new("benchmark", MediaKind::Movie, format!("movie-{index}")),
                title: format!("Movie {index}"),
                parent: None,
            }
        };
        db.upsert_media(&item)?;
        db.add_to_watchlist(&item.key)?;
        if item.key.kind == MediaKind::Series {
            let episodes = releases(&item.key, 100);
            db.sync_episode_releases(&item.key, &episodes)?;
            db.record_playback(&episodes[0].media.key, 600.0, 2_400.0, 600.0)?;
        } else if index % 3 == 0 {
            db.set_watched(&item.key, true)?;
        }
    }
    Ok(db)
}

pub fn home(db: &Library) -> anyhow::Result<()> {
    std::hint::black_box(db.list_up_next(NOW, Page::new(200, 0)?)?);
    std::hint::black_box(db.list_library(false, MediaKind::Movie, Page::new(200, 0)?)?);
    std::hint::black_box(db.list_library(false, MediaKind::Series, Page::new(200, 0)?)?);
    Ok(())
}
