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
        assert!(rt.find("stats-lifetime").is_none());
        let career = rect(&rt, "stats-watch-time");
        let summary = rect(&rt, "stats-summary");
        assert!(summary[1] > career[1]);
        assert!(summary[1] + summary[3] <= career[1] + career[3]);
        assert!(summary[0] > career[0]);
        assert!(summary[0] + summary[2] < career[0] + career[2]);
        let tabs = rect(&rt, "stats-tabs");
        let last_tab = rect(&rt, "stats-tab-achievements");
        assert!((tabs[0] + tabs[2] - last_tab[0] - last_tab[2]).abs() < 0.5);
        assert!(rt.find("achievement-collection").is_none());
        let mix = rect(&rt, "stats-mix");
        let mastery = rect(&rt, "stats-mastery");
        let rankings = rect(&rt, "stats-top-titles");
        let health = rect(&rt, "stats-library-health");
        let records = rect(&rt, "stats-records");
        assert!((mix[2] - mastery[2]).abs() < 0.5);
        assert!((mix[2] - rankings[2]).abs() < 0.5);
        assert!((mastery[0] - health[0]).abs() < 0.5);
        assert!((rankings[1] + rankings[3] - records[1] - records[3]).abs() < 0.5);
    }
}

#[test]
fn achievements_show_requirements_and_progress_inline_without_duplicate_panels() {
    for width in [880.0, 1280.0] {
        let mut state = init();
        super::super::stats::scene(&mut state);
        let mut rt = rune::Runtime::new(state, view, (width, 820.0), rune::Env::default());
        rt.frame(0.0);
        tap(&mut rt, "stats-tab-achievements");
        assert!(rt.find("stats-watch-time").is_none());
        let first = rect(&rt, "achievement-pilot-error");
        let second = rect(&rt, "achievement-seasoned-professional");
        assert!(
            second[1] >= first[1] + first[3],
            "Achievements form a vertical list"
        );
        let last = rect(&rt, "achievement-full-time-viewer");
        assert!(
            (first[2] - last[2]).abs() < 0.5,
            "Achievement rows share the same width"
        );
        assert!(rt.find("achievement-icon-pilot-error").is_some());
        assert!(rt.find("achievement-icon-seasoned-professional").is_some());
        for key in [
            "achievement-detail",
            "achievement-featured",
            "achievement-next",
        ] {
            assert!(rt.find(key).is_none(), "Removed achievement panel: {key}");
        }
        let elements = rt.elements();
        let mut db = Library::memory().unwrap();
        for entry in db.evaluate_achievements().unwrap().entries {
            assert!(rt.find(&format!("achievement-{}", entry.id)).is_some());
            let key = format!("achievement-requirement-{}", entry.id);
            let requirement = elements
                .iter()
                .find(|element| element.key.as_deref() == Some(key.as_str()));
            if entry.hidden {
                assert!(
                    requirement.is_none(),
                    "Secret requirements must stay hidden"
                );
                assert!(
                    !elements
                        .iter()
                        .any(|element| element.text.as_deref() == Some(entry.name))
                );
            } else {
                assert_eq!(
                    requirement.and_then(|element| element.text.as_deref()),
                    Some(entry.requirement)
                );
            }
        }
        for key in [
            "achievement-requirement-pilot-error",
            "achievement-status-pilot-error",
            "achievement-requirement-expert",
            "achievement-progress-expert",
        ] {
            assert!(rt.find(key).is_some(), "Inline detail missing: {key}");
        }
        assert!(
            elements
                .iter()
                .any(|element| element.text.as_deref() == Some("Unlocked from watch history"))
        );
        tap(&mut rt, "stats-tab-stats");
        assert!(rt.find("stats-watch-time").is_some());
        assert!(rt.find("achievement-collection").is_none());
    }
}
