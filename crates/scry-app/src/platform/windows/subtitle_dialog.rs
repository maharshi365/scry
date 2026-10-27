use super::Host;
use anyhow::{Result, bail};
use rune::raw_window_handle::RawWindowHandle;
use std::path::PathBuf;
use windows_sys::Win32::UI::Controls::Dialogs::{
    CommDlgExtendedError, GetOpenFileNameW, OFN_DONTADDTORECENT, OFN_EXPLORER, OFN_FILEMUSTEXIST,
    OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};

pub(super) fn pick(host: &Host) -> Result<Option<PathBuf>> {
    let window = host.0.window.lock().ok().and_then(|window| window.clone());
    let Some(window) = window else {
        bail!("The subtitle file picker needs an app window");
    };
    window
        .with_raw_handle(|handle| {
            let RawWindowHandle::Win32(raw) = handle.as_raw() else {
                bail!("Expected a Windows window");
            };
            let mut path = vec![0_u16; 32768];
            let filter: Vec<_> =
                "Subtitle files (*.srt;*.vtt;*.ass;*.ssa)\0*.srt;*.vtt;*.ass;*.ssa\0\0"
                    .encode_utf16()
                    .collect();
            let title: Vec<_> = "Load subtitle file\0".encode_utf16().collect();
            // SAFETY: OPENFILENAMEW is a plain Win32 structure whose optional members
            // accept null/zero. The owner is borrowed live on its UI thread. All UTF-16
            // buffers remain allocated for the synchronous dialog call, and nMaxFile
            // matches the writable buffer length. Windows does not retain these pointers.
            unsafe {
                let mut dialog: OPENFILENAMEW = std::mem::zeroed();
                dialog.lStructSize = u32::try_from(std::mem::size_of::<OPENFILENAMEW>())?;
                dialog.hwndOwner = raw.hwnd.get() as _;
                dialog.lpstrFilter = filter.as_ptr();
                dialog.lpstrTitle = title.as_ptr();
                dialog.lpstrFile = path.as_mut_ptr();
                dialog.nMaxFile = u32::try_from(path.len())?;
                dialog.Flags = OFN_EXPLORER
                    | OFN_FILEMUSTEXIST
                    | OFN_PATHMUSTEXIST
                    | OFN_NOCHANGEDIR
                    | OFN_DONTADDTORECENT;
                if GetOpenFileNameW(&raw mut dialog) == 0 {
                    if CommDlgExtendedError() != 0 {
                        bail!("Cannot open the subtitle file picker");
                    }
                    return Ok(None);
                }
            }
            let end = path.iter().position(|&c| c == 0).unwrap_or(path.len());
            let value = path
                .get(..end)
                .ok_or_else(|| anyhow::anyhow!("Invalid subtitle file path"))?;
            Ok(Some(PathBuf::from(String::from_utf16(value)?)))
        })
        .map_err(anyhow::Error::msg)?
}
