//! Catalog and search headings above the shared poster grid.
use super::{
    State,
    controls::action,
    design::{icons, tokens},
    poster_grid,
    primitives::{
        button::{Button, DropdownVariant, Variant, dropdown_trigger, menu_choice},
        pattern::detail_header,
        surface::{dropdown, empty_state},
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
    if s.addons.installed.is_empty() && s.discover.results.is_empty() {
        return body.child(empty_state(
            "catalog-empty",
            "No catalogs installed",
            "Add a catalog provider to browse titles.",
            &icons::ADD,
            row().child(
                Button::new("setup-cinemeta", "Open add-ons")
                    .variant(Variant::Primary)
                    .on_press(|s: &mut State| s.navigate(super::Page::Addons)),
            ),
        ));
    } else if s.discover.results.is_empty() && !s.discover.request.loading {
        let mut actions = row();
        if s.discover.paging.failed {
            actions = actions.child(
                Button::new("catalog-retry", "Retry loading titles")
                    .variant(Variant::Primary)
                    .on_press(State::fetch_catalog),
            );
        }
        return body.child(empty_state(
            "catalog-empty",
            "No titles found",
            "Try another search or choose a different catalog.",
            &icons::LIBRARY,
            actions,
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
    Some(dropdown(
        "catalog-menu",
        "catalog-select",
        anchor[2].max(tokens::layout::CONTROL_MIN_WIDTH),
        choices,
        |s: &mut State| s.discover.menu_open = false,
    ))
}
