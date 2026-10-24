//! Visible movie state is read on the database worker, never on the UI thread.
use super::{MediaKey, MediaKind, State};
use crate::library_worker::{Command, MovieState};

#[derive(Default)]
pub(in crate::ui) struct MovieUi {
    pub generation: u64,
    pub requested: Vec<MediaKey>,
    pub states: Vec<MovieState>,
}

impl State {
    pub(in crate::ui) fn request_movie_states(&mut self) {
        if !matches!(
            self.navigation.page,
            super::super::Page::Discover
                | super::super::Page::Library
                | super::super::Page::Watching
        ) {
            return;
        }
        let Some(worker) = &self.library.worker else {
            return;
        };
        let keys: Vec<_> = super::super::poster_grid::movie_keys(self);
        if keys == self.library.movies.requested {
            return;
        }
        let generation = self.library.movies.generation + 1;
        if worker
            .send(Command::MovieStates {
                generation,
                keys: keys.clone(),
            })
            .is_ok()
        {
            self.library.movies.generation = generation;
            self.library.movies.requested = keys;
        }
    }

    pub(in crate::ui) fn movie_status(
        &self,
        key: &MediaKey,
    ) -> (bool, Option<scry_library::Progress>) {
        if key.kind != MediaKind::Movie {
            return (false, None);
        }
        if let Some(state) = self
            .library
            .movies
            .states
            .iter()
            .find(|s| s.key == *key)
            .filter(|_| self.navigation.page != super::super::Page::Detail)
        {
            return (state.watched, state.progress);
        }
        if let Some(selected) = self
            .library
            .snapshot
            .as_ref()
            .and_then(|s| s.selected_state.as_ref())
            .filter(|selected| selected.key == *key)
        {
            return (selected.watched.watched, selected.progress);
        }
        (
            false,
            self.library
                .watching
                .iter()
                .find(|(media, _)| media.key == *key)
                .map(|(_, p)| *p),
        )
    }
}
