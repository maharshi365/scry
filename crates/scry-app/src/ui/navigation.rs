//! Navigation history and keyboard shortcuts. Player routes restore their source page.
use super::State;

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Page {
    Watching,
    Calendar,
    Guide,
    Discover,
    Library,
    Addons,
    Settings,
    Stats,
    Detail,
    Player,
}

pub(super) struct NavigationState {
    pub page: Page,
    pub back: Vec<Route>,
    pub forward: Vec<Route>,
    pub route_key: u64,
    presented_route: Option<(Page, u64)>,
    next_route_key: u64,
    pub anchor: Option<String>,
    pub reveal: Option<String>,
}

impl Default for NavigationState {
    fn default() -> Self {
        Self {
            page: Page::Watching,
            back: Vec::new(),
            forward: Vec::new(),
            route_key: 0,
            presented_route: None,
            next_route_key: 0,
            anchor: None,
            reveal: None,
        }
    }
}

impl NavigationState {
    pub(super) fn present(&mut self, tx: &mut rune::Tx) -> bool {
        let route = (self.page, self.route_key);
        if self.presented_route == Some(route) {
            return false;
        }
        self.presented_route = Some(route);
        super::primitives::motion::route(tx);
        true
    }
}

#[derive(Clone)]
pub(super) struct Route {
    page: Page,
    discover: super::catalog::DiscoverSnapshot,
    detail: super::detail::DetailSnapshot,
    key: u64,
    anchor: Option<String>,
}

impl State {
    pub(super) fn navigate(&mut self, page: Page) {
        if self.navigation.page == page {
            return;
        }
        self.remember();
        if matches!(self.navigation.page, Page::Player | Page::Guide) {
            self.stop();
        }
        self.leave_guide();
        self.navigation.page = page;
        self.shell.context_menu = None;
        self.addons.install_open = false;
        self.calendar.expanded_day = None;
        self.detail.generation += 1;
        self.runtime.streams.invalidate();
        self.discover.generation += 1;
        self.discover.request.loading = false;
        self.detail.request.loading = false;
        self.detail.sources.request.loading = false;
        if page == Page::Stats {
            self.refresh_stats();
        }
        if matches!(page, Page::Guide | Page::Settings) {
            self.refresh_guide();
        }
        if page == Page::Discover
            && self.discover.catalog.is_some()
            && self.discover.results.is_empty()
        {
            self.fetch_catalog();
        }
        if matches!(page, Page::Library | Page::Watching | Page::Calendar) {
            self.refresh_library();
        }
    }

    fn route(&self) -> Route {
        Route {
            page: if self.navigation.page == Page::Player {
                if self.detail.meta.is_some() {
                    Page::Detail
                } else {
                    Page::Addons
                }
            } else {
                self.navigation.page
            },
            discover: self.discover.snapshot(),
            detail: self.detail.snapshot(),
            key: self.navigation.route_key,
            anchor: self.navigation.anchor.clone(),
        }
    }

    pub(super) fn remember(&mut self) {
        self.discover.menu_open = false;
        if self.navigation.back.len() == 32 {
            self.navigation.back.remove(0);
        }
        let route = self.route();
        self.navigation.back.push(route);
        self.navigation.forward.clear();
        self.navigation.next_route_key += 1;
        self.navigation.route_key = self.navigation.next_route_key;
        self.navigation.anchor = None;
    }

    pub(super) fn travel(&mut self, forward: bool) {
        let route = if forward {
            self.navigation.forward.pop()
        } else {
            self.navigation.back.pop()
        };
        let Some(route) = route else {
            return;
        };
        self.discover.menu_open = false;
        self.shell.context_menu = None;
        self.addons.install_open = false;
        self.calendar.expanded_day = None;
        let current = self.route();
        if forward {
            self.navigation.back.push(current);
        } else {
            self.navigation.forward.push(current);
        }
        if matches!(self.navigation.page, Page::Player | Page::Guide) {
            self.stop();
        }
        self.leave_guide();
        self.discover.generation += 1;
        self.detail.generation += 1;
        self.runtime.streams.invalidate();
        self.navigation.page = route.page;
        let restart_discover = self.discover.restore(route.discover);
        let restart_detail = self.detail.restore(route.detail);
        self.navigation.route_key = route.key;
        self.navigation.anchor = route.anchor;
        self.navigation.reveal = self.navigation.anchor.clone();
        match self.navigation.page {
            Page::Guide | Page::Settings => self.refresh_guide(),
            Page::Stats => self.refresh_stats(),
            Page::Discover if restart_discover => self.fetch_catalog(),
            Page::Detail => {
                if restart_detail && self.detail.episodes.selected.is_none() {
                    if let (Some(services), Some(meta)) =
                        (&mut self.runtime.services, &self.detail.meta)
                    {
                        self.detail.request.loading = true;
                        services.detail(
                            self.detail.generation,
                            self.addons.installed.clone(),
                            meta.clone(),
                            self.detail.namespace.clone(),
                        );
                    }
                } else if let (Some(meta), Some(id)) =
                    (&self.detail.meta, &self.detail.episodes.selected)
                {
                    self.find_streams(meta.kind.clone(), id.clone());
                }
                self.register_library_detail();
            }
            Page::Library | Page::Watching | Page::Calendar => self.refresh_library(),
            _ => {}
        }
    }
}

pub(super) mod shortcuts;
