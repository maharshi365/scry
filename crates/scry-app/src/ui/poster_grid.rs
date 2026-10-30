//! One virtualized poster grid for catalogs, search, Library, and Currently watching.
use super::{
    Page, State,
    artwork::{poster, poster_columns, poster_overlay, poster_status_overlay},
    design::tokens::{
        self,
        poster::{CARD_HEIGHT as POSTER_CARD_HEIGHT, GAP as POSTER_GAP},
    },
    primitives::{image, motion::Zoom, poster as gallery, surface::scroll, text::label},
};
use rune::{Cx, El, GrowthSpec, Tx, col};
use scry_library::{Media, MediaKey, MediaKind};

pub(super) struct PosterGridState {
    pub visible: std::ops::Range<usize>,
    pub columns: usize,
    pub viewport: Option<[f32; 4]>,
    pub route: u64,
    hovered: Option<(u64, String)>,
}

impl Default for PosterGridState {
    fn default() -> Self {
        Self {
            visible: 0..0,
            columns: 3,
            viewport: None,
            route: 0,
            hovered: None,
        }
    }
}

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
    let meta = &s.discover.results[index];
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
        } else if s.discover.displayed_query.is_empty() {
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
        (0..s.discover.results.len())
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
        Page::Discover => (0..s.discover.results.len())
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
    visible_urls(s, s.discover.results.iter().map(|m| m.poster.as_ref()))
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
    s.posters.route == s.navigation.route_key
        && s.posters
            .visible
            .contains(&row_index(s, index, s.posters.columns))
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
    let mut rows = Rows::new(s, columns);
    rows.reduced_motion = cx.is_reduced_motion();
    let route_key = s.navigation.route_key;
    let source_generation = if matches!(s.navigation.page, Page::Library | Page::Watching) {
        s.library.grid_generation
    } else {
        s.discover.generation
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
        .virtual_rows_version((
            rows.main,
            rows.next,
            rows.loading,
            rows.reduced_motion,
            &s.posters.hovered,
        ))
        .on_visible_rows(move |s: &mut State, range| {
            let pending = s.discover.paging.pending;
            s.posters.route = route_key;
            s.posters.columns = columns;
            s.posters.visible = range;
            refresh_visible(s);
            pending != s.discover.paging.pending
        });
    scroll(viewport_key, cards).on_layout(|s: &mut State, rect| {
        s.posters.viewport = Some(rect);
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
    let columns = s.posters.viewport.map_or(3, |rect| poster_columns(rect[2]));
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
    loading: bool,
    prefix: &'static str,
    reduced_motion: bool,
}

impl Rows {
    fn new(s: &State, columns: usize) -> Self {
        Self {
            columns,
            main: match s.navigation.page {
                Page::Watching => s.library.watching.len(),
                Page::Library => s.library.entries.as_ref().map_or(0, Vec::len),
                _ => s.discover.results.len(),
            },
            next: if s.navigation.page == Page::Watching {
                s.library.up_next.len()
            } else {
                0
            },
            loading: s.navigation.page == Page::Discover
                && s.discover.paging.pending.is_some_and(|offset| offset > 0),
            prefix: prefix(s),
            reduced_motion: false,
        }
    }

    fn len(self) -> usize {
        self.main.div_ceil(self.columns)
            + self.next.div_ceil(self.columns)
            + if self.next > 0 { 2 } else { 0 }
            + usize::from(self.loading)
    }

    fn key(self, index: usize) -> String {
        let main_rows = self.main.div_ceil(self.columns);
        if index < main_rows {
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
            card_row = card_row.child(card(s, &item, index, prefix, self.reduced_motion));
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

fn card(s: &State, entry: &Entry, index: usize, prefix: &str, reduced_motion: bool) -> El {
    let selected = s.navigation.page == Page::Detail
        && s.detail.preview
        && s.detail
            .meta
            .as_ref()
            .zip(s.discover.results.get(index))
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
    let poster_key = format!("{prefix}-poster-{index}");
    let route = s.navigation.route_key;
    let hovered = s
        .posters
        .hovered
        .as_ref()
        .is_some_and(|(owner, hovered)| *owner == route && hovered == &poster_key);
    let hover_key = poster_key.clone();
    let surface = image::gallery(
        &poster_key,
        poster(s, &entry.title, entry.poster.as_deref(), index),
        if hovered && !reduced_motion {
            Zoom::Hovered
        } else {
            Zoom::Rest
        },
    )
    .on_hover_change(move |s: &mut State, hovered, _| {
        if hovered {
            s.posters.hovered = Some((route, hover_key.clone()));
        } else if s
            .posters
            .hovered
            .as_ref()
            .is_some_and(|(owner, key)| *owner == route && key == &hover_key)
        {
            s.posters.hovered = None;
        }
    });
    let artwork = match entry.overlay {
        Overlay::Summary => poster_overlay(
            s,
            surface,
            entry.identity.as_ref(),
            &poster_key,
            tokens::poster::HEIGHT,
        ),
        Overlay::Item { watched, progress } => poster_status_overlay(
            surface,
            &poster_key,
            tokens::poster::HEIGHT,
            watched,
            progress,
        ),
    };
    gallery::card(
        &key,
        gallery::Content {
            title: &entry.title,
            subtitle: &entry.subtitle,
            subtitle_key: &format!("{prefix}-info-{index}"),
            artwork,
        },
        if selected {
            gallery::Selection::Selected
        } else {
            gallery::Selection::Normal
        },
    )
    .on_press(move |s: &mut State| {
        s.navigation.anchor = Some(key.clone());
        if let Some(media) = &item {
            s.open_library_media(media);
        } else if let Some(result) = &result
            && let Some(index) = (0..s.discover.results.len()).find(|&index| {
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
}
