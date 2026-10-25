use super::*;
use scry_library::guide::{Channel, ChannelSource, Program};

fn channel(id: &str) -> Channel {
    let now = chrono::Utc::now().timestamp();
    Channel {
        id: id.into(),
        name: id.into(),
        kind: MediaKind::Series,
        source: ChannelSource::Library,
        slots: vec![Slot {
            starts_at: now - 600,
            program: Program {
                media: Media {
                    key: MediaKey::new("test", MediaKind::Episode, format!("{id}:1")),
                    title: format!("{id} pilot"),
                    parent: Some(MediaKey::new("test", MediaKind::Series, id)),
                },
                show_title: id.into(),
                season: Some(1),
                episode: Some(1),
                description: "A program already in progress".into(),
                artwork: None,
                duration: 1800,
                estimated: true,
            },
        }],
    }
}

fn state() -> State {
    let mut state = super::super::app::init();
    state.config_busy = false;
    state.navigation.page = Page::Guide;
    state.guide.data.channels = vec![channel("office"), channel("himym")];
    state.guide.selected = Some("office".into());
    state
}

fn stream(value: serde_json::Value) -> Stream {
    serde_json::from_value(value).unwrap()
}

#[test]
fn automatic_sources_play_direct_streams_by_default_and_never_choose_blocked_torrents() {
    let sources = vec![
        (
            "Provider".into(),
            stream(
                serde_json::json!({"infoHash":"0123456789abcdef0123456789abcdef01234567","name":"1080"}),
            ),
        ),
        (
            "Provider".into(),
            stream(serde_json::json!({"url":"https://example.com/video","name":"1080"})),
        ),
    ];
    assert_eq!(
        select_sources(sources.clone(), &SourcePolicy::default()).len(),
        1
    );
    let policy = SourcePolicy {
        quality: "1080".into(),
        ..SourcePolicy::default()
    };
    let selected = select_sources(sources.clone(), &policy);
    assert_eq!(selected.len(), 1);
    assert!(selected.first().unwrap().url.is_some());
    assert!(selected.first().unwrap().info_hash.is_none());
    let all = select_sources(
        sources,
        &SourcePolicy {
            allow_torrents: true,
            ..policy
        },
    );
    assert_eq!(all.len(), 2);
    assert!(all.first().unwrap().url.is_some());
}

#[test]
fn source_policy_rejects_other_providers_and_unsupported_streams_but_allows_quality_fallback() {
    let sources = vec![
        (
            "Other".into(),
            stream(serde_json::json!({"url":"https://example.com/a","name":"1080"})),
        ),
        (
            "Chosen".into(),
            stream(serde_json::json!({"url":"https://example.com/b","name":"720"})),
        ),
        (
            "Chosen".into(),
            stream(
                serde_json::json!({"url":"https://example.com/c","name":"1080","behaviorHints":{"proxyHeaders":{"request":{"Authorization":"secret"}}}}),
            ),
        ),
    ];
    let policy = SourcePolicy {
        provider: "Chosen".into(),
        quality: "1080".into(),
        ..SourcePolicy::default()
    };
    let selected = select_sources(sources, &policy);
    assert_eq!(selected.len(), 1);
    assert_eq!(selected.first().unwrap().name, "720");
}

#[test]
fn browsing_does_not_touch_playback_and_tuning_stays_in_the_guide() {
    let mut state = state();
    let down = rune::KeyEvent {
        key: rune::Key::Down,
        mods: rune::Mods::default(),
    };
    assert!(shortcut(
        &mut state,
        &down,
        super::super::shortcuts::Focus::Screen
    ));
    assert_eq!(state.guide.selected.as_deref(), Some("himym"));
    assert!(state.guide.tuned.is_none());
    assert!(state.library.playing.is_none());
    assert!(!state.player.playing);
    let enter = rune::KeyEvent {
        key: rune::Key::Enter,
        mods: rune::Mods::default(),
    };
    assert!(shortcut(
        &mut state,
        &enter,
        super::super::shortcuts::Focus::Screen
    ));
    assert!(!state.guide.settings);
    assert_eq!(state.guide.tuned.as_ref().unwrap().0, "himym");
    assert!(!state.guide.resolving);
}

#[test]
fn entering_guide_automatically_tunes_an_available_program() {
    let mut state = state();
    let data = state.guide.data.clone();
    state.apply_guide(0, data);
    assert_eq!(state.guide.tuned.as_ref().unwrap().0, "office");
    assert!(!state.guide.settings);
    assert!(
        (600..=602).contains(
            &state
                .guide
                .tuned
                .as_ref()
                .unwrap()
                .1
                .elapsed_seconds(chrono::Utc::now().timestamp())
        )
    );
}

