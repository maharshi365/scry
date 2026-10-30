//! Target-specific runtime discovery and native renderer options.
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as native;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as native;
#[cfg(not(any(windows, target_os = "macos")))]
mod unix;
#[cfg(not(any(windows, target_os = "macos")))]
use unix as native;

pub(super) use native::{LIBRARY_NAME, library_path, native_options};
