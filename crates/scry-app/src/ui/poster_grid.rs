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
    Item {
        watched: bool,
        progress: Option<scry_library::Progress>,
    },
}

fn entry(s: &State, saved: &[&Media], index: usize) -> Entry {
    if let Some(media) = saved.get(index).copied() {
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
            s.series_progress_label(&media.key)
                .unwrap_or_else(|| info.map_or_else(String::new, |p| p.year.clone()))
        };
        return Entry {
            title: media.title.clone(),
            subtitle,
            poster: info.and_then(|p| p.poster.clone()),
            media: Some(media.clone()),
            identity: Some(State::poster_key(media).clone()),
            overlay: if s.navigation.page == Page::Watching
                && (media.key.kind == MediaKind::Episode || index >= s.library.watching.len())
            {
                Overlay::Item {
                    watched: index < s.library.watching.len()
                        && s.library.watching_watched.contains(&media.key),
                    progress: if index < s.library.watching.len() {
                        Some(s.library.watching[index].1)
                    } else {
                        s.library.up_next[index - s.library.watching.len()].1
                    },
                }
            } else {
                Overlay::Summary
            },
        };
    }
    let meta = &s.metas[index];
    Entry {
        title: meta.name.clone(),
        poster: meta.poster.clone(),
        media: None,
        identity: s.result_media(index).map(|m| m.key),
        overlay: Overlay::Summary,
        subtitle: if meta.kind == "series"
            && let Some(summary) = s
                .result_media(index)
                .and_then(|media| s.series_progress_label(&media.key))
        {
            summary
        } else if s.displayed_query.is_empty() {
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
    }
}

