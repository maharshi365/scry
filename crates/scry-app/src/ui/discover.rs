//! Catalog and search headings above the shared poster grid.
use super::{
    State,
    controls::action,
    design::tokens,
    poster_grid,
    primitives::{
        pattern::detail_header,
        text::{heading, label},
    },
};
use rune::{Cx, El, col, row};

pub(super) fn discover(s: &State, cx: &Cx<State>) -> El {
    let mut body = col()
        .key("discover")
        .gap(tokens::layout::PAGE_GAP)
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .child(detail_header(
            "discover-header",
            &if s.displayed_query.is_empty() {
                super::catalog::category_label(&s.kind)
            } else {
                format!("Results for “{}”", s.displayed_query)
            },
            &format!(
                "{:03} titles loaded{}",
                s.metas.len(),
                if s.loading { " / loading" } else { "" }
            ),
            row(),
        ));
    if s.addons.is_empty() {
        body = body.child(
            super::primitives::surface::Variant::Setup
                .apply(col())
                .padding(tokens::layout::PANEL_INSET)
                .gap(tokens::layout::PAGE_GAP)
                .child(heading("No catalogs installed"))
                .child(label("Add a catalog provider to browse titles."))
                .child(action("setup-cinemeta", "Open add-ons", |s| {
                    s.navigate(super::Page::Addons);
                })),
        );
    } else if s.metas.is_empty() && !s.loading {
        body = body.child(label(
            "No supported titles found. Try searching for a movie or TV show.",
        ));
    }
    if s.catalog_paging.failed {
        body = body.child(action("catalog-retry", "Retry loading titles", |s| {
            if s.metas.is_empty() {
                s.fetch_catalog();
            } else {
                s.catalog_paging.failed = false;
                s.error = None;
                s.request_more_catalog();
            }
        }));
    }
    body.child(poster_grid::view(s, cx))
}
