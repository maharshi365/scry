//! OS boundary. Shared UI and services must not import Win32 or Cocoa APIs.
use anyhow::Result;
use scry_playback::PlaybackTarget;

#[derive(Clone, Copy)]
pub struct VideoClick {
    pub at: std::time::Instant,
    pub double: bool,
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
mod window;
pub use browser::validate_browser_url;
pub use navigation::{HistoryDirection, NavigationInput};
pub use window::{Fullscreen, Window};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as native;
#[cfg(not(any(windows, target_os = "macos")))]
mod unsupported;
#[cfg(windows)]
mod windows;
pub use native::Host;
#[cfg(not(any(windows, target_os = "macos")))]
use unsupported as native;
#[cfg(windows)]
use windows as native;

/// The way a video host presents its output in the shared scene.
pub enum VideoPresentation<'a> {
    Native,
    Pixels(&'a rune::Pixels),
}

pub trait VideoHost: Clone + Default {
    /// # Errors
    /// Returns an error if the native parent or embedded video window is unavailable.
    fn attach(&self, parent: &rune::WindowHandle) -> Result<()>;
    /// # Errors
    /// Returns an error if no video window has been attached.
    fn target(&self) -> Result<PlaybackTarget>;
    fn presentation(&self) -> VideoPresentation<'_> {
        VideoPresentation::Native
    }
    fn place(&self, rect: [f32; 4], scale: f32);
    fn show(&self);
    fn hide(&self);
    /// Exclude controls and a rounded video-local overlay from native video and input.
    fn cover(&self, _controls: Option<VideoOverlay>, _overlay: Option<VideoOverlay>, _scale: f32) {}
    fn video_clicks(&self) -> Vec<VideoClick> {
        Vec::new()
    }
    fn pointer_position(&self, _scale: f32) -> VideoPointer {
        VideoPointer::Unavailable
    }
}
