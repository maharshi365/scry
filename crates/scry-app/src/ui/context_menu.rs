use super::{
    Page, State,
    primitives::{button::menu_item, text::label},
};
use crate::library_worker::Command;
use rune::{El, col};
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
        if let Some(artwork) = &mut self.artwork.posters {
            artwork.retry_failures();
        }
        if let Some(artwork) = &mut self.artwork.player {
            artwork.retry_failures();
        }
        self.retry_series_details();
        self.request_series_tracking(true);
        match self.navigation.page {
            Page::Guide | Page::Settings => self.refresh_guide(),
            Page::Stats => self.refresh_stats(),
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
                    self.detail.generation += 1;
                    self.open_detail_from(meta);
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
    let mut menu = col().w_full().gap(5.0);
    if let Some(Target::Title(media)) = item {
        if s.navigation.page == Page::Watching {
            let media = media.clone();
            menu = menu.child(menu_item(
                "context-choose-stream",
                "Choose another stream",
                move |s: &mut State| {
                    s.shell.context_menu = None;
                    s.open_library_media(&media);
                },
            ));
        }
        menu = menu.children(dismiss_watching(s, media));
        if matches!(media.key.kind, MediaKind::Movie | MediaKind::Series) {
            let media = media.clone();
            menu = menu.child(menu_item(
                "context-collection",
                "Add to a Guide collection…",
                move |s: &mut State| {
                    s.shell.context_menu = None;
                    s.guide.adding = Some(media.clone());
                    s.guide.collections_open = true;
                    s.navigate(Page::Guide);
                },
            ));
        }
        for (key, title, choice) in [
            ("later", "Add to library", 0),
            ("remove", "Remove from library", 1),
            ("watched", "Mark as watched", 2),
            ("unwatched", "Mark as unwatched", 3),
        ] {
            let media = media.clone();
            menu = menu.child(menu_item(
                format!("context-{key}"),
                title,
                move |s: &mut State| {
                    s.shell.context_menu = None;
                    if choice >= 2 {
                        s.mark_media_watched(media.clone(), choice == 2);
                        return;
                    }
                    s.db(Command::SetWatchlist(media.clone(), choice == 0), true);
                },
            ));
        }
    } else if let Some(Target::Season {
        parent,
        season,
        episodes,
        inventory,
    }) = item
    {
        menu = menu.child(label(super::episodes::season_label(*season)));
        for (key, title, watched) in [
            ("season-watched", "Mark all episodes watched", true),
            ("season-unwatched", "Mark all episodes unwatched", false),
        ] {
            let (parent, episodes) = (parent.clone(), episodes.clone());
            let inventory = inventory.clone();
            menu = menu.child(menu_item(
                format!("context-{key}"),
                title,
                move |s: &mut State| {
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
                },
            ));
        }
    }
    Some(super::primitives::surface::context_menu(
        "context-menu",
        *point,
        250.0,
        menu.child(menu_item("context-refresh", "Refresh", State::refresh_page)),
        |s: &mut State| s.shell.context_menu = None,
    ))
}

fn dismiss_watching(s: &State, media: &Media) -> Option<El> {
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
    Some(menu_item(
        "context-dismiss-watching",
        "Dismiss from Currently watching",
        move |s: &mut State| {
            s.shell.context_menu = None;
            s.db(Command::DismissWatching(key.clone()), true);
        },
    ))
}
