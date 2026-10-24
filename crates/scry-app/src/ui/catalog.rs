//! Catalog, search, and metadata requests initiated by the UI.
use super::{Page, State};
use rune::TextEdit;
use scry_addons::{Addon, Catalog, Meta};
use std::sync::Arc;

impl State {
    pub(super) fn install(&mut self, input: String) {
        if self.config_busy {
            return;
        }
        self.error = None;
        self.config_busy = true;
        if let Some(services) = &self.services {
            services.install(input);
        }
    }

    pub(super) fn catalogs(&self) -> Vec<(Addon, Catalog)> {
        self.addons
            .iter()
            .flat_map(|a| {
                a.manifest
                    .catalogs
                    .iter()
                    .filter(|c| {
                        c.kind == self.kind
                            && !c.extra.iter().any(|e| e.is_required && e.name != "search")
                    })
                    .map(|c| (a.clone(), c.clone()))
            })
            .collect()
    }

    pub(super) fn choose_kind(&mut self, kind: &str) {
        self.remember();
        if self.navigation.page == Page::Player {
            self.stop();
        }
        self.navigation.page = Page::Discover;
        self.kind = kind.into();
        self.catalog = self.catalogs().into_iter().next();
        self.search = TextEdit::new("");
        self.fetch_catalog();
    }

    pub(super) fn fetch_catalog(&mut self) {
        self.displayed_query = self.search.text.trim().to_owned();
        if !self.search.text.trim().is_empty() {
            self.fetch_search();
            return;
        }
        self.catalog_id += 1;
        self.metas = Arc::default();
        self.providers = Arc::default();
        self.error = None;
        let Some((addon, catalog)) = self.catalog.clone() else {
            self.loading = false;
            return;
        };
        if self.search.text.trim().is_empty()
            && catalog
                .extra
                .iter()
                .any(|e| e.name == "search" && e.is_required)
        {
            self.status = "Enter a title to search this catalog.".into();
            self.loading = false;
            return;
        }
        self.loading = true;
        self.status = "Loading catalog…".into();
        if let Some(services) = &mut self.services {
            services.catalog(self.catalog_id, addon, catalog, self.search.text.clone());
        }
    }

    pub(super) fn submit_search(&mut self) {
        self.remember();
        if self.navigation.page == Page::Player {
            self.stop();
        }
        self.navigation.page = Page::Discover;
        self.detail_id += 1;
        self.stream_id += 1;
        self.fetch_catalog();
    }

    fn fetch_search(&mut self) {
        self.catalog_id += 1;
        self.metas = Arc::default();
        self.providers = Arc::default();
        self.error = None;
        self.warnings.clear();
        self.loading = true;
        self.status = "Searching…".into();
        if let Some(services) = &mut self.services {
            services.search(
                self.catalog_id,
                self.addons.clone(),
                self.search.text.trim().to_owned(),
            );
        }
    }

    fn open_detail(&mut self, meta: Meta) {
        self.pending_episode = None;
        self.detail_namespace = self
            .catalog
            .as_ref()
            .map_or_else(|| "unknown".into(), |(a, _)| a.manifest.id.clone());
        self.open_detail_from(meta);
    }

    pub(super) fn open_result(&mut self, index: usize) {
        let Some(meta) = self.metas.get(index).cloned() else {
            return;
        };
        self.navigation.anchor = Some(format!("title-{index}"));
        if let Some(addon) = self.providers.get(index) {
            let namespace = addon.manifest.id.clone();
            self.pending_episode = None;
            self.detail_namespace = namespace;
            self.open_detail_from(meta);
        } else {
            self.open_detail(meta);
        }
    }

    pub(super) fn open_detail_from(&mut self, meta: Meta) {
        self.navigate(Page::Detail);
        self.detail = Some(meta.clone());
        self.selected_video_id = None;
        self.episode_offset = 0;
        self.episode_season = None;
        self.season_menu_open = false;
        self.source_menu_open = false;
        self.library.snapshot = None;
        self.streams.clear();
        self.warnings.clear();
        self.loading = true;
        self.status = "Loading details…".into();
        if let Some(services) = &mut self.services {
            services.detail(
                self.detail_id,
                self.addons.clone(),
                meta,
                self.detail_namespace.clone(),
            );
        }
    }

    pub(super) fn find_streams(&mut self, kind: String, id: String) {
        self.stream_id += 1;
        self.source_menu_open = false;
        self.stream_provider = None;
        self.selected_video_id = Some(id.clone());
        self.refresh_library();
        self.streams.clear();
        self.warnings.clear();
        self.error = None;
        self.loading = true;
        self.status = "Loading streams…".into();
        if let Some(services) = &mut self.services {
            services.streams(self.stream_id, self.addons.clone(), kind, id);
        }
    }
}
