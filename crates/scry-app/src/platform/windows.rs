use super::VideoHost;
pub(super) mod navigation;
mod subtitle_dialog;
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
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow},
    System::LibraryLoader::GetModuleHandleW,
    System::SystemServices::SS_BLACKRECT,
    UI::WindowsAndMessaging::{
        CreateWindowExW, GWL_STYLE, GetSystemMetrics, GetWindowLongPtrW, GetWindowPlacement,
        ICON_BIG, ICON_SMALL, IMAGE_ICON, LR_SHARED, LoadImageW, SM_CXICON, SM_CXSMICON, SM_CYICON,
        SM_CYSMICON, SW_HIDE, SW_SHOW, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOZORDER,
        SendMessageW, SetWindowLongPtrW, SetWindowPlacement, SetWindowPos, ShowWindow,
        WINDOWPLACEMENT, WM_SETICON, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS,
        WS_OVERLAPPEDWINDOW,
    },
};

pub(super) fn open_browser(url: &str) -> Result<()> {
    let operation: Vec<u16> = "open\0".encode_utf16().collect();
    let target: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
    // ShellExecute resolves the registered HTTP(S) handler. Starting Explorer
    // as a process can interpret the URL as a shell location instead.
    // SAFETY: operation and target are live NUL-terminated UTF-16 strings.
    // Optional arguments are null; the call does not retain the string pointers.
    let result = unsafe {
        windows_sys::Win32::UI::Shell::ShellExecuteW(
            ptr::null_mut(),
            operation.as_ptr(),
            target.as_ptr(),
            ptr::null(),
            ptr::null(),
            windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        )
    };
    let status = result.addr();
    if status <= 32 {
        bail!("Windows could not open the default browser (code {status})");
    }
    Ok(())
}

#[derive(Clone, Default)]
pub struct Host(Arc<NativeHost>);

#[derive(Default)]
struct NativeHost {
    child: AtomicIsize,
    input: AtomicIsize,
    window: Mutex<Option<rune::WindowHandle>>,
    restore: Mutex<Option<Windowed>>,
    clicks: Mutex<std::collections::VecDeque<super::VideoClick>>,
    clip: Mutex<Option<video_input::Clip>>,
}

struct Windowed {
    style: isize,
    placement: WINDOWPLACEMENT,
}

impl VideoHost for Host {
    fn attach(&self, parent: &rune::WindowHandle) -> Result<()> {
        parent
            .with_raw_handle(|handle| {
                let RawWindowHandle::Win32(raw) = handle.as_raw() else {
                    bail!("Expected a Windows window");
                };
                // SAFETY: raw.hwnd is borrowed live on its owning UI thread.
                // Resource 1 is generated from the selected SVG by build.rs. Shared
                // handles live with the executable; the window does not own them.
                unsafe {
                    let module = GetModuleHandleW(ptr::null());
                    for (kind, width, height) in [
                        (
                            ICON_SMALL,
                            GetSystemMetrics(SM_CXSMICON),
                            GetSystemMetrics(SM_CYSMICON),
                        ),
                        (
                            ICON_BIG,
                            GetSystemMetrics(SM_CXICON),
                            GetSystemMetrics(SM_CYICON),
                        ),
                    ] {
                        let icon = LoadImageW(
                            module,
                            ptr::without_provenance(1),
                            IMAGE_ICON,
                            width,
                            height,
                            LR_SHARED,
                        );
                        if icon.is_null() {
                            bail!("Cannot load Scry window icon");
                        }
                        SendMessageW(
                            raw.hwnd.get() as HWND,
                            WM_SETICON,
                            kind as usize,
                            icon as isize,
                        );
                    }
                }
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
                *self
                    .0
                    .window
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Window lock failed"))? = Some(parent.clone());
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

    fn pick_subtitle(&self) -> Result<Option<std::path::PathBuf>> {
        subtitle_dialog::pick(self)
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

    fn set_fullscreen(&self, fullscreen: bool) -> Result<()> {
        let window = self
            .0
            .window
            .lock()
            .map_err(|_| anyhow::anyhow!("Window lock failed"))?
            .clone();
        // Headless UI runtimes have no native window.
        let Some(window) = window else {
            return Ok(());
        };
        window
            .with_raw_handle(|handle| {
                let RawWindowHandle::Win32(raw) = handle.as_raw() else {
                    bail!("Expected a Windows window");
                };
                let hwnd = raw.hwnd.get() as HWND;
                let mut restore = self
                    .0
                    .restore
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Window lock failed"))?;
                // SAFETY: all window mutations run on the UI thread, inside the live
                // handle borrow. The plain-data output structs are initialized and sized
                // before Win32 writes them; saved placement stays owned by restore.
                unsafe {
                    if fullscreen {
                        if restore.is_some() {
                            return Ok(());
                        }
                        let mut placement: WINDOWPLACEMENT = std::mem::zeroed();
                        placement.length = u32::try_from(std::mem::size_of::<WINDOWPLACEMENT>())?;
                        let mut monitor: MONITORINFO = std::mem::zeroed();
                        monitor.cbSize = u32::try_from(std::mem::size_of::<MONITORINFO>())?;
                        if GetWindowPlacement(hwnd, &raw mut placement) == 0
                            || GetMonitorInfoW(
                                MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST),
                                &raw mut monitor,
                            ) == 0
                        {
                            bail!("Cannot read window placement");
                        }
                        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
                        if SetWindowLongPtrW(
                            hwnd,
                            GWL_STYLE,
                            style & !isize::try_from(WS_OVERLAPPEDWINDOW)?,
                        ) == 0
                        {
                            bail!("Cannot enter full screen");
                        }
                        let rect = monitor.rcMonitor;
                        if SetWindowPos(
                            hwnd,
                            ptr::null_mut(),
                            rect.left,
                            rect.top,
                            rect.right - rect.left,
                            rect.bottom - rect.top,
                            SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                        ) == 0
                        {
                            SetWindowLongPtrW(hwnd, GWL_STYLE, style);
                            SetWindowPlacement(hwnd, &raw const placement);
                            bail!("Cannot enter full screen");
                        }
                        *restore = Some(Windowed { style, placement });
                    } else if let Some(saved) = restore.as_ref() {
                        if SetWindowLongPtrW(hwnd, GWL_STYLE, saved.style) == 0
                            || SetWindowPlacement(hwnd, &raw const saved.placement) == 0
                        {
                            bail!("Cannot restore window");
                        }
                        if SetWindowPos(
                            hwnd,
                            ptr::null_mut(),
                            0,
                            0,
                            0,
                            0,
                            SWP_NOZORDER
                                | SWP_NOACTIVATE
                                | SWP_FRAMECHANGED
                                | windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOMOVE
                                | windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOSIZE,
                        ) == 0
                        {
                            bail!("Cannot restore window");
                        }
                        *restore = None;
                    }
                }
                Ok(())
            })
            .map_err(anyhow::Error::msg)?
    }
}
