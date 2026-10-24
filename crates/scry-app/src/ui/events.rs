//! Bounded service polling and generation-checked event application.
use super::{EPISODES_PER_PAGE, Page, State, app::headless, artwork::poster_columns};
use crate::services::{Event, Source};
use rune::{TextEdit, Tx};
use scry_addons::Meta;
use scry_app::platform::VideoHost;
use std::sync::Arc;

pub(super) fn poll(s: &mut State, _: f64, tx: &mut Tx) -> bool {
    let history = s.native_navigation.drain();
    let navigated = !history.is_empty();
    let fullscreen = s.player.fullscreen;
    for direction in history {
        s.travel(direction == scry_app::platform::HistoryDirection::Forward);
    }
    if s.player.fullscreen != fullscreen {
        tx.instant();
    }
    if let Some(anchor) = s.navigation.reveal.take() {
        if s.navigation.page == Page::Discover
            && let Some(index) = anchor
                .strip_prefix("title-")
                .and_then(|index| index.parse::<usize>().ok())
        {
            let columns = s.poster_viewport.map_or(3, |rect| poster_columns(rect[2]));
            tx.scroll_into_view(format!("title-row-{}", index / columns));
        } else if s.navigation.page == Page::Library
            && let Some(index) = anchor
                .strip_prefix("library-open-")
                .and_then(|index| index.parse::<usize>().ok())
        {
            let columns = s.poster_viewport.map_or(3, |rect| poster_columns(rect[2]));
            tx.scroll_into_view(format!("library-row-{}", index / columns));
        } else if s.navigation.page == Page::Watching
            && let Some(index) = anchor
                .strip_prefix("up-next-open-")
                .and_then(|index| index.parse::<usize>().ok())
        {
            let columns = s.poster_viewport.map_or(3, |rect| poster_columns(rect[2]));
            tx.scroll_into_view(format!("up-next-row-{}", index / columns));
        } else if s.navigation.page == Page::Watching
            && let Some(index) = anchor
                .strip_prefix("watching-open-")
                .and_then(|index| index.parse::<usize>().ok())
        {
            let columns = s.poster_viewport.map_or(3, |rect| poster_columns(rect[2]));
            tx.scroll_into_view(format!("watching-row-{}", index / columns));
        } else {
            tx.scroll_into_view(anchor);
        }
    }
    s.refresh_artwork();
    let mut changed = navigated
        | s.poll_library()
        | s.player_artwork
            .as_mut()
            .is_some_and(crate::artwork::Artwork::poll)
        | s.artwork
            .as_mut()
            .is_some_and(crate::artwork::Artwork::poll);
    if !headless()
        && s.services.is_some()
        && s.host.target().is_ok()
        && let Some(input) = s.startup_play.take()
    {
        if input.starts_with("http://") || input.starts_with("https://") {
            s.direct = TextEdit::new(input);
            s.play_direct();
        } else {
            s.save_library_progress();
            s.library.playing = None;
            s.player.meta_id = None;
            s.player.video_id = None;
            s.play_source(Source::Local(input.into()), "Local video".into());
        }
        changed = true;
    }
    for _ in 0..64 {
        let event = s.events.as_mut().and_then(|events| events.try_recv().ok());
        let Some(event) = event else {
            break;
        };
        changed = true;
        apply(s, event);
    }
    s.request_library_posters();
    s.request_series_states();
    s.request_series_tracking(false);
    changed
}

