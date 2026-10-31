//! Personal folders, metadata review, and trusted native-file sources.
mod ordering;
use super::{
    Page, State,
    controls::input,
    design::tokens,
    primitives::{
        button::{Button, Variant},
        feedback::{self, Tone},
        pattern,
        text::label,
    },
};
use crate::{
    local_media::{Episode, Index, Item, Match},
    services::{Source, my_media::Command},
};
pub(super) use ordering::scene as scene_ordering;
use rune::{El, TextEdit, col, row};
use scry_addons::Meta;
use std::path::PathBuf;

#[derive(Default)]
pub(super) struct MediaUi {
    pub index: Index,
    pub busy: bool,
    pub error: Option<String>,
    pub status: String,
    root: TextEdit,
    filter: TextEdit,
    review_only: bool,
    kind: MediaFilter,
    selected: Option<String>,
    query: TextEdit,
    pub search_generation: u64,
    pub search: MatchSearch,
    pub candidates: Vec<Meta>,
    episode_file: Option<PathBuf>,
    mapping: TextEdit,
    rects: std::collections::BTreeMap<String, [f32; 4]>,
    viewport: Option<[f32; 4]>,
    orders: ordering::Review,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum MediaFilter {
    #[default]
    All,
    Movies,
    Shows,
}

#[derive(Default, PartialEq, Eq)]
pub(super) enum MatchSearch {
    #[default]
    Idle,
    Loading,
    Complete,
}

impl State {
    pub(super) fn media_command(&mut self, command: Command) {
        if self.my_media.busy && !matches!(command, Command::StopMatching) {
            return;
        }
        let result = self
            .runtime
            .services
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("My Media is unavailable"))
            .and_then(|s| s.media_command(command));
        match result {
            Ok(()) => {
                self.my_media.busy = true;
                self.my_media.error = None;
                self.my_media.status = "Scanning and saving local media…".into();
            }
            Err(e) => self.my_media.error = Some(e.to_string()),
        }
    }

    fn add_media_root(&mut self) {
        let root = self.my_media.root.text.trim();
        if !root.is_empty() {
            self.media_command(Command::Add(PathBuf::from(root)));
        }
    }

    fn review_media(&mut self, id: &str) {
        let Some(item) = self.my_media.index.items.iter().find(|i| i.id == id) else {
            return;
        };
        self.my_media.query = TextEdit::new(&item.title);
        self.my_media.selected = Some(id.into());
        self.my_media.episode_file = None;
        self.my_media.search_generation += 1;
        self.my_media.search = MatchSearch::Idle;
        self.my_media.candidates.clear();
        self.my_media.orders.reset();
        self.my_media.error = None;
    }

    fn search_media_match(&mut self) {
        self.my_media.search_generation += 1;
        self.my_media.candidates.clear();
        self.my_media.search = MatchSearch::Idle;
        let command = Command::Search(
            self.my_media.search_generation,
            self.my_media.query.text.trim().into(),
        );
        let result = self
            .runtime
            .services
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Metadata search is unavailable"))
            .and_then(|s| s.media_command(command));
        match result {
            Ok(()) => {
                self.my_media.search = MatchSearch::Loading;
                self.my_media.error = None;
            }
            Err(e) => self.my_media.error = Some(e.to_string()),
        }
    }

    fn choose_media_match(&mut self, meta: &Meta) {
        if let Some(id) = self.my_media.selected.clone() {
            self.media_command(Command::Match(id, Some(Match::from_meta(meta, true))));
        }
    }

    fn save_episode_mapping(&mut self) {
        let parsed = self
            .my_media
            .mapping
            .text
            .trim()
            .split_once(':')
            .and_then(|(s, e)| {
                Some(Episode {
                    season: s.trim().parse().ok()?,
                    numbers: vec![e.trim().parse().ok()?],
                })
            });
        if let (Some(id), Some(path), Some(episode)) = (
            self.my_media.selected.clone(),
            self.my_media.episode_file.clone(),
            parsed,
        ) {
            self.media_command(Command::Episode(id, path, episode));
            self.my_media.episode_file = None;
        } else {
            self.my_media.error =
                Some("Enter season:episode, for example 1:2 or 0:1 for a special.".into());
        }
    }

