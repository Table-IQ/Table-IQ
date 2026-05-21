//! Server-side benchmarking helpers.

/// Current process resident set size in bytes (best-effort).
pub fn process_rss() -> u64 {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    let Ok(pid) = sysinfo::get_current_pid() else {
        return 0;
    };
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[Pid::from_u32(pid.as_u32())]),
        true,
        ProcessRefreshKind::everything(),
    );
    sys.process(pid).map(|p| p.memory()).unwrap_or(0)
}
