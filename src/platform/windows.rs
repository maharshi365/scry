use super::VideoHost;
use crate::playback::PlaybackTarget;
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

pub fn memory() -> (Option<u64>, Option<u64>) {
    use windows_sys::Win32::System::{
        ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX},
        Threading::GetCurrentProcess,
    };
    unsafe {
        let mut counters: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
        let size = std::mem::size_of_val(&counters) as u32;
        if GetProcessMemoryInfo(GetCurrentProcess(), &mut counters as *mut _ as *mut _, size) != 0 {
            return (
                Some(counters.WorkingSetSize as u64),
                Some(counters.PrivateUsage as u64),
            );
        }
    }
    (None, None)
}

pub fn cpu_time_s() -> Option<f64> {
    use windows_sys::Win32::{
        Foundation::FILETIME,
        System::Threading::{GetCurrentProcess, GetProcessTimes},
    };
    unsafe {
        let mut created: FILETIME = std::mem::zeroed();
        let mut exited: FILETIME = std::mem::zeroed();
        let mut kernel: FILETIME = std::mem::zeroed();
        let mut user: FILETIME = std::mem::zeroed();
        if GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        ) == 0
        {
            return None;
        }
        let ticks = |t: FILETIME| ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64;
        Some((ticks(kernel) + ticks(user)) as f64 / 10_000_000.)
    }
}
