//! Bounded service polling and generation-checked event application.
use super::{EPISODES_PER_PAGE, Page, State, app::headless};
use crate::services::{Event, Source};
use rune::{TextEdit, Tx};
use scry_addons::Meta;
use scry_app::platform::VideoHost;

pub(super) fn poll(s: &mut State, _: f64, tx: &mut Tx) -> bool {
    let history = s.shell.native_navigation.drain();
    let navigated = !history.is_empty();
    let fullscreen = s.player.fullscreen;
    for direction in history {
        s.travel(direction == scry_app::platform::HistoryDirection::Forward);
    }
    if s.player.fullscreen != fullscreen {
        tx.instant();
    }
    if let Some(anchor) = s.navigation.reveal.take() {
        super::poster_grid::reveal(s, anchor, tx);
    }
    s.refresh_artwork();
    let mut changed = s.poll_player_interaction(tx)
        | navigated
        | s.poll_library()
        | s.artwork
            .player
            .as_mut()
            .is_some_and(crate::artwork::Artwork::poll)
        | s.artwork
            .posters
            .as_mut()
            .is_some_and(crate::artwork::Artwork::poll);
    changed |= s.poll_achievement_toast(tx);
    changed |= s.poll_guide(tx);
    if let Some(release) = s.library.next_release_at {
        let now = chrono::Utc::now().timestamp();
        if release <= now {
            s.library.next_release_at = None;
            s.refresh_library();
            changed = true;
        } else {
            tx.wake_after(std::time::Duration::from_secs(
                u64::try_from(release - now).unwrap_or(u64::MAX).min(3600),
            ));
        }
    }
    if !headless()
        && s.runtime.services.is_some()
        && s.shell.host.target().is_ok()
        && let Some(input) = s.runtime.startup_play.take()
    {
        if input.starts_with("http://") || input.starts_with("https://") {
            s.addons.direct = TextEdit::new(input);
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
        let event = s
            .runtime
            .events
            .as_mut()
            .and_then(|events| events.try_recv().ok());
        let Some(event) = event else {
            break;
        };
        changed = true;
        apply(s, event);
    }
    changed |= request_library_updates(s, tx);
    if let Some(delay) = s.metadata_retry_after() {
        tx.wake_after(delay);
    }
    #[cfg(feature = "profiling")]
    {
        changed |= super::profiling::poll(s, tx);
    }
    changed | s.navigation.present(tx)
}

fn request_library_updates(s: &mut State, tx: &mut Tx) -> bool {
    s.poll_movie_runtimes(tx);
    s.request_library_posters();
    s.request_series_states();
    s.request_series_tracking(false);
    s.flush_known_runtimes()
}

#[expect(
    clippy::too_many_lines,
    reason = "The exhaustive protocol match keeps each event beside its stale-generation guard"
)]
fn apply(s: &mut State, event: Event) {
    match event {
        Event::TorrentCacheCleared(result) => {
            s.settings.streaming.clearing = false;
            s.settings.streaming.cache_error = result.is_err();
            s.settings.streaming.cache_message = Some(match result {
                Ok(()) => "Downloaded torrent data has been removed.".into(),
                Err(error) => error,
            });
        }
        Event::StreamingPreferences(result) => {
            s.settings.streaming.busy = false;
            match result {
                Ok(preferences) => {
                    s.settings.streaming.preferences = preferences;
                    s.settings.streaming.error = None;
                    s.settings.streaming.pending = None;
                }
                Err(error) => s.settings.streaming.error = Some(error),
            }
        }
        Event::SubtitlePreferences(result) => {
            s.settings.subtitles.busy = false;
            match result {
                Ok(preferences) => s.settings.subtitle_preferences = preferences,
                Err(error) => s.settings.subtitles.error = Some(error),
            }
            if s.player.playing {
                s.player.subtitles.defaults = s.settings.subtitle_preferences.clone();
                s.apply_subtitle_appearance();
            }
        }
        Event::SubtitlePreferencesSaved(result) => {
            s.settings.subtitles.busy = false;
            s.settings.subtitles.error = result.err();
        }
        Event::Subtitles(id, snapshot) => s.apply_subtitle_snapshot(id, snapshot),
        Event::IntroSegments(id, intro) => {
            if id == s.player.generation && s.player.playing {
                s.player.intro = intro;
            }
        }
        Event::Audio(id, snapshot) => s.apply_audio_snapshot(id, snapshot),
        Event::SubtitleSources(id, sources, done, failed) => {
            s.apply_subtitle_sources(id, sources, done, failed);
        }
        Event::SubtitleFile(id, request, file) => s.apply_subtitle_file(id, request, file),
        Event::SubtitleLoadFailed {
            generation,
            request,
            error,
        } => s.apply_subtitle_load_failed(generation, request, error),
        Event::GuideDetails(generation, found) => s.apply_guide_details(generation, found),
        Event::WatchMetadata(token, media, watched, result) => {
            s.apply_watch_metadata(token, media, watched, result);
        }
        Event::SeriesTrackingDetails(generation, found, done) => {
            s.apply_series_tracking_details(generation, found, done);
        }
        Event::MovieRuntimeDetails(generation, found, done) => {
            s.apply_movie_runtime_details(generation, found, done);
        }
        Event::KnownRuntimes(values) => s.queue_known_runtimes(values),
        Event::SeriesDetails(generation, found, done) => {
            s.apply_series_details(generation, found, done);
        }
        Event::LibraryPosters(generation, found, done) => {
            s.apply_library_posters(generation, found, done);
        }
        Event::Calendar(generation, found, failed) => s.apply_calendar(generation, found, failed),
        Event::Addons(result) => {
            s.addons.busy = false;
            s.addons.pending = None;
            match result {
                Ok(addons) => {
                    if s.addons.install_open {
                        s.addons.status = "Add-on installed successfully".into();
                        s.addons.install_complete = true;
                    }
                    s.retry_library_posters();
                    s.library.posters.cache.clear();
                    s.addons.installed = addons;
                    s.reset_movie_runtimes();
                    s.request_series_tracking(true);
                    if s.navigation.page == Page::Calendar {
                        s.fetch_calendar();
                    }
                    let catalogs = s.catalogs();
                    s.discover.catalog = s
                        .discover
                        .catalog
                        .as_ref()
                        .and_then(|(addon, catalog)| {
                            catalogs
                                .iter()
                                .find(|(a, c)| {
                                    a.manifest.id == addon.manifest.id
                                        && c.id == catalog.id
                                        && c.kind == catalog.kind
                                })
                                .cloned()
                        })
                        .or_else(|| catalogs.into_iter().next());
                    if s.navigation.page == Page::Discover {
                        s.fetch_catalog();
                    }
                }
                Err(error) => s.addons.error = Some(error),
            }
        }
        Event::Catalog(id, offset, result) => s.apply_catalog_page(id, offset, result),
        Event::Search(id, results, warnings, done)
            if id == s.discover.generation && s.navigation.page == Page::Discover =>
        {
            s.discover.request.loading = !done;
            s.discover.results.replace_search(results);
            s.discover.request.warnings = warnings;
            s.discover.request.status.clear();
        }
        Event::Detail(id, result)
            if id == s.detail.generation && s.navigation.page == Page::Detail =>
        {
            s.detail.request.loading = false;
            match result {
                Ok((meta, namespace)) => {
                    s.detail.namespace = namespace;
                    apply_detail(s, meta);
                }
                Err(error) => s.detail.request.error = Some(error),
            }
            s.try_resume_stream();
        }
        Event::Streams(id, streams, warnings, done)
            if s.runtime
                .streams
                .accepts(id, super::request::StreamOwner::Guide)
                && s.navigation.page == Page::Guide =>
        {
            s.apply_guide_streams(streams, done);
            let _ = warnings;
        }
        Event::Streams(id, streams, warnings, done)
            if s.runtime
                .streams
                .accepts(id, super::request::StreamOwner::Detail)
                && s.navigation.page == Page::Detail =>
        {
            s.detail.sources.request.loading = !done;
            s.detail.sources.streams = streams;
            s.detail.sources.request.warnings = warnings;
            s.detail.sources.request.status = if !done {
                format!("{} streams · loading…", s.detail.sources.streams.len())
            } else if s.detail.sources.streams.is_empty() {
                "No streams found. Install a compatible stream add-on, or try another episode."
                    .into()
            } else {
                format!("{} streams", s.detail.sources.streams.len())
            };
            if done && s.library.quick_play == Some(s.detail.generation) {
                s.library.quick_play = None;
                let playable: Vec<_> = s
                    .detail
                    .sources
                    .streams
                    .iter()
                    .filter(|(_, stream)| stream.validate_playback().is_ok())
                    .collect();
                if playable.len() == 1 {
                    let stream = playable[0].1.clone();
                    let title = s
                        .detail
                        .meta
                        .as_ref()
                        .map_or_else(String::new, |meta| meta.name.clone());
                    s.play(stream, title);
                }
            }
        }
        Event::Playing(id, path) if id == s.player.generation => {
            s.apply_subtitle_appearance();
            s.player.report = Some(path);
            s.player.status = "Loading…".into();
            let volume = s.player.volume.to_string();
            s.command(&["set", "volume", &volume]);
        }
        Event::Ready(id) if id == s.player.generation && s.player.playing => {
            let first_ready = !s.player.ready;
            if first_ready {
                s.library.replaying = None;
                if let Some(candidate) = s.library.stream_candidate.take()
                    && candidate.generation == id
                    && let Ok(json) = serde_json::to_string(&candidate.stream)
                {
                    s.db(
                        crate::library_worker::Command::SaveStream(candidate.media, json),
                        false,
                    );
                }
            }
            s.player.ready = true;
            if s.navigation.page == Page::Guide && first_ready {
                s.seek_guide_now();
            } else if let Some(position) = s.library.resume_position.take() {
                s.player.sample.cache_s = None;
                s.command(&["seek", &position.to_string(), "absolute"]);
                s.library.observed = None;
            }
            if s.navigation.page == Page::Player
                || (s.navigation.page == Page::Guide && !s.guide.collections_open)
            {
                s.shell.host.show();
            }
            s.player.status.clear();
        }
        Event::Metrics(id, sample) if id == s.player.generation => {
            s.player.status = if sample.buffering {
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
        Event::TorrentMetrics(id, stats)
            if id == s.player.generation && s.player.playing && s.player.torrent_source =>
        {
            s.player.torrent_stats = stats;
        }
        Event::Stopped(id, error) if id == s.player.generation => {
            let replay_failed = error
                .is_some()
                .then(|| s.library.replaying.take())
                .flatten();
            s.library.stream_candidate = None;
            if s.navigation.page != Page::Guide {
                s.set_fullscreen(false);
            }
            s.finish_library_progress(error.is_none());
            s.library.playing = None;
            s.refresh_library();
            s.player.playing = false;
            s.player.subtitles = super::player::subtitles::Subtitles::default();
            s.player.audio = super::player::audio::Audio::default();
            if let Some(services) = &mut s.runtime.services {
                services.cancel_subtitles();
            }
            s.player.ready = false;
            s.player.seek_preview = None;
            s.player.sample.cache_s = None;
            s.player.stats_overlay.dismiss();
            s.player.torrent_stats = None;
            s.player.torrent_source = false;
            s.shell.host.hide();
            s.player.status = "Playback stopped".into();
            if error.is_some() {
                s.player.error = error;
                s.guide_join_failed();
            }
            if let Some(media) = replay_failed {
                s.open_library_media(&media);
                s.detail.request.error =
                    Some("Previous stream could not be opened. Choose another stream.".into());
            }
        }
        Event::PlayerWarning(id, error) if id == s.player.generation => {
            s.player.error = Some(error);
        }
        _ => {}
    }
}

fn apply_detail(s: &mut State, meta: Meta) {
    let movie = meta.kind == "movie";
    let (kind, id) = (meta.kind.clone(), meta.id.clone());
    let episode = s
        .detail
        .pending_episode
        .take()
        .or_else(|| s.detail.episodes.selected.clone())
        .filter(|_| !movie);
    if let Some(index) = episode
        .as_ref()
        .and_then(|id| meta.videos.iter().position(|video| &video.id == id))
    {
        s.detail.episodes.season =
            super::episodes::SeasonSelection::Selected(meta.videos[index].season);
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
        s.detail.episodes.offset = position / EPISODES_PER_PAGE * EPISODES_PER_PAGE;
    }
    s.detail.episodes.selected.clone_from(&episode);
    s.detail.meta = Some(meta);
    s.register_library_detail();
    s.remember_series_detail();
    s.detail.sources.request.status = "Choose an episode to find streams.".into();
    if movie {
        s.find_streams(kind, id);
    } else if let Some(episode) = episode {
        s.find_streams(kind, episode);
    }
}
