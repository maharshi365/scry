//! One virtualized poster grid for catalogs, search, Library, and Currently watching.
use super::{
    Page, State,
    artwork::{poster, poster_columns, poster_overlay, poster_status_overlay},
    design::tokens::{
        self,
        poster::{CARD_HEIGHT as POSTER_CARD_HEIGHT, GAP as POSTER_GAP},
        theme, type_,
    },
    primitives::{surface::scroll, text::label},
};
use rune::{Cx, El, GrowthSpec, Tx, col, text};
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

fn saved_media(s: &State, index: usize) -> Option<&Media> {
    match s.navigation.page {
        Page::Library => s.library.entries.as_ref()?.get(index),
        Page::Watching => s
            .library
            .watching
            .get(index)
            .map(|(media, _)| media)
            .or_else(|| {
                s.library
                    .up_next
                    .get(index.checked_sub(s.library.watching.len())?)
                    .map(|(media, _)| media)
            }),
        _ => None,
    }
}

fn entry(s: &State, index: usize) -> Entry {
    if let Some(media) = saved_media(s, index) {
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
    }
    urls
}

pub(super) fn visible(s: &State, index: usize) -> bool {
    s.poster_route == s.navigation.route_key
        && s.poster_visible
            .contains(&row_index(s, index, s.poster_columns))
}

fn row_index(s: &State, index: usize, columns: usize) -> usize {
    if s.navigation.page == Page::Watching && index >= s.library.watching.len() {
        s.library.watching.len().div_ceil(columns)
            + 1
            + (index - s.library.watching.len()) / columns
    } else {
        index / columns
    }
}

pub(super) fn view(s: &State, cx: &Cx<State>) -> El {
    let prefix = prefix(s);
    let viewport_key = viewport_key(s);
    let columns = cx
        .layout_size(&viewport_key)
        .map_or(3, |size| poster_columns(size[0]));
    let rows = Rows::new(s, columns);
    let route_key = s.navigation.route_key;
    let source_generation = if matches!(s.navigation.page, Page::Library | Page::Watching) {
        s.library.grid_generation
    } else {
        s.catalog_id
    };
    let cards = col()
        .key(format!("{prefix}-cards-{source_generation}-{columns}"))
        .w_full()
        .growth(GrowthSpec::default())
        .virtual_rows(
            rows.len(),
            POSTER_CARD_HEIGHT + POSTER_GAP,
            move |_: &State, index| rows.key(index).into(),
            move |s: &State, index| rows.build(s, index),
        )
        .virtual_rows_version((rows.main, rows.next, rows.watching, rows.loading))
        .on_visible_rows(move |s: &mut State, range| {
            let pending = s.catalog_paging.pending;
            s.poster_route = route_key;
            s.poster_columns = columns;
            s.poster_visible = range;
            refresh_visible(s);
            pending != s.catalog_paging.pending
        });
    scroll(viewport_key, cards).on_layout(|s: &mut State, rect| {
        s.poster_viewport = Some(rect);
        refresh_visible(s);
    })
}

fn viewport_key(s: &State) -> String {
    match s.navigation.page {
        Page::Library => super::library_view::grid_key(s),
        Page::Watching => "watching-scroll".into(),
        _ => format!("titles-scroll-{}", s.navigation.route_key),
    }
}

pub(super) fn reveal(s: &State, anchor: String, tx: &mut Tx) {
    let columns = s.poster_viewport.map_or(3, |rect| poster_columns(rect[2]));
    let row = [
        (Page::Discover, "title-", "title"),
        (Page::Library, "library-open-", "library"),
        (Page::Watching, "up-next-open-", "up-next"),
        (Page::Watching, "watching-open-", "watching"),
    ]
    .into_iter()
    .filter(|(page, _, _)| *page == s.navigation.page)
    .find_map(|(_, prefix, row)| {
        anchor
            .strip_prefix(prefix)?
            .parse::<usize>()
            .ok()
            .map(|index| format!("{row}-row-{}", index / columns))
    });
    if let Some(row) = row {
        tx.scroll_into_view_in(viewport_key(s), row);
    } else {
        tx.scroll_into_view(anchor);
    }
}

fn refresh_visible(s: &mut State) {
    s.refresh_artwork();
    s.request_library_posters();
    s.request_movie_states();
    s.request_series_states();
    s.request_more_catalog();
}

#[derive(Clone, Copy)]
struct Rows {
    columns: usize,
    main: usize,
    next: usize,
    watching: bool,
    loading: bool,
    prefix: &'static str,
}

impl Rows {
    fn new(s: &State, columns: usize) -> Self {
        Self {
            columns,
            main: match s.navigation.page {
                Page::Watching => s.library.watching.len(),
                Page::Library => s.library.entries.as_ref().map_or(0, Vec::len),
                _ => s.metas.len(),
            },
            next: if s.navigation.page == Page::Watching {
                s.library.up_next.len()
            } else {
                0
            },
            watching: s.navigation.page == Page::Watching,
            loading: s.navigation.page == Page::Discover
                && s.catalog_paging.pending.is_some_and(|offset| offset > 0),
            prefix: prefix(s),
        }
    }

