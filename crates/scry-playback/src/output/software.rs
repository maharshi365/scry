//! Software render-API bridge. No native window pointer crosses threads.

use anyhow::{Context, Result, bail};
use libloading::Library;
use std::{
    ffi::{c_int, c_void},
    path::Path,
    ptr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
};

/// A bounded, replaceable video surface delivered to a compositor.
#[derive(Clone)]
pub struct Surface {
    size: Arc<Mutex<[u32; 2]>>,
    wake: Arc<Mutex<Option<mpsc::SyncSender<()>>>>,
    present: Arc<dyn Fn(u32, u32, Vec<u8>) + Send + Sync>,
}

impl Surface {
    /// Create a surface whose callback replaces, rather than queues, the displayed frame.
    pub fn new(present: impl Fn(u32, u32, Vec<u8>) + Send + Sync + 'static) -> Self {
        Self {
            size: Arc::new(Mutex::new([640, 360])),
            wake: Arc::default(),
            present: Arc::new(present),
        }
    }

    /// Set physical-pixel dimensions, capped at 1280×720 to bound CPU rendering work.
    pub fn resize(&self, width: u32, height: u32) {
        if let Ok(mut size) = self.size.lock() {
            let next = bounded_size(width, height);
            if *size == next {
                return;
            }
            *size = next;
        }
        // A paused video still needs a redraw when its layout changes.
        if let Ok(wake) = self.wake.lock()
            && let Some(wake) = wake.as_ref()
        {
            let _ = wake.try_send(());
        }
    }
}

fn bounded_size(width: u32, height: u32) -> [u32; 2] {
    let width = u64::from(width.max(1));
    let height = u64::from(height.max(1));
    let (width, height) = if width <= 1280 && height <= 720 {
        (width, height)
    } else if width * 720 >= height * 1280 {
        (1280, height * 1280 / width)
    } else {
        (width * 720 / height, 720)
    };
    // Both quotients are bounded by the limits above and fit in u32.
    [
        u32::try_from(width).unwrap_or(1).max(1),
        u32::try_from(height).unwrap_or(1).max(1),
    ]
}

#[repr(C)]
struct Param {
    kind: c_int,
    data: *mut c_void,
}

impl Param {
    fn end() -> Self {
        Self {
            kind: 0,
            data: ptr::null_mut(),
        }
    }
}

type Create = unsafe extern "C" fn(*mut *mut c_void, *mut c_void, *mut Param) -> c_int;
type Update = unsafe extern "C" fn(*mut c_void) -> u64;
type Render = unsafe extern "C" fn(*mut c_void, *mut Param) -> c_int;
type Free = unsafe extern "C" fn(*mut c_void);
type SetUpdate = unsafe extern "C" fn(*mut c_void, Option<extern "C" fn(*mut c_void)>, *mut c_void);

extern "C" fn frame_ready(data: *mut c_void) {
    // SAFETY: ContextState registers a pointer to its stable boxed sender and
    // unregisters the callback before freeing the box. The callback never calls
    // mpv or blocks on a full queue: multiple frame notifications coalesce.
    let wake = unsafe { &*data.cast::<mpsc::SyncSender<()>>() };
    let _ = wake.try_send(());
}

