use super::*;
use scry_library::{Library, Media, MediaKey, MediaKind};

fn earned() -> scry_library::AchievementEvaluation {
    let mut db = Library::memory().unwrap();
    let item = Media {
        key: MediaKey::new("test", MediaKind::Movie, "one"),
        title: "One".into(),
        parent: None,
    };
    db.upsert_media(&item).unwrap();
    db.set_known_runtimes(&[(item.key.clone(), 400_000.0)])
        .unwrap();
    db.set_watched(&item.key, true).unwrap();
    db.evaluate_achievements().unwrap()
}

#[test]
fn historical_unlocks_make_one_summary_and_earned_rules_show_in_the_collection() {
    let mut state = init();
    state.navigation.page = Page::Stats;
    state.apply_achievements(earned());
    assert!(state.poll_achievement_toast(&mut Tx::default()));
    let mut rt = rune::Runtime::new(state, view, (880.0, 600.0), rune::Env::default());
    rt.frame(0.0);
    assert!(rt.find("achievement-toast-1").is_some());
    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
    }
    let bounds = rect(&rt, "achievement-toast-1");
    assert!((bounds[1] - 24.0).abs() < 0.5);
    assert!((bounds[0] + bounds[2] - 856.0).abs() < 0.5);
    tap(&mut rt, "achievement-view");
    assert_eq!(rt.state.navigation.page, Page::Stats);
    assert!(rt.find("achievement-centurion").is_some());
    assert!(rt.find("achievement-toast-1").is_none());
}

#[test]
fn notifications_wait_until_video_has_stopped() {
    let mut state = init();
    state.navigation.page = Page::Player;
    state.player.playing = true;
    state.apply_achievements(earned());
    assert!(!state.poll_achievement_toast(&mut Tx::default()));
    assert!(super::super::stats::toast(&state).is_none());
    state.player.sample.paused = true;
    assert!(!state.poll_achievement_toast(&mut Tx::default()));
    assert!(super::super::stats::toast(&state).is_none());
    state.player.playing = false;
    assert!(state.poll_achievement_toast(&mut Tx::default()));
    assert!(super::super::stats::toast(&state).is_some());
}

#[test]
fn stats_page_keeps_content_inside_the_minimum_window_width() {
    let directory = tempfile::tempdir().unwrap();
    let mut rt = rune::Runtime::new(init(), view, (880.0, 600.0), rune::Env::default());
    rt.poll = Some(poll);
    let worker = crate::library_worker::Worker::start(
        directory.path().join("stats.sqlite3"),
        std::sync::Arc::new(|| {}),
    )
    .unwrap();
    rt.update(|state, _| {
        state.library.worker = Some(worker);
        state.navigate(Page::Stats);
    });
    settle_library(&mut rt, |state| state.library.entries.is_some());
    rt.frame(0.0);
    let viewport = rect(&rt, "nerd-stats-scroll");
    for key in [
        "stats-watch-time",
        "stats-mastery",
        "stats-mix",
        "stats-top-titles",
        "stats-library-health",
    ] {
        let bounds = rect(&rt, key);
        assert!(bounds[0] >= viewport[0] - 0.5, "{key} left");
        assert!(
            bounds[0] + bounds[2] <= viewport[0] + viewport[2] + 0.5,
            "{key} right"
        );
    }
    for key in [
        "stats-week",
        "stats-month",
        "stats-year",
        "stats-all",
        "stats-activity",
    ] {
        assert!(rt.find(key).is_none(), "Removed dashboard control: {key}");
    }
}

#[test]
fn lifetime_dashboard_uses_compact_summary_panels_and_distinct_achievement_icons() {
    for width in [880.0, 1280.0] {
        let mut state = init();
        super::super::stats::scene(&mut state);
        let mut rt = rune::Runtime::new(state, view, (width, 820.0), rune::Env::default());
        rt.frame(0.0);
        assert!(rt.find("stats-methodology").is_none());
        assert!(
            rt.elements()
                .iter()
                .filter_map(|element| element.text.as_deref())
                .all(|text| !text.contains("science") && !text.contains("mastered*"))
        );
        assert!(
            rect(&rt, "stats-summary")[3] < 85.0,
            "Summary should be a compact strip"
        );
        assert!(
            rect(&rt, "stats-mix")[3] < 190.0,
            "Mix is a chart, not a report"
        );
        rt.update(|_, tx| tx.scroll_into_view("achievements-toggle"));
        for _ in 0..60 {
            rt.frame(1.0 / 60.0);
        }
        tap(&mut rt, "achievements-toggle");
        let first = rect(&rt, "achievement-pilot-error");
        let second = rect(&rt, "achievement-seasoned-professional");
        assert!(
            (first[1] - second[1]).abs() < 0.5,
            "Badges share a grid row"
        );
        assert!(first[3] <= 90.0, "Badges must stay compact");
        let last = rect(&rt, "achievement-full-time-viewer");
        assert!(
            (first[2] - last[2]).abs() < 0.5,
            "The last row must not stretch into oversized cards"
        );
        assert!(rt.find("achievement-icon-pilot-error").is_some());
        assert!(rt.find("achievement-icon-seasoned-professional").is_some());
    }
}
