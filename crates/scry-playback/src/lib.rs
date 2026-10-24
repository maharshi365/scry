//! Small dynamic binding to libmpv's stable C client API. No mpv subprocess or webview.
mod process;
pub mod telemetry;

use crate::telemetry::Sample;
use anyhow::{Context, Result, bail};
use libloading::Library;
use std::{
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
type Free = unsafe extern "C" fn(*mut c_void);
type WaitEvent = unsafe extern "C" fn(*mut c_void, f64) -> *const Event;

pub struct Player {
    handle: *mut c_void,
    _library: Library,
    destroy: Destroy,
    command: Command,
    get: GetProperty,
    free: Free,
    wait: WaitEvent,
}

#[derive(Debug, PartialEq)]
pub enum PlaybackEvent {
    Loaded,
    Ended,
    Failed,
    Shutdown,
}

#[derive(Clone, Copy)]
pub enum PlaybackTarget {
    NativeWindow(u64),
    DecodeOnly,
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
            _ => bail!("SCRY_VIDEO_RENDERER must be gpu or gpu-next"),
        }
    }

    fn from_env() -> Result<Self> {
        match std::env::var("SCRY_VIDEO_RENDERER") {
            Ok(value) => Self::parse(&value),
            // gpu saved working set but increased private commit in matched probes.
            // See docs/rendering-performance.md before changing the default.
            Err(std::env::VarError::NotPresent) => Ok(Self::GpuNext),
            Err(_) => bail!("SCRY_VIDEO_RENDERER must be gpu or gpu-next"),
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
    fn load(&self, url: &str) -> Result<()>;
    fn command(&self, args: &[&str]) -> Result<()>;
    fn event(&self, timeout: f64) -> Option<PlaybackEvent>;
    fn sample(&self, elapsed_s: f64) -> Sample;
}

impl Player {
    pub fn open(target: PlaybackTarget) -> Result<Self> {
        let renderer = match target {
            PlaybackTarget::NativeWindow(_) => Some(VideoRenderer::from_env()?),
            PlaybackTarget::DecodeOnly => None,
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
            let free: Free = *library.get(b"mpv_free\0")?;
            let wait: WaitEvent = *library.get(b"mpv_wait_event\0")?;
            let handle = create();
            if handle.is_null() {
                bail!("libmpv could not allocate a player");
            }
            let player = Self {
                handle,
                _library: library,
                destroy,
                command,
                get,
                free,
                wait,
            };
            let mut options = vec![
                ("config", "no".to_owned()),
                ("load-scripts", "no".into()),
                ("ytdl", "no".into()),
                ("terminal", "no".into()),
                ("input-default-bindings", "no".into()),
                ("osc", "no".into()),
                ("idle", "yes".into()),
                ("keep-open", "no".into()),
                ("cache", "yes".into()),
                ("demuxer-max-bytes", "32MiB".into()),
                ("demuxer-max-back-bytes", "8MiB".into()),
                ("cache-secs", "10".into()),
                ("network-timeout", "20".into()),
                ("msg-level", "all=no".into()),
            ];
            if let PlaybackTarget::NativeWindow(hwnd) = target {
                options.extend([
                    ("wid", hwnd.to_string()),
                    (
                        "vo",
                        renderer.expect("Native renderer selected").name().into(),
                    ),
                    ("hwdec", "auto-safe".into()),
                ]);
                // Keep both renderers on the same native API with D3D11 decode interop.
                #[cfg(windows)]
                options.extend([("gpu-api", "d3d11".into()), ("gpu-context", "d3d11".into())]);
            } else {
                options.extend([
                    ("vo", "null".into()),
                    ("ao", "null".into()),
                    ("hwdec", "no".into()),
                ]);
            }
            for (key, value) in options {
                let key_c = CString::new(key)?;
                let value_c = CString::new(value)?;
                if set(handle, key_c.as_ptr(), value_c.as_ptr()) < 0 {
                    bail!("libmpv rejected option {key}");
                }
            }
            if initialize(handle) < 0 {
                bail!("libmpv initialization failed");
            }
            Ok(player)
        }
    }

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
    pub fn load(&self, url: &str) -> Result<()> {
        self.command(&["loadfile", url, "replace"])
    }
    pub fn property(&self, name: &str) -> Option<String> {
        let key = CString::new(name).ok()?;
        unsafe {
            let value = (self.get)(self.handle, key.as_ptr());
            if value.is_null() {
                return None;
            }
            let result = CStr::from_ptr(value).to_string_lossy().into_owned();
            (self.free)(value as *mut c_void);
            Some(result)
        }
    }
    pub fn event(&self, timeout: f64) -> Option<PlaybackEvent> {
        // SAFETY: copy event data before the next wait_event invalidates it.
        unsafe {
            let event = &*(self.wait)(self.handle, timeout);
            match event.id {
                1 => Some(PlaybackEvent::Shutdown),
                8 => Some(PlaybackEvent::Loaded),
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
    pub fn sample(&self, elapsed_s: f64) -> Sample {
        let number = |name| {
            self.property(name)
                .and_then(|s| s.parse::<f64>().ok())
                .filter(|v| v.is_finite())
        };
        let (resident_bytes, private_bytes) = telemetry::memory();
        Sample {
            elapsed_s,
            position_s: number("time-pos"),
            duration_s: number("duration"),
            cache_s: number("demuxer-cache-duration"),
            buffering: self.property("paused-for-cache").as_deref() == Some("yes"),
            paused: self.property("pause").as_deref() == Some("yes"),
            dropped_frames: number("frame-drop-count").map(|v| v as u64),
            decoder_drops: number("decoder-frame-drop-count").map(|v| v as u64),
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
        unsafe {
            (self.destroy)(self.handle);
        }
    }
}

fn library_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("SCRY_MPV_LIBRARY") {
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
    bail!("libmpv is missing. Put {name} in runtime/ or set SCRY_MPV_LIBRARY. See README.md")
}

#[cfg(test)]
mod tests {
    use super::*;
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
                "SCRY_VIDEO_RENDERER must be gpu or gpu-next"
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
