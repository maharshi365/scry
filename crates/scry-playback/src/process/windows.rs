pub fn memory() -> (Option<u64>, Option<u64>) {
    use windows_sys::Win32::System::{
        ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX},
        Threading::GetCurrentProcess,
    };
    // SAFETY: the counter structure is plain integer data, and its full size is
    // passed with a writable pointer. The process pseudo-handle is always valid.
    unsafe {
        let mut counters: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
        let Ok(size) = u32::try_from(std::mem::size_of_val(&counters)) else {
            return (None, None);
        };
        if GetProcessMemoryInfo(GetCurrentProcess(), (&raw mut counters).cast(), size) != 0 {
            return (
                Some(counters.WorkingSetSize as u64),
                Some(counters.PrivateUsage as u64),
            );
        }
    }
    (None, None)
}

pub fn cpu_time_s() -> Option<f64> {
    use windows_sys::Win32::{
        Foundation::FILETIME,
        System::Threading::{GetCurrentProcess, GetProcessTimes},
    };
    // SAFETY: FILETIME is plain integer data. All four output pointers are valid
    // for the call, and GetCurrentProcess returns a valid pseudo-handle.
    unsafe {
        let mut created: FILETIME = std::mem::zeroed();
        let mut exited: FILETIME = std::mem::zeroed();
        let mut kernel: FILETIME = std::mem::zeroed();
        let mut user: FILETIME = std::mem::zeroed();
        if GetProcessTimes(
            GetCurrentProcess(),
            &raw mut created,
            &raw mut exited,
            &raw mut kernel,
            &raw mut user,
        ) == 0
        {
            return None;
        }
        let ticks = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
        let total = ticks(kernel) + ticks(user);
        let duration = std::time::Duration::from_secs(total / 10_000_000)
            + std::time::Duration::from_nanos((total % 10_000_000) * 100);
        Some(duration.as_secs_f64())
    }
}
