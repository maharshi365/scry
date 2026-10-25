//! Synchronous database requests. This module does not depend on the UI.
use super::{Command, Event, MovieState, SelectedState, SeriesState, Snapshot};
use anyhow::Result;
use scry_library::{Library, MAX_PAGE_SIZE, MediaKind, Page};

#[expect(
    clippy::too_many_lines,
    reason = "The exhaustive command match documents database effects and acknowledgement semantics together"
)]
pub(super) fn apply(library: &mut Library, command: Command) -> Result<Event> {
    match command {
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
            anyhow::ensure!(
                keys.len() <= MAX_PAGE_SIZE as usize
                    && keys.iter().all(|key| key.kind == MediaKind::Series),
                "Too many series poster keys"
            );
            let mut states = Vec::new();
            for state in library.media_states(&keys)? {
                let tracked = state.stored;
                let summary = if tracked {
                    Some(library.series_progress(&state.key, chrono::Utc::now().timestamp())?)
                } else {
                    None
                };
                let progress = summary.and_then(scry_library::SeriesProgress::normalized_progress);
                states.push(SeriesState {
                    key: state.key,
                    tracked,
                    watched: state.watched.watched,
                    progress,
                    summary,
                });
            }
            return Ok(Event::SeriesStates { generation, states });
        }
        Command::MovieStates { generation, keys } => {
            anyhow::ensure!(
                keys.len() <= MAX_PAGE_SIZE as usize,
                "Too many visible movies"
            );
            let mut states = Vec::new();
            anyhow::ensure!(
                keys.iter().all(|key| key.kind == MediaKind::Movie),
                "Expected a movie"
            );
            for state in library.media_states(&keys)? {
                states.push(MovieState {
                    watched: state.watched.watched,
                    progress: state.progress,
                    key: state.key,
                });
            }
            return Ok(Event::MovieStates { generation, states });
        }
        Command::Browse {
            generation,
            watched,
            kind,
            offset,
        } => {
            return Ok(Event::Browse {
                generation,
                items: library.list_library(watched, kind, Page::new(MAX_PAGE_SIZE, offset)?)?,
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
        Command::SavePlayback(item, position, duration, delta) => {
            library.record_media_playback(&item, position, duration, delta)?;
        }
        Command::FinishPlayback(key, next) => library.finish_playback(&key, next.as_ref())?,
        Command::Refresh {
            generation,
            selected,
            episodes,
        } => {
            let page = Page::new(MAX_PAGE_SIZE, 0)?;
            let selected_media = match selected {
                Some(key) => library.get_media(&key)?,
                None => None,
            };
            let series = selected_media
                .as_ref()
                .and_then(|media| match media.key.kind {
                    MediaKind::Series => Some(&media.key),
                    MediaKind::Episode => media.parent.as_ref(),
                    MediaKind::Movie => None,
                });
            let mut episode_states = Vec::new();
            anyhow::ensure!(
                episodes.len() <= MAX_PAGE_SIZE as usize,
                "Too many visible episodes"
            );
            if !episodes.is_empty() {
                episode_states = library
                    .media_states(&episodes)?
                    .into_iter()
                    .map(|state| (state.key, state.watched))
                    .collect();
            } else if let Some(series) = series {
                let keys: Vec<_> = library
                    .list_episodes(series, page)?
                    .into_iter()
                    .map(|media| media.key)
                    .collect();
                episode_states = library
                    .media_states(&keys)?
                    .into_iter()
                    .map(|state| (state.key, state.watched))
                    .collect();
            }
            let selected_state = match selected_media {
                Some(media) => Some(SelectedState {
                    watched: library.watched_state(&media.key)?,
                    watchlist: library
                        .is_in_watchlist(media.parent.as_ref().unwrap_or(&media.key))?,
                    progress: library.load_progress(&media.key)?,
                    key: media.key,
                }),
                None => None,
            };
            return Ok(Event::Snapshot {
                generation,
                data: Snapshot {
                    selected_state,
                    episode_states,
                },
            });
        }
        Command::Upsert(items) => {
            library.upsert_media_batch(&items)?;
        }
        Command::RegisterRelease(item, released_at) => {
            library.register_release(&item, released_at)?;
        }
        Command::ToggleWatchlist(item) => {
            let item = match &item.parent {
                Some(parent) => library
                    .get_media(parent)?
                    .ok_or_else(|| anyhow::anyhow!("missing series"))?,
                None => item,
            };
            library.upsert_media(&item)?;
            if library.is_in_watchlist(&item.key)? {
                library.remove_from_watchlist(&item.key)?;
            } else {
                library.add_to_watchlist(&item.key)?;
            }
        }
        Command::SetWatchlist(item, added) => {
            let item = match &item.parent {
                Some(parent) => library
                    .get_media(parent)?
                    .ok_or_else(|| anyhow::anyhow!("missing series"))?,
                None => item,
            };
            library.upsert_media(&item)?;
            if added {
                library.add_to_watchlist(&item.key)?;
            } else {
                library.remove_from_watchlist(&item.key)?;
            }
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
