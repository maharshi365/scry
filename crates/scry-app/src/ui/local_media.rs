//! Local folders, review workflow, and first-class local playback sources.
use super::{
    Page, State,
    app::on_input_key,
    primitives::{
        button::Button,
        input::input,
        surface::card,
        text::{heading, label},
    },
};
use crate::services::{
    Source,
    local_media::{Candidate, Request, Snapshot},
};
use rune::{El, TextEdit, col, row};
use scry_library::{MediaKind, local_media::LocalFile};

#[derive(Default)]
pub(super) struct LocalSources {
    pub files: Vec<LocalFile>,
    pub generation: u64,
    pub pending: bool,
    pub remote_done: bool,
}

pub(super) struct LocalUi {
    path: TextEdit,
    query: TextEdit,
    pub data: Snapshot,
    pub busy: bool,
    pub error: Option<String>,
    pub message: String,
    offset: i64,
    unmatched: bool,
    selected: Option<LocalFile>,
    candidates: Vec<Candidate>,
    chosen: Option<Candidate>,
    pending_confirmation: Option<i64>,
    kind: MediaKind,
    group: bool,
    episode_offset: usize,
    pub sources: LocalSources,
}

impl Default for LocalUi {
    fn default() -> Self {
        Self {
            path: TextEdit::new(""),
            query: TextEdit::new(""),
            data: Snapshot::default(),
            busy: false,
            error: None,
            message: String::new(),
            offset: 0,
            unmatched: false,
            selected: None,
            candidates: Vec::new(),
            chosen: None,
            pending_confirmation: None,
            kind: MediaKind::Movie,
            group: true,
            episode_offset: 0,
            sources: LocalSources::default(),
        }
    }
}

impl State {
    pub(super) fn request_local(&mut self, request: Request) {
        if self.local.busy {
            return;
        }
        self.local.error = None;
        if let Some(services) = &self.services {
            match services.local_request(request, self.local.offset, self.local.unmatched) {
                Ok(()) => {
                    self.local.busy = true;
                    self.local.message = "Working…".into();
                }
                Err(error) => self.local.error = Some(error.to_string()),
            }
        }
    }

    pub(super) fn apply_local_snapshot(&mut self, result: Result<Snapshot, String>) {
        self.local.busy = false;
        match result {
            Ok(data) => {
                if self.local.pending_confirmation.take().is_some() {
                    self.local.selected = None;
                    self.local.chosen = None;
                    self.local.candidates.clear();
                }
                if data.message.is_empty() {
                    self.local.message = format!(
                        "{} root folder(s) · {} files on this page",
                        data.roots.len(),
                        data.files.len()
                    );
                } else {
                    self.local.message.clone_from(&data.message);
                }
                self.local.data = data;
            }
            Err(error) => {
                self.local.pending_confirmation = None;
                self.local.error = Some(error);
                self.local.message = "Refresh the list to see any completed work.".into();
            }
        }
    }

    pub(super) fn apply_local_candidates(&mut self, id: i64, candidates: Vec<Candidate>) {
        if self.local.selected.as_ref().is_some_and(|f| f.id == id) {
            self.local.candidates = candidates;
            self.local.chosen = None;
        }
    }

    pub(super) fn apply_local_choice(&mut self, id: i64, candidate: Candidate) {
        if self.local.selected.as_ref().is_some_and(|f| f.id == id) {
            self.local.chosen = Some(candidate);
            self.local.episode_offset = 0;
        }
    }

    fn add_local_root(&mut self) {
        let path = self.local.path.text.trim().to_owned();
        if path.is_empty() {
            self.local.error = Some("Enter a root folder path".into());
            return;
        }
        self.request_local(Request::Add(path.into()));
    }

    fn search_local_title(&mut self) {
        let Some(file) = &self.local.selected else {
            return;
        };
        self.request_local(Request::Search(
            file.id,
            self.local.query.text.clone(),
            if self.local.kind == MediaKind::Series {
                "series"
            } else {
                "movie"
            }
            .into(),
        ));
    }

