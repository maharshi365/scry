//! Small dynamic binding to libmpv's stable C client API. No mpv subprocess or webview.
pub mod audio;
mod cache_budget;
#[cfg(target_os = "macos")]
pub mod macos;
mod process;
pub mod subtitles;
pub mod telemetry;

use crate::telemetry::Sample;
use anyhow::{Context, Result, bail};
use libloading::Library;
use std::{
    cell::Cell,
    ffi::{CStr, CString, c_char, c_int, c_void},
    path::{Path, PathBuf},
    ptr,
};

#[repr(C)]
struct Event {
    id: c_int,
    error: c_int,
    userdata: u64,
    data: *mut c_void,
}
#[repr(C)]
struct EndFile {
    reason: c_int,
    error: c_int,
}
type Create = unsafe extern "C" fn() -> *mut c_void;
type Initialize = unsafe extern "C" fn(*mut c_void) -> c_int;
type Destroy = unsafe extern "C" fn(*mut c_void);
type SetOption = unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int;
type Command = unsafe extern "C" fn(*mut c_void, *const *const c_char) -> c_int;
type GetProperty = unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_char;
type GetNode = unsafe extern "C" fn(*mut c_void, *const c_char, c_int, *mut c_void) -> c_int;
type FreeNode = unsafe extern "C" fn(*mut Node);
type Free = unsafe extern "C" fn(*mut c_void);
type WaitEvent = unsafe extern "C" fn(*mut c_void, f64) -> *const Event;

// Stable mpv_node ABI from client.h. Only the integer member of the cache map
// is inspected; mpv owns and frees the entire returned tree.
#[repr(C)]
union NodeValue {
    integer: i64,
    list: *mut NodeList,
}

#[repr(C)]
struct Node {
    value: NodeValue,
    format: c_int,
}

#[repr(C)]
struct NodeList {
    count: c_int,
    values: *mut Node,
    keys: *mut *mut c_char,
}

pub struct Player {
    #[cfg(target_os = "macos")]
    renderer: Option<macos::Renderer>,
    handle: *mut c_void,
    _library: Library,
    destroy: Destroy,
    command: Command,
    get: GetProperty,
    get_node: GetNode,
    free_node: FreeNode,
    free: Free,
    wait: WaitEvent,
    cache_budget: Cell<Option<cache_budget::CacheBudget>>,
}

#[derive(Debug, PartialEq)]
pub enum PlaybackEvent {
    Loaded,
    Ready,
    Ended,
    Failed,
    Shutdown,
}

#[derive(Clone)]
#[cfg_attr(not(target_os = "macos"), derive(Copy))]
pub enum PlaybackTarget {
    NativeWindow(u64),
    DecodeOnly,
    #[cfg(target_os = "macos")]
    Macos(macos::Surface),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum VideoRenderer {
    Gpu,
    GpuNext,
}

impl VideoRenderer {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "gpu" => Ok(Self::Gpu),
            "gpu-next" => Ok(Self::GpuNext),
            _ => bail!("Scry_VIDEO_RENDERER must be gpu or gpu-next"),
        }
    }

    fn from_env() -> Result<Self> {
        match std::env::var("Scry_VIDEO_RENDERER") {
            Ok(value) => Self::parse(&value),
            // gpu saved working set but increased private commit in matched probes.
            Err(std::env::VarError::NotPresent) => Ok(Self::GpuNext),
            Err(_) => bail!("Scry_VIDEO_RENDERER must be gpu or gpu-next"),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Gpu => "gpu",
            Self::GpuNext => "gpu-next",
        }
    }
}

pub trait PlaybackEngine {
    fn property(&self, name: &str) -> Option<String>;
    /// # Errors
    /// Returns an error if the engine cannot load the source.
    fn load(&self, url: &str) -> Result<()>;
    /// # Errors
    /// Returns an error if the engine rejects the command.
    fn command(&self, args: &[&str]) -> Result<()>;
    fn event(&self, timeout: f64) -> Option<PlaybackEvent>;
    fn sample(&self, elapsed_s: f64) -> Sample;
}

