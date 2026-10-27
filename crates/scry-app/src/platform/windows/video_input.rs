//! Input and UI occlusion for the native video child, without reentrant UI updates.
use super::Host;
use crate::platform::{VideoClick, VideoPointer};
use anyhow::{Result, bail};
use std::{
    ptr,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Gdi::{
        CombineRgn, CreateRectRgn, CreateRoundRectRgn, DeleteObject, RGN_DIFF, ScreenToClient,
        SetWindowRgn,
    },
    System::SystemServices::SS_NOTIFY,
    UI::{
        Input::KeyboardAndMouse::{GetDoubleClickTime, ReleaseCapture, SetCapture, SetFocus},
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, GA_ROOT, GetAncestor, GetClientRect, GetCursorPos,
            GetParent, LWA_ALPHA, SetLayeredWindowAttributes, WM_CAPTURECHANGED, WM_LBUTTONDBLCLK,
            WM_LBUTTONDOWN, WM_LBUTTONUP, WM_NCDESTROY, WS_CHILD, WS_CLIPSIBLINGS, WS_EX_LAYERED,
            WindowFromPoint,
        },
    },
};

const SUBCLASS: usize = 0x5343_5650;

struct Input {
    host: Host,
    down: Option<[f64; 2]>,
}

pub(super) fn create(parent: HWND, host: Host) -> Result<HWND> {
    let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
    // mpv owns a nested video window on another thread. A nearly transparent
    // sibling receives input without subclassing that foreign-thread window.
    // Alpha must be nonzero: Windows passes clicks through zero-alpha pixels.
    // SAFETY: parent is live on this UI thread, class is NUL-terminated, and
    // Windows owns this child until its parent is destroyed.
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED,
            class.as_ptr(),
            ptr::null(),
            WS_CHILD | WS_CLIPSIBLINGS | SS_NOTIFY,
            0,
            0,
            1,
            1,
            parent,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null(),
        )
    };
    if hwnd.is_null() {
        bail!(
            "Cannot create video input layer: {}",
            std::io::Error::last_os_error()
        );
    }
    // SAFETY: hwnd is the live child just created; no borrowed data is retained.
    let layered = unsafe { SetLayeredWindowAttributes(hwnd, 0, 1, LWA_ALPHA) };
    if layered == 0 || attach(hwnd, host).is_err() {
        // SAFETY: initialization failed; no caller owns this UI-thread child.
        unsafe {
            DestroyWindow(hwnd);
        }
        bail!("Cannot initialize video input layer");
    }
    Ok(hwnd)
}

pub(super) fn attach(hwnd: HWND, host: Host) -> Result<()> {
    let data = Box::into_raw(Box::new(Input { host, down: None }));
    // SAFETY: called on the child's owning UI thread. The subclass owns data
    // until WM_NCDESTROY; failure returns ownership to this function.
    if unsafe { SetWindowSubclass(hwnd, Some(video_proc), SUBCLASS, data as usize) } == 0 {
        // SAFETY: failed installation cannot retain or free this allocation.
        unsafe {
            drop(Box::from_raw(data));
        }
        bail!("Cannot register video mouse input");
    }
    Ok(())
}

unsafe extern "system" fn video_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    data: usize,
) -> LRESULT {
    if message == WM_NCDESTROY {
        // SAFETY: remove the callback before releasing the allocation it owns.
        unsafe {
            RemoveWindowSubclass(hwnd, Some(video_proc), id);
            drop(Box::from_raw(data as *mut Input));
            return DefSubclassProc(hwnd, message, wparam, lparam);
        }
    }
    // SAFETY: a successful attach keeps this allocation alive for the callback.
    let input = unsafe { &mut *(data as *mut Input) };
    let packed = lparam.to_le_bytes();
    let position = [
        f64::from(i16::from_le_bytes([packed[0], packed[1]])),
        f64::from(i16::from_le_bytes([packed[2], packed[3]])),
    ];
    match message {
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
            input.down = Some(position);
            // SAFETY: hwnd belongs to this callback's UI thread. The input
            // borrow is no longer used before this potentially reentrant call.
            unsafe {
                SetCapture(hwnd);
            }
            return 0;
        }
        WM_LBUTTONUP => {
            let down = input.down.take();
            let host = input.host.clone();
            // SAFETY: this input child and its parent live on the callback's UI
            // thread. The mutable input borrow ends before SetFocus, which can
            // reenter this procedure. Keep keyboard shortcuts with the app.
            unsafe {
                ReleaseCapture();
                SetFocus(GetParent(hwnd));
            }
            if let Some(down) = down
                && (down[0] - position[0]).abs() <= 4.0
                && (down[1] - position[1]).abs() <= 4.0
                && let Ok(mut clicks) = host.0.clicks.lock()
                && clicks.len() < 8
            {
                clicks.push_back(VideoClick {
                    at: Instant::now(),
                    position,
                });
            }
            return 0;
        }
        WM_CAPTURECHANGED => input.down = None,
        _ => {}
    }
    // SAFETY: forward unchanged parameters from this live native callback.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

