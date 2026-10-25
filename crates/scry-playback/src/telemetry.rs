use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    time::Instant,
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Sample {
    pub elapsed_s: f64,
    pub position_s: Option<f64>,
    pub duration_s: Option<f64>,
    pub cache_s: Option<f64>,
    #[cfg(feature = "profiling")]
    pub cache_forward_bytes: Option<f64>,
    #[cfg(feature = "profiling")]
    pub cache_limit_bytes: Option<u64>,
    pub buffering: bool,
    pub paused: bool,
    pub dropped_frames: Option<u64>,
    pub decoder_drops: Option<u64>,
    pub av_sync_s: Option<f64>,
    pub speed: Option<f64>,
    pub video_bitrate: Option<f64>,
    pub hw_decoder: Option<String>,
    pub resident_bytes: Option<u64>,
    pub private_bytes: Option<u64>,
    /// Cumulative process CPU time, summed across cores (not percent).
    pub cpu_time_s: Option<f64>,
}

impl Sample {
    /// Actual playback advancement, excluding seeks and pause/buffer transitions.
    #[must_use]
    pub fn playback_advance_since(&self, previous: &Self) -> f64 {
        if self.paused || previous.paused || self.buffering || previous.buffering {
            return 0.0;
        }
        let (Some(before), Some(after)) = (previous.position_s, self.position_s) else {
            return 0.0;
        };
        let delta = after - before;
        let elapsed = (self.elapsed_s - previous.elapsed_s).max(0.0);
        let speed = self.speed.unwrap_or(1.0).clamp(0.1, 10.0);
        if delta.is_finite() && delta > 0.0 && delta <= elapsed * speed + 0.5 {
            delta
        } else {
            0.0
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Summary {
    pub samples: u64,
    pub startup_s: Option<f64>,
    pub buffering_s: f64,
    pub stalls: u64,
    pub peak_resident_bytes: u64,
    pub peak_private_bytes: u64,
    pub dropped_frames: u64,
    pub playback_advanced_s: f64,
    pub cpu_time_s: f64,
    pub error: bool,
}

#[derive(Default)]
pub struct Tracker {
    pub summary: Summary,
    previous: Option<Sample>,
    stall_s: f64,
    in_stall: bool,
}

impl Tracker {
    pub fn observe(&mut self, sample: &Sample) {
        let summary = &mut self.summary;
        summary.samples += 1;
        summary.peak_resident_bytes = summary
            .peak_resident_bytes
            .max(sample.resident_bytes.unwrap_or(0));
        summary.peak_private_bytes = summary
            .peak_private_bytes
            .max(sample.private_bytes.unwrap_or(0));
        summary.dropped_frames = summary
            .dropped_frames
            .max(sample.dropped_frames.unwrap_or(0));
        if summary.startup_s.is_none() && sample.position_s.is_some_and(|p| p > 0.05) {
            summary.startup_s = Some(sample.elapsed_s);
        }
        if let Some(previous) = &self.previous {
            let dt = (sample.elapsed_s - previous.elapsed_s).max(0.0);
            if let (Some(before), Some(after)) = (previous.cpu_time_s, sample.cpu_time_s) {
                summary.cpu_time_s += (after - before).max(0.0);
            }
            // Do not count an interval that crosses a pause/unpause transition.
            if !sample.paused && !previous.paused {
                if previous.buffering {
                    summary.buffering_s += dt;
                }
                if let (Some(before), Some(after)) = (previous.position_s, sample.position_s) {
                    let advance = after - before;
                    // Ignore seeks rather than treating them as playback progress.
                    if sample.playback_advance_since(previous) > 0.01 {
                        summary.playback_advanced_s += advance;
                        self.stall_s = 0.0;
                        self.in_stall = false;
                    } else if advance.abs() <= 0.01 && summary.startup_s.is_some() {
                        self.stall_s += dt;
                        if self.stall_s >= 2.0 && !self.in_stall {
                            summary.stalls += 1;
                            self.in_stall = true;
                        }
                    } else {
                        self.stall_s = 0.0;
                        self.in_stall = false;
                    }
                } else {
                    self.stall_s = 0.0;
                    self.in_stall = false;
                }
            } else {
                self.stall_s = 0.0;
                self.in_stall = false;
            }
        }
        self.previous = Some(sample.clone());
    }
}

pub struct Report {
    file: BufWriter<File>,
    pub tracker: Tracker,
    started: Instant,
}

impl Report {
    /// # Errors
    /// Returns an error if the report cannot be created or its session record cannot be written.
    pub fn create(path: &Path, mode: &str) -> Result<Self> {
        let mut file =
            BufWriter::new(File::create(path).context("Cannot create performance report")?);
        writeln!(
            file,
            "{}",
            serde_json::json!({"event":"session", "schema":1,"mode":mode,"app_version":env!("CARGO_PKG_VERSION")})
        )?;
        Ok(Self {
            file,
            tracker: Tracker::default(),
            started: Instant::now(),
        })
    }
    #[must_use]
    pub fn elapsed(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }
    /// # Errors
    /// Returns an error if the sample cannot be serialized or written to the report.
    pub fn sample(&mut self, sample: &Sample) -> Result<()> {
        self.tracker.observe(sample);
        writeln!(
            self.file,
            "{}",
            serde_json::json!({"event":"sample", "metrics":sample})
        )?;
        self.file.flush()?;
        Ok(())
    }
    /// # Errors
    /// Returns an error if the final summary cannot be written or flushed.
    pub fn finish(mut self, error: bool) -> Result<Summary> {
        self.tracker.summary.error = error;
        writeln!(
            self.file,
            "{}",
            serde_json::json!({"event":"summary", "metrics":self.tracker.summary})
        )?;
        self.file.flush()?;
        Ok(self.tracker.summary)
    }
}

#[must_use]
pub fn memory() -> (Option<u64>, Option<u64>) {
    crate::process::memory()
}

#[cfg(test)]
#[expect(
    clippy::float_cmp,
    reason = "Fixtures use exactly representable times and assert exact counter arithmetic"
)]
mod tests {
    use super::*;
    fn sample(t: f64, p: f64) -> Sample {
        Sample {
            elapsed_s: t,
            position_s: Some(p),
            ..Sample::default()
        }
    }
    #[test]
    fn stalled_intervals_count_once_and_recovery_resets() {
        let mut tracker = Tracker::default();
        for (t, p) in [
            (0., 0.),
            (1., 1.),
            (2., 1.),
            (3., 1.),
            (4., 1.),
            (5., 2.),
            (6., 2.),
            (7., 2.),
        ] {
            tracker.observe(&sample(t, p));
        }
        assert_eq!(tracker.summary.stalls, 2);
        assert_eq!(tracker.summary.startup_s, Some(1.));
        assert_eq!(tracker.summary.playback_advanced_s, 2.);
    }
    #[test]
    fn pauses_and_seeks_do_not_create_stalls_or_false_progress() {
        let mut tracker = Tracker::default();
        tracker.observe(&sample(0., 1.));
        for t in 1..5 {
            tracker.observe(&Sample {
                paused: true,
                ..sample(f64::from(t), 1.)
            });
        }
        tracker.observe(&sample(5., 1.));
        tracker.observe(&sample(6., 100.));
        assert_eq!(tracker.summary.stalls, 0);
        assert_eq!(tracker.summary.playback_advanced_s, 0.);
    }
    #[test]
    fn buffering_time_uses_previous_state_and_memory_uses_peak() {
        let mut tracker = Tracker::default();
        tracker.observe(&Sample {
            buffering: true,
            private_bytes: Some(400),
            ..sample(0., 0.)
        });
        tracker.observe(&Sample {
            private_bytes: Some(200),
            ..sample(1., 1.)
        });
        assert_eq!(tracker.summary.buffering_s, 1.);
        assert_eq!(tracker.summary.peak_private_bytes, 400);
    }

    #[test]
    fn missing_positions_break_a_stall_interval() {
        let mut tracker = Tracker::default();
        for point in [
            sample(0., 0.),
            sample(1., 1.),
            sample(2., 1.),
            Sample {
                elapsed_s: 3.,
                ..Sample::default()
            },
            sample(4., 1.),
            sample(5., 1.),
        ] {
            tracker.observe(&point);
        }
        assert_eq!(tracker.summary.stalls, 0);
    }

    #[test]
    fn playback_advancement_respects_speed_and_buffering_transitions() {
        for (speed, advance, buffering, expected) in [
            (0.5, 1.5, false, 0.0),
            (3.0, 3.0, false, 3.0),
            (1.0, 10.0, false, 0.0),
            (1.0, 1.0, true, 0.0),
        ] {
            let mut tracker = Tracker::default();
            tracker.observe(&Sample {
                buffering,
                ..sample(0.0, 1.0)
            });
            tracker.observe(&Sample {
                speed: Some(speed),
                ..sample(1.0, 1.0 + advance)
            });
            assert_eq!(
                tracker.summary.playback_advanced_s, expected,
                "speed={speed}, buffering={buffering}"
            );
        }
    }
}