    fn open_local_item(&mut self, id: &str) {
        let Some(item) = self.my_media.index.items.iter().find(|i| i.id == id) else {
            return;
        };
        match item.meta() {
            Ok(meta) => {
                self.detail.namespace = if let Some(matched) = &item.matched {
                    if matched.id.starts_with("kitsu:") {
                        "community.anime.kitsu"
                    } else {
                        "com.linvo.cinemeta"
                    }
                } else {
                    "org.scry.local"
                }
                .into();
                // The local inventory is authoritative here, including unmatched episodes.
                self.navigate(Page::Detail);
                self.detail.meta = Some(meta.clone());
                self.detail.preview = false;
                self.detail.episodes = super::episodes::EpisodesState::default();
                self.detail.sources.reset();
                self.detail.request = super::request::RequestState::default();
                self.library.snapshot = None;
                self.register_library_detail();
                if meta.kind == "movie" {
                    self.find_streams(meta.kind.clone(), meta.id.clone());
                }
            }
            Err(_) => self.my_media.error = Some("Cannot open this local title".into()),
        }
    }

    pub(super) fn local_sources(&self) -> Vec<PathBuf> {
        let Some(meta) = &self.detail.meta else {
            return Vec::new();
        };
        let id = self.detail.episodes.selected.as_deref().unwrap_or(&meta.id);
        self.my_media
            .index
            .items
            .iter()
            .flat_map(|item| item.sources(&meta.kind, id))
            .map(|f| f.path.clone())
            .collect()
    }

    pub(super) fn play_local_media(&mut self, path: &std::path::Path) {
        // Only paths from the private index can enter native playback here.
        if !self.local_sources().iter().any(|p| p == path) {
            return;
        }
        let title = self
            .detail
            .meta
            .as_ref()
            .map_or_else(|| "Local video".into(), |m| m.name.clone());
        self.play_media_source(Source::Local(path.to_owned()), title);
    }
}

pub(super) fn view(s: &State) -> El {
    let media = &s.my_media;
    let actions = row()
        .child(
            Button::new("media-rescan", "Rescan")
                .enabled(!media.busy)
                .on_press(|s: &mut State| s.media_command(Command::Scan)),
        )
        .child(
            Button::new("media-stop-matching", "Stop matching")
                .enabled(media.busy)
                .on_press(|s: &mut State| s.media_command(Command::StopMatching)),
        );
    let mut content = col().w_full().gap(tokens::layout::PAGE_GAP)
        .child(label("Folders are watched on this computer. Parsed titles go to Cinemeta and Anime Kitsu; DVD orders come from TVmaze. Files are never moved or renamed."))
        .child(row().w_full().gap(tokens::layout::ACTION_GAP)
            .child(input("media-root", &media.root, "Root folder path…", |s| &mut s.my_media.root, State::add_media_root).grow(1.0))
            .child(Button::new("media-add", "Add folder").variant(Variant::Primary).enabled(!media.busy).on_press(State::add_media_root)));
    for (n, root) in media.index.roots.iter().enumerate() {
        let path = root.clone();
        content = content.child(
            row()
                .w_full()
                .gap(tokens::layout::ACTION_GAP)
                .child(label(root.to_string_lossy()).grow(1.0))
                .child(
                    Button::new(format!("media-remove-{n}"), "Remove folder")
                        .enabled(!media.busy)
                        .on_press(move |s: &mut State| {
                            s.media_command(Command::Remove(path.clone()));
                        }),
                ),
        );
    }
    if !media.status.is_empty() {
        content = content.child(feedback::status(
            "media-status",
            &media.status,
            Tone::Neutral,
        ));
    }
    if let Some(error) = &media.error {
        content = content.child(feedback::status("media-error", error, Tone::Danger));
    }
    if media.selected.is_some() {
        content = content.child(review(s));
    }
    content = content.child(filters(s));
    content = content.child(title_list(s));
    if media.index.items.is_empty() {
        content = content.child(feedback::status(
            "media-empty",
            "Add a folder to find your movies and TV episodes.",
            Tone::Neutral,
        ));
    }
    if media.index.items.len() > 200 {
        content = content.child(label(
            "Showing up to 200 titles. Use the filter to narrow the list.",
        ));
    }
    pattern::page(
        "my-media",
        "My Media",
        "Your movies and TV shows, from your own folders.",
        actions,
        content,
    )
    .on_layout(|s: &mut State, rect| s.my_media.viewport = Some(rect))
}

