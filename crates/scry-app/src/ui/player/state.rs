//! Playback state shared by player views and other features.
use scry_playback::telemetry::Sample;
use std::path::PathBuf;

#[expect(
    clippy::struct_excessive_bools,
    reason = "Playback readiness, window mode, and mute are independent state"
)]
pub(crate) struct PlayerState {
    pub error: Option<String>,
    pub status: String,
    pub playing: bool,
    pub ready: bool,
    pub fullscreen: bool,
    pub interaction: super::interaction::Interaction,
    pub background: Option<String>,
    pub logo: Option<String>,
    pub sample: Sample,
    pub stats_overlay: super::super::primitives::hover_overlay::HoverOverlay,
    pub subtitles: super::subtitles::Subtitles,
    pub audio: super::audio::Audio,
    pub torrent_source: bool,
    pub torrent_stats: Option<scry_torrents::Stats>,
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
            error: None,
            status: String::new(),
            playing: false,
            ready: false,
            fullscreen: false,
            interaction: super::interaction::Interaction::default(),
            background: None,
            logo: None,
            sample: Sample::default(),
            stats_overlay: super::super::primitives::hover_overlay::HoverOverlay::default(),
            subtitles: super::subtitles::Subtitles::default(),
            audio: super::audio::Audio::default(),
            torrent_source: false,
            torrent_stats: None,
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
