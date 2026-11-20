//! macOS video is composited by Rune, so controls and pointer input stay in one scene.
use super::{NavigationInput, VideoHost, VideoPresentation};
use anyhow::Result;
use scry_playback::{FrameSurface, PlaybackTarget};

#[derive(Clone)]
pub struct Host {
    pixels: rune::Pixels,
    surface: FrameSurface,
}

impl Default for Host {
    fn default() -> Self {
        let pixels = rune::Pixels::new();
        let output = pixels.clone();
        let surface = FrameSurface::new(move |w, h, rgba| output.set(w, h, rgba));
        Self { pixels, surface }
    }
}

impl VideoHost for Host {
    fn attach(&self, _: &rune::WindowHandle) -> Result<()> {
        Ok(())
    }
    fn target(&self) -> Result<PlaybackTarget> {
        Ok(PlaybackTarget::Software(self.surface.clone()))
    }
    fn presentation(&self) -> VideoPresentation<'_> {
        VideoPresentation::Pixels(&self.pixels)
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Layout dimensions saturate to physical pixels; Surface bounds their allocation"
    )]
    fn place(&self, [_, _, w, h]: [f32; 4], scale: f32) {
        self.surface
            .resize((w * scale).max(1.0) as u32, (h * scale).max(1.0) as u32);
    }
    fn show(&self) {}
    fn hide(&self) {
        self.pixels.set(0, 0, Vec::new());
    }
}

#[derive(Clone, Default)]
pub struct Window {
    _private: (),
}

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