impl Player {
    /// # Errors
    /// Returns an error if the runtime is missing, its API cannot be loaded,
    /// renderer settings are invalid, or player initialization fails.
    pub fn open(target: PlaybackTarget) -> Result<Self> {
        let renderer = match &target {
            PlaybackTarget::NativeWindow(_) => Some(VideoRenderer::from_env()?),
            PlaybackTarget::DecodeOnly => None,
            #[cfg(target_os = "macos")]
            PlaybackTarget::Macos(_) => None,
        };
        let path = library_path()?;
        // SAFETY: the library remains alive for all resolved functions and the mpv handle.
        unsafe {
            let library = Library::new(&path).context(
                "Cannot load libmpv. Install the matching 64-bit runtime; see README.md",
            )?;
            let create: Create = *library.get(b"mpv_create\0")?;
            let initialize: Initialize = *library.get(b"mpv_initialize\0")?;
            let set: SetOption = *library.get(b"mpv_set_option_string\0")?;
            let destroy: Destroy = *library.get(b"mpv_terminate_destroy\0")?;
            let command: Command = *library.get(b"mpv_command\0")?;
            let get: GetProperty = *library.get(b"mpv_get_property_string\0")?;
            let get_node: GetNode = *library.get(b"mpv_get_property\0")?;
            let free_node: FreeNode = *library.get(b"mpv_free_node_contents\0")?;
            let free: Free = *library.get(b"mpv_free\0")?;
            let wait: WaitEvent = *library.get(b"mpv_wait_event\0")?;
            let handle = create();
            if handle.is_null() {
                bail!("libmpv could not allocate a player");
            }
            let player = Self {
                #[cfg(target_os = "macos")]
                renderer: None,
                handle,
                _library: library,
                destroy,
                command,
                get,
                get_node,
                free_node,
                free,
                wait,
                cache_budget: Cell::new(renderer.map(|_| cache_budget::CacheBudget::default())),
            };
            let mut options = common_options();
            if let PlaybackTarget::NativeWindow(hwnd) = target {
                options.extend([
                    ("wid", hwnd.to_string()),
                    (
                        "vo",
                        renderer
                            .context("Native renderer was not selected")?
                            .name()
                            .into(),
                    ),
                    ("hwdec", "auto-safe".into()),
                ]);
                // Keep both renderers on the same native API with D3D11 decode interop.
                #[cfg(windows)]
                options.extend([("gpu-api", "d3d11".into()), ("gpu-context", "d3d11".into())]);
            } else if matches!(target, PlaybackTarget::DecodeOnly) {
                options.extend([
                    ("vo", "null".into()),
                    ("ao", "null".into()),
                    ("hwdec", "no".into()),
                ]);
            }
            #[cfg(target_os = "macos")]
            if matches!(target, PlaybackTarget::Macos(_)) {
                options.extend([("vo", "libmpv".into()), ("hwdec", "auto-safe".into())]);
            }
            for (key, value) in options {
                let key_c = CString::new(key)?;
                let value_c = CString::new(value)?;
                if set(handle, key_c.as_ptr(), value_c.as_ptr()) < 0 {
                    bail!("libmpv rejected option {key}");
                }
            }
            #[cfg(feature = "profiling")]
            if let Ok(extra) = std::env::var("SCRY_PROFILE_MPV_OPTIONS") {
                let extra: std::collections::BTreeMap<String, String> =
                    serde_json::from_str(&extra)?;
                if extra.contains_key("demuxer-max-bytes") {
                    player.cache_budget.set(None);
                }
                for (key, value) in extra {
                    let key_c = CString::new(key)?;
                    let value_c = CString::new(value)?;
                    if set(handle, key_c.as_ptr(), value_c.as_ptr()) < 0 {
                        bail!("libmpv rejected a profiling option");
                    }
                }
            }
            if initialize(handle) < 0 {
                bail!("libmpv initialization failed");
            }
            #[cfg(target_os = "macos")]
            let mut player = player;
            #[cfg(target_os = "macos")]
            if let PlaybackTarget::Macos(surface) = target {
                player.renderer = Some(macos::Renderer::start(handle, &path, surface)?);
            }
            Ok(player)
        }
    }

