//! Bounded, privacy-safe decode-only probes and offline JSONL analysis.
use anyhow::{Result, bail, ensure};
use scry_playback::{
    PlaybackEvent, PlaybackTarget, Player,
    telemetry::{Report, Sample, Summary, Tracker},
};
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const LIMITATIONS: &str = "Decode-only: software decoding, null video/audio output. Not renderer, GPU, display, or audio latency. Startup/stalls are sampled at 1 Hz; missing counters are unknown, not zero. Memory is process-wide. Drops/sec is not a dropped-frame percentage.";

#[derive(Debug, PartialEq)]
enum Mode {
    Probe(String),
    SelfTest,
    Analyze(PathBuf),
}

#[derive(Debug)]
struct Options {
    mode: Mode,
    seconds: f64,
    report: PathBuf,
    baseline: Option<PathBuf>,
}

impl Options {
    fn parse(args: &[String]) -> Result<Self> {
        let mut mode = None;
        let mut seconds = None;
        let mut report = None;
        let mut baseline = None;
        let mut i = 0;
        while i < args.len() {
            let flag = args[i].as_str();
            if flag == "--self-test" {
                ensure!(mode.is_none(), "Choose exactly one diagnostic mode");
                mode = Some(Mode::SelfTest);
                i += 1;
                continue;
            }
            ensure!(
                [
                    "--probe",
                    "--analyze",
                    "--seconds",
                    "--report",
                    "--baseline"
                ]
                .contains(&flag),
                "Unknown diagnostic option"
            );
            let value = args
                .get(i + 1)
                .filter(|v| !v.is_empty() && !v.starts_with("--"))
                .ok_or_else(|| anyhow::anyhow!("Diagnostic option needs a value"))?;
            match flag {
                "--probe" | "--analyze" => {
                    ensure!(mode.is_none(), "Choose exactly one diagnostic mode");
                    mode = Some(if flag == "--probe" {
                        Mode::Probe(value.clone())
                    } else {
                        Mode::Analyze(value.into())
                    });
                }
                "--seconds" => {
                    ensure!(seconds.is_none(), "Duplicate duration option");
                    let n = value
                        .parse::<f64>()
                        .map_err(|_| anyhow::anyhow!("Duration must be a finite number"))?;
                    ensure!(
                        n.is_finite() && (1.0..=3600.0).contains(&n),
                        "Duration must be between 1 and 3600 seconds"
                    );
                    seconds = Some(n);
                }
                "--report" => {
                    ensure!(report.is_none(), "Duplicate report option");
                    report = Some(PathBuf::from(value));
                }
                "--baseline" => {
                    ensure!(baseline.is_none(), "Duplicate baseline option");
                    baseline = Some(PathBuf::from(value));
                }
                _ => bail!("Unknown diagnostic option"),
            }
            i += 2;
        }
        let mode =
            mode.ok_or_else(|| anyhow::anyhow!("Use --probe URL, --self-test, or --analyze PATH"))?;
        if matches!(mode, Mode::Analyze(_)) {
            ensure!(
                seconds.is_none() && report.is_none(),
                "Analysis does not accept --seconds or --report"
            );
        } else {
            ensure!(baseline.is_none(), "--baseline requires --analyze");
        }
        let seconds = seconds.unwrap_or(if matches!(mode, Mode::SelfTest) {
            5.0
        } else {
            30.0
        });
        ensure!(
            !matches!(mode, Mode::SelfTest) || seconds <= 120.0,
            "Self-test duration must not exceed 120 seconds"
        );
        Ok(Self {
            mode,
            seconds,
            report: report.unwrap_or_else(|| "scry-probe.jsonl".into()),
            baseline,
        })
    }
}

