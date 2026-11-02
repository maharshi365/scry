use crate::{
    history_import::Preview,
    library_worker::Command,
    ui::{
        State,
        design::tokens,
        primitives::{
            button::{Button, Variant},
            feedback::{self, Tone},
            form::option_row,
            pattern::settings_group,
            text::label,
        },
    },
};
use rune::{El, col, row};
use scry_library::HistoryImportResult;

const ISSUES_PER_PAGE: usize = 20;

pub(in crate::ui) fn scene(s: &mut State) {
    use scry_library::{HistoryEntry, HistoryImport, Media, MediaKey, MediaKind, Progress};
    s.navigation.page = crate::ui::Page::Settings;
    s.settings.tab = super::SettingsTab::Data;
    s.settings.history = SettingsUi::default();
    s.settings.history.preview = Some(Preview {
        titles: 2,
        input: HistoryImport {
            entries: vec![
                HistoryEntry {
                    media: Media {
                        key: MediaKey::new("imdb", MediaKind::Movie, "tt100"),
                        title: "Sample movie".into(),
                        parent: None,
                    },
                    watched: true,
                    progress: None,
                    active: false,
                },
                HistoryEntry {
                    media: Media {
                        key: MediaKey::new("imdb", MediaKind::Movie, "tt101"),
                        title: "Another movie".into(),
                        parent: None,
                    },
                    watched: false,
                    progress: Progress::new(600.0, 5400.0),
                    active: true,
                },
            ],
            ..HistoryImport::default()
        },
        issues: vec![
            crate::history_import::Issue {
                title: "Sample anime".into(),
                reason: "Unsupported ID or media type; no title matching was attempted",
            },
            crate::history_import::Issue {
                title: "Sample series".into(),
                reason: "Episode history skipped: episode order or anchor could not be verified",
            },
        ],
    });
    s.settings.history.details = true;
}

pub(in crate::ui) fn scene_complete(s: &mut State) {
    scene(s);
    s.settings.history.phase = Phase::Complete;
    s.settings.history.result = Some(HistoryImportResult {
        watched: 1,
        progress: 1,
        ..HistoryImportResult::default()
    });
}

#[derive(Default, PartialEq, Eq)]
pub(in crate::ui) enum Phase {
    #[default]
    Idle,
    Preparing,
    Applying,
    Complete,
}

#[derive(Default)]
pub(in crate::ui) struct SettingsUi {
    pub generation: u64,
    pub phase: Phase,
    pub preview: Option<Preview>,
    pub result: Option<HistoryImportResult>,
    pub error: Option<String>,
    pub processed: usize,
    pub total: usize,
    pub details: bool,
    pub issue_offset: usize,
}

impl SettingsUi {
    fn busy(&self) -> bool {
        matches!(self.phase, Phase::Preparing | Phase::Applying)
    }
}

pub(super) fn view(s: &State) -> El {
    let history = &s.settings.history;
    let mut actions = row().gap(tokens::layout::ACTION_GAP).child(
        Button::new("history-select", "Select JSON file")
            .enabled(!history.busy())
            .on_press_tx(State::select_history),
    );
    if history.phase == Phase::Preparing {
        actions =
            actions.child(Button::new("history-cancel", "Cancel").on_press(State::cancel_history));
    }
    let mut panel = settings_group(
        "history-settings",
        vec![option_row(
            "history-source",
            "Stremio watch history",
            "Select a Stremio library JSON export. Import watched status and resume positions, not saved titles, add-ons, or account settings. Watch dates and playback totals are not recreated. Existing Scry state is kept.",
            actions,
        )],
    ).gap(tokens::layout::SECTION_GAP);
    if history.phase == Phase::Preparing {
        panel = panel.child(feedback::status(
            "history-progress",
            &format!(
                "Checking history… {} of {} titles",
                history.processed, history.total
            ),
            Tone::Neutral,
        ));
    }
    if let Some(preview) = &history.preview {
        let watched = preview.input.entries.iter().filter(|e| e.watched).count();
        let progress = preview
            .input
            .entries
            .iter()
            .filter(|e| e.progress.is_some())
            .count();
        if history.phase != Phase::Complete {
            panel = panel.child(feedback::message("history-preview", "History preview",
                &format!("Titles: {} · Watched items: {watched} · Resume positions: {progress}. Existing local state may reduce the import count. Nothing has been changed yet.", preview.titles), Tone::Neutral,
                row().child(Button::new("history-import", if history.phase == Phase::Applying { "Importing…" } else { "Import history" })
                    .variant(Variant::Primary).enabled(!history.busy() && !preview.input.entries.is_empty() && !s.player.playing)
                    .on_press(State::import_history))));
        }
        if !preview.issues.is_empty() {
            panel = notices(panel, history, preview);
        }
    }
    if let Some(result) = &history.result {
        panel = panel.child(feedback::message("history-result", "Import complete",
            &format!("{} watched items added · {} resume positions added · {} unchanged · {} local states kept · {} release checks skipped. No playback sessions or measured watch time were created.",result.watched,result.progress,result.unchanged,result.preserved,result.ineligible),Tone::Success,row()));
    }
    if let Some(error) = &history.error {
        panel = panel.child(feedback::message(
            "history-error",
            "History not imported",
            error,
            Tone::Danger,
            row(),
        ));
    }
    if s.player.playing {
        panel = panel.child(feedback::status(
            "history-stop-playback",
            "Stop playback before importing history.",
            Tone::Neutral,
        ));
    }
    col().w_full().child(panel)
}