    /// # Errors
    /// Returns an error for arguments containing NUL or a rejected player command.
    pub fn command(&self, args: &[&str]) -> Result<()> {
        let strings = args
            .iter()
            .map(|s| CString::new(*s))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut pointers: Vec<_> = strings.iter().map(|s| s.as_ptr()).collect();
        pointers.push(ptr::null());
        // SAFETY: mpv_command consumes the null-terminated argument array synchronously.
        if unsafe { (self.command)(self.handle, pointers.as_ptr()) } < 0 {
            bail!("Player command failed");
        }
        Ok(())
    }
    /// # Errors
    /// Returns an error if the load command is invalid or rejected.
    pub fn load(&self, url: &str) -> Result<()> {
        self.command(&["loadfile", url, "replace"])
    }
    #[must_use]
    pub fn property(&self, name: &str) -> Option<String> {
        let key = CString::new(name).ok()?;
        // SAFETY: the player and library are live, and key is NUL-terminated.
        // mpv returns an owned NUL-terminated string; copy it before mpv_free.
        unsafe {
            let value = (self.get)(self.handle, key.as_ptr());
            if value.is_null() {
                return None;
            }
            let result = CStr::from_ptr(value).to_string_lossy().into_owned();
            (self.free)(value.cast::<c_void>());
            Some(result)
        }
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "Packet bytes are normally bounded to 32 MiB and sized in whole MiB, not sub-byte precision"
    )]
    fn cache_forward_bytes(&self) -> Option<f64> {
        // SAFETY: mpv fills a node with its documented ABI. Read the map only
        // after a successful request, and release it before returning.
        unsafe {
            let mut node: Node = std::mem::zeroed();
            if (self.get_node)(
                self.handle,
                c"demuxer-cache-state".as_ptr(),
                6,
                (&raw mut node).cast(),
            ) < 0
            {
                return None;
            }
            let mut bytes = None;
            if node.format == 8 && !node.value.list.is_null() {
                let list = &*node.value.list;
                for index in 0..usize::try_from(list.count).unwrap_or(0) {
                    let key = *list.keys.add(index);
                    let value = &*list.values.add(index);
                    if !key.is_null() && CStr::from_ptr(key) == c"fw-bytes" && value.format == 4 {
                        bytes = u64::try_from(value.value.integer)
                            .ok()
                            .map(|value| value as f64);
                        break;
                    }
                }
            }
            (self.free_node)(&raw mut node);
            bytes
        }
    }
    #[must_use]
    pub fn event(&self, timeout: f64) -> Option<PlaybackEvent> {
        #[cfg(target_os = "macos")]
        if self
            .renderer
            .as_ref()
            .is_some_and(macos::Renderer::is_finished)
        {
            return Some(PlaybackEvent::Failed);
        }
        // SAFETY: copy event data before the next wait_event invalidates it.
        unsafe {
            let event = &*(self.wait)(self.handle, event_timeout(timeout));
            match event.id {
                1 => Some(PlaybackEvent::Shutdown),
                8 => Some(PlaybackEvent::Loaded),
                // MPV_EVENT_PLAYBACK_RESTART: initial playback/seek has resumed.
                21 => Some(PlaybackEvent::Ready),
                7 => {
                    if event.data.is_null() {
                        return Some(PlaybackEvent::Failed);
                    }
                    let end = &*(event.data as *const EndFile);
                    classify_end(end.reason, end.error)
                }
                _ => None,
            }
        }
    }
    #[must_use]
    pub fn sample(&self, elapsed_s: f64) -> Sample {
        let number = |name| {
            self.property(name)
                .and_then(|s| s.parse::<f64>().ok())
                .filter(|v| v.is_finite())
        };
        let (resident_bytes, private_bytes) = telemetry::memory();
        let cache_s = number("demuxer-cache-duration");
        let cache_forward_bytes = self.cache_forward_bytes();
        if let Some(mut budget) = self.cache_budget.get() {
            if let Some(limit) = cache_forward_bytes
                .zip(cache_s)
                .and_then(|(bytes, seconds)| budget.observe(bytes, seconds))
                && self
                    .command(&["set", "demuxer-max-bytes", &limit.to_string()])
                    .is_ok()
            {
                budget.limit = limit;
            }
            self.cache_budget.set(Some(budget));
        }
        Sample {
            elapsed_s,
            position_s: number("time-pos"),
            duration_s: number("duration"),
            cache_s,
            #[cfg(feature = "profiling")]
            cache_forward_bytes,
            #[cfg(feature = "profiling")]
            cache_limit_bytes: self.cache_budget.get().map(|budget| budget.limit),
            buffering: self.property("paused-for-cache").as_deref() == Some("yes"),
            paused: self.property("pause").as_deref() == Some("yes"),
            dropped_frames: self
                .property("frame-drop-count")
                .and_then(|v| v.parse().ok()),
            decoder_drops: self
                .property("decoder-frame-drop-count")
                .and_then(|v| v.parse().ok()),
            av_sync_s: number("avsync"),
            speed: number("speed"),
            video_bitrate: number("video-bitrate"),
            hw_decoder: self.property("hwdec-current"),
            resident_bytes,
            private_bytes,
            cpu_time_s: process::cpu_time_s(),
        }
    }
}

