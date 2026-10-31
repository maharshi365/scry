fn task_info() -> Option<libc::proc_taskinfo> {
    let mut info = std::mem::MaybeUninit::<libc::proc_taskinfo>::uninit();
    let size = i32::try_from(std::mem::size_of_val(&info)).ok()?;
    // SAFETY: proc_pidinfo writes at most size bytes into this aligned task-info
    // buffer. Only a complete successful write is assumed initialized. getpid
    // always returns the current process; no foreign process is inspected.
    unsafe {
        if libc::proc_pidinfo(
            libc::getpid(),
            libc::PROC_PIDTASKINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        ) != size
        {
            return None;
        }
        Some(info.assume_init())
    }
}

pub(crate) fn memory() -> (Option<u64>, Option<u64>) {
    // Virtual size is not private committed memory; do not mislabel it as such.
    (task_info().map(|info| info.pti_resident_size), None)
}

pub(crate) fn cpu_time_s() -> Option<f64> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: getrusage initializes this correctly sized/aligned output on success.
    // RUSAGE_SELF includes live render/decoder threads and returns timeval units.
    let usage = unsafe {
        if libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) != 0 {
            return None;
        }
        usage.assume_init()
    };
    let user = cpu_duration(usage.ru_utime)?;
    let system = cpu_duration(usage.ru_stime)?;
    Some(user.checked_add(system)?.as_secs_f64())
}

fn cpu_duration(time: libc::timeval) -> Option<std::time::Duration> {
    let seconds = u64::try_from(time.tv_sec).ok()?;
    let micros = u64::try_from(time.tv_usec)
        .ok()
        .filter(|micros| *micros < 1_000_000)?;
    std::time::Duration::from_secs(seconds).checked_add(std::time::Duration::from_micros(micros))
}

#[cfg(test)]
mod tests {
    #[test]
    fn cpu_counter_uses_seconds_and_microseconds_without_accepting_invalid_values() {
        let time = |seconds, micros| libc::timeval {
            tv_sec: seconds,
            tv_usec: micros,
        };
        assert_eq!(
            super::cpu_duration(time(2, 500_000)),
            Some(std::time::Duration::from_millis(2500))
        );
        assert_eq!(super::cpu_duration(time(-1, 0)), None);
        assert_eq!(super::cpu_duration(time(0, -1)), None);
        assert_eq!(super::cpu_duration(time(0, 1_000_000)), None);
    }
    #[test]
    fn current_process_has_resident_memory_and_monotonic_cpu_time() {
        assert!(super::memory().0.is_some_and(|bytes| bytes > 0));
        assert_eq!(super::memory().1, None);
        let before = super::cpu_time_s();
        let after = super::cpu_time_s();
        assert!(
            before
                .zip(after)
                .is_some_and(|(before, after)| after >= before)
        );
    }
}