fn notices(mut panel: El, history: &SettingsUi, preview: &Preview) -> El {
    panel = panel.child(feedback::message(
        "history-skips",
        "Some data cannot be imported",
        &format!(
            "{} notices. A title may still import when only one part is skipped.",
            preview.issues.len()
        ),
        Tone::Warning,
        row().child(
            Button::new(
                "history-details",
                if history.details {
                    "Hide details"
                } else {
                    "View details"
                },
            )
            .on_press(|s: &mut State| {
                s.settings.history.details = !s.settings.history.details;
            }),
        ),
    ));
    if history.details {
        let mut details = col().w_full().gap(tokens::layout::COPY_GAP);
        for (index, issue) in preview
            .issues
            .iter()
            .enumerate()
            .skip(history.issue_offset)
            .take(ISSUES_PER_PAGE)
        {
            details = details.child(
                label(format!("{} — {}", issue.title, issue.reason))
                    .key(format!("history-issue-{index}")),
            );
        }
        panel = panel.child(details);
        if preview.issues.len() <= ISSUES_PER_PAGE {
            return panel;
        }
        let end = (history.issue_offset + ISSUES_PER_PAGE).min(preview.issues.len());
        panel = panel.child(
            row()
                .gap(tokens::layout::ACTION_GAP)
                .center_y()
                .flex_wrap()
                .child(
                    Button::new("history-previous", "Previous")
                        .enabled(history.issue_offset > 0)
                        .on_press(|s: &mut State| {
                            s.settings.history.issue_offset = s
                                .settings
                                .history
                                .issue_offset
                                .saturating_sub(ISSUES_PER_PAGE);
                        }),
                )
                .child(label(format!(
                    "{}–{end} of {}",
                    history.issue_offset + 1,
                    preview.issues.len()
                )))
                .child(
                    Button::new("history-next", "Next")
                        .enabled(end < preview.issues.len())
                        .on_press(|s: &mut State| {
                            s.settings.history.issue_offset += ISSUES_PER_PAGE;
                        }),
                ),
        );
    }
    panel
}

impl State {
    #[cfg(target_os = "linux")]
    fn select_history(&mut self, tx: &mut rune::Tx) {
        if self.settings.history.busy() {
            return;
        }
        tx.open_file(
            rune::FileDialogOptions {
                title: "Import watch history".into(),
                ..Default::default()
            }
            .filter("JSON", &["json"]),
            |s: &mut Self, result, _| match result {
                Ok(Some(path)) if !s.settings.history.busy() => s.preview_history_path(path),
                Ok(_) => {}
                Err(_) => {
                    s.settings.history.error = Some("Cannot open the history file picker".into());
                }
            },
        );
    }

    #[cfg(not(target_os = "linux"))]
    fn select_history(&mut self, _: &mut rune::Tx) {
        if self.settings.history.busy() {
            return;
        }
        let path = match self.shell.window.pick_history() {
            Ok(Some(path)) => path,
            Ok(None) => return,
            Err(_) => {
                self.settings.history.error = Some("Cannot open the history file picker".into());
                return;
            }
        };
        self.preview_history_path(path);
    }

    fn preview_history_path(&mut self, path: std::path::PathBuf) {
        let Some(services) = &mut self.runtime.services else {
            self.settings.history.error = Some("History import is unavailable".into());
            return;
        };
        let generation = self.settings.history.generation.wrapping_add(1);
        self.settings.history = SettingsUi {
            generation,
            phase: Phase::Preparing,
            ..SettingsUi::default()
        };
        services.preview_history(generation, path, self.addons.installed.clone());
    }

    pub(in crate::ui) fn cancel_history(&mut self) {
        if self.settings.history.phase != Phase::Preparing {
            return;
        }
        if let Some(services) = &mut self.runtime.services {
            services.cancel_history_preview();
        }
        self.settings.history.generation = self.settings.history.generation.wrapping_add(1);
        self.settings.history.phase = Phase::Idle;
    }

    pub(in crate::ui) fn apply_history_preview(
        &mut self,
        generation: u64,
        result: Result<Preview, String>,
    ) {
        let history = &mut self.settings.history;
        if generation != history.generation || history.phase != Phase::Preparing {
            return;
        }
        history.phase = Phase::Idle;
        match result {
            Ok(preview) => history.preview = Some(preview),
            Err(error) => history.error = Some(error),
        }
    }

    pub(in crate::ui) fn import_history(&mut self) {
        let history = &mut self.settings.history;
        if history.busy() || history.phase == Phase::Complete || self.player.playing {
            return;
        }
        let Some(preview) = &history.preview else {
            return;
        };
        if preview.input.entries.is_empty() {
            return;
        }
        let result = self
            .library
            .worker
            .as_ref()
            .ok_or_else(|| "Local library is unavailable".to_owned())
            .and_then(|worker| {
                worker
                    .send(Command::ImportHistory {
                        generation: history.generation,
                        input: preview.input.clone(),
                    })
                    .map_err(|_| "Library is busy or unavailable; try again".into())
            });
        match result {
            Ok(()) => {
                history.phase = Phase::Applying;
                history.error = None;
            }
            Err(error) => history.error = Some(error),
        }
    }

    pub(in crate::ui) fn apply_history_result(
        &mut self,
        generation: u64,
        result: Result<HistoryImportResult, String>,
    ) {
        let history = &mut self.settings.history;
        if history.generation != generation || history.phase != Phase::Applying {
            return;
        }
        match result {
            Ok(result) => {
                if let Some(preview) = &mut history.preview {
                    for notice in &result.notices {
                        preview.issue(&notice.title, notice.reason);
                    }
                }
                history.result = Some(result);
                history.phase = Phase::Complete;
                self.invalidate_stats();
                self.retry_movie_runtimes();
                self.refresh_library();
                self.request_series_tracking(true);
            }
            Err(error) => {
                history.error = Some(error);
                history.phase = Phase::Idle;
            }
        }
    }
}