#[test]
fn clicking_the_current_program_tunes_the_channel_without_opening_settings() {
    let state = state();
    let key = format!(
        "guide-program-office-{}",
        state
            .guide
            .data
            .channels
            .first()
            .unwrap()
            .slots
            .first()
            .unwrap()
            .starts_at
    );
    let mut runtime = rune::Runtime::new(
        state,
        super::super::shell::view,
        (1280.0, 820.0),
        rune::Env::default(),
    );
    runtime.frame(1.0 / 60.0);
    let (x, y) = runtime.find(&key).unwrap();
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0 / 60.0);
    assert_eq!(runtime.state.guide.tuned.as_ref().unwrap().0, "office");
    assert!(!runtime.state.guide.settings);
}

#[test]
fn settings_use_aligned_horizontal_choices_without_repeated_channel_controls() {
    for viewport in [(1280.0, 820.0), (880.0, 600.0)] {
        let mut state = state();
        state.guide.settings = true;
        state.addons = vec![serde_json::from_value(serde_json::json!({"manifest_url":"https://example.com/manifest.json","manifest":{"id":"streams","name":"Streams","resources":["stream"]}})).unwrap()];
        let mut runtime = rune::Runtime::new(
            state,
            super::super::shell::view,
            viewport,
            rune::Env::default(),
        );
        runtime.frame(1.0 / 60.0);
        let rect = |key: &str| {
            runtime
                .elements()
                .into_iter()
                .find(|e| e.key.as_deref() == Some(key))
                .unwrap()
                .rect
        };
        let any = rect("guide-provider-any");
        let provider = rect("guide-provider-0");
        assert!((any[1] - provider[1]).abs() < 1.0);
        assert!(
            any[2] < 200.0 && provider[2] < 200.0,
            "Provider choices must not stretch across the page"
        );
        let quality_label = rect("guide-quality-label");
        let provider_label = rect("guide-provider-label");
        assert!((quality_label[0] - provider_label[0]).abs() < 1.0);
        assert!((rect("guide-quality-")[0] - any[0]).abs() < 1.0);
        assert!(runtime.find("guide-order-shuffle").is_some());
        assert!(
            (rect("guide-order-shuffle")[1] - rect("guide-episodes-all")[1]).abs() < 1.0,
            "Shared programming choices should share a row"
        );
        let panel = rect("guide-shared-settings");
        assert!(panel[3] < 300.0, "Shared settings are too tall: {panel:?}");
        assert!(
            rect("guide-episodes-watched")[0] + rect("guide-episodes-watched")[2] <= viewport.0,
            "Programming options overflow the window"
        );
        for key in [
            "guide-order-office",
            "guide-watched-office",
            "guide-move-up-office",
            "guide-policy-approve",
        ] {
            assert!(
                runtime.find(key).is_none(),
                "Unexpected per-channel or approval control {key}"
            );
        }
    }
}

#[test]
fn preferred_quality_is_ranked_first_without_discarding_unlabelled_fallbacks() {
    let policy = SourcePolicy {
        quality: "1080".into(),
        ..SourcePolicy::default()
    };
    let sources = vec![
        (
            "A".into(),
            stream(serde_json::json!({"url":"https://example.com/unlabelled"})),
        ),
        (
            "B".into(),
            stream(serde_json::json!({"url":"https://example.com/preferred","name":"1080p"})),
        ),
    ];
    let selected = select_sources(sources, &policy);
    assert_eq!(selected.len(), 2);
    assert_eq!(selected.first().unwrap().name, "1080p");
}

#[test]
fn guide_rejects_player_seek_and_pause_actions_but_regular_player_keeps_them() {
    use super::super::player::actions::PlayerAction;
    let mut state = state();
    state.player.playing = true;
    state.player.sample.position_s = Some(600.0);
    state.player_action(PlayerAction::SeekTo(0.0));
    state.player_action(PlayerAction::TogglePause);
    assert_eq!(state.player.sample.position_s, Some(600.0));
    assert!(!state.player.sample.paused);
    state.navigation.page = Page::Player;
    state.player_action(PlayerAction::SeekTo(0.0));
    state.player_action(PlayerAction::TogglePause);
    assert_eq!(state.player.sample.position_s, Some(0.0));
    assert!(state.player.sample.paused);
}

#[test]
fn leaving_guide_invalidates_pending_tunes_and_clears_source_candidates() {
    let mut state = state();
    state.guide.tuned = Some(("office".into(), channel("office").slots.remove(0)));
    state.guide.resolving = true;
    state.guide.alternatives.push(stream(
        serde_json::json!({"url":"https://example.com/video"}),
    ));
    let generation = state.stream_id;
    state.navigate(Page::Watching);
    assert!(state.stream_id > generation);
    assert!(state.guide.tuned.is_none());
    assert!(!state.guide.resolving);
    assert!(state.guide.alternatives.is_empty());
    assert!(!state.player.playing);
}

