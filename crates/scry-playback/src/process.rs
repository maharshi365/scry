#[cfg(windows)]
mod windows;

pub fn memory() -> (Option<u64>, Option<u64>) {
    #[cfg(windows)]
    {
        windows::memory()
    }
    #[cfg(not(windows))]
    {
        (None, None)
    }
}

pub fn cpu_time_s() -> Option<f64> {
    #[cfg(windows)]
    {
        windows::cpu_time_s()
    }
    #[cfg(not(windows))]
    {
        None
    }
}
