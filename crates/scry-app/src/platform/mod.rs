//! OS boundary. Shared UI and services must not import Win32 or Cocoa APIs.
use anyhow::Result;
use scry_playback::PlaybackTarget;

#[derive(Clone, Copy)]
pub struct VideoClick {
    pub at: std::time::Instant,
    pub position: [f64; 2],
}

pub enum VideoPointer {
    Unavailable,
    Outside,
    Inside([f64; 2]),
}

#[derive(Clone, Copy)]
pub struct VideoOverlay {
    pub rect: [f32; 4],
    pub radius: f32,
}
mod browser;
mod navigation;
pub use browser::open_browser;
pub use navigation::{HistoryDirection, NavigationInput};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::Host;
#[cfg(not(any(windows, target_os = "macos")))]
mod unsupported;
#[cfg(windows)]
mod windows;
#[cfg(not(any(windows, target_os = "macos")))]
pub use unsupported::Host;
#[cfg(windows)]
pub use windows::Host;

pub trait VideoHost: Clone + Default {
    /// # Errors
    /// Returns an error if the native parent or embedded video window is unavailable.
    fn attach(&self, parent: &rune::WindowHandle) -> Result<()>;
    /// # Errors
    /// Returns an error if no video window has been attached.
    fn target(&self) -> Result<PlaybackTarget>;
    fn place(&self, rect: [f32; 4], scale: f32);
    fn show(&self);
    fn hide(&self);
    /// # Errors
    /// Returns an error when the native subtitle file picker fails or is unavailable.
    fn pick_subtitle(&self) -> Result<Option<std::path::PathBuf>> {
        anyhow::bail!("Subtitle file selection is not supported on this platform")
    }
    /// Exclude controls and a rounded video-local overlay from native video and input.
    fn cover(&self, _controls: Option<VideoOverlay>, _overlay: Option<VideoOverlay>, _scale: f32) {}
    fn video_clicks(&self) -> Vec<VideoClick> {
        Vec::new()
    }
    fn pointer_position(&self, _scale: f32) -> VideoPointer {
        VideoPointer::Unavailable
    }
    fn double_click_interval(&self) -> std::time::Duration {
        std::time::Duration::from_millis(500)
    }
    /// # Errors
    /// Returns an error if the native window cannot enter or leave full screen.
    fn set_fullscreen(&self, fullscreen: bool) -> Result<()>;
}
