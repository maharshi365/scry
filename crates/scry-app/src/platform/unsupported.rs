use super::{NavigationInput, VideoHost};
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
}
