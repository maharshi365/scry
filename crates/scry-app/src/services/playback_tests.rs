use super::*;
use std::sync::Mutex as StdMutex;

#[derive(Debug, PartialEq)]
enum Recorded {
    Opened(u64),
    Loaded(String),
    Command(Vec<String>),
    Dropped,
}

#[derive(Clone, Default)]
struct FakePlayback(Arc<StdMutex<Vec<Recorded>>>);

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
        None
    }

    fn sample(&self, _: f64) -> Sample {
        Sample::default()
    }
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