#[expect(
    clippy::too_many_lines,
    reason = "The exhaustive protocol match keeps each event beside its stale-generation guard"
)]
fn apply(s: &mut State, event: Event) {
    match event {
        Event::SeriesTrackingDetails(generation, found, done) => {
            s.apply_series_tracking_details(generation, found, done);
        }
        Event::SeriesDetails(generation, found, done) => {
            s.apply_series_details(generation, found, done);
        }
        Event::LibraryPosters(generation, found, done) => {
            s.apply_library_posters(generation, found, done);
        }
        Event::Calendar(generation, found, failed) => s.apply_calendar(generation, found, failed),
        Event::Addons(result) => {
            s.config_busy = false;
            match result {
                Ok(addons) => {
                    s.retry_library_posters();
                    s.library.posters.cache.clear();
                    s.addons = addons;
                    s.request_series_tracking(true);
                    s.addon_install_open = false;
                    if s.navigation.page == Page::Calendar {
                        s.fetch_calendar();
                    }
                    if s.catalog.as_ref().is_none_or(|(a, _)| {
                        !s.addons
                            .iter()
                            .any(|current| current.manifest.id == a.manifest.id)
                    }) {
                        s.catalog = s.catalogs().into_iter().next();
                        if s.navigation.page == Page::Discover {
                            s.fetch_catalog();
                        }
                    }
                }
                Err(error) => s.error = Some(error),
            }
        }
        Event::Catalog(id, result) if id == s.catalog_id && s.navigation.page == Page::Discover => {
            s.loading = false;
            match result {
                Ok(metas) => {
                    s.status.clear();
                    s.metas = Arc::new(metas);
                }
                Err(error) => s.error = Some(error),
            }
        }
        Event::Search(id, results, warnings, done)
            if id == s.catalog_id && s.navigation.page == Page::Discover =>
        {
            s.loading = !done;
            s.providers = Arc::new(results.iter().map(|result| result.addon.clone()).collect());
            s.metas = Arc::new(results.into_iter().map(|result| result.meta).collect());
            s.warnings = warnings;
            s.status = if done { "" } else { "Searching…" }.into();
        }
        Event::Detail(id, result) if id == s.detail_id && s.navigation.page == Page::Detail => {
            s.loading = false;
            match result {
                Ok((meta, namespace)) => {
                    s.detail_namespace = namespace;
                    apply_detail(s, meta);
                }
                Err(error) => s.error = Some(error),
            }
        }
        Event::Streams(id, streams, warnings, done)
            if id == s.stream_id && s.navigation.page == Page::Detail =>
        {
            s.loading = !done;
            s.streams = streams;
            s.warnings = warnings;
            s.status = if !done {
                format!("{} streams · loading…", s.streams.len())
            } else if s.streams.is_empty() {
                "No streams found. Install a compatible stream add-on, or try another episode."
                    .into()
            } else {
                format!("{} streams", s.streams.len())
            };
        }
        Event::Playing(id, path) if id == s.player.generation => {
            s.player.report = Some(path);
            s.status = "Loading…".into();
            let volume = s.player.volume.to_string();
            s.command(&["set", "volume", &volume]);
        }
        Event::Ready(id) if id == s.player.generation && s.player.playing => {
            s.player.ready = true;
            if let Some(position) = s.library.resume_position.take() {
                s.command(&["seek", &position.to_string(), "absolute"]);
                s.library.observed = None;
            }
            if s.navigation.page == Page::Player {
                s.host.show();
            }
            s.status.clear();
        }
        Event::Metrics(id, sample) if id == s.player.generation => {
            s.status = if sample.buffering {
                "Buffering"
            } else if sample.paused {
                "Paused"
            } else if sample.position_s.is_none() {
                "Opening media…"
            } else {
                "Playing"
            }
            .into();
            s.player.sample = sample;
            s.observe_library_progress();
        }
        Event::Stopped(id, error) if id == s.player.generation => {
            s.set_fullscreen(false);
            s.finish_library_progress(error.is_none());
            s.library.playing = None;
            s.refresh_library();
            s.player.playing = false;
            s.player.ready = false;
            s.player.seek_preview = None;
            s.host.hide();
            s.status = "Playback stopped".into();
            if error.is_some() {
                s.error = error;
            }
        }
        Event::PlayerWarning(id, error) if id == s.player.generation => s.error = Some(error),
        _ => {}
    }
}

fn apply_detail(s: &mut State, meta: Meta) {
    let movie = meta.kind == "movie";
    let (kind, id) = (meta.kind.clone(), meta.id.clone());
    let episode = s.pending_episode.take().filter(|_| !movie);
    if let Some(index) = episode
        .as_ref()
        .and_then(|id| meta.videos.iter().position(|video| &video.id == id))
    {
        s.episode_season = Some(meta.videos[index].season);
        let mut indices: Vec<_> = meta
            .videos
            .iter()
            .enumerate()
            .filter(|(_, video)| video.season == meta.videos[index].season)
            .map(|(index, _)| index)
            .collect();
        indices.sort_by_key(|index| meta.videos[*index].episode);
        let position = indices
            .iter()
            .position(|value| *value == index)
            .unwrap_or(0);
        s.episode_offset = position / EPISODES_PER_PAGE * EPISODES_PER_PAGE;
    }
    s.selected_video_id.clone_from(&episode);
    s.detail = Some(meta);
    s.register_library_detail();
    s.remember_series_detail();
    s.status = "Choose an episode to find streams.".into();
    if movie {
        s.find_streams(kind, id);
    } else if let Some(episode) = episode {
        s.find_streams(kind, episode);
    }
}
