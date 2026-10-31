use super::VideoHost;
mod window;
pub(super) use window::Window;
pub(super) mod navigation;
mod subtitle_dialog;
mod titlebar;
mod video_input;
use anyhow::{Result, bail};
use rune::raw_window_handle::RawWindowHandle;
use scry_playback::PlaybackTarget;
use std::{
    ptr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicIsize, Ordering},
    },
};
use windows_sys::Win32::{
    Foundation::HWND,
    System::SystemServices::SS_BLACKRECT,
    UI::WindowsAndMessaging::{
        CreateWindowExW, SW_HIDE, SW_SHOW, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos, ShowWindow,
        WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS,
    },
};

#[derive(Clone, Default)]
pub struct Host(Arc<NativeHost>);

#[derive(Default)]
struct NativeHost {
    child: AtomicIsize,
    input: AtomicIsize,
    clicks: Mutex<std::collections::VecDeque<super::VideoClick>>,
    clip: Mutex<Option<video_input::Clip>>,
}

impl VideoHost for Host {
    fn attach(&self, parent: &rune::WindowHandle) -> Result<()> {
        parent
            .with_raw_handle(|handle| {
                let RawWindowHandle::Win32(raw) = handle.as_raw() else {
                    bail!("Expected a Windows window");
                };
                let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
                // SAFETY: class is NUL-terminated and the parent handle is borrowed live.
                // The child belongs to the UI thread. libmpv renders into it, never owns it.
                let child = unsafe {
                    CreateWindowExW(
                        0,
                        class.as_ptr(),
                        ptr::null(),
                        WS_CHILD | WS_CLIPSIBLINGS | WS_CLIPCHILDREN | SS_BLACKRECT,
                        0,
                        0,
                        1,
                        1,
                        raw.hwnd.get() as HWND,
                        ptr::null_mut(),
                        ptr::null_mut(),
                        ptr::null(),
                    )
                };
                if child.is_null() {
                    bail!("Cannot create embedded video host");
                }
                self.0.child.store(child as isize, Ordering::Release);
                let input = video_input::create(raw.hwnd.get() as HWND, self.clone())?;
                self.0.input.store(input as isize, Ordering::Release);
                Ok(())
            })
            .map_err(anyhow::Error::msg)?
    }
    fn target(&self) -> Result<PlaybackTarget> {
        let handle = self.0.child.load(Ordering::Acquire);
        if handle == 0 {
            bail!("Video window is not ready");
        }
        Ok(PlaybackTarget::NativeWindow(handle.cast_unsigned() as u64))
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Windows requires integer pixels; rounded layout coordinates intentionally saturate to i32"
    )]
    fn place(&self, [x, y, w, h]: [f32; 4], scale: f32) {
        let hwnd = self.0.child.load(Ordering::Acquire) as HWND;
        if hwnd.is_null() {
            return;
        }
        // SAFETY: attach created this child on the UI thread, where placement runs.
        // SetWindowPos receives no pointers to caller-owned memory.
        unsafe {
            SetWindowPos(
                hwnd,
                ptr::null_mut(),
                (x * scale).round() as i32,
                (y * scale).round() as i32,
                (w * scale).round().max(1.) as i32,
                (h * scale).round().max(1.) as i32,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            let input = self.0.input.load(Ordering::Acquire) as HWND;
            if !input.is_null() {
                SetWindowPos(
                    input,
                    ptr::null_mut(),
                    (x * scale).round() as i32,
                    (y * scale).round() as i32,
                    (w * scale).round().max(1.0) as i32,
                    (h * scale).round().max(1.0) as i32,
                    SWP_NOACTIVATE,
                );
            }
        }
    }
    fn hide(&self) {
        let hwnd = self.0.child.load(Ordering::Acquire) as HWND;
        if !hwnd.is_null() {
            // SAFETY: this is the child handle created by attach; the call runs on
            // its UI thread and does not transfer ownership of the window.
            unsafe {
                ShowWindow(hwnd, SW_HIDE);
                ShowWindow(self.0.input.load(Ordering::Acquire) as HWND, SW_HIDE);
            }
        }
    }

    fn cover(
        &self,
        controls: Option<super::VideoOverlay>,
        overlay: Option<super::VideoOverlay>,
        scale: f32,
    ) {
        let scaled = |overlay: super::VideoOverlay| super::VideoOverlay {
            rect: overlay.rect.map(|v| v * scale),
            radius: overlay.radius * scale,
        };
        video_input::cover(self, controls.map(scaled), overlay.map(scaled));
    }

    fn video_clicks(&self) -> Vec<super::VideoClick> {
        self.0
            .clicks
            .lock()
            .map(|mut clicks| clicks.drain(..).collect())
            .unwrap_or_default()
    }

    fn pointer_position(&self, scale: f32) -> super::VideoPointer {
        video_input::pointer_position(self, scale)
    }

    fn double_click_interval(&self) -> std::time::Duration {
        video_input::double_click_interval()
    }

    fn show(&self) {
        let hwnd = self.0.child.load(Ordering::Acquire) as HWND;
        if !hwnd.is_null() {
            // SAFETY: this is the child handle created by attach; the call runs on
            // its UI thread and does not transfer ownership of the window.
            unsafe {
                ShowWindow(hwnd, SW_SHOW);
                ShowWindow(self.0.input.load(Ordering::Acquire) as HWND, SW_SHOW);
            }
        }
    }
}
