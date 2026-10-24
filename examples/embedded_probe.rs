//! Finite native-window playback smoke test. No services, browser, or visual checks.
use anyhow::{Result, bail, ensure};
use rune::{App, AppSpec, Cx, El, Tx, col, el, text};
use scry::{
    platform::{Host, VideoHost},
    playback::{PlaybackEvent, PlaybackTarget, Player},
    telemetry::{Report, Summary},
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

#[derive(Debug)]
struct Options {
    source: PathBuf,
    seconds: f64,
    report: PathBuf,
}

impl Options {
    fn parse(args: Vec<String>) -> Result<Self> {
        ensure!(
            args.len() <= 3,
            "Usage: embedded_probe [path] [seconds] [report]"
        );
        let seconds = args
            .get(1)
            .map(|s| s.parse::<f64>())
            .transpose()
            .map_err(|_| anyhow::anyhow!("Duration must be a finite number"))?
            .unwrap_or(8.0);
        ensure!(
            seconds.is_finite() && (1.0..=120.0).contains(&seconds),
            "Duration must be between 1 and 120 seconds"
        );
        Ok(Self {
            source: args
                .first()
                .map(PathBuf::from)
                .unwrap_or_else(|| "target/playback-fixture.mp4".into()),
            seconds,
            report: args
                .get(2)
                .map(PathBuf::from)
                .unwrap_or_else(|| "target/native-probe.jsonl".into()),
        })
    }
}

struct State {
    host: Host,
    options: Option<Options>,
    target: Option<mpsc::Receiver<Result<PlaybackTarget>>>,
    stop: mpsc::SyncSender<()>,
    commands: Option<mpsc::Receiver<()>>,
    events: Option<mpsc::Receiver<()>>,
    quit: rune::AppHandle<State>,
    outcome: Arc<Mutex<Option<Result<Summary>>>>,
    stopping: bool,
}

fn init() -> State {
    unreachable!("The example supplies its state with App::init")
}

fn setup(s: &mut State, wake: rune::Waker) {
    let options = s.options.take().unwrap();
    let target = s.target.take().unwrap();
    let commands = s.commands.take().unwrap();
    // Only completion crosses this bounded boundary; metrics go directly to JSONL.
    let (events, receiver) = mpsc::sync_channel(1);
    s.events = Some(receiver);
    let outcome = s.outcome.clone();
    let quit = s.quit.clone();
    let spawn = std::thread::Builder::new()
        .name("embedded-probe".into())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                probe(options, target, commands)
            }))
            .unwrap_or_else(|_| Err(anyhow::anyhow!("Native probe worker panicked")));
            // probe has returned, so Player::drop has finished before this acknowledgment.
            *outcome.lock().unwrap() = Some(result);
            if events.send(()).is_ok() {
                (wake)();
            }
            // Quit is a shell request, so it also works if close arrived before the
            // first visible frame and Rune never polls that hidden startup surface.
            quit.quit();
        });
    if spawn.is_err() {
        *s.outcome.lock().unwrap() = Some(Err(anyhow::anyhow!("Cannot start native probe worker")));
        s.quit.quit();
    }
}

fn poll(s: &mut State, _: f64, tx: &mut Tx) -> bool {
    if s.events
        .as_ref()
        .is_some_and(|events| events.try_recv().is_ok())
    {
        s.host.hide();
        tx.quit();
    } else if s.outcome.lock().unwrap().is_some() {
        tx.quit();
    }
    false // No continuously scheduled UI frames or idle polling.
}

fn on_close(s: &mut State, _: &mut Tx) {
    s.stopping = true;
    let _ = s.stop.try_send(());
    // Do not join or quit here. The Windows parent must still dispatch messages.
}

fn view(s: &State, cx: &mut Cx<State>) -> El {
    let scale = cx.env().scale;
    col()
        .w_full()
        .h_full()
        .padding(16.0)
        .gap(12.0)
        .child(text("Scry · embedded native playback probe"))
        .child(
            el().key("native-video")
                .w_full()
                .grow(1.0)
                .fill(rune::oklch(0.08, 0.0, 0.0))
                .on_layout(move |s: &mut State, rect| s.host.place(rect, scale)),
        )
        .child(text(if s.stopping {
            "Stopping; waiting for native player teardown…"
        } else {
            "Finite smoke test · metrics saved once per second"
        }))
        .child(
            el().padding(10.0)
                .child(text("Stop and exit"))
                .on_press(|s: &mut State| {
                    s.stopping = true;
                    let _ = s.stop.try_send(());
                }),
        )
}

