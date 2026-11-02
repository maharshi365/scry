use super::*;
use crate::{
    history_import::{Issue, Preview},
    ui::settings::{SettingsTab, history::Phase},
};
use scry_library::{HistoryEntry, HistoryImport, HistoryImportResult, Media, MediaKey, MediaKind};

fn preview() -> Preview {
    Preview {
        titles: 1,
        input: HistoryImport {
            entries: vec![HistoryEntry {
                media: Media {
                    key: MediaKey::new("imdb", MediaKind::Movie, "tt100"),
                    title: "Sample movie".into(),
                    parent: None,
                },
                watched: true,
                progress: None,
                active: false,
            }],
            ..HistoryImport::default()
        },
        issues: (0..25)
            .map(|index| Issue {
                title: format!("Sample {index}"),
                reason: "Unsupported ID",
            })
            .collect(),
    }
}

#[test]
fn history_preview_is_non_mutating_and_details_are_paged() {
    let mut state = init();
    state.navigation.page = Page::Settings;
    state.settings.tab = SettingsTab::Data;
    state.settings.history.preview = Some(preview());
    let mut rt = rune::Runtime::new(state, view, (1000.0, 1800.0), rune::Env::default());
    rt.frame(0.0);
    assert!(rt.find("history-preview").is_some());
    assert!(rt.find("history-result").is_none());
    assert!(rt.find("history-issue-0").is_none());
    tap(&mut rt, "history-details");
    assert!(rt.find("history-issue-0").is_some());
    assert!(rt.find("history-issue-19").is_some());
    assert!(rt.find("history-issue-20").is_none());
    tap(&mut rt, "history-next");
    assert!(rt.find("history-issue-20").is_some());
    assert!(rt.find("history-issue-0").is_none());
    assert!(rt.state.settings.history.phase == Phase::Idle);
}

#[test]
fn stale_or_cancelled_preview_cannot_replace_current_import() {
    let (mut state, tx) = state_with_events();
    state.settings.history.generation = 2;
    state.settings.history.phase = Phase::Preparing;
    tx.try_send(Event::HistoryPreview(1, Ok(preview())))
        .unwrap();
    poll(&mut state, 0.0, &mut Tx::default());
    assert!(state.settings.history.preview.is_none());
    state.cancel_history();
    tx.try_send(Event::HistoryPreview(2, Ok(preview())))
        .unwrap();
    poll(&mut state, 0.0, &mut Tx::default());
    assert!(state.settings.history.preview.is_none());
    assert!(state.settings.history.phase == Phase::Idle);
}

#[test]
fn import_is_disabled_during_playback_and_success_requires_database_receipt() {
    let mut state = init();
    state.settings.history.preview = Some(preview());
    state.player.playing = true;
    state.import_history();
    assert!(state.settings.history.phase == Phase::Idle);
    assert!(state.settings.history.result.is_none());
    state.player.playing = false;
    state.import_history();
    assert!(state.settings.history.error.is_some());
    assert!(state.settings.history.result.is_none());
    state.apply_history_result(0, Ok(HistoryImportResult::default()));
    assert!(
        state.settings.history.result.is_none(),
        "Unsolicited completion must not report success"
    );
    state.settings.history.phase = Phase::Applying;
    state.apply_history_result(0, Err("No changes were saved".into()));
    assert!(state.settings.history.result.is_none());
    assert!(state.settings.history.phase == Phase::Idle);
    assert_eq!(
        state.settings.history.error.as_deref(),
        Some("No changes were saved")
    );
}

#[test]
fn import_commit_is_acknowledged_and_does_not_touch_saved_membership() {
    let temp = tempfile::TempDir::new().unwrap();
    let path = temp.path().join(scry_library::DATABASE_NAME);
    let mut state = init();
    state.library.worker =
        Some(crate::library_worker::Worker::start(path.clone(), Arc::new(|| {})).unwrap());
    state.navigation.page = Page::Library;
    state.settings.history.preview = Some(preview());
    let mut rt = rune::Runtime::new(state, view, (1000.0, 800.0), rune::Env::default());
    rt.poll = Some(poll);
    settle_library(&mut rt, |s| s.library.entries.is_some());
    rt.state.import_history();
    assert!(rt.state.settings.history.phase == Phase::Applying);
    assert!(rt.state.settings.history.result.is_none());
    settle_library(&mut rt, |s| s.settings.history.result.is_some());
    let result = rt.state.settings.history.result.as_ref().unwrap();
    assert_eq!(result.watched, 1);
    let library = scry_library::Library::open(path).unwrap();
    assert!(
        library
            .media_states(&[MediaKey::new("imdb", MediaKind::Movie, "tt100")])
            .unwrap()[0]
            .watched
            .watched
    );
    assert!(
        library
            .list_library(false, None, scry_library::Page::default())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn import_scenes_keep_controls_inside_small_and_wide_settings_panels() {
    for viewport in [(880.0, 600.0), (1280.0, 820.0)] {
        for complete in [false, true] {
            let mut state = init();
            if complete {
                super::super::settings::history::scene_complete(&mut state);
            } else {
                super::super::settings::history::scene(&mut state);
            }
            let mut rt = rune::Runtime::new(state, view, viewport, rune::Env::default());
            rt.frame(0.0);
            let panel = rect(&rt, "history-settings");
            for key in ["history-select", "history-details"] {
                let control = rect(&rt, key);
                assert!(
                    control[0] >= panel[0] && control[0] + control[2] <= panel[0] + panel[2],
                    "{key} must stay inside the panel at {viewport:?}"
                );
            }
            assert_eq!(rt.find("history-result").is_some(), complete);
            assert_eq!(rt.find("history-import").is_some(), !complete);
            assert!(
                rt.find("history-previous").is_none(),
                "Short reports do not need pagination controls"
            );
        }
    }
}