/// Accept only diagnostic arguments, without the executable name.
#[expect(
    clippy::print_stdout,
    clippy::cast_precision_loss,
    reason = "This diagnostic CLI prints approximate human-readable memory deltas, not exact byte counts"
)]
pub fn run(args: &[String]) -> Result<()> {
    let options = Options::parse(args)?;
    match &options.mode {
        Mode::Analyze(path) => {
            let analysis = analyze(path)?;
            print_analysis(&analysis);
            if let Some(path) = &options.baseline {
                let baseline = analyze(path)?;
                ensure!(
                    analysis.mode == baseline.mode,
                    "Cannot compare native playback with decode-only probes"
                );
                println!("Baseline deltas (current minus baseline):");
                if let (Some(a), Some(b)) = (analysis.summary.startup_s, baseline.summary.startup_s)
                {
                    println!("  startup: {:+.3} s", a - b);
                }
                println!(
                    "  stalls: {:+}; peak resident: {:+.2} MiB; peak private: {:+.2} MiB",
                    i128::from(analysis.summary.stalls) - i128::from(baseline.summary.stalls),
                    (analysis.summary.peak_resident_bytes as f64
                        - baseline.summary.peak_resident_bytes as f64)
                        / 1_048_576.0,
                    (analysis.summary.peak_private_bytes as f64
                        - baseline.summary.peak_private_bytes as f64)
                        / 1_048_576.0
                );
                println!(
                    "Compare only matching media, durations, builds, and machines; these are not pass/fail thresholds."
                );
            }
            if analysis.mode == "decode-only" {
                println!("{LIMITATIONS}");
            } else {
                println!(
                    "Native playback: includes the app and embedded player. Process memory excludes dedicated GPU memory; sampled progress does not prove display or audio quality."
                );
            }
            ensure!(
                !analysis.summary.error,
                "Report records a failed playback session"
            );
            Ok(())
        }
        _ => probe(&options),
    }
}

#[expect(
    clippy::print_stdout,
    reason = "This diagnostic CLI explicitly prints its probe result"
)]
fn probe(options: &Options) -> Result<()> {
    if let Mode::Probe(source) = &options.mode {
        validate_report_destination(source, &options.report)?;
    }
    // Once created, finish is attempted even if fixture creation, initialization, or load fails.
    let mut report = Report::create(&options.report, "decode-only")?;
    let result = (|| -> Result<()> {
        let fixture = if matches!(options.mode, Mode::SelfTest) {
            Some(Fixture::create(options.seconds)?)
        } else {
            None
        };
        let source = match (&options.mode, &fixture) {
            (Mode::Probe(source), _) => source.clone(),
            (_, Some(fixture)) => fixture.path.to_string_lossy().into_owned(),
            _ => bail!("Diagnostic mode has no playback source"),
        };
        let player = Player::open(PlaybackTarget::DecodeOnly)?;
        let started = Instant::now();
        let deadline = started + Duration::from_secs_f64(options.seconds);
        report.sample(&player.sample(report.elapsed()))?;
        player
            .load(&source)
            .map_err(|_| anyhow::anyhow!("Playback load command failed"))?;
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
            if now >= next_sample || now >= deadline {
                report.sample(&player.sample(report.elapsed()))?;
                next_sample = now + Duration::from_secs(1);
            }
            if now >= deadline {
                break;
            }
            let timeout = deadline
                .min(next_sample)
                .saturating_duration_since(Instant::now())
                .as_secs_f64()
                .min(0.1);
            match player.event(timeout) {
                Some(PlaybackEvent::Loaded | PlaybackEvent::Ready) => {
                    // Capture the initial position so sub-second clips can show progress.
                    report.sample(&player.sample(report.elapsed()))?;
                }
                Some(PlaybackEvent::Failed) => bail!("Playback failed before the probe deadline"),
                Some(PlaybackEvent::Shutdown) => {
                    bail!("Player shut down before the probe deadline")
                }
                Some(PlaybackEvent::Ended) => {
                    let mut sample = player.sample(report.elapsed());
                    // mpv can clear time-pos when unloading. Retain the last observed
                    // position (polled at most 100 ms apart), not a guessed duration.
                    sample.position_s = sample.position_s.or(last_position);
                    report.sample(&sample)?;
                    break;
                }
                _ => {}
            }
        }
        ensure!(
            report.tracker.summary.playback_advanced_s > 0.05,
            "No advancing playback was measured before the probe ended"
        );
        // Explicitly destroy the player before the fixture is removed (Windows file sharing).
        drop(player);
        drop(fixture);
        Ok(())
    })();
    let finished = report.finish(result.is_err());
    match (result, finished) {
        (Err(error), Ok(_)) => Err(error),
        (Err(_), Err(_)) => bail!("Probe failed and its final report could not be written"),
        (Ok(()), Err(_)) => bail!("Probe completed but its final report could not be written"),
        (Ok(()), Ok(summary)) => {
            println!(
                "Probe complete: {} samples, {:.3} s advancing playback, {} stalls.",
                summary.samples, summary.playback_advanced_s, summary.stalls
            );
            println!("{LIMITATIONS}");
            Ok(())
        }
    }
}

