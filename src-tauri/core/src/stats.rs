//! CPU and memory percentages for the THIS PC gauges (pure math of `src/main/system/stats.ts`).
//! bridge-sys reads the OS counters and passes them in.
use crate::types::PcStats;

fn clamp(n: f64) -> u8 {
    n.round().clamp(0.0, 100.0) as u8
}

/// CPU% is measured between consecutive samples (the first returns 0); if no time passed, the last value repeats.
#[derive(Debug, Clone, Default)]
pub struct StatsSampler {
    prev: Option<(u64, u64)>,
    cpu: u8,
}

impl StatsSampler {
    pub fn new() -> Self {
        Self::default()
    }

    /// `idle`/`total` are cumulative CPU times summed over all cores (any unit); memory in bytes.
    pub fn sample(&mut self, idle: u64, total: u64, mem_free: u64, mem_total: u64) -> PcStats {
        if let Some((p_idle, p_total)) = self.prev {
            let d_total = total as i128 - p_total as i128;
            let d_idle = idle as i128 - p_idle as i128;
            if d_total > 0 {
                self.cpu = clamp((1.0 - d_idle as f64 / d_total as f64) * 100.0);
            }
        }
        self.prev = Some((idle, total));
        let mem = if mem_total > 0 { clamp((1.0 - mem_free as f64 / mem_total as f64) * 100.0) } else { 0 };
        PcStats { cpu: self.cpu, mem }
    }
}
