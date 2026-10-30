//! Catalog and search state, snapshots, and requests.
use super::{Page, State};
use rune::TextEdit;
use scry_addons::{Addon, Catalog, Meta};
use std::sync::Arc;

#[derive(Clone)]
struct CatalogEntry {
    meta: Meta,
    namespace: Option<String>,
}

#[derive(Clone, Default)]
pub(super) struct CatalogResults {
    entries: Arc<Vec<CatalogEntry>>,
}

impl From<Vec<Meta>> for CatalogResults {
    fn from(metas: Vec<Meta>) -> Self {
        Self {
            entries: Arc::new(
                metas
                    .into_iter()
                    .map(|meta| CatalogEntry {
                        meta,
                        namespace: None,
                    })
                    .collect(),
            ),
        }
    }
}

impl std::ops::Index<usize> for CatalogResults {
    type Output = Meta;

    fn index(&self, index: usize) -> &Self::Output {
        &self.entries[index].meta
    }
}

impl CatalogResults {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Meta> {
        self.entries.iter().map(|entry| &entry.meta)
    }

    pub fn get(&self, index: usize) -> Option<&Meta> {
        self.entries.get(index).map(|entry| &entry.meta)
    }

    pub fn namespace(&self, index: usize) -> Option<&str> {
        self.entries.get(index)?.namespace.as_deref()
    }

    pub fn replace_search(&mut self, results: Vec<crate::search::SearchResult>) {
        self.entries = Arc::new(
            results
                .into_iter()
                .map(|result| CatalogEntry {
                    meta: result.meta,
                    namespace: Some(result.addon.manifest.id),
                })
                .collect(),
        );
    }

    #[cfg(test)]
    pub fn to_vec(&self) -> Vec<Meta> {
        self.iter().cloned().collect()
    }
}

pub(super) struct DiscoverState {
    pub request: super::request::RequestState,
    pub search: TextEdit,
    pub displayed_query: String,
    pub kind: String,
    pub catalog: Option<(Addon, Catalog)>,
    pub paging: CatalogPaging,
    pub menu_open: bool,
    pub results: CatalogResults,
    pub generation: u64,
}

impl Default for DiscoverState {
    fn default() -> Self {
        Self {
            request: super::request::RequestState::default(),
            search: TextEdit::new(""),
            displayed_query: String::new(),
            kind: "movie".into(),
            catalog: None,
            paging: CatalogPaging::default(),
            menu_open: false,
            results: CatalogResults::default(),
            generation: 0,
        }
    }
}

#[derive(Clone)]
pub(super) struct DiscoverSnapshot {
    kind: String,
    query: String,
    catalog: Option<(Addon, Catalog)>,
    paging: CatalogPaging,
    results: CatalogResults,
    request: super::request::RequestSnapshot,
}

impl DiscoverState {
    pub fn snapshot(&self) -> DiscoverSnapshot {
        DiscoverSnapshot {
            kind: self.kind.clone(),
            query: self.displayed_query.clone(),
            catalog: self.catalog.clone(),
            paging: self.paging.clone(),
            results: self.results.clone(),
            request: self.request.snapshot(),
        }
    }

    pub fn restore(&mut self, snapshot: DiscoverSnapshot) -> bool {
        self.kind = snapshot.kind;
        self.displayed_query.clone_from(&snapshot.query);
        self.search = TextEdit::new(snapshot.query);
        self.catalog = snapshot.catalog;
        self.paging = snapshot.paging;
        if self.paging.pending.is_some_and(|offset| offset > 0) {
            self.paging.pending = None;
        }
        self.results = snapshot.results;
        self.menu_open = false;
        self.request.restore(snapshot.request)
    }
}

// Allow transient repeated pages without following a broken provider indefinitely.
const MAX_NO_PROGRESS_PAGES: u8 = 3;

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
    no_progress_pages: u8,
}

