//! Catalog and search headings above the shared poster grid.
use super::{
    State,
    controls::action,
    design::tokens::theme,
    poster_grid,
    primitives::text::{heading, label},
};
use rune::{Cx, El, col};

pub(super) fn discover(s: &State, cx: &Cx<State>) -> El {
    let mut body = col()
        .key("discover")
        .gap(18.0)
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .child(heading(if s.displayed_query.is_empty() {
            if s.kind == "series" {
                "TV shows".to_owned()
            } else {
                "Movies".to_owned()
            }
        } else {
            format!("Results for “{}”", s.displayed_query)
        }));
    if s.addons.is_empty() {
        body = body.child(
            col()
                .padding(28.0)
                .gap(16.0)
                .radius(16.0)
                .fill(theme().card)
                .child(heading("No catalogs installed"))
                .child(label("Add Cinemeta to browse movies and TV shows."))
                .child(action("setup-cinemeta", "Open add-ons", |s| {
                    s.navigate(super::Page::Addons);
                })),
        );
    } else if s.metas.is_empty() && !s.loading {
        body = body.child(label(
            "No titles found. Try searching for a movie or TV show.",
        ));
    }
    body.child(poster_grid::view(s, cx))
}
