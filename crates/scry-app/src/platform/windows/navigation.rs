//! Rune currently drops side-button events. Subclass only the app's own window.
use crate::platform::{HistoryDirection, NavigationInput};
use anyhow::{Result, bail};
use rune::raw_window_handle::RawWindowHandle;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            WM_APPCOMMAND, WM_NCDESTROY, WM_XBUTTONDBLCLK, WM_XBUTTONDOWN, WM_XBUTTONUP,
        },
    },
};

const SUBCLASS: usize = 0x5343_5259;

pub(in crate::platform) fn attach(
    window: &rune::WindowHandle,
    input: NavigationInput,
) -> Result<()> {
    window
        .with_raw_handle(|handle| {
            let RawWindowHandle::Win32(raw) = handle.as_raw() else {
                bail!("Expected a Windows window");
            };
            attach_hwnd(raw.hwnd.get() as HWND, input)
        })
        .map_err(anyhow::Error::msg)?
}

fn attach_hwnd(hwnd: HWND, input: NavigationInput) -> Result<()> {
    let data = Box::into_raw(Box::new(input));
    // SAFETY: runs on the window's UI thread. The subclass owns this allocation
    // until WM_NCDESTROY; failed installation drops it here instead.
    if unsafe { SetWindowSubclass(hwnd, Some(history_proc), SUBCLASS, data as usize) } == 0 {
        unsafe {
            drop(Box::from_raw(data));
        }
        bail!("Cannot register mouse history buttons");
    }
    Ok(())
}

unsafe extern "system" fn history_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    data: usize,
) -> LRESULT {
    if message == WM_NCDESTROY {
        // SAFETY: remove the callback before releasing its uniquely owned data.
        unsafe {
            RemoveWindowSubclass(hwnd, Some(history_proc), id);
            drop(Box::from_raw(data as *mut NavigationInput));
            return DefSubclassProc(hwnd, message, wparam, lparam);
        }
    }
    let direction = match message {
        WM_XBUTTONUP => match (wparam >> 16) & 0xffff {
            1 => Some(HistoryDirection::Back),
            2 => Some(HistoryDirection::Forward),
            _ => None,
        },
        WM_APPCOMMAND => match (lparam.cast_unsigned() >> 16) & 0x0fff {
            1 => Some(HistoryDirection::Back),
            2 => Some(HistoryDirection::Forward),
            _ => None,
        },
        WM_XBUTTONDOWN | WM_XBUTTONDBLCLK if matches!((wparam >> 16) & 0xffff, 1 | 2) => return 1,
        _ => None,
    };
    if let Some(direction) = direction {
        // SAFETY: data belongs to this live subclass. Enqueue only; do not mutate
        // app state reentrantly inside the native window procedure.
        unsafe { &*(data as *const NavigationInput) }.push(direction);
        return 1;
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, SendMessageW,
    };

    #[test]
    fn side_buttons_and_browser_commands_enqueue_history_once() -> Result<()> {
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        // A hidden test-owned window exercises actual subclass message delivery.
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                std::ptr::null(),
                0,
                0,
                0,
                1,
                1,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        anyhow::ensure!(!hwnd.is_null(), "Cannot create input test window");
        let input = NavigationInput::default();
        attach_hwnd(hwnd, input.clone())?;
        unsafe {
            SendMessageW(hwnd, WM_XBUTTONDOWN, 1 << 16, 0);
            SendMessageW(hwnd, WM_XBUTTONUP, 1 << 16, 0);
            SendMessageW(hwnd, WM_XBUTTONDOWN, 2 << 16, 0);
            SendMessageW(hwnd, WM_XBUTTONUP, 2 << 16, 0);
            SendMessageW(hwnd, WM_APPCOMMAND, 0, 1 << 16);
            SendMessageW(hwnd, WM_APPCOMMAND, 0, 2 << 16);
            DestroyWindow(hwnd);
        }
        assert_eq!(
            input.drain(),
            vec![
                HistoryDirection::Back,
                HistoryDirection::Forward,
                HistoryDirection::Back,
                HistoryDirection::Forward
            ]
        );
        assert!(input.drain().is_empty());
        Ok(())
    }
}
