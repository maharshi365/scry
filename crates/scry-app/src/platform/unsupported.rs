use super::{Fullscreen, NavigationInput, VideoHost};
use anyhow::{Result, bail};
use scry_playback::PlaybackTarget;

#[derive(Clone, Default)]
pub struct Host;
impl VideoHost for Host {
    fn attach(&self, _: &rune::WindowHandle) -> Result<()> {
        Ok(())
    }
    fn target(&self) -> Result<PlaybackTarget> {
        bail!("Embedded video host is not implemented for this OS yet")
    }
    fn place(&self, _: [f32; 4], _: f32) {}
    fn hide(&self) {}
    fn show(&self) {}
}

#[derive(Clone, Default)]
pub struct Window;

impl Window {
    /// # Errors
    /// This operation does not fail.
    pub fn attach(
        &self,
        _: &rune::WindowHandle,
        _: rune::Color,
        _: &NavigationInput,
    ) -> Result<()> {
        Ok(())
    }

    /// # Errors
    /// This operation does not fail.
    pub fn set_fullscreen(&self, _: Fullscreen) -> Result<()> {
        Ok(())
    }

    /// # Errors
    /// Returns an error because subtitle selection is not supported on this platform.
    pub fn pick_subtitle(&self) -> Result<Option<std::path::PathBuf>> {
        bail!("Subtitle file selection is not supported on this platform")
    }

    /// # Errors
    /// Returns an error because file selection is not supported on this platform.
    pub fn pick_history(&self) -> Result<Option<std::path::PathBuf>> {
        bail!("History file selection is not supported on this platform")
    }
}