// Rune can enter its first-frame close path before on_close is active. Intercept
// the native request instead: never hide/destroy the parent before mpv is gone.
#[cfg(windows)]
mod close_gate {
    use super::*;
    use rune::raw_window_handle::RawWindowHandle;
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        UI::WindowsAndMessaging::*,
    };
    static HOOK: std::sync::OnceLock<(isize, mpsc::SyncSender<()>)> = std::sync::OnceLock::new();

    unsafe extern "system" fn procedure(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        let (previous, stop) = HOOK.get().expect("Native close gate installed");
        if message == WM_CLOSE {
            let _ = stop.try_send(());
            return 0;
        }
        // SAFETY: previous is the WNDPROC returned when subclassing this parent.
        unsafe {
            CallWindowProcW(
                Some(std::mem::transmute::<
                    isize,
                    unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
                >(*previous)),
                hwnd,
                message,
                w,
                l,
            )
        }
    }

    pub fn install(window: &rune::WindowHandle, stop: mpsc::SyncSender<()>) -> Result<()> {
        window
            .with_raw_handle(|handle| {
                let RawWindowHandle::Win32(raw) = handle.as_raw() else {
                    bail!("Expected Windows parent");
                };
                // SAFETY: installation is on the owning UI thread. No callback can
                // run between SetWindowLongPtrW and storing its previous procedure.
                let previous = unsafe {
                    SetWindowLongPtrW(
                        raw.hwnd.get() as HWND,
                        GWLP_WNDPROC,
                        procedure as *const () as isize,
                    )
                };
                ensure!(previous != 0, "Cannot install native close gate");
                HOOK.set((previous, stop))
                    .map_err(|_| anyhow::anyhow!("Native close gate already installed"))?;
                Ok(())
            })
            .map_err(|_| anyhow::anyhow!("Cannot access native parent"))?
    }
}

fn probe(
    options: Options,
    target: mpsc::Receiver<Result<PlaybackTarget>>,
    commands: mpsc::Receiver<()>,
) -> Result<Summary> {
    // Resolve before creating the report so an accidental matching path cannot truncate media.
    let source = options
        .source
        .canonicalize()
        .map_err(|_| anyhow::anyhow!("Cannot open local fixture"));
    if let Ok(source) = &source
        && options.report.exists()
    {
        let same = same_file::is_same_file(source, &options.report)
            .map_err(|_| anyhow::anyhow!("Cannot verify report destination"))?;
        ensure!(!same, "Report must not overwrite the fixture");
    }
    let mut report = Report::create(&options.report, "native")
        .map_err(|_| anyhow::anyhow!("Cannot create native probe report"))?;
    let result = (|| -> Result<()> {
        let source = source?;
        let target = target
            .recv()
            .map_err(|_| anyhow::anyhow!("Native host disconnected"))??;
        ensure!(
            matches!(target, PlaybackTarget::NativeWindow(_)),
            "Expected a native video target"
        );
        ensure!(
            commands.try_recv().is_err(),
            "Native probe stopped before startup"
        );
        // Construct and destroy the !Send mpv handle on this worker, never the UI thread.
        let player =
            Player::open(target).map_err(|_| anyhow::anyhow!("Cannot initialize native player"))?;
        player
            .command(&["set", "volume", "0"])
            .map_err(|_| anyhow::anyhow!("Cannot mute native probe"))?;
        let started = Instant::now();
        let deadline = started + Duration::from_secs_f64(options.seconds);
        report.sample(&player.sample(report.elapsed()))?;
        ensure!(
            commands.try_recv().is_err(),
            "Native probe stopped during startup"
        );
        player
            .load(&source.to_string_lossy())
            .map_err(|_| anyhow::anyhow!("Fixture load command failed"))?;
        let mut next_sample = started + Duration::from_secs(1);
        let mut last_position = None;
        loop {
            if let Some(position) = player
                .property("time-pos")
                .and_then(|p| p.parse::<f64>().ok())
                .filter(|p| p.is_finite())
            {
                last_position = Some(position);
            }
            let now = Instant::now();
            let stopped = commands.try_recv().is_ok();
            if now >= next_sample || now >= deadline || stopped {
                report.sample(&player.sample(report.elapsed()))?;
                next_sample = now + Duration::from_secs(1);
            }
            if now >= deadline || stopped {
                break;
            }
            let timeout = deadline
                .min(next_sample)
                .saturating_duration_since(Instant::now())
                .as_secs_f64()
                .min(0.1);
            match player.event(timeout) {
                Some(PlaybackEvent::Loaded) => report.sample(&player.sample(report.elapsed()))?,
                Some(PlaybackEvent::Failed) => bail!("Native playback failed"),
                Some(PlaybackEvent::Shutdown) => bail!("Native player shut down early"),
                Some(PlaybackEvent::Ended) => {
                    let mut sample = player.sample(report.elapsed());
                    // EOF may clear mpv properties. Preserve an observed position, not duration.
                    sample.position_s = sample.position_s.or(last_position);
                    report.sample(&sample)?;
                    break;
                }
                None => {}
            }
        }
        ensure!(
            report.tracker.summary.playback_advanced_s > 0.05,
            "No advancing native playback measured"
        );
        drop(player);
        Ok(())
    })(); // Also drops Player on each error path, while Rune is still pumping messages.
    let finished = report
        .finish(result.is_err())
        .map_err(|_| anyhow::anyhow!("Cannot finish native probe report"));
    result?;
    finished
}