pub(super) fn double_click_interval() -> Duration {
    // SAFETY: this reads a system setting and takes no pointers.
    Duration::from_millis(u64::from(unsafe { GetDoubleClickTime() }))
}

pub(super) fn pointer_position(host: &Host, scale: f32) -> VideoPointer {
    let child = host.0.child.load(Ordering::Acquire) as HWND;
    if child.is_null() {
        return VideoPointer::Unavailable;
    }
    let mut point = POINT { x: 0, y: 0 };
    // SAFETY: the child and its parent are live UI-thread windows. The output
    // POINT is initialized and borrowed only for each synchronous call.
    unsafe {
        let parent = GetParent(child);
        if GetCursorPos(&raw mut point) == 0
            || GetAncestor(WindowFromPoint(point), GA_ROOT) != parent
        {
            return VideoPointer::Outside;
        }
        if ScreenToClient(parent, &raw mut point) == 0 {
            return VideoPointer::Outside;
        }
    }
    VideoPointer::Inside([
        f64::from(point.x) / f64::from(scale),
        f64::from(point.y) / f64::from(scale),
    ])
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Clip {
    width: i32,
    height: i32,
    bottom: i32,
    overlay: Option<[i32; 5]>,
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "Rounded native pixel height intentionally saturates to i32"
)]
pub(super) fn cover(host: &Host, bottom: f32, overlay: Option<crate::platform::VideoOverlay>) {
    let child = host.0.child.load(Ordering::Acquire) as HWND;
    if child.is_null() {
        return;
    }
    let Ok(mut clip) = host.0.clip.lock() else {
        return;
    };
    // SAFETY: the child is live on this UI thread. GetClientRect writes a local
    // RECT. Regions are local owned GDI objects; CombineRgn copies their data.
    // A successful SetWindowRgn transfers region ownership to Windows;
    // failure leaves ownership here, so only that path deletes the final region.
    unsafe {
        let mut rect: RECT = std::mem::zeroed();
        if GetClientRect(child, &raw mut rect) == 0 {
            return;
        }
        let bottom = bottom.round().max(0.0) as i32;
        let overlay = overlay.map(|overlay| {
            let [x, y, w, h] = overlay.rect;
            [
                x.round() as i32,
                y.round() as i32,
                (x + w).round() as i32,
                (y + h).round() as i32,
                (overlay.radius * 2.0).round().max(0.0) as i32,
            ]
        });
        let next = Clip {
            width: rect.right,
            height: rect.bottom,
            bottom,
            overlay,
        };
        if *clip == Some(next) {
            return;
        }
        let mut success = true;
        for window in [child, host.0.input.load(Ordering::Acquire) as HWND] {
            if window.is_null() {
                continue;
            }
            if bottom == 0 && overlay.is_none() {
                success &= SetWindowRgn(window, ptr::null_mut(), 1) != 0;
                continue;
            }
            let region = CreateRectRgn(0, 0, rect.right, rect.bottom.saturating_sub(bottom).max(0));
            if region.is_null() {
                success = false;
                continue;
            }
            if let Some([left, top, right, bottom, diameter]) = overlay {
                let hole = CreateRoundRectRgn(left, top, right, bottom, diameter, diameter);
                if hole.is_null() {
                    DeleteObject(region);
                    success = false;
                    continue;
                }
                let combined = CombineRgn(region, region, hole, RGN_DIFF);
                DeleteObject(hole);
                if combined == 0 {
                    DeleteObject(region);
                    success = false;
                    continue;
                }
            }
            if SetWindowRgn(window, region, 1) == 0 {
                DeleteObject(region);
                success = false;
            }
        }
        if success {
            *clip = Some(next);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::VideoHost;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, SendMessageW,
    };

    #[test]
    fn overlay_preserves_video_at_rounded_corners_and_restores_video_on_dismiss() -> Result<()> {
        use windows_sys::Win32::Graphics::Gdi::{GetWindowRgn, PtInRegion};
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        // SAFETY: built-in class and NUL-terminated string, no borrowed creation data.
        // The windows are owned and destroyed by this test thread below.
        let child = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                ptr::null(),
                0,
                0,
                0,
                200,
                200,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            )
        };
        anyhow::ensure!(!child.is_null(), "Cannot create clipping test window");
        let host = Host::default();
        host.0.child.store(child as isize, Ordering::Release);
        let input = create(child, host.clone())?;
        host.0.input.store(input as isize, Ordering::Release);
        host.place([0.0, 0.0, 200.0, 200.0], 1.0);
        host.cover(
            20.0,
            Some(crate::platform::VideoOverlay {
                rect: [100.0, 60.0, 80.0, 100.0],
                radius: 12.0,
            }),
            1.0,
        );
        // SAFETY: the region is owned locally; both windows are live on this thread.
        // GetWindowRgn copies rather than transfers the window's region.
        let checks = unsafe {
            let region = CreateRectRgn(0, 0, 0, 0);
            let mut checks = Vec::new();
            for window in [child, input] {
                checks.push(GetWindowRgn(window, region) != 0);
                checks.push(PtInRegion(region, 50, 100) != 0);
                checks.push(PtInRegion(region, 120, 100) == 0);
                checks.push(PtInRegion(region, 100, 60) != 0);
                checks.push(PtInRegion(region, 112, 62) == 0);
                checks.push(PtInRegion(region, 50, 190) == 0);
            }
            host.cover(20.0, None, 1.0);
            for window in [child, input] {
                checks.push(GetWindowRgn(window, region) != 0);
                checks.push(PtInRegion(region, 120, 100) != 0);
                checks.push(PtInRegion(region, 50, 190) == 0);
            }
            DeleteObject(region);
            DestroyWindow(child);
            checks
        };
        assert!(
            checks.iter().all(|passed| *passed),
            "Clipping checks: {checks:?}"
        );
        Ok(())
    }

    #[test]
    fn layered_video_input_child_can_be_created() -> Result<()> {
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        // SAFETY: the built-in class name is NUL-terminated and this thread owns
        // the returned parent and its children until DestroyWindow below.
        let parent = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                ptr::null(),
                0,
                0,
                0,
                100,
                100,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            )
        };
        anyhow::ensure!(!parent.is_null(), "Cannot create video layer test parent");
        let result = create(parent, Host::default());
        // SAFETY: destroys the test-owned parent and any successfully created child.
        unsafe {
            DestroyWindow(parent);
        }
        assert!(
            result.is_ok(),
            "{}",
            result.err().map_or_else(String::new, |e| e.to_string())
        );
        Ok(())
    }

    #[test]
    fn native_video_clicks_are_queued_once_and_drags_are_ignored() -> Result<()> {
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        // SAFETY: STATIC is a built-in class, the string is NUL-terminated,
        // and the returned window is owned and destroyed on this test thread.
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                ptr::null(),
                0,
                0,
                0,
                100,
                100,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            )
        };
        anyhow::ensure!(!hwnd.is_null(), "Cannot create video input test window");
        let host = Host::default();
        attach(hwnd, host.clone())?;
        // SAFETY: all messages use a live test-owned window and integer payloads.
        unsafe {
            SendMessageW(hwnd, WM_LBUTTONDOWN, 0, 0x000a | (0x0014 << 16));
            SendMessageW(hwnd, WM_LBUTTONUP, 0, 0x000a | (0x0014 << 16));
            SendMessageW(hwnd, WM_LBUTTONDOWN, 0, 0x000a | (0x0014 << 16));
            SendMessageW(hwnd, WM_LBUTTONUP, 0, 0x0032 | (0x0014 << 16));
            DestroyWindow(hwnd);
        }
        let clicks = host.video_clicks();
        assert_eq!(clicks.len(), 1);
        assert_eq!(clicks.first().map(|c| c.position), Some([10.0, 20.0]));
        assert!(host.video_clicks().is_empty());
        Ok(())
    }
}
