//! macOS video is composited by Rune, so controls and pointer input stay in one scene.
use super::{Fullscreen, NavigationInput, VideoHost, VideoPresentation};
use anyhow::{Result, bail};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSOpenPanel, NSView, NSWindowStyleMask};
use rune::raw_window_handle::RawWindowHandle;
use scry_playback::{FrameSurface, PlaybackTarget};
use std::sync::{Arc, Mutex};

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
pub(super) struct Window(Arc<Mutex<Option<rune::WindowHandle>>>);

impl Window {
    pub(super) fn attach(
        &self,
        window: &rune::WindowHandle,
        _: rune::Color,
        _: &NavigationInput,
    ) -> Result<()> {
        *self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Window lock failed"))? = Some(window.clone());
        Ok(())
    }

    #[expect(
        clippy::unused_self,
        reason = "Matches the platform Window interface; macOS uses an application-modal picker"
    )]
    pub(super) fn pick_subtitle(&self) -> Result<Option<std::path::PathBuf>> {
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| anyhow::anyhow!("File picker requires the main thread"))?;
        let panel = NSOpenPanel::openPanel(mtm);
        panel.setCanChooseFiles(true);
        panel.setCanChooseDirectories(false);
        panel.setAllowsMultipleSelection(false);
        if panel.runModal() != 1 {
            return Ok(None);
        }
        Ok(panel
            .URL()
            .and_then(|url| url.path())
            .map(|path| std::path::PathBuf::from(path.to_string())))
    }
    pub(super) fn set_fullscreen(&self, mode: Fullscreen) -> Result<()> {
        let window = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Window lock failed"))?
            .clone();
        let Some(window) = window else {
            return Ok(());
        };
        window
            .with_raw_handle(|handle| {
                let RawWindowHandle::AppKit(raw) = handle.as_raw() else {
                    bail!("Expected a macOS window");
                };
                let _mtm = MainThreadMarker::new()
                    .ok_or_else(|| anyhow::anyhow!("Fullscreen requires the main thread"))?;
                // SAFETY: Rune lends its live NSView on the owning main thread for
                // this closure. Neither the view nor its window is retained as a raw pointer.
                let view = unsafe { raw.ns_view.cast::<NSView>().as_ref() };
                let window = view
                    .window()
                    .ok_or_else(|| anyhow::anyhow!("macOS window is unavailable"))?;
                if window.styleMask().contains(NSWindowStyleMask::FullScreen)
                    != (mode == Fullscreen::Fullscreen)
                {
                    window.toggleFullScreen(None);
                }
                Ok(())
            })
            .map_err(anyhow::Error::msg)?
    }
}