#[test]
fn guide_layout_has_grid_preview_and_no_episode_or_seek_controls() {
    for (width, height) in [(1280.0, 820.0), (880.0, 600.0)] {
        let mut runtime = rune::Runtime::new(
            state(),
            super::super::shell::view,
            (width, height),
            rune::Env::default(),
        );
        runtime.frame(1.0 / 60.0);
        for key in [
            "guide-grid",
            "guide-video",
            "guide-channel-office",
            "guide-now-marker",
        ] {
            assert!(runtime.find(key).is_some(), "Missing {key} at {width}");
        }
        for key in [
            "player-timeline",
            "seek-back",
            "seek-forward",
            "pause",
            "episode-0",
        ] {
            assert!(runtime.find(key).is_none(), "Unexpected {key}");
        }
        let video = runtime
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("guide-video"))
            .unwrap()
            .rect;
        assert!(video[2] > 0.0 && video[3] > 0.0);
        assert!(
            video[0] + video[2] <= width + 1.0,
            "Preview extends beyond the window: {video:?}"
        );
    }
}

#[test]
fn schedule_boundary_tunes_the_next_program_without_an_episode_choice() {
    let mut state = state();
    let now = chrono::Utc::now().timestamp();
    let mut old = channel("office").slots.remove(0);
    old.starts_at = now - 1860;
    let mut next = old.clone();
    next.starts_at = old.ends_at();
    next.program.media.key.id = "office:2".into();
    next.program.episode = Some(2);
    state.guide.data.channels.first_mut().unwrap().slots = vec![old.clone(), next.clone()];
    state.guide.tuned = Some(("office".into(), old));
    state.guide.resolving = true;
    state.guide.refreshed = now;
    state.poll_guide(&mut Tx::default());
    let (_, tuned) = state.guide.tuned.as_ref().unwrap();
    assert_eq!(tuned.program.media.key, next.program.media.key);
    assert_eq!(tuned.elapsed_seconds(now), 60);
    assert!(
        state.library.playing.is_none(),
        "Resolving a source is not watched time"
    );
}

#[test]
fn stale_source_results_cannot_replace_a_new_channel_tune() {
    let mut state = state();
    state.guide.resolving = true;
    state.guide.tuned = Some(("office".into(), channel("office").slots.remove(0)));
    state.stream_id = 20;
    let (sender, events) = tokio::sync::mpsc::channel(4);
    state.events = Some(events);
    sender
        .try_send(crate::services::Event::Streams(
            19,
            vec![(
                "Provider".into(),
                stream(serde_json::json!({"url":"https://example.com/old"})),
            )],
            Vec::new(),
            true,
        ))
        .unwrap();
    super::super::events::poll(&mut state, 0.0, &mut Tx::default());
    assert!(state.guide.resolving);
    assert!(state.guide.alternatives.is_empty());
    assert!(!state.player.playing);
}

#[test]
fn a_suitable_direct_source_does_not_wait_for_all_providers_to_finish() {
    let mut state = state();
    state.guide.data.policy.allow_torrents = true;
    state.guide.resolving = true;
    state.guide.tuned = Some(("office".into(), channel("office").slots.remove(0)));
    state.apply_guide_streams(
        vec![(
            "Provider".into(),
            stream(serde_json::json!({"infoHash":"0123456789abcdef0123456789abcdef01234567"})),
        )],
        false,
    );
    assert!(
        state.guide.resolving,
        "A later provider can still supply a preferred direct source"
    );
    state.apply_guide_streams(
        vec![(
            "Provider".into(),
            stream(serde_json::json!({"url":"https://example.com/video"})),
        )],
        false,
    );
    assert!(
        !state.guide.resolving,
        "A usable source must not wait for slow providers"
    );
    assert_eq!(state.library.playing.as_ref().unwrap().key.id, "office:1");
}

#[test]
fn playback_restart_after_joining_does_not_seek_again_or_reset_watched_observation() {
    let mut state = state();
    state.player.generation = 4;
    state.player.playing = true;
    state.guide.tuned = Some(("office".into(), channel("office").slots.remove(0)));
    let (sender, events) = tokio::sync::mpsc::channel(4);
    state.events = Some(events);
    sender.try_send(crate::services::Event::Ready(4)).unwrap();
    super::super::events::poll(&mut state, 0.0, &mut Tx::default());
    assert!(state.player.ready);
    state.library.observed = Some(scry_playback::telemetry::Sample {
        position_s: Some(605.0),
        ..Default::default()
    });
    sender.try_send(crate::services::Event::Ready(4)).unwrap();
    super::super::events::poll(&mut state, 0.0, &mut Tx::default());
    assert_eq!(
        state
            .library
            .observed
            .as_ref()
            .and_then(|sample| sample.position_s),
        Some(605.0),
        "The playback-restart event caused another seek and discarded actual playback tracking"
    );
}

