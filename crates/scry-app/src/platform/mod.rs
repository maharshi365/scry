//! OS boundary. Shared UI and services must not import Win32 or Cocoa APIs.
use anyhow::Result;
use scry_playback::PlaybackTarget;

#[cfg(not(windows))]
mod unsupported;
#[cfg(windows)]
mod windows;
#[cfg(not(windows))]
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
    fn hide(&self);
}