fn main() -> Result<()> {
    let options = Options::parse(std::env::args().skip(1).collect())?;
    let host = Host::default();
    let window_host = host.clone();
    let (target_tx, target) = mpsc::sync_channel(1);
    let (stop, commands) = mpsc::sync_channel(1);
    #[cfg(windows)]
    let native_stop = stop.clone();
    let outcome = Arc::new(Mutex::new(None));
    let result = outcome.clone();
    let app = App::new(AppSpec {
        setup: Some(setup),
        poll: Some(poll),
        ..AppSpec::new("Scry native probe", (960.0, 640.0), init, view)
    })
    .keep_running();
    let quit = app.handle();
    app.args(Vec::<String>::new())
        .init(move || State {
            host,
            options: Some(options),
            target: Some(target),
            stop,
            commands: Some(commands),
            events: None,
            outcome,
            quit,
            stopping: false,
        })
        .on_window(move |window| {
            let target = (|| {
                #[cfg(windows)]
                close_gate::install(window, native_stop.clone())?;
                window_host.attach(window)?;
                window_host.target()
            })()
            .map_err(|_| anyhow::anyhow!("Cannot attach native video host"));
            let _ = target_tx.try_send(target);
        })
        .on_close(on_close)
        .run();
    let summary =
        result.lock().unwrap().take().ok_or_else(|| {
            anyhow::anyhow!("Native probe exited without worker acknowledgment")
        })??;
    println!(
        "Native probe complete: {} samples; {:.3} s advancing playback; {} stalls; {} dropped frames.",
        summary.samples, summary.playback_advanced_s, summary.stalls, summary.dropped_frames
    );
    println!(
        "Sampled progress is not proof of display or audio quality. Memory is process-wide, excluding dedicated GPU memory."
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_alias_cannot_overwrite_the_fixture() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("fixture.mp4");
        let report = directory.path().join("report.jsonl");
        std::fs::write(&source, b"original media").unwrap();
        std::fs::hard_link(&source, &report).unwrap();
        let (sender, target) = mpsc::channel();
        drop(sender);
        let (_stop, commands) = mpsc::sync_channel(1);
        assert!(
            probe(
                Options {
                    source: source.clone(),
                    seconds: 1.0,
                    report
                },
                target,
                commands
            )
            .is_err()
        );
        assert_eq!(std::fs::read(source).unwrap(), b"original media");
    }

    #[test]
    fn duration_is_finite_and_bounded_without_echoing_input() {
        for duration in ["NaN", "inf", "0", "-1", "121", "private-input"] {
            let error = Options::parse(vec!["private-source".into(), duration.into()]).unwrap_err();
            assert!(!error.to_string().contains("private-source"));
            assert!(!error.to_string().contains("private-input"));
        }
        assert_eq!(Options::parse(vec![]).unwrap().seconds, 8.0);
        assert_eq!(
            Options::parse(vec!["fixture".into(), "120".into()])
                .unwrap()
                .seconds,
            120.0
        );
    }
}
