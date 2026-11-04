use super::*;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[derive(Debug, PartialEq)]
enum Recorded {
    Opened(u64),
    Loaded(String),
    Command(Vec<String>),
    Dropped,
}

#[derive(Clone, Default)]
struct FakePlayback(
    Arc<StdMutex<Vec<Recorded>>>,
    Arc<AtomicBool>,
    Arc<AtomicUsize>,
);

impl FakePlayback {
    fn record(&self, event: Recorded) {
        self.0.lock().unwrap().push(event);
    }
}

impl PlaybackFactory for FakePlayback {
    fn open(&self, target: PlaybackTarget) -> Result<Box<dyn PlaybackEngine>> {
        let PlaybackTarget::NativeWindow(window) = target else {
            bail!("Unexpected output target");
        };
        self.record(Recorded::Opened(window));
        Ok(Box::new(FakeEngine(self.clone())))
    }
}

struct FakeEngine(FakePlayback);

impl PlaybackEngine for FakeEngine {
    fn property(&self, _: &str) -> Option<String> {
        None
    }

    fn load(&self, url: &str) -> Result<()> {
        self.0.record(Recorded::Loaded(url.into()));
        Ok(())
    }

    fn command(&self, args: &[&str]) -> Result<()> {
        self.0.record(Recorded::Command(
            args.iter().map(|arg| (*arg).into()).collect(),
        ));
        Ok(())
    }

    fn event(&self, _: f64) -> Option<PlaybackEvent> {
        self.0
            .1
            .swap(false, Ordering::Relaxed)
            .then_some(PlaybackEvent::Ready)
    }

    fn sample(&self, _: f64) -> Sample {
        self.0.2.fetch_add(1, Ordering::Relaxed);
        Sample::default()
    }
}

#[tokio::test(start_paused = true)]
async fn playback_readiness_does_not_wait_for_the_one_second_metrics_sample() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("fixture.mp4");
    std::fs::write(&source, []).unwrap();
    let playback = FakePlayback::default();
    let (tx, mut events) = mpsc::channel(16);
    let inbox = Inbox {
        tx,
        wake: Arc::new(|| {}),
    };
    let (control, lifecycle) = watch::channel(None);
    let (_commands, commands) = mpsc::channel(1);
    let (_stats, stats) = watch::channel(None);
    control
        .send(Some(Control::Play(
            1,
            PlaybackTarget::NativeWindow(123),
            Source::Local(source),
        )))
        .unwrap();
    let worker = player_loop(
        lifecycle,
        commands,
        stats,
        inbox,
        directory.path().into(),
        &playback,
    );
    let drive = async {
        assert!(matches!(events.recv().await, Some(Event::Playing(1, _))));
        playback.1.store(true, Ordering::Relaxed);
        let ready = tokio::time::timeout(Duration::from_millis(100), async {
            loop {
                if matches!(events.recv().await, Some(Event::Ready(1))) {
                    break;
                }
            }
        })
        .await;
        assert!(
            ready.is_ok(),
            "Ready must reach the UI within 100 ms, not at the next metrics tick"
        );
        assert_eq!(
            playback.2.load(Ordering::Relaxed),
            0,
            "Polling readiness must not sample metrics"
        );
        // Keep draining the bounded UI queue while virtual time advances.
        tokio::time::timeout(Duration::from_millis(1100), async {
            loop {
                events.recv().await.unwrap();
            }
        })
        .await
        .unwrap_err();
        assert_eq!(
            playback.2.load(Ordering::Relaxed),
            1,
            "Metrics must retain their one-second cadence"
        );
        drop(control);
    };
    tokio::join!(worker, drive);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn torrent_session_reuses_networking_but_reopens_when_the_cache_budget_changes() {
    let directory = tempfile::tempdir().unwrap();
    let folder = directory.path().join("cache");
    let mut session = None;
    torrent_session(&mut session, folder.clone(), 1024)
        .await
        .unwrap();
    torrent_session(&mut session, folder.clone(), 1024)
        .await
        .unwrap();
    assert_eq!(session.as_ref().unwrap().cache_budget(), 1024);
    torrent_session(&mut session, folder.clone(), 2048)
        .await
        .unwrap();
    assert_eq!(session.as_ref().unwrap().cache_budget(), 2048);
    assert!(
        Torrents::clear_cache(folder.clone()).await.is_err(),
        "An active session must retain exclusive cache ownership"
    );
    drop(session);
    Torrents::clear_cache(folder).await.unwrap();
}