    fn len(self) -> usize {
        if self.watching && self.main == 0 && self.next == 0 {
            1
        } else {
            self.main.div_ceil(self.columns)
                + self.next.div_ceil(self.columns)
                + if self.next > 0 { 2 } else { 0 }
                + usize::from(self.loading)
        }
    }

    fn key(self, index: usize) -> String {
        let main_rows = self.main.div_ceil(self.columns);
        if self.watching && self.main == 0 && self.next == 0 {
            "watching-empty".into()
        } else if index < main_rows {
            format!("{}-row-{index}", self.prefix)
        } else if self.next > 0 {
            if index == main_rows {
                "up-next-heading".into()
            } else if index == self.len() - 1 {
                "up-next-pages".into()
            } else {
                format!("up-next-row-{}", index - main_rows - 1)
            }
        } else {
            "catalog-loading-more".into()
        }
    }

    fn build(self, s: &State, index: usize) -> El {
        let main_rows = self.main.div_ceil(self.columns);
        if self.watching && self.main == 0 && self.next == 0 {
            return super::library_view::watching_empty();
        }
        let (start, count, row_index, prefix) = if index < main_rows {
            (0, self.main, index, self.prefix)
        } else if self.next > 0 {
            if index == main_rows {
                return super::library_view::up_next_header(s).pb(POSTER_GAP);
            }
            if index == self.len() - 1 {
                return super::library_view::up_next_pages(s);
            }
            (self.main, self.next, index - main_rows - 1, "up-next")
        } else {
            return label("Loading more titles…");
        };
        let mut card_row = col()
            .w_full()
            .h(POSTER_CARD_HEIGHT + POSTER_GAP)
            .pb(POSTER_GAP)
            .gap(POSTER_GAP)
            .equal_columns(self.columns);
        for index in row_index * self.columns..((row_index + 1) * self.columns).min(count) {
            let item = entry(s, start + index);
            card_row = card_row.child(card(s, &item, index, prefix));
        }
        card_row
    }
}

fn prefix(s: &State) -> &'static str {
    match s.navigation.page {
        Page::Library => "library",
        Page::Watching => "watching",
        _ => "title",
    }
}

fn card(s: &State, entry: &Entry, index: usize, prefix: &str) -> El {
    let selected = s.navigation.page == Page::Detail
        && s.detail_preview
        && s.detail
            .as_ref()
            .zip(s.metas.get(index))
            .is_some_and(|(detail, meta)| detail.id == meta.id && detail.kind == meta.kind);
    let key = if entry.media.is_some() {
        format!("{prefix}-open-{index}")
    } else {
        format!("{prefix}-{index}")
    };
    let item = entry.media.clone();
    let result = entry
        .media
        .is_none()
        .then(|| s.result_media(index))
        .flatten();
    let menu = entry.media.clone().or_else(|| result.clone());
    super::primitives::interaction::action(
        super::primitives::surface::Variant::Poster
            .apply_selected(col(), selected)
            .key(key.clone())
            .label(format!("Open {}", entry.title)),
        selected,
    )
    .w_full()
    .min_w(0.0)
    .gap(6.0)
    .padding(4.0)
    .on_press(move |s: &mut State| {
        s.navigation.anchor = Some(key.clone());
        if let Some(media) = &item {
            s.open_library_media(media);
        } else if let Some(result) = &result
            && let Some(index) = (0..s.metas.len()).find(|&index| {
                s.result_media(index)
                    .is_some_and(|media| media.key == result.key)
            })
        {
            s.open_result(index);
        }
    })
    .on_secondary_press(move |s: &mut State, p, _| {
        s.open_title_menu(p, menu.clone());
    })
    .child({
        let surface = poster(s, &entry.title, entry.poster.as_deref(), index);
        let key = format!("{prefix}-poster-{index}");
        match entry.overlay {
            Overlay::Summary => poster_overlay(
                s,
                surface,
                entry.identity.as_ref(),
                &key,
                tokens::poster::HEIGHT,
            ),
            Overlay::Item { watched, progress } => {
                poster_status_overlay(surface, &key, tokens::poster::HEIGHT, watched, progress)
            }
        }
    })
    .child(
        col()
            .gap(3.0)
            .child(
                text(entry.title.clone())
                    .w_full()
                    .truncate()
                    .font(type_::POSTER_TITLE)
                    .color(theme().text),
            )
            .child(
                label(entry.subtitle.clone())
                    .font(type_::CAPTION)
                    .key(format!("{prefix}-info-{index}"))
                    .w_full()
                    .truncate(),
            ),
    )
}
