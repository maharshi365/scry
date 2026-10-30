//! Catalog and search headings above the shared poster grid.
use super::{
    State,
    controls::action,
    design::tokens,
    poster_grid,
    primitives::{
        button::{DropdownVariant, dropdown_trigger, menu_choice},
        pattern::detail_header,
        surface::dropdown,
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
            &if s.discover.displayed_query.is_empty() {
                super::catalog::category_label(&s.discover.kind)
            } else {
                format!("Results for “{}”", s.discover.displayed_query)
            },
            &format!(
                "{} titles loaded{}",
                s.discover.results.len(),
                if s.discover.request.loading {
                    " / loading"
                } else {
                    ""
                }
            ),
            catalog_selector(s),
        ));
    if s.addons.installed.is_empty() {
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
    } else if s.discover.results.is_empty() && !s.discover.request.loading {
        body = body.child(label(
            "No supported titles found. Try searching for a movie or TV show.",
        ));
    }
    if s.discover.paging.failed {
        body = body.child(action("catalog-retry", "Retry loading titles", |s| {
            if s.discover.results.is_empty() {
                s.fetch_catalog();
            } else {
                s.discover.paging.failed = false;
                s.discover.request.error = None;
                s.request_more_catalog();
            }
        }));
    }
    body.child(poster_grid::view(s, cx))
}

fn catalog_selector(s: &State) -> El {
    let mut actions = row();
    if s.navigation.page == super::Page::Discover
        && s.discover.displayed_query.is_empty()
        && let Some((_, catalog)) = &s.discover.catalog
    {
        let name = if catalog.name.is_empty() {
            &catalog.id
        } else {
            &catalog.name
        };
        let title = if catalog.supports_paging() {
            name.clone()
        } else {
            format!("{name} (limited list)")
        };
        actions = actions.child(
            dropdown_trigger("catalog-select", title, DropdownVariant::Framed)
                .on_press(|s: &mut State| s.discover.menu_open = !s.discover.menu_open),
        );
    }
    actions
}

pub(super) fn catalog_menu(s: &State, cx: &Cx<State>) -> Option<El> {
    if !s.discover.menu_open || s.navigation.page != super::Page::Discover {
        return None;
    }
    let anchor = cx.layout("catalog-select")?;
    let catalogs = s.catalogs();
    let mut choices = col().w_full().gap(tokens::layout::COPY_GAP);
    for (index, (addon, catalog)) in catalogs.iter().enumerate() {
        let selected = s
            .discover
            .catalog
            .as_ref()
            .is_some_and(|(current_addon, current)| {
                current_addon.manifest_url == addon.manifest_url
                    && current.id == catalog.id
                    && current.kind == catalog.kind
            });
        let name = if catalog.name.is_empty() {
            &catalog.id
        } else {
            &catalog.name
        };
        choices = choices.child(
            menu_choice(
                format!("catalog-option-{index}"),
                format!("{name} · {}", addon.manifest.name),
                selected,
                move |s: &mut State| s.select_catalog(index),
            )
            .w_full(),
        );
    }
    let visible = u16::try_from(catalogs.len().min(5)).unwrap_or(5);
    Some(
        dropdown(
            "catalog-menu",
            "catalog-select",
            anchor[2].max(tokens::layout::CONTROL_MIN_WIDTH),
            f32::from(visible) * (tokens::control::HEIGHT + tokens::layout::COPY_GAP),
            |s: &mut State| s.discover.menu_open = false,
        )
        .child(choices),
    )
}
