use super::VideoHost;
use crate::playback::PlaybackTarget;
use anyhow::{Result, bail};

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
}
