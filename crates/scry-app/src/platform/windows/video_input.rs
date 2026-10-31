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
            WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCDESTROY, WS_CHILD, WS_CLIPSIBLINGS,
            WS_EX_LAYERED, WindowFromPoint,
        },
    },
};

const SUBCLASS: usize = 0x5343_5650;

struct Input {
    host: Host,
    clicks: Clicks,
}

#[derive(Default)]
struct Clicks {
    down: Option<Press>,
    first: Option<(Instant, [f64; 2])>,
}

struct Press {
    at: Instant,
    position: [f64; 2],
    double: bool,
    moved: bool,
}

impl Clicks {
    fn down(&mut self, position: [f64; 2], at: Instant, interval: Duration) {
        let double = self.first.take().is_some_and(|(first, point)| {
            at.saturating_duration_since(first) <= interval && Self::near(point, position)
        });
        self.down = Some(Press {
            at,
            position,
            double,
            moved: false,
        });
    }

    fn near(first: [f64; 2], second: [f64; 2]) -> bool {
        // Match Rune's click tolerance in logical points, not device pixels.
        (first[0] - second[0]).abs() <= 4.0 && (first[1] - second[1]).abs() <= 4.0
    }

    fn moved(&mut self, position: [f64; 2]) {
        if let Some(down) = &mut self.down {
            down.moved |= !Self::near(down.position, position);
        }
    }

    fn up(
        &mut self,
        position: [f64; 2],
        at: Instant,
        interval: Duration,
        inside: bool,
    ) -> Option<VideoClick> {
        self.moved(position);
        let down = self.down.take()?;
        if down.moved || !inside {
            self.first = None;
            return None;
        }
        // Rune measures pairs from pointer-down and excludes held presses from pairing.
        let click = at.saturating_duration_since(down.at) <= interval;
        let double = click && down.double;
        self.first = (click && !double).then_some((down.at, down.position));
        Some(VideoClick { at, double })
    }

