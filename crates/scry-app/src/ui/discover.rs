//! Virtualized catalog grid. Placeholder rows preserve scroll geometry.
use super::{
    State,
    artwork::{POSTER_CARD_HEIGHT, POSTER_GAP, poster, poster_columns},
    controls::action,
    design::tokens::{panel, theme, type_},
    primitives::{
        surface::scroll,
        text::{heading, label},
    },
};
use rune::{Cx, El, col, row, text};

#[expect(
    clippy::too_many_lines,
    reason = "Grid geometry and virtualized row layout are kept together so viewport invariants stay visible"
)]
pub(super) fn discover(s: &State, cx: &Cx<State>) -> El {
    let t = theme();
    let viewport_key = format!("titles-scroll-{}", s.navigation.route_key);
    let columns = cx
        .layout(&viewport_key)
        .map_or(3, |rect| poster_columns(rect[2]));
    let mut cards = col().key("title-cards").gap(POSTER_GAP).w_full();
    if s.addons.is_empty() {
        cards = cards.child(
            col()
                .padding(28.0)
                .gap(16.0)
                .radius(16.0)
                .fill(t.card)
                .child(heading("No catalogs installed"))
                .child(label("Add Cinemeta to browse movies and TV shows."))
                .child(action("setup-cinemeta", "Open add-ons", |s| {
                    s.navigate(super::Page::Addons);
                })),
        );
    } else if s.metas.is_empty() && !s.loading {
        cards = cards.child(label(
            "No titles found. Try searching for a movie or TV show.",
        ));
    }
    for (row_index, metas) in s.metas.chunks(columns).enumerate() {
        let route_key = s.navigation.route_key;
        let mut card_row = col()
            .key(format!("title-row-{row_index}"))
            .w_full()
            .h(POSTER_CARD_HEIGHT)
            .gap(POSTER_GAP)
            .equal_columns(columns)
            .on_layout(move |s: &mut State, rect| {
                if s.poster_route != route_key || s.poster_columns != columns {
                    s.poster_rects.clear();
                }
                s.poster_route = route_key;
                s.poster_columns = columns;
                s.poster_rects.insert(row_index, rect);
                // Poll runs before layout. Queue these URLs now; an idle window
                // might not get another frame until a worker or user wakes it.
                s.refresh_artwork();
            });
        // Keep cheap row-sized placeholders for scroll geometry and history.
        // Only nearby rows need poster handles, text shaping and hit targets.
        let nearby = if s.poster_route == s.navigation.route_key && s.poster_columns == columns {
            s.poster_viewport
                .zip(s.poster_rects.get(&row_index))
                .is_none_or(|(viewport, rect)| {
                    rect[1] < viewport[1] + viewport[3] * 2.0
                        && rect[1] + rect[3] > viewport[1] - viewport[3]
                })
        } else {
            row_index < 4
        };
        if !nearby {
            cards = cards.child(card_row);
            continue;
        }
        for (column, meta) in metas.iter().enumerate() {
            let index = row_index * columns + column;
            card_row = card_row.child(
                col()
                    .key(format!("title-{index}"))
                    .role("button")
                    .label(format!("Open {}", meta.name))
                    .focusable(true)
                    .focus_ring(2.0, 2.0, super::design::tokens::accent())
                    .w_full()
                    .min_w(0.0)
                    .gap(8.0)
                    .padding(4.0)
                    .radius(4.0)
                    .fill(panel())
                    .hover(t.hover)
                    .on_press(move |s: &mut State| s.open_result(index))
                    .on_secondary_press(move |s: &mut State, p, _| {
                        s.context_menu = Some((p, s.result_media(index)));
                    })
                    .child(poster(s, meta, index))
                    .child(
                        col()
                            .gap(5.0)
                            .child(
                                text(meta.name.clone())
                                    .w_full()
                                    .truncate()
                                    .font(type_::POSTER_TITLE)
                                    .color(t.text),
                            )
                            .child(
                                label(if s.displayed_query.is_empty() {
                                    meta.year.clone()
                                } else {
                                    format!(
                                        "{}{}{}",
                                        meta.year,
                                        if meta.year.is_empty() { "" } else { " · " },
                                        if meta.kind == "series" {
                                            "TV show"
                                        } else {
                                            "Movie"
                                        }
                                    )
                                })
                                .key(format!("title-info-{index}")),
                            ),
                    ),
            );
        }
        cards = cards.child(card_row);
    }
    col()
        .key("discover")
        .gap(18.0)
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .child(
            row()
                .gap(10.0)
                .center_y()
                .child(
                    col()
                        .grow(1.0)
                        .child(heading(if s.displayed_query.is_empty() {
                            if s.kind == "series" {
                                "TV shows"
                            } else {
                                "Movies"
                            }
                            .to_owned()
                        } else {
                            format!("Results for “{}”", s.displayed_query)
                        })),
                ),
        )
        .child(
            scroll(viewport_key, cards).on_layout(|s: &mut State, rect| {
                s.poster_viewport = Some(rect);
                s.refresh_artwork();
            }),
        )
}
