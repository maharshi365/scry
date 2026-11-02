use super::{Page, State, design::tokens};
use crate::library_worker::Command;
use rune::{El, el};
use rune_kit::MenuItem;
use scry_library::{Media, MediaKind};

pub(super) enum Target {
    Title(Media),
    Season {
        parent: Media,
        season: Option<u32>,
        episodes: Vec<Media>,
        inventory: Vec<scry_library::EpisodeRelease>,
    },
}

impl State {
    pub(super) fn open_title_menu(&mut self, point: rune::PointerPress, media: Option<Media>) {
        self.shell.context_menu = Some((point, media.map(Target::Title)));
    }

    pub(super) fn open_season_menu(&mut self, point: rune::PointerPress, season: Option<u32>) {
        let Some(parent) = self
            .parent_media()
            .filter(|m| m.key.kind == MediaKind::Series)
        else {
            return;
        };
        let episodes = self
            .detail
            .meta
            .iter()
            .flat_map(|meta| &meta.videos)
            .filter(|video| video.season == season && !video.id.trim().is_empty())
            .map(|video| Media {
                key: super::library_view::key(&parent.key.namespace, MediaKind::Episode, &video.id),
                title: format!(
                    "{} · {}",
                    parent.title,
                    if video.title.trim().is_empty() {
                        &video.id
                    } else {
                        &video.title
                    }
                ),
                parent: Some(parent.key.clone()),
            })
            .collect();
        self.detail.episodes.menu_open = false;
        self.detail.sources.menu_open = false;
        self.detail.sources.sort_open = false;
        let inventory = self.detail.meta.as_ref().map_or_else(Vec::new, |meta| {
            crate::media_metadata::episode_inventory(&parent.key, meta)
        });
        self.shell.context_menu = Some((
            point,
            Some(Target::Season {
                parent,
                season,
                episodes,
                inventory,
            }),
        ));
    }

    pub(super) fn refresh_page(&mut self) {
        self.shell.context_menu = None;
        if let Some(services) = &self.runtime.services {
            services.invalidate_cache();
        }
        if self.navigation.page != Page::Detail {
            if let Some(artwork) = &mut self.artwork.posters {
                artwork.retry_failures();
            }
            if let Some(artwork) = &mut self.artwork.player {
                artwork.retry_failures();
            }
            self.retry_series_details();
            self.request_series_tracking(true);
        }
        match self.navigation.page {
            Page::Guide | Page::Settings => self.refresh_guide(),
            Page::Stats => self.refresh_stats(),
            Page::MyMedia => self.media_command(crate::services::my_media::Command::Scan),
            Page::Discover => self.fetch_catalog(),
            Page::Calendar => self.refresh_calendar(),
            Page::Library | Page::Watching => {
                self.retry_library_posters();
                self.refresh_library();
            }
            Page::Player => {
                self.refresh_library();
            }
            Page::Detail => {
                if let Some(meta) = self.detail.meta.clone() {
                    if meta.kind == "movie" {
                        self.find_streams(meta.kind.clone(), meta.id.clone());
                    } else if meta.kind == "series"
                        && let Some(episode) = self.detail.episodes.selected.clone()
                    {
                        self.find_streams(meta.kind.clone(), episode);
                    } else {
                        self.detail.generation += 1;
                        self.detail.episode_refresh_generation = (meta.kind == "series"
                            && self.detail.pending_episode.is_none())
                        .then_some(self.detail.generation);
                        self.runtime.streams.invalidate();
                        self.detail.sources.reset();
                        self.detail.request = super::request::RequestState {
                            loading: true,
                            status: "Loading details…".into(),
                            ..super::request::RequestState::default()
                        };
                        if let Some(services) = &mut self.runtime.services {
                            services.detail(
                                self.detail.generation,
                                self.addons.installed.clone(),
                                meta,
                                self.detail.namespace.clone(),
                            );
                        }
                    }
                }
            }
            Page::Addons => {
                if !self.addons.busy
                    && let Some(services) = &self.runtime.services
                {
                    services.reload_addons();
                    self.addons.busy = true;
                }
            }
        }
    }