impl State {
    pub(super) fn catalog_categories(&self) -> Vec<String> {
        let mut categories = vec!["movie".to_owned(), "series".to_owned()];
        for catalog in self
            .addons
            .installed
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

    pub(super) fn catalogs(&self) -> Vec<(Addon, Catalog)> {
        self.addons
            .installed
            .iter()
            .filter(|a| !a.disabled)
            .flat_map(|a| {
                a.manifest
                    .catalogs
                    .iter()
                    .filter(|c| {
                        c.kind == self.discover.kind
                            && compatible_catalog(c)
                            && !c.extra.iter().any(|e| e.name == "search" && e.is_required)
                    })
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
        self.discover.kind = kind.into();
        self.discover.catalog = self.catalogs().into_iter().next();
        self.discover.search = TextEdit::new("");
        self.fetch_catalog();
    }

    pub(super) fn select_catalog(&mut self, index: usize) {
        self.discover.menu_open = false;
        let Some((addon, catalog)) = self.catalogs().into_iter().nth(index) else {
            return;
        };
        if self
            .discover
            .catalog
            .as_ref()
            .is_some_and(|(current_addon, current)| {
                current_addon.manifest_url == addon.manifest_url
                    && current.id == catalog.id
                    && current.kind == catalog.kind
            })
        {
            return;
        }
        self.remember();
        self.navigation.page = Page::Discover;
        self.discover.catalog = Some((addon, catalog));
        self.discover.search = TextEdit::new("");
        self.fetch_catalog();
    }

    pub(super) fn fetch_catalog(&mut self) {
        self.discover.menu_open = false;
        self.discover.displayed_query = self.discover.search.text.trim().to_owned();
        if !self.discover.search.text.trim().is_empty() {
            self.fetch_search();
            return;
        }
        self.discover.generation += 1;
        self.discover.paging = CatalogPaging::default();
        self.discover.results = CatalogResults::default();
        self.posters.visible = 0..0;
        self.discover.request.error = None;
        self.discover.request.warnings.clear();
        self.discover.request.status.clear();
        let Some((addon, catalog)) = self.discover.catalog.clone() else {
            self.discover.request.loading = false;
            return;
        };
        if self.discover.search.text.trim().is_empty()
            && catalog
                .extra
                .iter()
                .any(|e| e.name == "search" && e.is_required)
        {
            self.discover.request.status = "Enter a title to search this catalog.".into();
            self.discover.request.loading = false;
            return;
        }
        self.discover.request.loading = true;
        self.discover.paging.pending = Some(0);
        self.discover.request.status = "Loading catalog…".into();
        if let Some(services) = &mut self.runtime.services {
            services.catalog(self.discover.generation, addon, catalog, 0);
        }
    }

    pub(super) fn submit_search(&mut self) {
        self.remember();
        if matches!(self.navigation.page, Page::Player | Page::Guide) {
            self.stop();
        }
        self.leave_guide();
        self.navigation.page = Page::Discover;
        self.detail.generation += 1;
        self.runtime.streams.invalidate();
        self.fetch_catalog();
    }

    fn fetch_search(&mut self) {
        self.discover.paging = CatalogPaging::default();
        self.posters.visible = 0..0;
        self.discover.generation += 1;
        self.discover.results = CatalogResults::default();
        self.discover.request.error = None;
        self.discover.request.warnings.clear();
        self.discover.request.loading = true;
        self.discover.request.status.clear();
        if let Some(services) = &mut self.runtime.services {
            services.search(
                self.discover.generation,
                self.addons.installed.clone(),
                self.discover.search.text.trim().to_owned(),
            );
        }
    }

    pub(super) fn request_more_catalog(&mut self) {
        if self.navigation.page != Page::Discover
            || !self.discover.displayed_query.is_empty()
            || !self.discover.paging.has_more
            || self.discover.paging.pending.is_some()
            || self.discover.paging.failed
        {
            return;
        }
        let near_end = self.posters.route == self.navigation.route_key
            && !self.posters.visible.is_empty()
            && self.posters.visible.end + self.posters.visible.len()
                >= self.discover.results.len().div_ceil(self.posters.columns);
        if !near_end {
            return;
        }
        let Some((addon, catalog)) = self
            .discover
            .catalog
            .clone()
            .filter(|(_, catalog)| catalog.supports_paging())
        else {
            return;
        };
        let offset = self.discover.paging.next_offset;
        self.discover.paging.pending = Some(offset);
        if let Some(services) = &mut self.runtime.services {
            services.catalog(self.discover.generation, addon, catalog, offset);
        }
    }

    pub(super) fn apply_catalog_page(
        &mut self,
        generation: u64,
        offset: u32,
        result: Result<Vec<Meta>, String>,
    ) {
        if generation != self.discover.generation
            || self.navigation.page != Page::Discover
            || self.discover.paging.pending != Some(offset)
        {
            return;
        }
        self.discover.paging.pending = None;
        self.discover.request.loading = false;
        match result {
            Ok(metas) => {
                let count = u32::try_from(metas.len()).unwrap_or(u32::MAX);
                let mut seen: std::collections::HashSet<_> = self
                    .discover
                    .results
                    .iter()
                    .map(|meta| (meta.kind.clone(), meta.id.clone()))
                    .collect();
                let mut added = 0;
                let titles = Arc::make_mut(&mut self.discover.results.entries);
                for meta in metas {
                    if matches!(meta.kind.as_str(), "movie" | "series")
                        && !meta.id.trim().is_empty()
                        && seen.insert((meta.kind.clone(), meta.id.clone()))
                    {
                        titles.push(CatalogEntry {
                            meta,
                            namespace: None,
                        });
                        added += 1;
                    }
                }
                self.discover.paging.next_offset = offset.saturating_add(count);
                self.discover.paging.no_progress_pages = if added > 0 {
                    0
                } else {
                    self.discover.paging.no_progress_pages.saturating_add(1)
                };
                self.discover.paging.has_more = count > 0
                    && self.discover.paging.no_progress_pages < MAX_NO_PROGRESS_PAGES
                    && self.discover.paging.next_offset > offset
                    && self
                        .discover
                        .catalog
                        .as_ref()
                        .is_some_and(|(_, catalog)| catalog.supports_paging());
                self.discover.request.status.clear();
                if added == 0 {
                    self.request_more_catalog();
                }
            }
            Err(error) => {
                self.discover.paging.failed = true;
                self.discover.request.status.clear();
                self.discover.request.error = Some(error);
            }
        }
    }

    fn open_detail(&mut self, meta: Meta) {
        self.detail.pending_episode = None;
        self.detail.namespace = self
            .discover
            .catalog
            .as_ref()
            .map_or_else(|| "unknown".into(), |(a, _)| a.manifest.id.clone());
        self.open_detail_from(meta);
    }

    pub(super) fn open_result(&mut self, index: usize) {
        let Some(meta) = self.discover.results.get(index).cloned() else {
            return;
        };
        self.navigation.anchor = Some(format!("title-{index}"));
        if let Some(namespace) = self.discover.results.namespace(index).map(str::to_owned) {
            self.detail.pending_episode = None;
            self.detail.namespace = namespace;
            self.open_detail_from(meta);
        } else {
            self.open_detail(meta);
        }
        self.detail.preview = true;
    }
}
