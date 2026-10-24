//! Navigation history and keyboard shortcuts. Player routes restore their source page.
use super::{Page, State};
use rune::{TextEdit, Tx};
use scry_addons::{Addon, Catalog, Meta};
use std::sync::Arc;

#[derive(Clone)]
pub(super) struct Route {
    page: Page,
    kind: String,
    query: String,
    catalog: Option<(Addon, Catalog)>,
    metas: Arc<Vec<Meta>>,
    providers: Arc<Vec<Addon>>,
    detail: Option<Meta>,
    namespace: String,
    episode: Option<String>,
    pending_episode: Option<String>,
    episode_offset: usize,
    #[expect(
        clippy::option_option,
        reason = "History preserves both absent selection and explicitly selected unnumbered episodes"
    )]
    episode_season: Option<Option<u32>>,
    loading: bool,
    key: u64,
    anchor: Option<String>,
    status: String,
    warnings: Vec<String>,
}

impl State {
    pub(super) fn navigate(&mut self, page: Page) {
        if self.page == page {
            return;
        }
        self.remember();
        if self.page == Page::Player {
            self.stop();
        }
        self.page = page;
        self.detail_id += 1;
        self.stream_id += 1;
        self.catalog_id += 1;
        self.loading = false;
        self.error = None;
        if page == Page::Discover && self.catalog.is_some() && self.metas.is_empty() {
            self.fetch_catalog();
        }
        if page == Page::Library {
            self.refresh_library();
        }
    }

    fn route(&self) -> Route {
        Route {
            page: if self.page == Page::Player {
                if self.detail.is_some() {
                    Page::Detail
                } else {
                    Page::Addons
                }
            } else {
                self.page
            },
            kind: self.kind.clone(),
            query: self.displayed_query.clone(),
            catalog: self.catalog.clone(),
            metas: self.metas.clone(),
            providers: self.providers.clone(),
            detail: self.detail.clone(),
            namespace: self.detail_namespace.clone(),
            episode: self.selected_video_id.clone(),
            pending_episode: self.pending_episode.clone(),
            episode_offset: self.episode_offset,
            episode_season: self.episode_season,
            loading: self.loading,
            key: self.route_key,
            anchor: self.anchor.clone(),
            status: self.status.clone(),
            warnings: self.warnings.clone(),
        }
    }

    pub(super) fn remember(&mut self) {
        if self.back.len() == 32 {
            self.back.remove(0);
        }
        self.back.push(self.route());
        self.forward.clear();
        self.next_route_key += 1;
        self.route_key = self.next_route_key;
        self.anchor = None;
    }

    pub(super) fn travel(&mut self, forward: bool) {
        let route = if forward {
            self.forward.pop()
        } else {
            self.back.pop()
        };
        let Some(route) = route else {
            return;
        };
        let current = self.route();
        if forward {
            self.back.push(current);
        } else {
            self.forward.push(current);
        }
        if self.page == Page::Player {
            self.stop();
        }
        self.catalog_id += 1;
        self.detail_id += 1;
        self.stream_id += 1;
        self.page = route.page;
        self.kind = route.kind;
        self.displayed_query = route.query.clone();
        self.search = TextEdit::new(route.query);
        self.catalog = route.catalog;
        self.metas = route.metas;
        self.providers = route.providers;
        self.detail = route.detail;
        self.detail_namespace = route.namespace;
        self.selected_video_id = route.episode;
        self.pending_episode = route.pending_episode;
        self.episode_offset = route.episode_offset;
        self.episode_season = route.episode_season;
        self.season_menu_open = false;
        self.route_key = route.key;
        self.anchor = route.anchor;
        self.reveal = self.anchor.clone();
        self.error = None;
        self.status = route.status;
        self.warnings = route.warnings;
        self.streams.clear();
        self.stream_provider = None;
        self.source_menu_open = false;
        self.loading = false;
        match self.page {
            Page::Discover if route.loading => self.fetch_catalog(),
            Page::Detail => {
                if route.loading && self.selected_video_id.is_none() {
                    if let (Some(services), Some(meta)) = (&mut self.services, &self.detail) {
                        self.loading = true;
                        services.detail(
                            self.detail_id,
                            self.addons.clone(),
                            meta.clone(),
                            self.detail_namespace.clone(),
                        );
                    }
                } else if let (Some(meta), Some(id)) = (&self.detail, &self.selected_video_id) {
                    self.find_streams(meta.kind.clone(), id.clone());
                }
                self.register_library_detail();
            }
            Page::Library => self.refresh_library(),
            _ => {}
        }
    }
}

pub(super) fn on_key(s: &mut State, event: &rune::KeyEvent, _: &mut Tx) -> bool {
    if event.mods.alt && event.key == rune::Key::Left {
        s.travel(false);
        true
    } else if event.mods.alt && event.key == rune::Key::Right {
        s.travel(true);
        true
    } else if event.key == rune::Key::Escape && matches!(s.page, Page::Detail | Page::Player) {
        s.travel(false);
        true
    } else {
        false
    }
}