fn cached_stream(data: &std::path::Path) -> Stream {
    // A ten-byte local torrent fixture, including its SHA-1 piece and info hashes.
    // Keeping this literal avoids adding a torrent engine dependency to app tests.
    let hash = "62e35fe54397bff5fe9e39a7920b2080f19c040f";
    let entry = data.join("torrent-cache/v1").join(hash);
    std::fs::create_dir_all(entry.join("data")).unwrap();
    std::fs::write(entry.join("data/fixture.mp4"), b"0123456789").unwrap();
    std::fs::write(entry.join("metadata.torrent"),
        b"d4:infod6:lengthi10e4:name11:fixture.mp412:piece lengthi16384e6:pieces20:\x87\xac\xec\x17\xcd\x9d\xcd\x20\xa7\x16\xcc\x2c\xf6\x74\x17\xb7\x1c\x8a\x70\x16ee").unwrap();
    serde_json::from_value(serde_json::json!({"infoHash":hash,"fileIdx":0})).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn torrent_switches_keep_the_server_but_explicit_stop_releases_the_cache() {
    let directory = tempfile::tempdir().unwrap();
    let stream = cached_stream(directory.path());
    let playback = FakePlayback::default();
    let (tx, mut events) = mpsc::channel(16);
    let inbox = Inbox {
        tx,
        wake: Arc::new(|| {}),
    };
    let (control, lifecycle) = watch::channel(None);
    let (_commands, commands) = mpsc::channel(1);
    let (_stats, stats) = watch::channel(None);
    let worker = player_loop(
        lifecycle,
        commands,
        stats,
        inbox,
        directory.path().into(),
        &playback,
    );
    let drive = async {
        for id in 1..=2 {
            control
                .send(Some(Control::Play(
                    id,
                    PlaybackTarget::NativeWindow(123),
                    Source::Stream(stream.clone()),
                )))
                .unwrap();
            loop {
                match events.recv().await.unwrap() {
                    Event::Playing(generation, _) if generation == id => break,
                    Event::Stopped(_, error) => {
                        assert!(error.is_none(), "Cached playback must succeed: {error:?}");
                    }
                    _ => {}
                }
            }
        }
        control.send(Some(Control::Stop(3))).unwrap();
        loop {
            if matches!(events.recv().await, Some(Event::Stopped(3, None))) {
                break;
            }
        }
        Torrents::clear_cache(directory.path().join("torrent-cache"))
            .await
            .unwrap();
        drop(control);
    };
    tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(worker, drive);
    })
    .await
    .unwrap();
    let recorded = playback.0.lock().unwrap();
    let urls: Vec<_> = recorded
        .iter()
        .filter_map(|event| match event {
            Recorded::Loaded(url) => Some(url::Url::parse(url).unwrap()),
            _ => None,
        })
        .collect();
    assert_eq!(
        urls.len(),
        2,
        "Both torrent selections must reach the player"
    );
    let mut urls = urls.iter();
    let first = urls.next().unwrap();
    let second = urls.next().unwrap();
    assert_eq!(
        first.origin(),
        second.origin(),
        "Torrent switches must retain the local HTTP listener"
    );
    assert_eq!(
        first.path_segments().unwrap().next(),
        second.path_segments().unwrap().next(),
        "Torrent switches must retain the session, not rebuild a server on the same port"
    );
}

impl Drop for FakeEngine {
    fn drop(&mut self) {
        self.0.record(Recorded::Dropped);
    }
}

#[tokio::test]
async fn injected_factory_failure_stops_the_request_without_falling_back_to_mpv() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("fixture.mp4");
    std::fs::write(&source, []).unwrap();
    let playback = FakePlayback::default();
    let (tx, mut events) = mpsc::channel(1);
    let inbox = Inbox {
        tx,
        wake: Arc::new(|| {}),
    };
    let (control, lifecycle) = watch::channel(None);
    let (_commands, commands) = mpsc::channel(1);
    let (_stats, stats) = watch::channel(None);
    control
        .send(Some(Control::Play(
            9,
            PlaybackTarget::DecodeOnly,
            Source::Local(source),
        )))
        .unwrap();
    let worker = player_loop(
        lifecycle,
        commands,
        stats,
        inbox,
        directory.path().into(),
        &playback,
    );
    let drive = async {
        let event = events.recv().await.unwrap();
        assert!(
            matches!(event, Event::Stopped(9, Some(error)) if error == "Unexpected output target")
        );
        drop(control);
    };
    tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(worker, drive);
    })
    .await
    .unwrap();
    assert!(
        playback.0.lock().unwrap().is_empty(),
        "A rejected target must not create an engine"
    );
}

#[tokio::test]
async fn injected_engine_receives_target_source_and_controls_and_is_released_on_stop() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("fixture.mp4");
    std::fs::write(&source, []).unwrap();
    let expected_url = url::Url::from_file_path(source.canonicalize().unwrap())
        .unwrap()
        .to_string();
    let playback = FakePlayback::default();
    let (tx, mut events) = mpsc::channel(16);
    let inbox = Inbox {
        tx,
        wake: Arc::new(|| {}),
    };
    let (control, lifecycle) = watch::channel(None);
    let (commands, command_rx) = mpsc::channel(1);
    let (_stats, stats) = watch::channel(None);
    control
        .send(Some(Control::Play(
            7,
            PlaybackTarget::NativeWindow(123),
            Source::Local(source),
        )))
        .unwrap();

    let worker = player_loop(
        lifecycle,
        command_rx,
        stats,
        inbox,
        directory.path().into(),
        &playback,
    );
    let drive = async {
        assert!(matches!(events.recv().await, Some(Event::Playing(7, _))));
        commands
            .send(Control::Command(
                7,
                vec!["set".into(), "pause".into(), "yes".into()],
            ))
            .await
            .unwrap();
        // A FIFO command-channel stop follows the pause, rather than overtaking it
        // through the latest-value lifecycle channel.
        commands.send(Control::Stop(7)).await.unwrap();
        loop {
            if let Event::Stopped(id, error) = events.recv().await.unwrap() {
                assert_eq!(id, 7);
                assert!(error.is_none());
                break;
            }
        }
        drop(control);
    };
    tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(worker, drive);
    })
    .await
    .unwrap();
    let recorded = playback.0.lock().unwrap();
    assert!(recorded.contains(&Recorded::Opened(123)));
    assert!(recorded.contains(&Recorded::Loaded(expected_url)));
    assert!(recorded.contains(&Recorded::Command(vec![
        "set".into(),
        "pause".into(),
        "yes".into()
    ])));
    let stop = recorded
        .iter()
        .position(|event| *event == Recorded::Command(vec!["stop".into()]))
        .unwrap();
    let dropped = recorded
        .iter()
        .position(|event| *event == Recorded::Dropped)
        .unwrap();
    assert!(
        stop < dropped,
        "Stop must reach the engine before it is destroyed"
    );
}