fn filters(s: &State) -> El {
    let media = &s.my_media;
    let filters = col().w_full().gap(tokens::layout::ACTION_GAP).child(
        row()
            .w_full()
            .gap(tokens::layout::ACTION_GAP)
            .child(
                input(
                    "media-filter",
                    &media.filter,
                    "Filter local titles…",
                    |s| &mut s.my_media.filter,
                    |_| {},
                )
                .grow(1.0),
            )
            .child(
                Button::new(
                    "media-review-only",
                    if media.review_only {
                        "Show all"
                    } else {
                        "Needs review"
                    },
                )
                .on_press(|s: &mut State| s.my_media.review_only = !s.my_media.review_only),
            ),
    );
    let mut kinds = row().gap(tokens::layout::ACTION_GAP);
    for (key, title, kind) in [
        ("media-all", "All", MediaFilter::All),
        ("media-movies", "Movies", MediaFilter::Movies),
        ("media-shows", "TV Shows", MediaFilter::Shows),
    ] {
        kinds = kinds.child(
            Button::new(key, title)
                .selected(media.kind == kind)
                .on_press(move |s: &mut State| s.my_media.kind = kind),
        );
    }
    filters.child(kinds)
}

fn visible_items(media: &MediaUi) -> Vec<&Item> {
    let filter = media.filter.text.to_lowercase();
    media
        .index
        .items
        .iter()
        .filter(|i| {
            let kind = i
                .matched
                .as_ref()
                .map_or(i.kind.as_str(), |m| m.kind.as_str());
            (media.kind == MediaFilter::All
                || media.kind == MediaFilter::Movies && kind == "movie"
                || media.kind == MediaFilter::Shows && kind == "series")
                && (!media.review_only || i.matched.is_none() && !i.keep_unmatched)
                && i.matched
                    .as_ref()
                    .map_or(&i.title, |m| &m.name)
                    .to_lowercase()
                    .contains(&filter)
        })
        .take(200)
        .collect()
}

pub(super) fn artwork_urls(s: &State) -> Vec<String> {
    visible_items(&s.my_media)
        .into_iter()
        .filter(|item| {
            s.my_media.rects.get(&item.id).is_none_or(|rect| {
                s.my_media
                    .viewport
                    .is_none_or(|viewport| super::artwork::intersects(*rect, viewport))
            })
        })
        .filter_map(|i| i.matched.as_ref()?.poster.clone())
        .take(32)
        .collect()
}

fn title_list(s: &State) -> El {
    let media = &s.my_media;
    let mut content = col().w_full().gap(tokens::layout::PAGE_GAP);
    for (index, item) in visible_items(media).into_iter().enumerate() {
        let id = item.id.clone();
        let review_id = id.clone();
        let layout_id = id.clone();
        let name = item.matched.as_ref().map_or(&item.title, |m| &m.name);
        let state = if !item.files.iter().any(|f| f.available) {
            "Offline"
        } else if item.keep_unmatched {
            "Kept unmatched"
        } else if let Some(m) = &item.matched {
            if m.manual {
                "Manually matched"
            } else {
                "Automatically matched"
            }
        } else {
            "Needs review"
        };
        content = content.child(
            row()
                .w_full()
                .min_w(0.0)
                .gap(tokens::layout::ACTION_GAP)
                .on_layout(move |s: &mut State, rect| {
                    s.my_media.rects.insert(layout_id.clone(), rect);
                })
                .child(thumbnail(s, item, index))
                .child(
                    col()
                        .grow(1.0)
                        .min_w(0.0)
                        .child(label(name))
                        .child(label(format!(
                            "{} · {} files · {state}",
                            item.matched.as_ref().map_or(&item.kind, |m| &m.kind),
                            item.files.len()
                        ))),
                )
                .child(
                    Button::new(format!("media-open-{id}"), "Open")
                        .on_press(move |s: &mut State| s.open_local_item(&id)),
                )
                .child(
                    Button::new(format!("media-fix-{review_id}"), "Fix match")
                        .on_press(move |s: &mut State| s.review_media(&review_id)),
                ),
        );
    }
    content
}

fn thumbnail(s: &State, item: &Item, index: usize) -> El {
    let frame = super::primitives::image::frame(
        &item.title,
        index,
        super::primitives::image::Variant::Thumbnail,
    );
    if let Some(image) = item
        .matched
        .as_ref()
        .and_then(|m| m.poster.as_deref())
        .and_then(|url| s.artwork.posters.as_ref()?.get(url))
    {
        frame.pixels_fit(&image, rune::SurfaceFit::Fill)
    } else {
        frame
            .center()
            .child(label(item.title.chars().next().unwrap_or('S').to_string()))
    }
}