pub(super) fn movie_keys(s: &State) -> Vec<MediaKey> {
    if matches!(s.navigation.page, Page::Library | Page::Watching) {
        s.saved_grid_media()
            .into_iter()
            .enumerate()
            .filter(|(index, media)| media.key.kind == MediaKind::Movie && visible(s, *index))
            .map(|(_, media)| media.key.clone())
            .take(200)
            .collect()
    } else {
        (0..s.metas.len())
            .filter(|index| visible(s, *index))
            .filter_map(|index| s.result_media(index).map(|m| m.key))
            .filter(|key| key.kind == MediaKind::Movie)
            .take(200)
            .collect()
    }
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
            .enumerate()
            .filter(|(index, _)| visible(s, *index))
            .map(|(_, m)| State::poster_key(m).clone())
            .collect::<Vec<_>>(),
        Page::Discover => (0..s.metas.len())
            .filter(|index| visible(s, *index))
            .filter_map(|index| s.result_media(index).map(|m| m.key))
            .collect(),
        _ => return Vec::new(),
    };
    let mut keys = Vec::new();
    for key in media {
        if key.kind == MediaKind::Series && !keys.contains(&key) {
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
    let saved = s.saved_grid_media();
    let count = if matches!(s.navigation.page, Page::Library | Page::Watching) {
        saved.len()
    } else {
        s.metas.len()
    };
    let mut cards = col().key(format!("{prefix}-sections")).w_full();
    if s.navigation.page == Page::Watching {
        let split = s.library.watching.len();
        if split == 0 && s.library.up_next.is_empty() {
            cards = cards.child(super::library_view::watching_empty());
        } else if split > 0 {
            cards = append_rows(cards, s, &saved, 0..split, columns, 0, prefix);
        }
        cards = cards.child(
            heading("What to watch next")
                .key("up-next-heading")
                .pb(POSTER_GAP),
        );
        if s.library.up_next.is_empty() {
            cards = cards.child(
                label(if !s.library.up_next_loaded && s.library.worker.is_some() {
                    "Loading your library…"
                } else {
                    "No unwatched movies or released episodes in your library."
                })
                .key("up-next-empty"),
            );
        } else {
            cards = append_rows(
                cards,
                s,
                &saved,
                split..count,
                columns,
                split.div_ceil(columns),
                "up-next",
            );
        }
        cards = cards.child(super::library_view::up_next_pages(s).key("up-next-pages"));
    } else {
        cards = append_rows(cards, s, &saved, 0..count, columns, 0, prefix);
        if s.navigation.page == Page::Discover
            && s.catalog_paging.pending.is_some_and(|offset| offset > 0)
        {
            cards = cards.child(label("Loading more titles…").key("catalog-loading-more"));
        }
    }
    scroll(viewport_key, cards).on_layout(|s: &mut State, rect| {
        s.poster_viewport = Some(rect);
        s.refresh_artwork();
        s.request_library_posters();
        s.request_movie_states();
        s.request_series_states();
        s.request_more_catalog();
    })
}

fn append_rows(
    mut cards: El,
    s: &State,
    saved: &[&Media],
    range: std::ops::Range<usize>,
    columns: usize,
    row_offset: usize,
    prefix: &'static str,
) -> El {
    let row_count = range.len().div_ceil(columns);
    let route_key = s.navigation.route_key;
    let stride = POSTER_CARD_HEIGHT + POSTER_GAP;
    #[expect(
        clippy::cast_precision_loss,
        reason = "Paged row counts are small layout coordinates"
    )]
    let height = row_count as f32 * stride;
    let source_generation = if matches!(s.navigation.page, Page::Library | Page::Watching) {
        s.library.grid_generation
    } else {
        s.catalog_id
    };
    let mut section = col()
        // Reset sources re-report geometry even if their bounds have not moved.
        .key(format!("{prefix}-cards-{source_generation}"))
        .w_full()
        .h(height)
        .min_h(height)
        .on_layout(move |s: &mut State, rect| {
            if s.poster_route != route_key || s.poster_columns != columns {
                s.poster_rects.clear();
            }
            s.poster_route = route_key;
            s.poster_columns = columns;
            for index in 0..row_count {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "Paged row counts are small layout coordinates"
                )]
                let y = rect[1] + index as f32 * stride;
                s.poster_rects
                    .insert(row_offset + index, [rect[0], y, rect[2], stride]);
            }
            // A section reports one coherent geometry snapshot, not one update per row.
            s.refresh_artwork();
            s.request_library_posters();
            s.request_movie_states();
            s.request_series_states();
            s.request_more_catalog();
        });
    let anchor_row = s.navigation.anchor.as_deref().and_then(|anchor| {
        anchor
            .strip_prefix(&format!("{prefix}-open-"))
            .or_else(|| anchor.strip_prefix(&format!("{prefix}-")))?
            .parse::<usize>()
            .ok()
            .map(|index| index / columns)
    });
    let mut skipped = 0;
    for row_index in 0..row_count {
        let layout_index = row_offset + row_index;
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
        if !nearby && anchor_row != Some(row_index) {
            skipped += 1;
            continue;
        }
        if skipped > 0 {
            section = section.child(spacer(prefix, row_index - skipped, skipped));
            skipped = 0;
        }
        let mut card_row = col()
            .key(format!("{prefix}-row-{row_index}"))
            .w_full()
            .h(POSTER_CARD_HEIGHT + POSTER_GAP)
            .pb(POSTER_GAP)
            .gap(POSTER_GAP)
            .equal_columns(columns);
        for index in row_index * columns..((row_index + 1) * columns).min(range.len()) {
            let item = entry(s, saved, range.start + index);
            card_row = card_row.child(card(s, &item, index, prefix));
        }
        section = section.child(card_row);
    }
    if skipped > 0 {
        section = section.child(spacer(prefix, row_count - skipped, skipped));
    }
    cards = cards.child(section);
    cards
}

fn spacer(prefix: &str, start: usize, count: usize) -> El {
    #[expect(
        clippy::cast_precision_loss,
        reason = "Paged row counts are small layout coordinates"
    )]
    let height = count as f32 * (POSTER_CARD_HEIGHT + POSTER_GAP);
    col()
        .key(format!("{prefix}-spacer-{start}"))
        .w_full()
        .h(height)
        .min_h(height)
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
                Overlay::Item { watched, progress } => {
                    poster_status_overlay(surface, &key, 246.0, watched, progress)
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
                .child(
                    label(entry.subtitle.clone())
                        .key(format!("{prefix}-info-{index}"))
                        .w_full()
                        .truncate(),
                ),
        )
}
