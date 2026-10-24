//! One virtualized poster grid for catalogs, search, Library, and Currently watching.
use super::{
    Page, State,
    artwork::{
        POSTER_CARD_HEIGHT, POSTER_GAP, poster, poster_columns, poster_overlay,
        poster_status_overlay,
    },
    design::tokens::{accent, panel, theme, type_},
    primitives::{
        surface::scroll,
        text::{heading, label},
    },
};
use rune::{Cx, El, col, text};
use scry_library::{Media, MediaKey, MediaKind};

struct Entry {
    title: String,
    subtitle: String,
    poster: Option<String>,
    media: Option<Media>,
    identity: Option<MediaKey>,
    overlay: Overlay,
}

#[derive(Clone, Copy)]
enum Overlay {
    Summary,
    Item(Option<scry_library::Progress>),
}

fn entries(s: &State) -> Vec<Entry> {
    if matches!(s.navigation.page, Page::Library | Page::Watching) {
        return s
            .saved_grid_media()
            .into_iter()
            .enumerate()
            .map(|(index, media)| {
                let info = s.library_poster(media);
                let subtitle = if s.navigation.page == Page::Watching
                    && media.key.kind == MediaKind::Episode
                    && s.library
                        .watching
                        .get(index)
                        .is_some_and(|(_, progress)| progress.position_seconds == 0.0)
                {
                    "Next episode".into()
                } else {
                    info.map_or_else(String::new, |p| p.year.clone())
                };
                Entry {
                    title: media.title.clone(),
                    subtitle,
                    poster: info.and_then(|p| p.poster.clone()),
                    media: Some(media.clone()),
                    identity: Some(State::poster_key(media).clone()),
                    overlay: if s.navigation.page == Page::Watching
                        && index >= s.library.watching.len()
                    {
                        Overlay::Item(s.library.up_next[index - s.library.watching.len()].1)
                    } else {
                        Overlay::Summary
                    },
                }
            })
            .collect();
    }
    s.metas
        .iter()
        .enumerate()
        .map(|(index, meta)| Entry {
            title: meta.name.clone(),
            poster: meta.poster.clone(),
            media: None,
            identity: s.result_media(index).map(|m| m.key),
            overlay: Overlay::Summary,
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

pub(super) fn movie_keys(s: &State) -> Vec<MediaKey> {
    let media = if matches!(s.navigation.page, Page::Library | Page::Watching) {
        s.saved_grid_media()
            .into_iter()
            .map(|m| m.key.clone())
            .collect::<Vec<_>>()
    } else {
        (0..s.metas.len())
            .filter_map(|index| s.result_media(index).map(|m| m.key))
            .collect()
    };
    media
        .into_iter()
        .enumerate()
        .filter(|(index, key)| key.kind == MediaKind::Movie && visible(s, *index))
        .map(|(_, key)| key)
        .take(200)
        .collect()
}

pub(super) fn series_keys(s: &State) -> Vec<MediaKey> {
    if s.navigation.page == Page::Detail {
        return s
            .library_media()
            .map(|m| State::poster_key(&m).clone())
            .filter(|key| key.kind == MediaKind::Series)
            .into_iter()
            .collect();
    }
    let media = match s.navigation.page {
        Page::Watching | Page::Library => s
            .saved_grid_media()
            .into_iter()
            .map(|m| State::poster_key(m).clone())
            .collect::<Vec<_>>(),
        Page::Discover => (0..s.metas.len())
            .filter_map(|index| s.result_media(index).map(|m| m.key))
            .collect(),
        _ => return Vec::new(),
    };
    let mut keys = Vec::new();
    for (index, key) in media.into_iter().enumerate() {
        if key.kind == MediaKind::Series && visible(s, index) && !keys.contains(&key) {
            keys.push(key);
        }
        if keys.len() == 200 {
            break;
        }
    }
    keys
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
            .zip(s.poster_rects.get(&row_index(s, index, s.poster_columns)))
            .is_some_and(|(viewport, rect)| {
                rect[1] < viewport[1] + viewport[3] && rect[1] + rect[3] > viewport[1]
            })
    } else {
        index < 12
    }
}

fn row_index(s: &State, index: usize, columns: usize) -> usize {
    if s.navigation.page == Page::Watching && index >= s.library.watching.len() {
        s.library.watching.len().div_ceil(columns) + (index - s.library.watching.len()) / columns
    } else {
        index / columns
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
    let cards = if s.navigation.page == Page::Watching {
        let split = s.library.watching.len();
        let current = if split == 0 && !s.library.up_next.is_empty() {
            label("Nothing in progress.").key("watching-empty")
        } else if split == 0 {
            super::library_view::watching_empty()
        } else {
            rows(s, &entries[..split], columns, 0, prefix)
        };
        let mut sections = col()
            .w_full()
            .gap(18.0)
            .child(current)
            .child(heading("What to watch next").key("up-next-heading"));
        if s.library.up_next.is_empty() {
            sections = sections.child(label(
                if !s.library.up_next_loaded && s.library.worker.is_some() {
                    "Loading your library…"
                } else {
                    "No unwatched movies or released episodes in your library."
                },
            ));
        } else {
            sections = sections.child(rows(
                s,
                &entries[split..],
                columns,
                split.div_ceil(columns),
                "up-next",
            ));
        }
        sections.child(super::library_view::up_next_pages(s))
    } else {
        rows(s, &entries, columns, 0, prefix)
    };
    scroll(viewport_key, cards).on_layout(|s: &mut State, rect| {
        s.poster_viewport = Some(rect);
        s.refresh_artwork();
        s.request_library_posters();
        s.request_movie_states();
        s.request_series_states();
    })
}

fn rows(
    s: &State,
    entries: &[Entry],
    columns: usize,
    row_offset: usize,
    prefix: &'static str,
) -> El {
    let mut cards = col()
        .key(format!("{prefix}-cards"))
        .gap(POSTER_GAP)
        .w_full();
    for (row_index, entries) in entries.chunks(columns).enumerate() {
        let layout_index = row_offset + row_index;
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
                s.poster_rects.insert(layout_index, rect);
                s.refresh_artwork();
                s.request_library_posters();
                s.request_movie_states();
                s.request_series_states();
            });
        let nearby = if s.poster_route == route_key && s.poster_columns == columns {
            s.poster_viewport
                .zip(s.poster_rects.get(&layout_index))
                .map_or(layout_index < 4, |(viewport, rect)| {
                    rect[1] < viewport[1] + viewport[3] * 2.0
                        && rect[1] + rect[3] > viewport[1] - viewport[3]
                })
        } else {
            layout_index < 4
        };
        if nearby {
            for (column, entry) in entries.iter().enumerate() {
                card_row = card_row.child(card(s, entry, row_index * columns + column, prefix));
            }
        }
        cards = cards.child(card_row);
    }
    cards
}

fn prefix(s: &State) -> &'static str {
    match s.navigation.page {
        Page::Library => "library",
        Page::Watching => "watching",
        _ => "title",
    }
}

fn card(s: &State, entry: &Entry, index: usize, prefix: &str) -> El {
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
            s.open_title_menu(p, menu.clone().or_else(|| s.result_media(index)));
        })
        .child({
            let surface = poster(s, &entry.title, entry.poster.as_deref(), index);
            let key = format!("{prefix}-poster-{index}");
            match entry.overlay {
                Overlay::Summary => {
                    poster_overlay(s, surface, entry.identity.as_ref(), &key, 246.0)
                }
                Overlay::Item(progress) => {
                    poster_status_overlay(surface, &key, 246.0, false, progress)
                }
            }
        })
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
