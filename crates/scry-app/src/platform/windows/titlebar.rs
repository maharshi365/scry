//! Blend the native Windows caption into the app.
use windows_sys::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongPtrW, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SWP_NOZORDER, SetWindowLongPtrW, SetWindowPos, SetWindowTextW, WS_EX_DLGMODALFRAME,
    },
};

pub(super) fn match_title_bar(window: &rune::WindowHandle, background: rune::Color) {
    use rune::raw_window_handle::RawWindowHandle;
    use windows_sys::Win32::Graphics::Dwm::{
        DWMWA_CAPTION_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute,
    };

    let [r, g, b, _] = background.premul_srgb(1.0);
    let color =
        u32::from(channel(r)) | (u32::from(channel(g)) << 8) | (u32::from(channel(b)) << 16);
    let dark = 1_i32;
    // Cosmetic attributes are unsupported on older Windows versions. Keep their
    // native caption rather than failing window startup.
    let _ = window.with_raw_handle(|handle| {
        if let RawWindowHandle::Win32(raw) = handle.as_raw() {
            let hwnd = raw.hwnd.get() as windows_sys::Win32::Foundation::HWND;
            // SAFETY: The on_window callback runs on the owning UI thread, and
            // with_raw_handle borrows a live HWND for the duration of this call.
            // Both attributes receive valid, correctly sized values, copied by DWM.
            unsafe {
                remove_branding(hwnd);
                DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
                    (&raw const dark).cast(),
                    4,
                );
                DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_CAPTION_COLOR as u32,
                    (&raw const color).cast(),
                    4,
                );
            }
        }
    });
}

/// # Safety
/// `hwnd` must be a live window owned by the calling UI thread.
unsafe fn remove_branding(hwnd: HWND) {
    let Ok(frame) = isize::try_from(WS_EX_DLGMODALFRAME) else {
        return;
    };
    // SAFETY: the caller lends a live, thread-owned HWND. The empty UTF-16 title
    // is copied synchronously. Changing only the extended dialog-frame bit hides
    // the caption icon without clearing the icons used by the taskbar and Alt+Tab.
    unsafe {
        SetWindowTextW(hwnd, [0_u16].as_ptr());
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | frame);
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Rounded sRGB channels are clamped to the byte range before conversion"
)]
fn channel(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{Result, ensure};
    use std::ptr;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, GWL_STYLE, GetWindowTextLengthW, ICON_BIG, IDI_APPLICATION,
        LoadIconW, SendMessageW, WM_GETICON, WM_SETICON, WS_EX_APPWINDOW, WS_OVERLAPPEDWINDOW,
    };

    #[test]
    fn caption_branding_is_removed_without_removing_controls_or_taskbar_icon() -> Result<()> {
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let title: Vec<u16> = "Scry\0".encode_utf16().collect();
        // SAFETY: this test creates and destroys its hidden window on one thread.
        // Class/title buffers are NUL-terminated, and no creation data is retained.
        // The stock application icon is shared and must not be destroyed.
        let (title_length, style, extended, icon, expected_icon) = unsafe {
            let hwnd = CreateWindowExW(
                WS_EX_APPWINDOW,
                class.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPEDWINDOW,
                0,
                0,
                320,
                200,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            );
            ensure!(!hwnd.is_null(), "Cannot create caption test window");
            let icon = LoadIconW(ptr::null_mut(), IDI_APPLICATION);
            SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, icon as isize);
            remove_branding(hwnd);
            let result = (
                GetWindowTextLengthW(hwnd),
                GetWindowLongPtrW(hwnd, GWL_STYLE),
                GetWindowLongPtrW(hwnd, GWL_EXSTYLE),
                SendMessageW(hwnd, WM_GETICON, ICON_BIG as usize, 0),
                icon as isize,
            );
            DestroyWindow(hwnd);
            result
        };
        assert_eq!(title_length, 0);
        assert_eq!(
            style & isize::try_from(WS_OVERLAPPEDWINDOW)?,
            isize::try_from(WS_OVERLAPPEDWINDOW)?
        );
        assert_ne!(extended & isize::try_from(WS_EX_DLGMODALFRAME)?, 0);
        assert_ne!(extended & isize::try_from(WS_EX_APPWINDOW)?, 0);
        assert_ne!(expected_icon, 0);
        assert_eq!(icon, expected_icon);
        Ok(())
    }
}