fn validate_report_destination(source: &str, report: &Path) -> Result<()> {
    let local = if Path::new(source).is_file() {
        Some(PathBuf::from(source))
    } else {
        url::Url::parse(source)
            .ok()
            .filter(|url| url.scheme() == "file")
            .and_then(|url| url.to_file_path().ok())
    };
    if let Some(local) = local
        && local.exists()
        && report.exists()
    {
        let same = same_file::is_same_file(local, report)
            .map_err(|_| anyhow::anyhow!("Cannot verify report destination"))?;
        ensure!(!same, "Report must not overwrite the media source");
    }
    Ok(())
}

struct Fixture {
    path: PathBuf,
}
impl Fixture {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Validated self-test duration is finite, positive, and bounded to 120 seconds"
    )]
    fn create(seconds: f64) -> Result<Self> {
        let fixture = Self {
            path: std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .map(|p| p.join("Temp/opencode"))
                .filter(|p| p.is_dir())
                .unwrap_or_else(std::env::temp_dir)
                .join(format!("scry-self-test-{}.y4m", uuid::Uuid::new_v4())),
        };
        let mut file = BufWriter::new(File::create(&fixture.path)?);
        // Original synthetic 32x24 YUV420 at 12 fps. No downloaded or encoded media.
        file.write_all(b"YUV4MPEG2 W32 H24 F12:1 Ip A1:1 C420jpeg\n")?;
        let mut frame = vec![128u8; 32 * 24 * 3 / 2];
        for n in 0..((seconds + 2.0) * 12.0).ceil() as usize {
            frame[..32 * 24].fill(32 + u8::try_from(n % 180)?);
            file.write_all(b"FRAME\n")?;
            file.write_all(&frame)?;
        }
        file.flush()?;
        Ok(fixture)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

struct Analysis {
    mode: String,
    summary: Summary,
    elapsed_s: f64,
    decoder_drops: Option<u64>,
    frame_drops: Option<u64>,
    resident_known: bool,
    private_known: bool,
    cpu_known: bool,
}

fn analyze(path: &Path) -> Result<Analysis> {
    let file = File::open(path).map_err(|_| anyhow::anyhow!("Cannot open diagnostic report"))?;
    analyze_reader(BufReader::new(file))
}

fn analyze_reader(reader: impl BufRead) -> Result<Analysis> {
    let mut tracker = Tracker::default();
    let mut summary = None;
    let mut session = false;
    let mut mode = String::new();
    let mut elapsed_s = 0.0;
    let mut decoder_drops: Option<u64> = None;
    let mut frame_drops: Option<u64> = None;
    let mut resident_known = false;
    let mut private_known = false;
    let mut cpu_known = false;
    let mut previous_cpu_known = false;
    for line in reader.lines() {
        let line = line.map_err(|_| anyhow::anyhow!("Cannot read diagnostic report"))?;
        ensure!(line.len() <= 65536, "Diagnostic report line is too large");
        let value: serde_json::Value =
            serde_json::from_str(&line).map_err(|_| anyhow::anyhow!("Invalid diagnostic JSONL"))?;
        ensure!(summary.is_none(), "Unexpected data after report summary");
        match value["event"].as_str() {
            Some("session") => {
                ensure!(
                    !session && tracker.summary.samples == 0,
                    "Duplicate or misplaced session record"
                );
                ensure!(
                    value["schema"] == 1
                        && matches!(value["mode"].as_str(), Some("decode-only" | "native")),
                    "Expected schema 1 native or decode-only report"
                );
                value["mode"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Report mode is missing"))?
                    .clone_into(&mut mode);
                session = true;
            }
            Some("sample") => {
                ensure!(session, "Sample precedes session record");
                let sample: Sample = serde_json::from_value(value["metrics"].clone())
                    .map_err(|_| anyhow::anyhow!("Invalid sample metrics"))?;
                ensure!(
                    sample.elapsed_s.is_finite()
                        && sample.elapsed_s >= elapsed_s
                        && sample.elapsed_s >= 0.0,
                    "Sample times must be finite and monotonic"
                );
                elapsed_s = sample.elapsed_s;
                decoder_drops = match (decoder_drops, sample.decoder_drops) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    (a, b) => a.or(b),
                };
                frame_drops = match (frame_drops, sample.dropped_frames) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    (a, b) => a.or(b),
                };
                resident_known |= sample.resident_bytes.is_some();
                private_known |= sample.private_bytes.is_some();
                cpu_known |= previous_cpu_known && sample.cpu_time_s.is_some();
                previous_cpu_known = sample.cpu_time_s.is_some();
                tracker.observe(&sample);
            }
            Some("summary") => {
                ensure!(session, "Summary precedes session record");
                summary = Some(
                    serde_json::from_value::<Summary>(value["metrics"].clone())
                        .map_err(|_| anyhow::anyhow!("Invalid summary metrics"))?,
                );
            }
            _ => bail!("Unknown diagnostic record type"),
        }
    }
    let summary =
        summary.ok_or_else(|| anyhow::anyhow!("Report is incomplete: final summary is missing"))?;
    tracker.summary.error = summary.error;
    ensure!(
        serde_json::to_value(&tracker.summary)? == serde_json::to_value(&summary)?,
        "Report summary does not match its samples"
    );
    Ok(Analysis {
        mode,
        summary,
        elapsed_s,
        decoder_drops,
        frame_drops,
        resident_known,
        private_known,
        cpu_known,
    })
}

