//! Preserve system caption controls and resize behavior on every platform.

/// Blends the Windows caption into the app; other systems own their decorations.
pub fn match_title_bar(window: &rune::WindowHandle, background: rune::Color) {
    #[cfg(windows)]
    windows_caption(window, background);
    #[cfg(not(windows))]
    let _ = (window, background);
}

#[cfg(windows)]
fn windows_caption(window: &rune::WindowHandle, background: rune::Color) {
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
    let _ = window.on_raw_handle(|handle| {
        if let RawWindowHandle::Win32(raw) = handle.as_raw() {
            let hwnd = raw.hwnd.get() as windows_sys::Win32::Foundation::HWND;
            // SAFETY: Rune calls this on the owning UI thread with a live HWND.
            // Both attributes receive valid, correctly sized values, copied by DWM.
            unsafe {
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
        Ok(())
    });
}

#[cfg(windows)]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Rounded sRGB channels are clamped to the byte range before conversion"
)]
fn channel(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}