    fn cancel(&mut self) {
        if self.down.take().is_some() {
            self.first = None;
        }
    }
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
    let data = Box::into_raw(Box::new(Input {
        host,
        clicks: Clicks::default(),
    }));
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
    let scale = f64::from(f32::from_bits(input.host.0.scale.load(Ordering::Acquire))).max(1.0);
    let logical = position.map(|point| point / scale);
    match message {
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
            input
                .clicks
                .down(logical, Instant::now(), double_click_interval());
            // SAFETY: hwnd belongs to this callback's UI thread. The input
            // borrow is no longer used before this potentially reentrant call.
            unsafe {
                SetCapture(hwnd);
            }
            return 0;
        }
        WM_MOUSEMOVE => input.clicks.moved(logical),
        WM_LBUTTONUP => {
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            // SAFETY: hwnd is the live input child on this callback's thread;
            // rect is writable and is not retained by GetClientRect.
            let inside = unsafe { GetClientRect(hwnd, &raw mut rect) } != 0
                && position[0] >= 0.0
                && position[1] >= 0.0
                && position[0] < f64::from(rect.right)
                && position[1] < f64::from(rect.bottom);
            let click = input
                .clicks
                .up(logical, Instant::now(), double_click_interval(), inside);
            let host = input.host.clone();
            // SAFETY: this input child and its parent live on the callback's UI
            // thread. The mutable input borrow ends before SetFocus, which can
            // reenter this procedure. Keep keyboard shortcuts with the app.
            unsafe {
                ReleaseCapture();
                SetFocus(GetParent(hwnd));
            }
            if let Some(click) = click
                && let Ok(mut clicks) = host.0.clicks.lock()
                && clicks.len() < 8
            {
                clicks.push_back(click);
            }
            return 0;
        }
        WM_CAPTURECHANGED => input.clicks.cancel(),
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
    controls: Option<[i32; 5]>,
    overlay: Option<[i32; 5]>,
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "Rounded native pixel height intentionally saturates to i32"
)]
pub(super) fn cover(
    host: &Host,
    controls: Option<crate::platform::VideoOverlay>,
    overlay: Option<crate::platform::VideoOverlay>,
) {
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
        let pixels = |overlay: crate::platform::VideoOverlay| {
            let [x, y, w, h] = overlay.rect;
            [
                x.round() as i32,
                y.round() as i32,
                (x + w).round() as i32,
                (y + h).round() as i32,
                (overlay.radius * 2.0).round().max(0.0) as i32,
            ]
        };
        let controls = controls.map(pixels);
        let overlay = overlay.map(pixels);
        let next = Clip {
            width: rect.right,
            height: rect.bottom,
            controls,
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
            if controls.is_none() && overlay.is_none() {
                success &= SetWindowRgn(window, ptr::null_mut(), 1) != 0;
                continue;
            }
            let region = CreateRectRgn(0, 0, rect.right, rect.bottom);
            if region.is_null() {
                success = false;
                continue;
            }
            let mut combined_ok = true;
            for [left, top, right, bottom, diameter] in [controls, overlay].into_iter().flatten() {
                let hole = CreateRoundRectRgn(left, top, right, bottom, diameter, diameter);
                if hole.is_null() {
                    combined_ok = false;
                    break;
                }
                let combined = CombineRgn(region, region, hole, RGN_DIFF);
                DeleteObject(hole);
                if combined == 0 {
                    combined_ok = false;
                    break;
                }
            }
            if !combined_ok {
                DeleteObject(region);
                success = false;
                continue;
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
    use anyhow::Context;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, SendMessageW, WS_POPUP,
    };

    #[test]
    fn clicks_emit_immediately_and_pair_from_press_time() -> Result<()> {
        let mut clicks = Clicks::default();
        let now = Instant::now();
        let interval = Duration::from_millis(500);
        let position = [10.0, 20.0];
        let mut doubles = Vec::new();
        for millis in [0, 200, 400, 600] {
            let at = now + Duration::from_millis(millis);
            clicks.down(position, at, interval);
            let event = clicks
                .up(position, at, interval, true)
                .context("Missing click")?;
            assert_eq!(
                event.at, at,
                "Each release produces a click without a timer"
            );
            doubles.push(event.double);
        }
        assert_eq!(doubles, [false, true, false, true]);

        clicks.down(position, now + Duration::from_secs(2), interval);
        clicks
            .up(position, now + Duration::from_millis(2400), interval, true)
            .context("Missing held click")?;
        clicks.down(position, now + Duration::from_millis(2600), interval);
        let event = clicks
            .up(position, now + Duration::from_millis(2600), interval, true)
            .context("Missing second click")?;
        assert!(!event.double, "Pair timing starts at press, not release");
        Ok(())
    }

    #[test]
    fn separated_and_held_clicks_do_not_pair() -> Result<()> {
        let now = Instant::now();
        let interval = Duration::from_millis(500);
        let position = [10.0, 20.0];
        for (second, delay, held) in [
            ([15.0, 20.0], 200, 0),
            ([10.0, 25.0], 200, 0),
            (position, 501, 0),
            (position, 200, 501),
        ] {
            let mut clicks = Clicks::default();
            clicks.down(position, now, interval);
            clicks
                .up(position, now, interval, true)
                .context("Missing first click")?;
            let at = now + Duration::from_millis(delay);
            clicks.down(second, at, interval);
            let event = clicks
                .up(second, at + Duration::from_millis(held), interval, true)
                .context("Missing second click")?;
            assert!(
                !event.double,
                "position={second:?}, delay={delay}, held={held}"
            );
        }
        Ok(())
    }

    #[test]
    fn drags_cancelled_presses_and_outside_releases_break_pairs() -> Result<()> {
        let now = Instant::now();
        let interval = Duration::from_millis(500);
        let position = [10.0, 20.0];
        for kind in 0..3 {
            let mut clicks = Clicks::default();
            clicks.down(position, now, interval);
            clicks
                .up(position, now, interval, true)
                .context("Missing first click")?;
            let at = now + Duration::from_millis(100);
            clicks.down(position, at, interval);
            match kind {
                0 => clicks.moved([30.0, 20.0]),
                1 => clicks.cancel(),
                _ => {}
            }
            assert!(clicks.up(position, at, interval, kind != 2).is_none());
            let at = now + Duration::from_millis(200);
            clicks.down(position, at, interval);
            assert!(
                !clicks
                    .up(position, at, interval, true)
                    .context("Missing click after cancellation")?
                    .double
            );
        }
        Ok(())
    }

    #[test]
    fn overlay_preserves_video_at_rounded_corners_and_restores_video_on_dismiss() -> Result<()> {
        use windows_sys::Win32::Graphics::Gdi::{GetWindowRgn, PtInRegion};
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        // Match the embedded host's borderless client area, including its bottom edge.
        // SAFETY: built-in class and NUL-terminated string, no borrowed creation data.
        // The windows are owned and destroyed by this test thread below.
        let child = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                ptr::null(),
                WS_POPUP,
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
        let controls = Some(crate::platform::VideoOverlay {
            rect: [20.0, 170.0, 160.0, 20.0],
            radius: 3.0,
        });
        host.cover(
            controls,
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
                checks.push(PtInRegion(region, 50, 180) == 0);
                checks.push(PtInRegion(region, 50, 195) != 0);
                checks.push(PtInRegion(region, 10, 180) != 0);
            }
            host.cover(controls, None, 1.0);
            for window in [child, input] {
                checks.push(GetWindowRgn(window, region) != 0);
                checks.push(PtInRegion(region, 120, 100) != 0);
                checks.push(PtInRegion(region, 50, 180) == 0);
            }
            host.cover(None, None, 1.0);
            for window in [child, input] {
                checks.push(GetWindowRgn(window, region) == 0);
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
            SendMessageW(hwnd, WM_LBUTTONDBLCLK, 0, 0x000a | (0x0014 << 16));
            SendMessageW(hwnd, WM_LBUTTONUP, 0, 0x000a | (0x0014 << 16));
            SendMessageW(hwnd, WM_LBUTTONDOWN, 0, 0x000a | (0x0014 << 16));
            SendMessageW(hwnd, WM_LBUTTONUP, 0, 0x0032 | (0x0014 << 16));
            DestroyWindow(hwnd);
        }
        let clicks = host.video_clicks();
        assert_eq!(
            clicks.iter().map(|c| c.double).collect::<Vec<_>>(),
            [false, true]
        );
        assert!(host.video_clicks().is_empty());
        Ok(())
    }
}