fn optional(value: Option<f64>, unit: &str) -> String {
    value.map_or_else(|| "unknown".into(), |v| format!("{v:.3} {unit}"))
}

#[expect(
    clippy::print_stdout,
    clippy::cast_precision_loss,
    reason = "Terminal output formats approximate memory sizes and rates for human reading"
)]
fn print_analysis(analysis: &Analysis) {
    let s = &analysis.summary;
    println!(
        "Report ({}): {} samples; error={}; elapsed {:.3} s",
        analysis.mode, s.samples, s.error, analysis.elapsed_s
    );
    println!(
        "Startup: {}; stalls: {}; buffering: {:.3} s; advancing playback: {:.3} s",
        optional(s.startup_s, "s"),
        s.stalls,
        s.buffering_s,
        s.playback_advanced_s
    );
    println!(
        "Peak resident: {}; peak private commit: {}",
        optional(
            analysis
                .resident_known
                .then_some(s.peak_resident_bytes as f64 / 1_048_576.0),
            "MiB"
        ),
        optional(
            analysis
                .private_known
                .then_some(s.peak_private_bytes as f64 / 1_048_576.0),
            "MiB"
        )
    );
    println!(
        "Sampled process CPU: {} (summed across cores)",
        optional(analysis.cpu_known.then_some(s.cpu_time_s), "CPU-s")
    );
    let rate = |n: Option<u64>| {
        if analysis.elapsed_s > 0.0 {
            n.map(|n| n as f64 / analysis.elapsed_s)
        } else {
            None
        }
    };
    println!(
        "Frame drops: {}; decoder drops: {}",
        optional(rate(analysis.frame_drops), "drops/s"),
        optional(rate(analysis.decoder_drops), "drops/s")
    );
}

#[cfg(test)]
#[expect(
    clippy::float_cmp,
    reason = "Fixtures use exactly representable durations and integral progress values"
)]
#[expect(
    clippy::unwrap_used,
    reason = "Test fixtures and asserted successful results must fail the test on error"
)]
mod tests {
    use super::*;