    fn review_local_file(&mut self, file: LocalFile) {
        let hints =
            serde_json::from_str::<crate::local_media::Hints>(&file.hints).unwrap_or_default();
        self.local.query = TextEdit::new(hints.title);
        self.local.kind = if hints.season.is_some() {
            MediaKind::Series
        } else {
            MediaKind::Movie
        };
        self.local.selected = Some(file);
        self.local.candidates.clear();
        self.local.chosen = None;
        self.local.error = None;
    }

    fn confirm_local_match(&mut self, episode: Option<String>) {
        let Some(file) = &self.local.selected else {
            return;
        };
        let Some(candidate) = &self.local.chosen else {
            return;
        };
        let id = file.id;
        self.request_local(Request::Confirm(
            file.id,
            candidate.addon.clone(),
            candidate.meta.clone(),
            episode,
            self.local.group,
        ));
        if self.local.busy {
            self.local.pending_confirmation = Some(id);
        }
    }

    pub(super) fn local_source_files(&self) -> &[LocalFile] {
        if self.local.sources.generation == self.stream_id {
            &self.local.sources.files
        } else {
            &[]
        }
    }

    pub(super) fn maybe_quick_play(&mut self) {
        if self.local.sources.pending
            || !self.local.sources.remote_done
            || self.library.quick_play != Some(self.detail_id)
        {
            return;
        }
        self.library.quick_play = None;
        let remote: Vec<_> = self
            .streams
            .iter()
            .filter(|(_, stream)| stream.validate_playback().is_ok())
            .collect();
        let local = self.local_source_files();
        if remote.len() + local.len() != 1 {
            return;
        }
        let title = self
            .detail
            .as_ref()
            .map_or_else(String::new, |meta| meta.name.clone());
        if let Some(file) = local.first() {
            let path = std::path::Path::new(&file.root_path).join(&file.relative_path);
            self.play_media_source(Source::Local(path), title);
        } else if let Some((_, stream)) = remote.first() {
            self.play((*stream).clone(), title);
        }
    }

    pub(super) fn open_local_file(&mut self, file: &LocalFile) {
        if let Some(metadata) = &file.metadata
            && let Ok(meta) = serde_json::from_str::<scry_addons::Meta>(metadata)
        {
            self.detail_namespace = file.namespace.clone().unwrap_or_default();
            self.navigate(Page::Detail);
            self.detail = Some(meta.clone());
            self.detail_preview = false;
            self.library.snapshot = None;
            self.selected_video_id.clone_from(&file.external_id);
            self.register_library_detail();
            self.find_streams(
                meta.kind.clone(),
                file.external_id.clone().unwrap_or_else(|| meta.id.clone()),
            );
        } else {
            let id = format!("local-file:{}", file.id);
            let meta = serde_json::from_value::<scry_addons::Meta>(serde_json::json!({
                "id":id,"type":"movie","name":file.relative_path
            }));
            if let Ok(meta) = meta {
                self.detail_namespace = "local".into();
                self.navigate(Page::Detail);
                self.detail = Some(meta);
                self.detail_preview = false;
                self.selected_video_id = Some(id.clone());
                self.library.snapshot = None;
                self.register_library_detail();
                self.find_streams("movie".into(), id);
            }
        }
    }
}

