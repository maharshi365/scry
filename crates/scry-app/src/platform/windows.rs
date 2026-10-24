use super::VideoHost;
use scry_playback::PlaybackTarget;
use anyhow::{Result, bail};
use rune::raw_window_handle::RawWindowHandle;
use std::{
    ptr,
    sync::{
        Arc,
        atomic::{AtomicIsize, Ordering},
    },
};
use windows_sys::Win32::{Foundation::HWND, UI::WindowsAndMessaging::*};

#[derive(Clone, Default)]
pub struct Host(Arc<AtomicIsize>);

impl VideoHost for Host {
    fn attach(&self, parent: &rune::WindowHandle) -> Result<()> {
        parent
            .with_raw_handle(|handle| {
                let RawWindowHandle::Win32(raw) = handle.as_raw() else {
                    bail!("Expected a Windows window");
                };
                let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
                // The child belongs to the UI thread. libmpv renders into it, never owns it.
                let child = unsafe {
                    CreateWindowExW(
                        0,
                        class.as_ptr(),
                        ptr::null(),
                        WS_CHILD | WS_CLIPSIBLINGS | WS_CLIPCHILDREN,
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
                self.0.store(child as isize, Ordering::Release);
                Ok(())
            })
            .map_err(anyhow::Error::msg)?
    }
    fn target(&self) -> Result<PlaybackTarget> {
        let handle = self.0.load(Ordering::Acquire);
        if handle == 0 {
            bail!("Video window is not ready");
        }
        Ok(PlaybackTarget::NativeWindow(handle as u64))
    }
    fn place(&self, [x, y, w, h]: [f32; 4], scale: f32) {
        let hwnd = self.0.load(Ordering::Acquire) as HWND;
        if hwnd.is_null() {
            return;
        }
        unsafe {
            SetWindowPos(
                hwnd,
                ptr::null_mut(),
                (x * scale).round() as i32,
                (y * scale).round() as i32,
                (w * scale).round().max(1.) as i32,
                (h * scale).round().max(1.) as i32,
                SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }
    fn hide(&self) {
        let hwnd = self.0.load(Ordering::Acquire) as HWND;
        if !hwnd.is_null() {
            unsafe {
                ShowWindow(hwnd, SW_HIDE);
            }
        }
    }
}
