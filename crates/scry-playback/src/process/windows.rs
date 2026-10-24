pub fn memory() -> (Option<u64>, Option<u64>) {
    use windows_sys::Win32::System::{
        ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX},
        Threading::GetCurrentProcess,
    };
    unsafe {
        let mut counters: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
        let size = std::mem::size_of_val(&counters) as u32;
        if GetProcessMemoryInfo(GetCurrentProcess(), &mut counters as *mut _ as *mut _, size) != 0 {
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
    unsafe {
        let mut created: FILETIME = std::mem::zeroed();
        let mut exited: FILETIME = std::mem::zeroed();
        let mut kernel: FILETIME = std::mem::zeroed();
        let mut user: FILETIME = std::mem::zeroed();
        if GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        ) == 0
        {
            return None;
        }
        let ticks = |t: FILETIME| ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64;
        Some((ticks(kernel) + ticks(user)) as f64 / 10_000_000.)
    }
}