    #[test]
    fn a_probe_cannot_truncate_its_media_through_a_path_or_file_url() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("media.mp4");
        let alias = directory.path().join("report.jsonl");
        fs::write(&source, b"original media").unwrap();
        fs::hard_link(&source, &alias).unwrap();
        for input in [
            source.to_string_lossy().into_owned(),
            url::Url::from_file_path(&source).unwrap().to_string(),
        ] {
            let options = Options {
                mode: Mode::Probe(input),
                seconds: 1.0,
                report: alias.clone(),
                baseline: None,
            };
            assert!(probe(&options).is_err());
            assert_eq!(fs::read(&source).unwrap(), b"original media");
        }
    }
    fn parse(args: &[&str]) -> Result<Options> {
        Options::parse(&args.iter().map(ToString::to_string).collect::<Vec<_>>())
    }

    #[test]
    fn rejects_unbounded_and_ambiguous_arguments_without_echoing_sources() {
        for seconds in ["NaN", "inf", "-1", "0", "3601"] {
            assert!(parse(&["--probe", "private-token", "--seconds", seconds]).is_err());
        }
        assert!(parse(&["--self-test", "--probe", "private-token"]).is_err());
        assert!(parse(&["--self-test", "--seconds", "121"]).is_err());
        assert!(parse(&["--analyze", "file", "--seconds", "1"]).is_err());
        assert!(parse(&["--self-test", "--baseline", "file"]).is_err());
        let error = parse(&["--probe", "private-token", "--bad"])
            .unwrap_err()
            .to_string();
        assert!(!error.contains("private-token"));
    }

    #[test]
    fn parses_modes_and_probe_options() {
        let options = parse(&["--probe", "source", "--seconds", "2.5", "--report", "out"]).unwrap();
        assert_eq!(options.mode, Mode::Probe("source".into()));
        assert_eq!(options.seconds, 2.5);
        assert_eq!(options.report, PathBuf::from("out"));
        assert_eq!(parse(&["--self-test"]).unwrap().seconds, 5.0);
        assert_eq!(
            parse(&["--analyze", "out", "--baseline", "before"])
                .unwrap()
                .baseline,
            Some("before".into())
        );
    }

    fn report_text() -> String {
        let mut tracker = Tracker::default();
        let mut lines = vec![
            serde_json::json!({"event":"session","schema":1,"mode":"decode-only"}).to_string(),
        ];
        for (elapsed_s, position_s) in [(0.0, 0.0), (1.0, 1.0), (2.0, 2.0)] {
            let sample = Sample {
                elapsed_s,
                position_s: Some(position_s),
                ..Sample::default()
            };
            tracker.observe(&sample);
            lines.push(serde_json::json!({"event":"sample","metrics":sample}).to_string());
        }
        lines.push(serde_json::json!({"event":"summary","metrics":tracker.summary}).to_string());
        lines.join("\n")
    }

    #[test]
    fn analysis_checks_summary_and_preserves_unknown_counters() {
        let text = report_text();
        let analysis = analyze_reader(text.as_bytes()).unwrap();
        assert_eq!(analysis.summary.startup_s, Some(1.0));
        assert_eq!(analysis.summary.playback_advanced_s, 2.0);
        assert_eq!(analysis.frame_drops, None);
        assert!(!analysis.resident_known);
        assert!(
            analyze_reader(text.replace("\"samples\":3", "\"samples\":99").as_bytes()).is_err()
        );
        assert!(
            analyze_reader(
                text.lines()
                    .take(4)
                    .collect::<Vec<_>>()
                    .join("\n")
                    .as_bytes()
            )
            .is_err()
        );
        assert!(analyze_reader(format!("{text}\n{{}}\n").as_bytes()).is_err());
    }

    #[test]
    fn native_reports_use_the_same_verified_metrics_without_being_decode_only() {
        let text = report_text().replace("decode-only", "native");
        let analysis = analyze_reader(text.as_bytes()).unwrap();
        assert_eq!(analysis.mode, "native");
        assert_eq!(analysis.summary.playback_advanced_s, 2.0);
        assert!(analyze_reader(text.replace("native", "unknown-mode").as_bytes()).is_err());
    }
}
