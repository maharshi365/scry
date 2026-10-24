//! OS boundary. Shared UI and services must not import Win32 or Cocoa APIs.
use scry_playback::PlaybackTarget;
use anyhow::Result;

#[cfg(not(windows))]
mod unsupported;
#[cfg(windows)]
mod windows;
#[cfg(not(windows))]
pub use unsupported::Host;
#[cfg(windows)]
pub use windows::Host;

pub trait VideoHost: Clone + Default {
    fn attach(&self, parent: &rune::WindowHandle) -> Result<()>;
    fn target(&self) -> Result<PlaybackTarget>;
    fn place(&self, rect: [f32; 4], scale: f32);
    fn hide(&self);
}
