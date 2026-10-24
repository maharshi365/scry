use super::{
    Page, State,
    controls::action,
    primitives::{surface::card, text::label},
};
use crate::library_worker::Command;
use rune::{Cx, El};
use scry_library::{Media, MediaKind};

impl State {
    pub(super) fn refresh_page(&mut self) {
        self.context_menu = None;
        match self.navigation.page {
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
                if let Some(meta) = self.detail.clone() {
                    self.detail_id += 1;
                    self.open_detail_from(meta);
                }
            }
            Page::Addons => {
                if !self.config_busy
                    && let Some(services) = &self.services
                {
                    services.reload_addons();
                    self.config_busy = true;
                }
            }
        }
    }

    pub(super) fn result_media(&self, index: usize) -> Option<Media> {
        let meta = self.metas.get(index)?;
        let namespace = self
            .providers
            .get(index)
            .map(|a| a.manifest.id.as_str())
            .or_else(|| self.catalog.as_ref().map(|(a, _)| a.manifest.id.as_str()))
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

pub(super) fn view(s: &State, cx: &Cx<State>) -> Option<El> {
    let (point, item) = s.context_menu.as_ref()?;
    let bounds = cx.layout("scry").unwrap_or([0.0, 0.0, 880.0, 600.0]);
    let mut menu = card()
        .key("context-menu")
        .w(250.0)
        .padding(6.0)
        .gap(5.0)
        .radius(8.0)
        .modal(true, |s: &mut State| s.context_menu = None)
        .focus_group()
        .absolute(
            point.x.min(bounds[2] - 258.0).max(8.0),
            point.y.min(bounds[3] - 230.0).max(8.0),
        );
    if let Some(media) = item {
        menu = menu.child(label(media.title.clone()));
        for (key, title, choice) in [
            ("later", "Watch later", 0),
            ("remove", "Remove from watch later", 1),
            ("watched", "Mark as watched", 2),
            ("unwatched", "Mark as unwatched", 3),
        ] {
            let media = media.clone();
            menu = menu.child(
                action(format!("context-{key}"), title, move |s| {
                    s.context_menu = None;
                    s.db(
                        match choice {
                            0 => Command::SetWatchlist(media.clone(), true),
                            1 => Command::SetWatchlist(media.clone(), false),
                            2 => Command::SetWatched(media.clone(), true),
                            _ => Command::SetWatched(media.clone(), false),
                        },
                        true,
                    );
                })
                .w_full()
                .justify_start()
                .focusable(true)
                .focus_follows(),
            );
        }
    }
    Some(
        menu.child(
            action("context-refresh", "Refresh", State::refresh_page)
                .w_full()
                .justify_start()
                .focusable(true)
                .focus_follows(),
        ),
    )
}