#[test]
fn torrent_selection_uses_a_healthy_swarm_and_then_a_smaller_file() {
    let torrents = [("A huge",459,"651.55 MB"),("B balanced",421,"334.08 MB"),("C weak",5,"50 MB")].map(|(name,seeds,size)| ("Provider".into(),stream(serde_json::json!({"name":"Torrentio\n1080p","title":format!("{name}\n👤 {seeds} 💾 {size}"),"infoHash":"0123456789abcdef0123456789abcdef01234567"}))));
    let selected = select_sources(
        torrents.to_vec(),
        &SourcePolicy {
            allow_torrents: true,
            ..Default::default()
        },
    );
    assert!(selected.first().unwrap().title.starts_with("B balanced"));
    assert!(selected.last().unwrap().title.starts_with("C weak"));
}

#[test]
fn torrent_candidates_start_after_a_bounded_grace_even_if_another_provider_stalls() {
    let mut state = state();
    state.guide.data.policy.allow_torrents = true;
    state.guide.resolving = true;
    state.guide.tuned = Some(("office".into(), channel("office").slots.remove(0)));
    let torrent = stream(
        serde_json::json!({"infoHash":"0123456789abcdef0123456789abcdef01234567","name":"1080p","title":"👤 100 💾 300 MB"}),
    );
    state.apply_guide_streams(vec![("Torrentio".into(), torrent.clone())], false);
    let deadline = state.guide.tuning.pending.as_ref().unwrap().deadline;
    state.apply_guide_streams(vec![("Torrentio".into(), torrent)], false);
    assert_eq!(
        state.guide.tuning.pending.as_ref().unwrap().deadline,
        deadline,
        "Additional provider updates must not extend the wait"
    );
    assert!(state.guide.resolving);
    state.poll_guide_tuning(deadline, &mut Tx::default());
    assert!(!state.guide.resolving);
    assert!(state.guide.tuning.pending.is_none());
    assert_eq!(state.guide.tuning.attempts, 1);
    assert_eq!(state.library.playing.as_ref().unwrap().key.id, "office:1");
}

#[test]
fn joining_timeout_tries_the_next_source_without_restarting_the_schedule() {
    let mut state = state();
    let slot = channel("office").slots.remove(0);
    state.guide.tuned = Some(("office".into(), slot.clone()));
    state.player.playing = true;
    state.player.generation = 5;
    state.guide.tuning.attempts = 1;
    let now = std::time::Instant::now();
    state.guide.tuning.joining = Some(now);
    state.guide.alternatives.push(stream(
        serde_json::json!({"url":"https://example.com/next"}),
    ));
    assert!(state.poll_guide_tuning(now, &mut Tx::default()));
    assert_eq!(state.guide.tuning.attempts, 2);
    assert!(state.guide.alternatives.is_empty());
    assert!(
        state.player.generation > 5,
        "Previous playback events must be stale after failover"
    );
    assert_eq!(
        state.guide.tuned.as_ref().unwrap().1.starts_at,
        slot.starts_at
    );
    assert!(
        state
            .guide
            .tuned
            .as_ref()
            .unwrap()
            .1
            .elapsed_seconds(chrono::Utc::now().timestamp())
            >= 600
    );
}

#[test]
fn repeated_join_failures_stop_after_three_attempts_and_success_clears_the_deadline() {
    let mut state = state();
    state.guide.tuned = Some(("office".into(), channel("office").slots.remove(0)));
    state.player.playing = true;
    state.guide.tuning.attempts = 3;
    let now = std::time::Instant::now();
    state.guide.tuning.joining = Some(now);
    state.guide.alternatives.push(stream(
        serde_json::json!({"url":"https://example.com/untried"}),
    ));
    state.poll_guide_tuning(now, &mut Tx::default());
    assert!(!state.player.playing);
    assert_eq!(state.guide.tuning.attempts, 3);
    assert!(
        state
            .guide
            .error
            .as_deref()
            .unwrap()
            .contains("could not start")
    );
    state.player.playing = true;
    state.player.ready = true;
    state.player.sample.position_s = Some(610.0);
    state.player.sample.buffering = false;
    state.guide.tuning.joining = Some(now);
    state.poll_guide_tuning(now, &mut Tx::default());
    assert!(state.player.playing);
    assert!(state.guide.tuning.joining.is_none());
}
