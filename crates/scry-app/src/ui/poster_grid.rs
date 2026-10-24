//! One virtualized poster grid for catalogs, search, Library, and Currently watching.
use super::{
    Page, State,
    artwork::{POSTER_CARD_HEIGHT, POSTER_GAP, poster, poster_columns},
    design::tokens::{accent, panel, theme, type_},
    formatting::clock,
    primitives::{surface::scroll, text::label},
};
use rune::{Cx, El, col, text};
use scry_library::Media;

struct Entry {
    title: String,
    subtitle: String,
    poster: Option<String>,
    media: Option<Media>,
}

fn entries(s: &State) -> Vec<Entry> {
    if matches!(s.navigation.page, Page::Library | Page::Watching) {
        return s
            .saved_grid_media()
            .into_iter()
            .enumerate()
            .map(|(index, media)| {
                let info = s.library_poster(media);
                let subtitle = if s.navigation.page == Page::Watching {
                    let progress = &s.library.watching[index].1;
                    if progress.position_seconds == 0.0 {
                        "Next episode".into()
                    } else {
                        format!(
                            "{} / {}",
                            clock(Some(progress.position_seconds)),
                            clock(Some(progress.duration_seconds))
                        )
                    }
                } else {
                    info.map_or_else(String::new, |p| p.year.clone())
                };
                Entry {
                    title: media.title.clone(),
                    subtitle,
                    poster: info.and_then(|p| p.poster.clone()),
                    media: Some(media.clone()),
                }
            })
            .collect();
    }
    s.metas
        .iter()
        .map(|meta| Entry {
            title: meta.name.clone(),
            poster: meta.poster.clone(),
            media: None,
            subtitle: if s.displayed_query.is_empty() {
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
            },
        })
        .collect()
}

pub(super) fn artwork_urls(s: &State) -> Vec<String> {
    if matches!(s.navigation.page, Page::Library | Page::Watching) {
        return visible_urls(
            s,
            s.saved_grid_media()
                .into_iter()
                .map(|m| s.library_poster(m).and_then(|p| p.poster.as_ref())),
        );
    }
    visible_urls(s, s.metas.iter().map(|m| m.poster.as_ref()))
}

fn visible_urls<'a>(s: &State, posters: impl Iterator<Item = Option<&'a String>>) -> Vec<String> {
    let mut urls = Vec::new();
    for (index, poster) in posters.enumerate() {
        if visible(s, index)
            && let Some(url) = poster
            && !urls.contains(url)
        {
            urls.push(url.clone());
        }
        if urls.len() == 32 {
            break;
        }
    }
    urls
}

pub(super) fn visible(s: &State, index: usize) -> bool {
    if s.poster_route == s.navigation.route_key && !s.poster_rects.is_empty() {
        s.poster_viewport
            .zip(s.poster_rects.get(&(index / s.poster_columns)))
            .is_some_and(|(viewport, rect)| {
                rect[1] < viewport[1] + viewport[3] && rect[1] + rect[3] > viewport[1]
            })
    } else {
        index < 12
    }
}

pub(super) fn view(s: &State, cx: &Cx<State>) -> El {
    let prefix = prefix(s);
    let viewport_key = match s.navigation.page {
        Page::Library => super::library_view::grid_key(s),
        Page::Watching => "watching-scroll".into(),
        _ => format!("titles-scroll-{}", s.navigation.route_key),
    };
    let columns = cx
        .layout(&viewport_key)
        .map_or(3, |rect| poster_columns(rect[2]));
    let entries = entries(s);
    let mut cards = col()
        .key(format!("{prefix}-cards"))
        .gap(POSTER_GAP)
        .w_full();
    for (row_index, entries) in entries.chunks(columns).enumerate() {
        let route_key = s.navigation.route_key;
        let mut card_row = col()
            .key(format!("{prefix}-row-{row_index}"))
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
                s.refresh_artwork();
                s.request_library_posters();
            });
        let nearby = if s.poster_route == route_key && s.poster_columns == columns {
            s.poster_viewport
                .zip(s.poster_rects.get(&row_index))
                .is_none_or(|(viewport, rect)| {
                    rect[1] < viewport[1] + viewport[3] * 2.0
                        && rect[1] + rect[3] > viewport[1] - viewport[3]
                })
        } else {
            row_index < 4
        };
        if nearby {
            for (column, entry) in entries.iter().enumerate() {
                card_row = card_row.child(card(s, entry, row_index * columns + column));
            }
        }
        cards = cards.child(card_row);
    }
    scroll(viewport_key, cards).on_layout(|s: &mut State, rect| {
        s.poster_viewport = Some(rect);
        s.refresh_artwork();
        s.request_library_posters();
    })
}

fn prefix(s: &State) -> &'static str {
    match s.navigation.page {
        Page::Library => "library",
        Page::Watching => "watching",
        _ => "title",
    }
}

fn card(s: &State, entry: &Entry, index: usize) -> El {
    let prefix = prefix(s);
    let key = if entry.media.is_some() {
        format!("{prefix}-open-{index}")
    } else {
        format!("{prefix}-{index}")
    };
    let item = entry.media.clone();
    let menu = entry.media.clone();
    col()
        .key(key.clone())
        .role("button")
        .label(format!("Open {}", entry.title))
        .focusable(true)
        .focus_ring(2.0, 2.0, accent())
        .w_full()
        .min_w(0.0)
        .gap(8.0)
        .padding(4.0)
        .radius(4.0)
        .fill(panel())
        .hover(theme().hover)
        .on_press(move |s: &mut State| {
            s.navigation.anchor = Some(key.clone());
            if let Some(media) = &item {
                s.open_library_media(media);
            } else {
                s.open_result(index);
            }
        })
        .on_secondary_press(move |s: &mut State, p, _| {
            s.context_menu = Some((p, menu.clone().or_else(|| s.result_media(index))));
        })
        .child(
            poster(s, &entry.title, entry.poster.as_deref(), index)
                .key(format!("{prefix}-poster-{index}")),
        )
        .child(
            col()
                .gap(5.0)
                .child(
                    text(entry.title.clone())
                        .w_full()
                        .truncate()
                        .font(type_::POSTER_TITLE)
                        .color(theme().text),
                )
                .child(label(entry.subtitle.clone()).key(format!("{prefix}-info-{index}"))),
        )
}
