//! Synchronous database requests. This module does not depend on the UI.
use super::{Command, Event, MovieState};
use anyhow::Result;
use scry_library::{Library, MAX_PAGE_SIZE, MediaKind, Page};

#[expect(
    clippy::too_many_lines,
    reason = "The exhaustive command match documents database effects and acknowledgement semantics together"
)]
pub(super) fn apply(library: &mut Library, command: Command) -> Result<Event> {
    match command {
        Command::ImportHistory { generation, input } => {
            return Ok(Event::HistoryImported(
                generation,
                library
                    .import_history(&input)
                    .map_err(|_| "Cannot import history. No changes were saved.".into()),
            ));
        }
        Command::Guide { generation, action } => {
            return Ok(
                match library.guide_update(action, chrono::Utc::now().timestamp()) {
                    Ok(data) => Event::Guide { generation, data },
                    Err(_) => Event::GuideError { generation },
                },
            );
        }
        Command::GuideMetadata(media, metadata, inventory) => {
            if media.key.kind == MediaKind::Movie {
                library.register_release(&media, metadata.released_at)?;
            }
            library.set_guide_metadata(&media, &metadata)?;
            if media.key.kind == MediaKind::Series && !inventory.is_empty() {
                library.sync_episode_releases(&media.key, &inventory)?;
            }
            return Ok(Event::GuideMetadataStored);
        }
        Command::Runtimes(values) => {
            library.set_known_runtimes(&values)?;
            return Ok(Event::RuntimesStored);
        }
        Command::MissingMovieRuntimes { generation, after } => {
            return Ok(Event::MissingMovieRuntimes {
                generation,
                items: library.movies_missing_runtime(after.as_ref(), 12)?,
            });
        }
        Command::NerdStats { generation } => {
            return Ok(
                match library.nerd_stats(scry_library::StatsPeriod::AllTime) {
                    Ok(data) => Event::NerdStats { generation, data },
                    Err(_) => Event::StatsError { generation },
                },
            );
        }
        Command::UpNext { generation, offset } => {
            let now = chrono::Utc::now().timestamp();
            let items = library.list_up_next(now, Page::new(MAX_PAGE_SIZE, offset)?)?;
            let has_more = items.len() == MAX_PAGE_SIZE as usize
                && !library
                    .list_up_next(now, Page::new(1, offset.saturating_add(MAX_PAGE_SIZE))?)?
                    .is_empty();
            return Ok(Event::UpNext {
                generation,
                items,
                has_more,
            });
        }
        Command::TrackedSeries { generation, offset } => {
            return Ok(Event::TrackedSeries {
                generation,
                items: library.list_tracked_series(Page::new(12, offset)?)?,
            });
        }
        Command::SyncSeries { key, releases } => {
            anyhow::ensure!(key.kind == MediaKind::Series, "Invalid series key");
            let changed = library.get_media(&key)?.is_some()
                && library.sync_episode_releases(&key, &releases)?;
            return Ok(Event::SeriesSynced { changed });
        }
        Command::SetEpisodesWatched {
            parent,
            episodes,
            inventory,
            watched,
        } => {
            library.set_episodes_watched_with_inventory(
                &parent,
                &episodes,
                Some(&inventory),
                watched,
            )?;
        }
        Command::SeriesStates { generation, keys } => {
            return Ok(Event::SeriesStates {
                generation,
                states: library.series_states(&keys, chrono::Utc::now().timestamp())?,
            });
        }
        Command::MovieStates { generation, keys } => {
            anyhow::ensure!(
                keys.len() <= MAX_PAGE_SIZE as usize,
                "Too many visible movies"
            );
            anyhow::ensure!(
                keys.iter().all(|key| key.kind == MediaKind::Movie),
                "Expected a movie"
            );
            let states = library
                .media_states(&keys)?
                .into_iter()
                .map(|state| MovieState {
                    watched: state.watched.watched,
                    progress: state.progress,
                    key: state.key,
                })
                .collect();
            return Ok(Event::MovieStates { generation, states });
        }
        Command::Browse {
            generation,
            watched,
            offset,
        } => {
            return Ok(Event::Browse {
                generation,
                items: library.list_library(watched, None, Page::new(MAX_PAGE_SIZE, offset)?)?,
            });
        }
        Command::Overview {
            generation,
            watching_offset,
        } => {
            let watching =
                library.list_currently_watching(Page::new(MAX_PAGE_SIZE, watching_offset)?)?;
            let keys: Vec<_> = watching
                .iter()
                .map(|(media, _)| media.key.clone())
                .collect();
            let watching_watched = library
                .media_states(&keys)?
                .into_iter()
                .filter(|state| state.watched.watched)
                .map(|state| state.key)
                .collect();
            return Ok(Event::Overview {
                generation,
                watching,
                watching_watched,
                next_release_at: library.next_release_at(chrono::Utc::now().timestamp())?,
                series: library.list_calendar_titles()?,
            });
        }
        Command::SavedStream { generation, key } => {
            return Ok(Event::SavedStream {
                generation,
                stream: library.saved_stream(&key)?,
            });
        }
        Command::SaveStream(item, stream) => {
            library.upsert_media(&item)?;
            library.save_stream(&item.key, &stream)?;
        }
        Command::SavePlayback(item, position, duration, delta) => {
            library.record_media_playback(&item, position, duration, delta)?;
        }
        Command::FinishPlayback(key, next) => library.finish_playback(&key, next.as_ref())?,
        Command::DismissWatching(key) => library.dismiss_currently_watching(&key)?,
        Command::Refresh {
            generation,
            selected,
            episodes,
        } => {
            return Ok(Event::Snapshot {
                generation,
                data: library.detail_snapshot(selected.as_ref(), &episodes)?,
            });
        }
        Command::Upsert(items) => {
            library.upsert_media_batch(&items)?;
        }
        Command::RegisterRelease(item, released_at) => {
            library.register_release(&item, released_at)?;
        }
        Command::ToggleWatchlist(item) => {
            library.update_media_watchlist(&item, None)?;
        }
        Command::SetWatchlist(item, added) => {
            library.update_media_watchlist(&item, Some(added))?;
        }
        Command::SetWatched(item, watched) => {
            library.set_media_watched(&item, None, None, watched)?;
        }
        Command::SetWatchedMetadata {
            item,
            released_at,
            inventory,
            watched,
        } => {
            library.set_media_watched(&item, released_at, inventory.as_deref(), watched)?;
        }
        Command::Flush(token) => return Ok(Event::Flushed(token)),
    }
    Ok(Event::Changed)
}
