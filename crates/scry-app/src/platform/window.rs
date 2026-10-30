//! Window integration independent of video hosting.
use super::{NavigationInput, native};
use anyhow::Result;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fullscreen {
    Windowed,
    Fullscreen,
}

/// Owns native window integration without requiring an active video host.
#[derive(Clone, Default)]
pub struct Window(native::Window);

impl Window {
    /// # Errors
    /// Returns an error if native window integration cannot be attached.
    pub fn attach(
        &self,
        window: &rune::WindowHandle,
        background: rune::Color,
        navigation: &NavigationInput,
    ) -> Result<()> {
        self.0.attach(window, background, navigation)
    }

    /// # Errors
    /// Returns an error if the native window cannot change fullscreen state.
    pub fn set_fullscreen(&self, mode: Fullscreen) -> Result<()> {
        self.0.set_fullscreen(mode)
    }

    /// # Errors
    /// Returns an error if the native subtitle file picker is unavailable or fails.
    pub fn pick_subtitle(&self) -> Result<Option<std::path::PathBuf>> {
        self.0.pick_subtitle()
    }
}