fn review(s: &State) -> El {
    let media = &s.my_media;
    let mut body = col().w_full().gap(tokens::layout::ACTION_GAP).child(
        row()
            .w_full()
            .gap(tokens::layout::ACTION_GAP)
            .child(
                input(
                    "media-match-query",
                    &media.query,
                    "Search for the correct title…",
                    |s| &mut s.my_media.query,
                    State::search_media_match,
                )
                .grow(1.0),
            )
            .child(
                Button::new("media-match-search", "Search metadata")
                    .enabled(media.search != MatchSearch::Loading && !media.busy)
                    .on_press(State::search_media_match),
            )
            .child(
                Button::new("media-review-close", "Close").on_press(|s: &mut State| {
                    s.my_media.selected = None;
                    s.my_media.search_generation += 1;
                    s.my_media.orders.reset();
                }),
            ),
    );
    if media.search == MatchSearch::Loading {
        body = body.child(feedback::status(
            "media-searching",
            "Searching metadata…",
            Tone::Neutral,
        ));
    }
    if media.search == MatchSearch::Complete && media.candidates.is_empty() {
        body = body.child(label("No matches. Try another title."));
    }
    for (n, meta) in media.candidates.iter().enumerate() {
        let selected = meta.clone();
        body = body.child(
            Button::new(
                format!("media-candidate-{n}"),
                format!(
                    "{} · {} · {} · {}",
                    meta.name,
                    meta.year,
                    meta.kind,
                    if meta.id.starts_with("kitsu:") {
                        "Anime Kitsu"
                    } else {
                        "Cinemeta"
                    }
                ),
            )
            .enabled(!media.busy)
            .on_press(move |s: &mut State| s.choose_media_match(&selected)),
        );
    }
    body = body.child(
        Button::new("media-keep-unmatched", "Keep unmatched")
            .enabled(!media.busy)
            .on_press(|s: &mut State| {
                if let Some(id) = s.my_media.selected.clone() {
                    s.media_command(Command::Match(id, None));
                }
            }),
    );
    body = body.child(ordering::view(s));
    body = body.child(episode_review(s));
    pattern::section(
        "media-review",
        "Fix match",
        "Choose a title for this group, or correct individual episode numbers.",
        body,
    )
}

fn episode_review(s: &State) -> El {
    let media = &s.my_media;
    let mut body = col().w_full().gap(tokens::layout::ACTION_GAP);
    if let Some(item) = media
        .index
        .items
        .iter()
        .find(|i| Some(&i.id) == media.selected.as_ref())
    {
        for (n, file) in item.files.iter().take(100).enumerate() {
            let path = file.path.clone();
            body = body.child(
                row()
                    .w_full()
                    .gap(tokens::layout::ACTION_GAP)
                    .child(label(file.path.to_string_lossy()).grow(1.0))
                    .child(
                        Button::new(format!("media-episode-{n}"), "Set episode")
                            .enabled(!media.busy)
                            .on_press(move |s: &mut State| {
                                s.my_media.episode_file = Some(path.clone());
                                s.my_media.mapping = TextEdit::new("");
                            }),
                    ),
            );
        }
    }
    if media.episode_file.is_some() {
        body = body.child(
            row()
                .w_full()
                .gap(tokens::layout::ACTION_GAP)
                .child(input(
                    "media-episode-map",
                    &media.mapping,
                    "season:episode (1:2)",
                    |s| &mut s.my_media.mapping,
                    State::save_episode_mapping,
                ))
                .child(
                    Button::new("media-episode-save", "Save episode")
                        .enabled(!media.busy)
                        .on_press(State::save_episode_mapping),
                ),
        );
    }
    body
}

pub(super) fn scene(s: &mut State) {
    use crate::local_media::VideoFile;
    s.navigation.page = Page::MyMedia;
    s.addons.busy = false;
    s.my_media = MediaUi::default();
    s.my_media.index.roots = vec![PathBuf::from("Media")];
    for (id, name, kind, year, episode) in [
        ("local:arrival", "Arrival", "movie", Some(2016), None),
        (
            "local:example",
            "Example",
            "series",
            None,
            Some(Episode {
                season: 1,
                numbers: vec![1],
            }),
        ),
        ("local:unknown", "Unidentified video", "movie", None, None),
    ] {
        s.my_media.index.items.push(Item {
            id: id.into(),
            title: name.into(),
            year,
            kind: kind.into(),
            files: vec![VideoFile {
                path: PathBuf::from(format!("Media/{name}.mkv")),
                episode,
                corrected: false,
                available: true,
            }],
            matched: (name == "Arrival").then(|| Match {
                id: "tt2543164".into(),
                kind: "movie".into(),
                name: name.into(),
                year: "2016".into(),
                poster: None,
                manual: false,
            }),
            keep_unmatched: false,
            ordering: None,
        });
    }
    s.my_media.status = "3 titles indexed. 2 need review.".into();
}