pub(super) fn settings(s: &State) -> El {
    let mut section = card().key("local-media-settings").w_full().padding(24.0).gap(12.0)
        .child(heading("Local media"))
        .child(label("Add separate root folders, then scan and auto-match movies and episodes. Videos are never renamed, moved, or deleted. Title/year and explicit ID queries go to enabled metadata add-ons; full local paths are not sent."))
        .child(row().w_full().gap(8.0).child(input("local-root-path", &s.local.path, "Root folder path", |s: &mut State| &mut s.local.path, on_input_key, State::add_local_root))
            .child(Button::new("local-root-add", "Add folder").enabled(!s.local.busy).on_press(State::add_local_root)))
        .child(row().gap(8.0).flex_wrap()
            .child(Button::new("local-refresh", "Refresh list").enabled(!s.local.busy).on_press(|s: &mut State| s.request_local(Request::Refresh)))
            .child(Button::new("local-scan", "Scan all folders").enabled(!s.local.busy && !s.local.data.roots.is_empty()).on_press(|s: &mut State| s.request_local(Request::Scan)))
            .child(Button::new("local-cancel", "Cancel").enabled(s.local.busy).on_press(|s: &mut State| { if let Some(services) = &s.services { services.cancel_local_scan(); } })));
    for root in &s.local.data.roots {
        let id = root.id;
        section = section.child(
            row()
                .w_full()
                .gap(8.0)
                .center_y()
                .child(
                    label(format!(
                        "{}{}",
                        root.path,
                        if root.online {
                            ""
                        } else {
                            " · Offline / incomplete scan"
                        }
                    ))
                    .w(0.0)
                    .grow(1.0),
                )
                .child(
                    Button::new(format!("local-remove-{id}"), "Remove from Scry")
                        .enabled(!s.local.busy)
                        .on_press(move |s: &mut State| {
                            s.local.offset = 0;
                            s.request_local(Request::Remove(id));
                        }),
                ),
        );
    }
    if !s.local.message.is_empty() {
        section = section.child(label(s.local.message.clone()));
    }
    if let Some(error) = &s.local.error {
        section = section.child(label(error.clone()));
    }
    section = section.child(
        row()
            .gap(8.0)
            .child(
                Button::new("local-all", "All files")
                    .selected(!s.local.unmatched)
                    .enabled(!s.local.busy)
                    .on_press(|s: &mut State| {
                        s.local.unmatched = false;
                        s.local.offset = 0;
                        s.request_local(Request::Refresh);
                    }),
            )
            .child(
                Button::new("local-unmatched", "Needs review")
                    .selected(s.local.unmatched)
                    .enabled(!s.local.busy)
                    .on_press(|s: &mut State| {
                        s.local.unmatched = true;
                        s.local.offset = 0;
                        s.request_local(Request::Refresh);
                    }),
            ),
    );
    if s.local.selected.is_some() {
        section = section.child(review(s));
    }
    for file in &s.local.data.files {
        section = section.child(file_card(s, file));
    }
    section.child(file_paging(s))
}

fn file_card(s: &State, file: &LocalFile) -> El {
    let open = file.clone();
    let review = file.clone();
    let id = file.id;
    let status = if !file.available {
        "Unavailable"
    } else if file.manual {
        "Confirmed"
    } else if file.external_id.is_some() {
        "Auto-matched: title/ID and episode evidence"
    } else {
        "Needs review"
    };
    card()
        .key(format!("local-file-{id}"))
        .w_full()
        .padding(12.0)
        .gap(6.0)
        .child(label(file.relative_path.clone()))
        .child(label(format!(
            "{status}{}",
            file.title
                .as_ref()
                .map_or_else(String::new, |t| format!(" · {t}"))
        )))
        .child(
            row()
                .gap(8.0)
                .flex_wrap()
                .child(
                    Button::new(
                        format!("local-play-{id}"),
                        if file.external_id.is_some() {
                            "Open sources"
                        } else {
                            "Play file"
                        },
                    )
                    .enabled(file.available)
                    .on_press(move |s: &mut State| s.open_local_file(&open)),
                )
                .child(
                    Button::new(format!("local-review-{id}"), "Match / correct")
                        .enabled(!s.local.busy)
                        .on_press(move |s: &mut State| s.review_local_file(review.clone())),
                )
                .child(
                    Button::new(format!("local-clear-{id}"), "Clear match")
                        .enabled(!s.local.busy && file.external_id.is_some())
                        .on_press(move |s: &mut State| s.request_local(Request::Clear(id))),
                ),
        )
}

fn file_paging(s: &State) -> El {
    row()
        .gap(8.0)
        .child(
            Button::new("local-prev", "Previous files")
                .enabled(!s.local.busy && s.local.offset > 0)
                .on_press(|s: &mut State| {
                    s.local.offset = (s.local.offset - 100).max(0);
                    s.request_local(Request::Refresh);
                }),
        )
        .child(
            Button::new("local-next", "Next files")
                .enabled(!s.local.busy && s.local.data.files.len() == 100)
                .on_press(|s: &mut State| {
                    s.local.offset += 100;
                    s.request_local(Request::Refresh);
                }),
        )
}