pub(super) struct Renderer {
    wake: mpsc::SyncSender<()>,
    stopping: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Renderer {
    pub(super) fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }
    pub(super) fn start(handle: *mut c_void, path: &Path, surface: Surface) -> Result<Self> {
        let (wake, updates) = mpsc::sync_channel(1);
        let stopping = Arc::new(AtomicBool::new(false));
        let stop_requested = stopping.clone();
        let frame_wake = wake.clone();
        *surface
            .wake
            .lock()
            .map_err(|_| anyhow::anyhow!("Render wake lock failed"))? = Some(wake.clone());
        let (ready, initialized) = mpsc::sync_channel(1);
        let path = path.to_owned();
        let address = handle as usize;
        let thread = thread::Builder::new()
            .name("scry-software-video".into())
            .spawn(move || {
                let context = ContextState::open(address as *mut c_void, &path, frame_wake);
                match context {
                    Ok(context) => {
                        if ready.send(Ok(())).is_ok() {
                            context.run(&updates, &stop_requested, &surface);
                        }
                    }
                    Err(error) => {
                        let _ = ready.send(Err(error));
                    }
                }
            })?;
        let renderer = Self {
            wake,
            stopping,
            thread: Some(thread),
        };
        initialized
            .recv()
            .context("Software renderer stopped during initialization")??;
        Ok(renderer)
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        let _ = self.wake.try_send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct ContextState {
    context: *mut c_void,
    update: Update,
    render: Render,
    free: Free,
    set_update: SetUpdate,
    _wake: Box<mpsc::SyncSender<()>>,
    _library: Library,
}

impl ContextState {
    fn open(handle: *mut c_void, path: &Path, wake: mpsc::SyncSender<()>) -> Result<Self> {
        // SAFETY: Player keeps the initialized mpv core alive until Renderer joins.
        // Only this thread uses the render context; it never calls client APIs.
        // Function signatures and parameter tags match mpv/render.h. The library is
        // retained until the context is freed; creation retains no parameter pointers.
        unsafe {
            let library = Library::new(path).context("Cannot load software render API")?;
            let create: Create = *library.get(b"mpv_render_context_create\0")?;
            let update = *library.get(b"mpv_render_context_update\0")?;
            let render = *library.get(b"mpv_render_context_render\0")?;
            let free = *library.get(b"mpv_render_context_free\0")?;
            let set_update: SetUpdate =
                *library.get(b"mpv_render_context_set_update_callback\0")?;
            let mut context = ptr::null_mut();
            let mut params = [
                Param {
                    kind: 1,
                    data: c"sw".as_ptr().cast_mut().cast(),
                },
                Param::end(),
            ];
            if create(&raw mut context, handle, params.as_mut_ptr()) < 0 {
                bail!("Cannot initialize software video renderer");
            }
            let mut wake = Box::new(wake);
            set_update(
                context,
                Some(frame_ready),
                ptr::from_mut(wake.as_mut()).cast(),
            );
            Ok(Self {
                context,
                update,
                render,
                free,
                set_update,
                _wake: wake,
                _library: library,
            })
        }
    }

    fn run(&self, updates: &mpsc::Receiver<()>, stopping: &AtomicBool, surface: &Surface) {
        let mut pixels = Vec::<u32>::new();
        let mut previous_size = None;
        while updates.recv().is_ok() {
            if stopping.load(Ordering::Acquire) {
                break;
            }
            let Ok(size) = surface.size.lock().map(|size| *size) else {
                break;
            };
            // SAFETY: this live context is exclusively used on this render thread.
            let has_frame = unsafe { (self.update)(self.context) } & 1 != 0;
            if !has_frame && previous_size == Some(size) {
                continue;
            }
            previous_size = Some(size);
            let [width, height] = size;
            let mut dimensions = [
                c_int::try_from(width).unwrap_or(1),
                c_int::try_from(height).unwrap_or(1),
            ];
            let mut stride = width as usize * 4;
            pixels.resize(width as usize * height as usize, 0);
            let mut params = [
                Param {
                    kind: 17,
                    data: dimensions.as_mut_ptr().cast(),
                },
                Param {
                    kind: 18,
                    data: c"rgb0".as_ptr().cast_mut().cast(),
                },
                Param {
                    kind: 19,
                    data: ptr::from_mut(&mut stride).cast(),
                },
                Param {
                    kind: 20,
                    data: pixels.as_mut_ptr().cast(),
                },
                Param::end(),
            ];
            // SAFETY: dimensions are bounded positive values; the u32 buffer is
            // pixel-aligned and writable for stride * height bytes. All parameter
            // pointers remain live for this synchronous call. No client API or
            // UI lock is held while rendering, including its audio-sync wait.
            if unsafe { (self.render)(self.context, params.as_mut_ptr()) } < 0 {
                break;
            }
            let rgba = opaque_rgba(&mut pixels);
            (surface.present)(width, height, rgba);
        }
    }
}

impl Drop for ContextState {
    fn drop(&mut self) {
        // SAFETY: this thread owns the context, Player has not destroyed the core,
        // and the library is still loaded. Unregister synchronizes with any
        // in-flight callback before its boxed sender can be dropped.
        unsafe {
            (self.set_update)(self.context, None, ptr::null_mut());
            (self.free)(self.context);
        }
    }
}

fn opaque_rgba(pixels: &mut [u32]) -> Vec<u8> {
    let alpha = u32::from_ne_bytes([0, 0, 0, 255]);
    for pixel in pixels.iter_mut() {
        *pixel |= alpha;
    }
    bytemuck::cast_slice(pixels).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_conversion_preserves_rgb_and_replaces_every_padding_byte() {
        let mut pixels = [[1, 2, 3, 0], [254, 128, 4, 99], [0, 0, 0, 255]].map(u32::from_ne_bytes);
        assert_eq!(
            opaque_rgba(&mut pixels),
            [1, 2, 3, 255, 254, 128, 4, 255, 0, 0, 0, 255]
        );
        assert!(opaque_rgba(&mut []).is_empty());
    }

    #[test]
    fn render_sizes_bound_retina_and_invalid_layouts() {
        assert_eq!(bounded_size(0, 0), [1, 1]);
        assert_eq!(bounded_size(1280, 720), [1280, 720]);
        assert_eq!(bounded_size(3840, 2160), [1280, 720]);
        assert_eq!(bounded_size(2560, 1600), [1152, 720]);
        assert_eq!(bounded_size(1281, 720), [1280, 719]);
        for (width, height) in [(u32::MAX, 1), (1, u32::MAX), (u32::MAX, u32::MAX)] {
            let [w, h] = bounded_size(width, height);
            assert!((1..=1280).contains(&w) && (1..=720).contains(&h));
        }
    }
}
