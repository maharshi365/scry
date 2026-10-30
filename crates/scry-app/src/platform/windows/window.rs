use crate::platform::{Fullscreen, NavigationInput};
use anyhow::{Result, bail};
use rune::raw_window_handle::RawWindowHandle;
use std::{
    ptr,
    sync::{Arc, Mutex},
};
use windows_sys::Win32::{
    Foundation::{GetLastError, HWND, SetLastError},
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow},
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GWL_STYLE, GetSystemMetrics, GetWindowLongPtrW, GetWindowPlacement, ICON_BIG,
        ICON_SMALL, IMAGE_ICON, LR_SHARED, LoadImageW, SM_CXICON, SM_CXSMICON, SM_CYICON,
        SM_CYSMICON, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOZORDER, SendMessageW,
        SetWindowLongPtrW, SetWindowPlacement, SetWindowPos, WINDOWPLACEMENT, WM_SETICON,
        WS_EX_DLGMODALFRAME, WS_OVERLAPPEDWINDOW,
    },
};

#[derive(Clone, Default)]
pub(in crate::platform) struct Window(Arc<NativeWindow>);

#[derive(Default)]
struct NativeWindow {
    handle: Mutex<Option<rune::WindowHandle>>,
    restore: Mutex<Option<Windowed>>,
}

struct Windowed {
    style: isize,
    extended_style: isize,
    placement: WINDOWPLACEMENT,
}

impl Window {
    pub(in crate::platform) fn attach(
        &self,
        window: &rune::WindowHandle,
        background: rune::Color,
        navigation: &NavigationInput,
    ) -> Result<()> {
        *self
            .0
            .handle
            .lock()
            .map_err(|_| anyhow::anyhow!("Window lock failed"))? = Some(window.clone());
        super::titlebar::match_title_bar(window, background);
        super::navigation::attach(window, navigation.clone())?;
        window
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
                Ok(())
            })
            .map_err(anyhow::Error::msg)?
    }

    pub(in crate::platform) fn pick_subtitle(&self) -> Result<Option<std::path::PathBuf>> {
        let window = self
            .0
            .handle
            .lock()
            .map_err(|_| anyhow::anyhow!("Window lock failed"))?
            .clone();
        let window = window
            .ok_or_else(|| anyhow::anyhow!("The subtitle file picker needs an app window"))?;
        super::subtitle_dialog::pick(&window)
    }

    pub(in crate::platform) fn set_fullscreen(&self, mode: Fullscreen) -> Result<()> {
        let window = self
            .0
            .handle
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
                self.set_fullscreen_hwnd(hwnd, mode)
            })
            .map_err(anyhow::Error::msg)?
    }

    fn set_fullscreen_hwnd(&self, hwnd: HWND, mode: Fullscreen) -> Result<()> {
        let mut restore = self
            .0
            .restore
            .lock()
            .map_err(|_| anyhow::anyhow!("Window lock failed"))?;
        // SAFETY: all window mutations run on the UI thread, inside the live
        // handle borrow. The plain-data output structs are initialized and sized
        // before Win32 writes them; saved placement stays owned by restore.
        unsafe {
            if mode == Fullscreen::Fullscreen {
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
                let extended_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                if SetWindowLongPtrW(
                    hwnd,
                    GWL_STYLE,
                    style & !isize::try_from(WS_OVERLAPPEDWINDOW)?,
                ) == 0
                {
                    bail!("Cannot enter full screen");
                }
                if let Err(error) = set_extended_style(
                    hwnd,
                    extended_style & !isize::try_from(WS_EX_DLGMODALFRAME)?,
                ) {
                    SetWindowLongPtrW(hwnd, GWL_STYLE, style);
                    return Err(error);
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
                    set_extended_style(hwnd, extended_style)?;
                    SetWindowPlacement(hwnd, &raw const placement);
                    bail!("Cannot enter full screen");
                }
                *restore = Some(Windowed {
                    style,
                    extended_style,
                    placement,
                });
            } else if let Some(saved) = restore.as_ref() {
                set_extended_style(hwnd, saved.extended_style)?;
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
    }
}

fn set_extended_style(hwnd: HWND, style: isize) -> Result<()> {
    // SAFETY: callers supply the live window borrowed on its owning UI thread.
    // No pointers are retained. Clearing last-error distinguishes a successful
    // update of a zero-valued previous style from a failed Win32 call.
    unsafe {
        SetLastError(0);
        if SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style) == 0 && GetLastError() != 0 {
            bail!("Cannot update window frame");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::ensure;
    use windows_sys::Win32::{
        Foundation::RECT,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, GWL_EXSTYLE, GetClientRect, GetWindowRect,
            WS_EX_APPWINDOW, WS_EX_DLGMODALFRAME,
        },
    };

    #[test]
    fn fullscreen_removes_caption_frame_and_restores_windowed_styles() -> Result<()> {
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        // SAFETY: STATIC is a built-in class and the class buffer is NUL-terminated.
        // This thread owns the hidden window and destroys it after the probe.
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_APPWINDOW | WS_EX_DLGMODALFRAME,
                class.as_ptr(),
                ptr::null(),
                WS_OVERLAPPEDWINDOW,
                100,
                100,
                640,
                480,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            )
        };
        ensure!(!hwnd.is_null(), "Cannot create fullscreen test window");
        let window = Window::default();
        let result = (|| -> Result<_> {
            // SAFETY: hwnd is live and owned by this test thread. Both calls read
            // integer styles and retain no pointers.
            let before = unsafe {
                (
                    GetWindowLongPtrW(hwnd, GWL_STYLE),
                    GetWindowLongPtrW(hwnd, GWL_EXSTYLE),
                )
            };
            window.set_fullscreen_hwnd(hwnd, Fullscreen::Fullscreen)?;
            let mut outer = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            let mut client = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            // SAFETY: hwnd remains live on its owner thread, and both rectangles
            // are valid writable outputs for the duration of these calls.
            unsafe {
                ensure!(
                    GetWindowRect(hwnd, &raw mut outer) != 0,
                    "Cannot read window bounds"
                );
                ensure!(
                    GetClientRect(hwnd, &raw mut client) != 0,
                    "Cannot read client bounds"
                );
            }
            window.set_fullscreen_hwnd(hwnd, Fullscreen::Windowed)?;
            // SAFETY: the test still owns the live window; these calls only read styles.
            let after = unsafe {
                (
                    GetWindowLongPtrW(hwnd, GWL_STYLE),
                    GetWindowLongPtrW(hwnd, GWL_EXSTYLE),
                )
            };
            Ok((before, after, outer, client))
        })();
        // SAFETY: the test owns this window, all handle borrows have ended, and
        // destruction runs on the same thread that created it, including on error.
        unsafe { DestroyWindow(hwnd) };
        let (before, after, outer, client) = result?;
        assert_eq!(
            client.right - client.left,
            outer.right - outer.left,
            "fullscreen must have no horizontal frame"
        );
        assert_eq!(
            client.bottom - client.top,
            outer.bottom - outer.top,
            "fullscreen must have no vertical frame"
        );
        let frame = isize::try_from(WS_OVERLAPPEDWINDOW)?;
        assert_eq!(
            after.0 & frame,
            before.0 & frame,
            "exiting fullscreen must restore the windowed controls and frame"
        );
        assert_eq!(
            after.1, before.1,
            "exiting fullscreen must restore the extended style"
        );
        Ok(())
    }
}
