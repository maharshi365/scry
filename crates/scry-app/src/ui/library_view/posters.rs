//! Metadata for the saved and in-progress pages. Images use the shared artwork cache.
use super::{Media, MediaKey, State};
use crate::services::LibraryPoster;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(in crate::ui) struct PosterUi {
    pub cache: HashMap<MediaKey, Option<LibraryPoster>>,
    failures: HashMap<MediaKey, std::time::Instant>,
    pub generation: u64,
    pub busy: bool,
}

impl State {
    pub(in crate::ui) fn apply_up_next_entries(
        &mut self,
        items: Vec<(Media, Option<scry_library::Progress>)>,
    ) {
        let relayout = self.library.up_next.len().div_ceil(self.poster_columns)
            != items.len().div_ceil(self.poster_columns);
        self.library.up_next = items;
        self.library.up_next_loaded = true;
        self.sync_library_posters();
        if self.navigation.page == crate::ui::Page::Watching && relayout {
            self.library.grid_generation += 1;
            self.poster_rects.clear();
        }
        self.refresh_artwork();
    }
    pub(in crate::ui) fn apply_library_entries(&mut self, items: Vec<Media>) {
        let relayout = self
            .library
            .entries
            .as_ref()
            .map_or(0, Vec::len)
            .div_ceil(self.poster_columns)
            != items.len().div_ceil(self.poster_columns);
        self.library.entries = Some(items);
        self.sync_library_posters();
        if self.navigation.page == crate::ui::Page::Library && relayout {
            self.library.grid_generation += 1;
            self.poster_rects.clear();
        }
        self.refresh_artwork();
    }

    pub(in crate::ui) fn apply_watching_entries(
        &mut self,
        items: Vec<(Media, scry_library::Progress)>,
    ) {
        let relayout = self.library.watching.len().div_ceil(self.poster_columns)
            != items.len().div_ceil(self.poster_columns);
        self.library.watching = items;
        self.library.watching_watched.retain(|key| {
            self.library
                .watching
                .iter()
                .any(|(media, _)| media.key == *key)
        });
        self.sync_library_posters();
        if self.navigation.page == crate::ui::Page::Watching && relayout {
            self.library.grid_generation += 1;
            self.poster_rects.clear();
        }
        self.refresh_artwork();
    }

    pub(in crate::ui) fn poster_key(media: &Media) -> &MediaKey {
        media.parent.as_ref().unwrap_or(&media.key)
    }

    pub(in crate::ui) fn saved_grid_media(&self) -> Vec<&Media> {
        match self.navigation.page {
            crate::ui::Page::Library => self.library.entries.iter().flatten().collect(),
            crate::ui::Page::Watching => self
                .library
                .watching
                .iter()
                .map(|(media, _)| media)
                .chain(self.library.up_next.iter().map(|(media, _)| media))
                .collect(),
            _ => Vec::new(),
        }
    }

    pub(in crate::ui) fn library_poster(&self, media: &Media) -> Option<&LibraryPoster> {
        self.library
            .posters
            .cache
            .get(Self::poster_key(media))
            .and_then(Option::as_ref)
    }

    fn sync_library_posters(&mut self) {
        let keys: HashSet<_> = self
            .library
            .entries
            .iter()
            .flatten()
            .chain(self.library.watching.iter().map(|(media, _)| media))
            .chain(self.library.up_next.iter().map(|(media, _)| media))
            .map(|m| Self::poster_key(m).clone())
            .collect();
        self.library
            .posters
            .cache
            .retain(|key, _| keys.contains(key));
        for key in keys {
            let kind = if key.kind == scry_library::MediaKind::Movie {
                "movie"
            } else {
                "series"
            };
            let known = self
                .detail
                .iter()
                .filter(|_| key.namespace == "imdb" || key.namespace == self.detail_namespace)
                .chain(self.metas.iter().filter(|_| key.namespace == "imdb"))
                .chain(
                    self.library
                        .series
                        .details
                        .get(&key)
                        .and_then(|(meta, _)| meta.as_ref()),
                )
                .find(|m| {
                    m.id == key.id
                        && m.kind == kind
                        && m.poster.as_deref().is_some_and(|p| !p.trim().is_empty())
                });
            if let Some(meta) = known {
                self.library.posters.cache.insert(
                    key,
                    Some(LibraryPoster {
                        poster: meta.poster.clone(),
                        year: meta.year.clone(),
                    }),
                );
            }
        }
    }