fn event_timeout(timeout: f64) -> f64 {
    // mpv converts seconds to an integer deadline. Keep nonfinite values out
    // of that conversion, while preserving its negative finite wait sentinel.
    if timeout.is_finite() { timeout } else { 0.0 }
}

fn classify_end(reason: c_int, error: c_int) -> Option<PlaybackEvent> {
    // Redirect is not EOF: mpv inserts the playlist entries and continues playback.
    if reason == 5 {
        return None;
    }
    Some(if reason == 4 || error < 0 {
        PlaybackEvent::Failed
    } else {
        PlaybackEvent::Ended
    })
}

impl PlaybackEngine for Player {
    fn property(&self, name: &str) -> Option<String> {
        self.property(name)
    }
    fn load(&self, url: &str) -> Result<()> {
        self.load(url)
    }
    fn command(&self, args: &[&str]) -> Result<()> {
        self.command(args)
    }
    fn event(&self, timeout: f64) -> Option<PlaybackEvent> {
        self.event(timeout)
    }
    fn sample(&self, elapsed_s: f64) -> Sample {
        self.sample(elapsed_s)
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        drop(self.renderer.take());
        // SAFETY: handle was created by this library and is destroyed exactly once,
        // before the library field is dropped and its function pointers become invalid.
        unsafe {
            (self.destroy)(self.handle);
        }
    }
}

fn common_options() -> Vec<(&'static str, String)> {
    [
        ("config", "no"),
        ("load-scripts", "no"),
        ("ytdl", "no"),
        ("terminal", "no"),
        ("input-default-bindings", "no"),
        ("osc", "no"),
        ("idle", "yes"),
        ("keep-open", "no"),
        ("cache", "yes"),
        ("demuxer-max-bytes", "32MiB"),
        ("demuxer-max-back-bytes", "2MiB"),
        ("cache-secs", "10"),
        ("network-timeout", "20"),
        ("msg-level", "all=no"),
    ]
    .into_iter()
    .map(|(key, value)| (key, value.to_owned()))
    .collect()
}

fn library_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("Scry_MPV_LIBRARY") {
        return Ok(path.into());
    }
    let name = if cfg!(windows) {
        "libmpv-2.dll"
    } else if cfg!(target_os = "macos") {
        "libmpv.2.dylib"
    } else {
        "libmpv.so.2"
    };
    let beside_exe = std::env::current_exe()?.with_file_name(name);
    if beside_exe.exists() {
        return Ok(beside_exe);
    }
    let local = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../runtime")
        .join(name);
    if local.exists() {
        return Ok(local);
    }
    #[cfg(target_os = "macos")]
    if let Some(path) = macos::library_path() {
        return Ok(path);
    }
    bail!("libmpv is missing. Put {name} in runtime/ or set Scry_MPV_LIBRARY. See README.md")
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "Tests assert successful parses and inspect the expected parse errors"
)]
mod tests {
    use super::*;
    #[test]
    fn nonfinite_event_timeouts_become_nonblocking_polls() {
        for timeout in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(event_timeout(timeout) == 0.0, "timeout={timeout}");
        }
        for timeout in [-1.0, 0.0, 0.25] {
            assert_eq!(
                event_timeout(timeout).to_bits(),
                timeout.to_bits(),
                "timeout={timeout}"
            );
        }
    }

    #[test]
    fn renderer_setting_accepts_only_supported_names() {
        assert_eq!(VideoRenderer::parse("gpu").unwrap(), VideoRenderer::Gpu);
        assert_eq!(
            VideoRenderer::parse("gpu-next").unwrap(),
            VideoRenderer::GpuNext
        );
        for value in ["", "GPU", "gpu ", "auto", "private-input"] {
            let error = VideoRenderer::parse(value).unwrap_err();
            assert_eq!(
                error.to_string(),
                "Scry_VIDEO_RENDERER must be gpu or gpu-next"
            );
        }
    }

    #[test]
    fn playlist_redirect_does_not_end_the_playback_session() {
        assert_eq!(classify_end(5, 0), None);
        assert_eq!(classify_end(0, 0), Some(PlaybackEvent::Ended));
        assert_eq!(classify_end(4, -13), Some(PlaybackEvent::Failed));
    }
}
