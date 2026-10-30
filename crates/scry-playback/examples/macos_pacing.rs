//! Local render-bridge timing probe; does not measure Rune presentation or network buffering.

#[cfg(target_os = "macos")]
#[expect(
    clippy::print_stdout,
    reason = "This diagnostic CLI emits numeric JSON only"
)]
fn main() -> anyhow::Result<()> {
    use anyhow::{Context, ensure};
    use scry_playback::{PlaybackEvent, PlaybackTarget, Player, macos::Surface};
    use std::{
        sync::{Arc, Mutex},
        time::{Duration, Instant},
    };

    let path = std::env::args_os()
        .nth(1)
        .context("Pass a local synthetic video fixture")?;
    ensure!(
        std::path::Path::new(&path).is_file(),
        "Fixture must be a local file"
    );
    let timestamps = Arc::new(Mutex::new(Vec::with_capacity(2048)));
    let output = timestamps.clone();
    let started = Instant::now();
    let surface = Surface::new(move |_, _, _| {
        if let Ok(mut timestamps) = output.lock()
            && timestamps.len() < 4096
            && started.elapsed() >= Duration::from_secs(1)
        {
            timestamps.push(Instant::now());
        }
    });
    surface.resize(1280, 720);
    let player = Player::open(PlaybackTarget::Macos(surface))?;
    let before = player.sample(0.0);
    player.command(&["set", "volume", "0"])?;
    player.load(path.to_str().context("Fixture path must be UTF-8")?)?;
    while started.elapsed() < Duration::from_secs(15) {
        match player.event(0.05) {
            Some(PlaybackEvent::Ended) => break,
            Some(PlaybackEvent::Failed | PlaybackEvent::Shutdown) => {
                anyhow::bail!("Fixture playback failed")
            }
            _ => {}
        }
    }
    let after = player.sample(started.elapsed().as_secs_f64());
    let timestamps = timestamps
        .lock()
        .map_err(|_| anyhow::anyhow!("Timing lock failed"))?;
    let mut intervals: Vec<_> = timestamps
        .windows(2)
        .filter_map(|pair| Some(pair.get(1)?.duration_since(*pair.first()?).as_secs_f64() * 1000.0))
        .collect();
    intervals.sort_by(f64::total_cmp);
    let percentile = |percent: usize| {
        intervals
            .get(intervals.len().saturating_sub(1) * percent / 100)
            .copied()
    };
    let span = timestamps
        .first()
        .zip(timestamps.last())
        .map(|(first, last)| last.duration_since(*first).as_secs_f64());
    let frames = u32::try_from(timestamps.len().saturating_sub(1))?;
    println!(
        "{}",
        serde_json::json!({
            "rendered_frames": frames,
            "delivery_fps": span.filter(|span| *span > 0.0).map(|span| f64::from(frames) / span),
            "interval_p50_ms": percentile(50), "interval_p95_ms": percentile(95),
            "interval_max_ms": intervals.last(),
            "dropped_frames": after.dropped_frames,
            "decoder_drops": after.decoder_drops,
            "cpu_time_s": after.cpu_time_s.zip(before.cpu_time_s).map(|(after, before)| after - before),
            "hw_decoder": after.hw_decoder,
        })
    );
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!("This probe requires macOS")
}