    pub(in crate::ui) fn missing_library_posters(&self) -> Vec<Media> {
        let mut seen = HashSet::new();
        self.saved_grid_media()
            .into_iter()
            .enumerate()
            .filter(|(index, media)| {
                !self
                    .library
                    .posters
                    .cache
                    .contains_key(Self::poster_key(media))
                    && crate::ui::poster_grid::visible(self, *index)
                    && seen.insert(Self::poster_key(media).clone())
            })
            .map(|(_, media)| Media {
                key: Self::poster_key(media).clone(),
                title: media.title.clone(),
                parent: None,
            })
            .take(12)
            .collect()
    }

    pub(in crate::ui) fn request_library_posters(&mut self) {
        let posters = &mut self.library.posters;
        posters.failures.retain(|key, time| {
            if time.elapsed() >= std::time::Duration::from_secs(30) {
                if posters.cache.get(key).is_some_and(Option::is_none) {
                    posters.cache.remove(key);
                }
                false
            } else {
                posters.cache.contains_key(key)
            }
        });
        if self.library.posters.busy || self.services.is_none() || self.addons.is_empty() {
            return;
        }
        // A full series metadata request also supplies its poster. Let that
        // request finish before falling back to a separate artwork lookup.
        let missing: Vec<_> = self
            .missing_library_posters()
            .into_iter()
            .filter(|media| {
                media.key.kind != scry_library::MediaKind::Series
                    || self.library.series.details.contains_key(&media.key)
            })
            .collect();
        if missing.is_empty() {
            return;
        }
        if let Some(services) = &mut self.services {
            self.library.posters.busy = true;
            services.library_posters(
                self.library.posters.generation,
                self.addons.clone(),
                missing,
            );
        }
    }

    pub(in crate::ui) fn metadata_retry_after(&self) -> Option<std::time::Duration> {
        let delay = std::time::Duration::from_secs(30);
        self.library
            .posters
            .failures
            .values()
            .map(|time| delay.saturating_sub(time.elapsed()))
            .chain(
                self.library
                    .series
                    .details
                    .values()
                    .filter(|(meta, _)| meta.is_none())
                    .map(|(_, time)| delay.saturating_sub(time.elapsed())),
            )
            .filter(|remaining| !remaining.is_zero())
            .min()
    }

    pub(super) fn remember_series_poster(&mut self, key: &MediaKey, meta: &scry_addons::Meta) {
        if meta
            .poster
            .as_deref()
            .is_some_and(|poster| !poster.trim().is_empty())
        {
            self.apply_library_posters(
                self.library.posters.generation,
                vec![(
                    key.clone(),
                    Some(LibraryPoster {
                        poster: meta.poster.clone(),
                        year: meta.year.clone(),
                    }),
                )],
                false,
            );
        }
    }

    pub(in crate::ui) fn apply_library_posters(
        &mut self,
        generation: u64,
        found: Vec<(MediaKey, Option<LibraryPoster>)>,
        done: bool,
    ) {
        if generation != self.library.posters.generation {
            return;
        }
        for (key, poster) in found {
            if self
                .library
                .entries
                .iter()
                .flatten()
                .any(|m| *Self::poster_key(m) == key)
                || self
                    .library
                    .watching
                    .iter()
                    .any(|(m, _)| *Self::poster_key(m) == key)
                || self
                    .library
                    .up_next
                    .iter()
                    .any(|(m, _)| *Self::poster_key(m) == key)
            {
                if poster.is_none() {
                    self.library
                        .posters
                        .failures
                        .insert(key.clone(), std::time::Instant::now());
                } else {
                    self.library.posters.failures.remove(&key);
                }
                self.library.posters.cache.insert(key, poster);
            }
        }
        if done {
            self.library.posters.busy = false;
        }
        self.refresh_artwork();
    }

    pub(in crate::ui) fn retry_library_posters(&mut self) {
        self.retry_series_details();
        self.library.posters.generation += 1;
        self.library.posters.busy = false;
        self.library.posters.failures.clear();
        self.library
            .posters
            .cache
            .retain(|_, poster| poster.is_some());
    }
}
