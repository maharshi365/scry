#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

pub fn memory() -> (Option<u64>, Option<u64>) {
    #[cfg(windows)]
    {
        windows::memory()
    }
    #[cfg(target_os = "macos")]
    {
        macos::memory()
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        (None, None)
    }
}

pub fn cpu_time_s() -> Option<f64> {
    #[cfg(windows)]
    {
        windows::cpu_time_s()
    }
    #[cfg(target_os = "macos")]
    {
        macos::cpu_time_s()
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        None
    }
}
