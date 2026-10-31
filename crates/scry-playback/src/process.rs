#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(windows, target_os = "macos")))]
mod unsupported;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "macos")]
pub(crate) use macos::{cpu_time_s, memory};
#[cfg(not(any(windows, target_os = "macos")))]
pub use unsupported::{cpu_time_s, memory};
#[cfg(windows)]
pub use windows::{cpu_time_s, memory};
