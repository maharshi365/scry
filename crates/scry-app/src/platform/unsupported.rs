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
pub(super) struct Window;

impl Window {
    pub(super) fn attach(
        &self,
        _: &rune::WindowHandle,
        _: rune::Color,
        _: &NavigationInput,
    ) -> Result<()> {
        Ok(())
    }

    pub(super) fn set_fullscreen(&self, _: Fullscreen) -> Result<()> {
        Ok(())
    }

    pub(super) fn pick_subtitle(&self) -> Result<Option<std::path::PathBuf>> {
        bail!("Subtitle file selection is not supported on this platform")
    }
}

pub(super) fn open_browser(url: &str) -> Result<()> {
    std::process::Command::new("xdg-open").arg(url).spawn()?;
    Ok(())
}
