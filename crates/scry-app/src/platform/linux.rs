//! Linux video shares Rune's scene with controls, on both X11 and Wayland.
use super::{Fullscreen, NavigationInput, VideoHost, VideoPresentation};
use anyhow::{Result, bail};
use rune::raw_window_handle::RawWindowHandle;
use scry_playback::{FrameSurface, PlaybackTarget};
use std::sync::{Arc, Mutex};
use x11rb::{
    connection::Connection,
    protocol::xproto::{ClientMessageEvent, ConnectionExt, EventMask},
};

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
pub struct Window(Arc<Mutex<Option<rune::WindowHandle>>>);

impl Window {
    /// # Errors
    /// Returns an error if the window lock is poisoned.
    pub fn attach(
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

    /// # Errors
    /// Returns an error if fullscreen cannot be requested from the desktop.
    pub fn set_fullscreen(&self, mode: Fullscreen) -> Result<()> {
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
                let id = match handle.as_raw() {
                    RawWindowHandle::Xlib(raw) => u32::try_from(raw.window)?,
                    RawWindowHandle::Xcb(raw) => raw.window.get(),
                    _ => bail!("Fullscreen on Wayland requires window support from Rune"),
                };
                request_fullscreen(id, mode)
            })
            .map_err(anyhow::Error::msg)?
    }
}

fn request_fullscreen(window: u32, mode: Fullscreen) -> Result<()> {
    let result = (|| -> Result<()> {
        let (connection, _) = x11rb::connect(None)?;
        let root = connection.query_tree(window)?.reply()?.root;
        let state = connection
            .intern_atom(false, b"_NET_WM_STATE")?
            .reply()?
            .atom;
        let fullscreen = connection
            .intern_atom(false, b"_NET_WM_STATE_FULLSCREEN")?
            .reply()?
            .atom;
        let event = ClientMessageEvent::new(
            32,
            window,
            state,
            [
                u32::from(mode == Fullscreen::Fullscreen),
                fullscreen,
                0,
                1,
                0,
            ],
        );
        connection
            .send_event(
                false,
                root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                event,
            )?
            .check()?;
        connection.flush()?;
        Ok(())
    })();
    result.map_err(|_| anyhow::anyhow!("Cannot request fullscreen from the X11 desktop"))
}
