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
        Command::Refresh {
            generation,
            selected,
            playlist,
            offset,
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
                    watchlist: library.is_in_watchlist(&media.key)?,
                    progress: library.load_progress(&media.key)?,
                    key: media.key,
                }),
                None => None,
            };
            return Ok(Event::Snapshot {
                generation,
                data: Snapshot {
                    watchlist: library.list_watchlist(Page::new(MAX_PAGE_SIZE, offset)?)?,
                    watched: library.list_watched(Page::new(MAX_PAGE_SIZE, offset)?)?,
                    playlists: library.list_playlists(page)?,
                    items: match playlist {
                        Some(id) => {
                            library.list_playlist_items(id, Page::new(MAX_PAGE_SIZE, offset)?)?
                        }
                        None => Vec::new(),
                    },
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
            library.upsert_media(&item)?;
            if library.is_in_watchlist(&item.key)? {
                library.remove_from_watchlist(&item.key)?;
            } else {
                library.add_to_watchlist(&item.key)?;
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
        Command::CreatePlaylist(name) => {
            library.create_playlist(&name)?;
        }
        Command::RenamePlaylist(id, name) => library.rename_playlist(id, &name)?,
        Command::DeletePlaylist(id) => library.delete_playlist(id)?,
        Command::AddToPlaylist(id, item) => {
            library.upsert_media(&item)?;
            library.add_playlist_item(id, &item.key)?;
        }
        Command::RemoveFromPlaylist(id, key) => library.remove_playlist_item(id, &key)?,
        Command::ReorderPlaylist(id, keys) => library.reorder_playlist(id, &keys)?,
        Command::SaveProgress(item, position, duration) => {
            library.upsert_media(&item)?;
            library.save_progress(&item.key, position, duration)?;
        }
        Command::Flush(token) => return Ok(Event::Flushed(token)),
    }
    Ok(Event::Changed)
}
