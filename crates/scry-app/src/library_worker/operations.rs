//! Synchronous database requests. This module does not depend on the UI.
use super::{Command, Event, SelectedState, Snapshot};
use anyhow::Result;
use scry_library::{Library, MAX_PAGE_SIZE, MediaKind, Page, WatchedState};

#[expect(
    clippy::too_many_lines,
    reason = "The exhaustive command match documents database effects and acknowledgement semantics together"
)]
pub(super) fn apply(library: &mut Library, command: Command) -> Result<Event> {
    match command {
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
        Command::Overview { generation, offset } => {
            return Ok(Event::Overview {
                generation,
                watching: library.list_currently_watching(Page::new(MAX_PAGE_SIZE, offset)?)?,
                series: library.list_calendar_series(Page::new(20, offset)?)?,
            });
        }
        Command::SavePlayback(item, position, duration, delta) => {
            library.upsert_media(&item)?;
            library.record_playback(&item.key, position, duration, delta)?;
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
                for key in episodes {
                    let watched = if library.get_media(&key)?.is_some() {
                        library.watched_state(&key)?
                    } else {
                        WatchedState {
                            watched: false,
                            manual_override: None,
                        }
                    };
                    episode_states.push((key, watched));
                }
            } else if let Some(series) = series {
                for episode in library.list_episodes(series, page)? {
                    let watched = library.watched_state(&episode.key)?;
                    episode_states.push((episode.key, watched));
                }
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
            for item in items {
                library.upsert_media(&item)?;
            }
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
            library.upsert_media(&item)?;
            library.set_watched(&item.key, watched)?;
        }
        Command::ClearOverride(item) => {
            library.upsert_media(&item)?;
            library.clear_watched_override(&item.key)?;
        }
        Command::Flush(token) => return Ok(Event::Flushed(token)),
    }
    Ok(Event::Changed)
}
