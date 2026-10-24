//! Playback state shared by player views and other features.
use scry_playback::telemetry::Sample;
use std::path::PathBuf;

#[expect(
    clippy::struct_excessive_bools,
    reason = "Playback readiness, window mode, and mute are independent state"
)]
pub(crate) struct PlayerState {
    pub playing: bool,
    pub ready: bool,
    pub fullscreen: bool,
    pub background: Option<String>,
    pub logo: Option<String>,
    pub sample: Sample,
    pub report: Option<PathBuf>,
    pub title: String,
    // Identity of the actual playback, not the last detail page visited.
    pub meta_id: Option<(String, String)>,
    pub video_id: Option<String>,
    pub volume: u32,
    pub muted: bool,
    pub seek_preview: Option<f64>,
    pub generation: u64,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            playing: false,
            ready: false,
            fullscreen: false,
            background: None,
            logo: None,
            sample: Sample::default(),
            report: None,
            title: String::new(),
            meta_id: None,
            video_id: None,
            volume: 70,
            muted: false,
            seek_preview: None,
            generation: 0,
        }
    }
}