    pub(super) fn result_media(&self, index: usize) -> Option<Media> {
        let meta = self.discover.results.get(index)?;
        let namespace = self
            .discover
            .results
            .namespace(index)
            .or_else(|| {
                self.discover
                    .catalog
                    .as_ref()
                    .map(|(a, _)| a.manifest.id.as_str())
            })
            .unwrap_or("unknown");
        Some(Media {
            key: super::library_view::key(
                namespace,
                if meta.kind == "series" {
                    MediaKind::Series
                } else {
                    MediaKind::Movie
                },
                &meta.id,
            ),
            title: meta.name.clone(),
            parent: None,
        })
    }
}

pub(super) fn view(s: &State) -> Option<El> {
    let (point, item) = s.shell.context_menu.as_ref()?;
    let mut menu = rune_kit::context_menu("context", el(), Some(*point))
        .min_width(250.0)
        .on_close(|s: &mut State| s.shell.context_menu = None);
    if let Some(Target::Title(media)) = item {
        if s.navigation.page == Page::Watching {
            let media = media.clone();
            menu = menu.item(MenuItem::new("Choose another stream").on_select(
                move |s: &mut State| {
                    s.shell.context_menu = None;
                    s.open_library_media(&media);
                },
            ));
        }
        if let Some(dismiss) = dismiss_watching(s, media) {
            menu = menu.item(dismiss);
        }
        if matches!(media.key.kind, MediaKind::Movie | MediaKind::Series) {
            let media = media.clone();
            menu = menu.item(MenuItem::new("Add to a Guide collection…").on_select(
                move |s: &mut State| {
                    s.shell.context_menu = None;
                    s.guide.adding = Some(media.clone());
                    s.guide.collections_open = true;
                    s.navigate(Page::Guide);
                },
            ));
        }
        for (title, choice) in [
            ("Add to library", 0),
            ("Remove from library", 1),
            ("Mark as watched", 2),
            ("Mark as unwatched", 3),
        ] {
            let media = media.clone();
            menu = menu.item(MenuItem::new(title).on_select(move |s: &mut State| {
                s.shell.context_menu = None;
                if choice >= 2 {
                    s.mark_media_watched(media.clone(), choice == 2);
                    return;
                }
                s.db(Command::SetWatchlist(media.clone(), choice == 0), true);
            }));
        }
    } else if let Some(Target::Season {
        parent,
        season,
        episodes,
        inventory,
    }) = item
    {
        menu = menu.group_label(super::episodes::season_label(*season));
        for (title, watched) in [
            ("Mark all episodes watched", true),
            ("Mark all episodes unwatched", false),
        ] {
            let (parent, episodes) = (parent.clone(), episodes.clone());
            let inventory = inventory.clone();
            menu = menu.item(MenuItem::new(title).on_select(move |s: &mut State| {
                s.shell.context_menu = None;
                s.db(
                    Command::SetEpisodesWatched {
                        parent: parent.clone(),
                        episodes: episodes.clone(),
                        inventory: inventory.clone(),
                        watched,
                    },
                    true,
                );
            }));
        }
    }
    let mut overlays = Vec::new();
    let trigger = menu
        .item(MenuItem::new("Refresh").on_select(State::refresh_page))
        .build(tokens::kit(), &mut overlays);
    Some(
        el().absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .child(trigger)
            .children(overlays),
    )
}

fn dismiss_watching(s: &State, media: &Media) -> Option<MenuItem> {
    if s.navigation.page != Page::Watching
        || !s
            .library
            .watching
            .iter()
            .any(|(item, _)| item.key == media.key)
    {
        return None;
    }
    let key = media.key.clone();
    Some(
        MenuItem::new("Dismiss from Currently watching").on_select(move |s: &mut State| {
            s.shell.context_menu = None;
            s.db(Command::DismissWatching(key.clone()), true);
        }),
    )
}
