//! Live CPU / memory counters for the THIS PC gauges (the sampling half of `src/main/system/stats.ts`).
//! Feed the result to `bridge_core::stats::StatsSampler::sample`.
use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::GetSystemTimes;

fn ticks(f: FILETIME) -> u64 {
    (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime)
}

/// `(cpu_idle, cpu_total, mem_free, mem_total)`: CPU times in 100 ns ticks since boot (all cores; total = kernel +
/// user, where kernel already includes idle), memory in bytes (free = available physical). Zeros on failure.
pub fn read_raw() -> (u64, u64, u64, u64) {
    let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
    // SAFETY: out-pointers to locals.
    let (cpu_idle, cpu_total) = match unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) } {
        Ok(()) => (ticks(idle), ticks(kernel) + ticks(user)),
        Err(_) => (0, 0),
    };
    let mut mem = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    // SAFETY: dwLength is set as the API requires.
    let (free, total) = match unsafe { GlobalMemoryStatusEx(&mut mem) } {
        Ok(()) => (mem.ullAvailPhys, mem.ullTotalPhys),
        Err(_) => (0, 0),
    };
    (cpu_idle, cpu_total, free, total)
}