fn review(s: &State) -> El {
    let mut panel = col()
        .key("local-review")
        .w_full()
        .gap(8.0)
        .child(heading("Match file"))
        .child(label(
            s.local
                .selected
                .as_ref()
                .map_or_else(String::new, |f| f.relative_path.clone()),
        ))
        .child(
            row()
                .w_full()
                .gap(8.0)
                .child(input(
                    "local-match-query",
                    &s.local.query,
                    "Movie or show title",
                    |s: &mut State| &mut s.local.query,
                    on_input_key,
                    State::search_local_title,
                ))
                .child(
                    Button::new("local-match-search", "Search")
                        .enabled(!s.local.busy)
                        .on_press(State::search_local_title),
                ),
        )
        .child(
            row()
                .gap(8.0)
                .flex_wrap()
                .child(
                    Button::new(
                        "local-kind",
                        if s.local.kind == MediaKind::Series {
                            "TV series"
                        } else {
                            "Movie"
                        },
                    )
                    .enabled(!s.local.busy)
                    .on_press(|s: &mut State| {
                        s.local.kind = if s.local.kind == MediaKind::Series {
                            MediaKind::Movie
                        } else {
                            MediaKind::Series
                        };
                        s.local.candidates.clear();
                        s.local.chosen = None;
                    }),
                )
                .child(
                    Button::new("local-group", "Apply show to numbered siblings")
                        .selected(s.local.group)
                        .enabled(!s.local.busy)
                        .on_press(|s: &mut State| s.local.group = !s.local.group),
                )
                .child(
                    Button::new("local-review-close", "Close review")
                        .enabled(!s.local.busy)
                        .on_press(|s: &mut State| s.local.selected = None),
                ),
        );
    if let Some(candidate) = &s.local.chosen {
        return panel.child(episode_choices(s, candidate));
    }
    for (index, candidate) in s.local.candidates.iter().enumerate() {
        let candidate = candidate.clone();
        panel = panel.child(
            Button::new(
                format!("local-candidate-{index}"),
                format!(
                    "{} ({}) · {}",
                    candidate.meta.name, candidate.meta.year, candidate.addon.manifest.name
                ),
            )
            .enabled(!s.local.busy)
            .on_press(move |s: &mut State| {
                if let Some(file) = &s.local.selected {
                    s.request_local(Request::Inspect(file.id, candidate.clone()));
                }
            }),
        );
    }
    panel
}

fn episode_choices(s: &State, candidate: &Candidate) -> El {
    let mut panel = col().w_full().gap(8.0).child(label(format!(
        "{} ({})",
        candidate.meta.name, candidate.meta.year
    )));
    if candidate.meta.kind == "movie" {
        return panel.child(
            Button::new("local-confirm-movie", "Confirm movie")
                .enabled(!s.local.busy)
                .on_press(|s: &mut State| s.confirm_local_match(None)),
        );
    }
    panel = panel.child(
        Button::new("local-confirm-numbered", "Use filename episode numbers")
            .enabled(!s.local.busy)
            .on_press(|s: &mut State| s.confirm_local_match(None)),
    );
    for video in candidate
        .meta
        .videos
        .iter()
        .skip(s.local.episode_offset)
        .take(50)
    {
        let id = video.id.clone();
        panel = panel.child(
            Button::new(
                format!("local-episode-{id}"),
                format!(
                    "S{} E{} · {}",
                    video.season.unwrap_or(0),
                    video.episode.unwrap_or(0),
                    video.title
                ),
            )
            .enabled(!s.local.busy)
            .on_press(move |s: &mut State| s.confirm_local_match(Some(id.clone()))),
        );
    }
    panel = panel.child(
        row()
            .gap(8.0)
            .child(
                Button::new("local-episodes-prev", "Previous episodes")
                    .enabled(s.local.episode_offset > 0)
                    .on_press(|s: &mut State| {
                        s.local.episode_offset = s.local.episode_offset.saturating_sub(50);
                    }),
            )
            .child(
                Button::new("local-episodes-next", "Next episodes")
                    .enabled(candidate.meta.videos.len() > s.local.episode_offset + 50)
                    .on_press(|s: &mut State| s.local.episode_offset += 50),
            ),
    );
    panel
}
