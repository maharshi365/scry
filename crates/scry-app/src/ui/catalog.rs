//! Catalog, search, and metadata requests initiated by the UI.
use super::{Page, State};
use rune::TextEdit;
use scry_addons::{Addon, Catalog, Meta};
use std::sync::Arc;

fn compatible_catalog(catalog: &Catalog) -> bool {
    !catalog.kind.trim().is_empty()
        && !catalog
            .extra
            .iter()
            .any(|extra| extra.is_required && !matches!(extra.name.as_str(), "search" | "skip"))
}

pub(super) fn category_label(kind: &str) -> String {
    match kind {
        "movie" => "Movies".into(),
        "series" => "TV shows".into(),
        _ => {
            let mut chars = kind.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        }
    }
}

#[derive(Clone, Default)]
pub(super) struct CatalogPaging {
    pub next_offset: u32,
    pub pending: Option<u32>,
    pub has_more: bool,
    pub failed: bool,
}

impl State {
    pub(super) fn catalog_categories(&self) -> Vec<String> {
        let mut categories = vec!["movie".to_owned(), "series".to_owned()];
        for catalog in self
            .addons
            .iter()
            .filter(|addon| !addon.disabled)
            .flat_map(|addon| &addon.manifest.catalogs)
            .filter(|catalog| compatible_catalog(catalog))
        {
            if !categories.contains(&catalog.kind) {
                categories.push(catalog.kind.clone());
            }
        }
        categories
    }

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
            .filter(|a| !a.disabled)
            .flat_map(|a| {
                a.manifest
                    .catalogs
                    .iter()
                    .filter(|c| c.kind == self.kind && compatible_catalog(c))
                    .map(|c| (a.clone(), c.clone()))
            })
            .collect()
    }

    pub(super) fn choose_kind(&mut self, kind: &str) {
        self.remember();
        if matches!(self.navigation.page, Page::Player | Page::Guide) {
            self.stop();
        }
        self.leave_guide();
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
        self.catalog_paging = CatalogPaging::default();
        self.metas = Arc::default();
        self.poster_rects.clear();
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
        self.catalog_paging.pending = Some(0);
        self.status = "Loading catalog…".into();
        if let Some(services) = &mut self.services {
            services.catalog(self.catalog_id, addon, catalog, 0);
        }
    }

    pub(super) fn submit_search(&mut self) {
        self.remember();
        if matches!(self.navigation.page, Page::Player | Page::Guide) {
            self.stop();
        }
        self.leave_guide();
        self.navigation.page = Page::Discover;
        self.detail_id += 1;
        self.stream_id += 1;
        self.fetch_catalog();
    }

    fn fetch_search(&mut self) {
        self.catalog_paging = CatalogPaging::default();
        self.poster_rects.clear();
        self.catalog_id += 1;
        self.metas = Arc::default();
        self.providers = Arc::default();
        self.error = None;
        self.warnings.clear();
        self.loading = true;
        self.status.clear();
        if let Some(services) = &mut self.services {
            services.search(
                self.catalog_id,
                self.addons.clone(),
                self.search.text.trim().to_owned(),
            );
        }
    }

    pub(super) fn request_more_catalog(&mut self) {
        if self.navigation.page != Page::Discover
            || !self.displayed_query.is_empty()
            || !self.catalog_paging.has_more
            || self.catalog_paging.pending.is_some()
            || self.catalog_paging.failed
        {
            return;
        }
        let near_end = self.poster_viewport.is_some_and(|viewport| {
            self.poster_rects
                .values()
                .next_back()
                .is_some_and(|last| last[1] + last[3] <= viewport[1] + viewport[3] * 2.0)
        });
        if !near_end {
            return;
        }
        let Some((addon, catalog)) = self
            .catalog
            .clone()
            .filter(|(_, catalog)| catalog.supports_paging())
        else {
            return;
        };
        let offset = self.catalog_paging.next_offset;
        self.catalog_paging.pending = Some(offset);
        if let Some(services) = &mut self.services {
            services.catalog(self.catalog_id, addon, catalog, offset);
        }
    }

    pub(super) fn apply_catalog_page(
        &mut self,
        generation: u64,
        offset: u32,
        result: Result<Vec<Meta>, String>,
    ) {
        if generation != self.catalog_id
            || self.navigation.page != Page::Discover
            || self.catalog_paging.pending != Some(offset)
        {
            return;
        }
        self.catalog_paging.pending = None;
        self.loading = false;
        match result {
            Ok(metas) => {
                let count = u32::try_from(metas.len()).unwrap_or(u32::MAX);
                let mut seen: std::collections::HashSet<_> = self
                    .metas
                    .iter()
                    .map(|meta| (meta.kind.clone(), meta.id.clone()))
                    .collect();
                let mut added = 0;
                let titles = Arc::make_mut(&mut self.metas);
                for meta in metas {
                    if matches!(meta.kind.as_str(), "movie" | "series")
                        && !meta.id.trim().is_empty()
                        && seen.insert((meta.kind.clone(), meta.id.clone()))
                    {
                        titles.push(meta);
                        added += 1;
                    }
                }
                self.catalog_paging.next_offset = offset.saturating_add(count);
                self.catalog_paging.has_more = added > 0
                    && count > 0
                    && self.catalog_paging.next_offset > offset
                    && self
                        .catalog
                        .as_ref()
                        .is_some_and(|(_, catalog)| catalog.supports_paging());
                self.status.clear();
            }
            Err(error) => {
                self.catalog_paging.failed = true;
                self.status.clear();
                self.error = Some(error);
            }
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
        self.detail_preview = true;
    }

    pub(super) fn open_detail_from(&mut self, meta: Meta) {
        if self.navigation.page == Page::Detail {
            self.detail_id += 1;
            self.stream_id += 1;
        }
        self.navigate(Page::Detail);
        self.detail = Some(meta.clone());
        self.detail_preview = false;
        self.selected_video_id = None;
        self.episode_offset = 0;
        self.episode_search = TextEdit::new("");
        self.episode_season = None;
        self.season_menu_open = false;
        self.source_menu_open = false;
        self.source_sort_open = false;
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
        self.source_sort_open = false;
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
