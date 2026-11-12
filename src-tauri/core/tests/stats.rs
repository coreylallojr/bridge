//! Port of tests/unit/stats.test.ts. The OS counters are read by bridge-sys; here the sampler gets summed times.
use bridge_core::stats::StatsSampler;
use bridge_core::PcStats;

/// Sum of per-core (idle, busy) like the TS `core()` helper: total = busy + idle.
fn sum(cores: &[(u64, u64)]) -> (u64, u64) {
    cores.iter().fold((0, 0), |(i, t), &(idle, busy)| (i + idle, t + idle + busy))
}

#[test]
fn first_call_returns_cpu_0_then_cpu_from_idle_total_deltas_mem_from_free_total() {
    let mut s = StatsSampler::new();
    let (i, t) = sum(&[(100, 100), (100, 100)]);
    assert_eq!(s.sample(i, t, 25, 100), PcStats { cpu: 0, mem: 75 });
    let (i, t) = sum(&[(150, 150), (175, 125)]); // deltas: idle 125, total 200 -> 37.5% busy
    assert_eq!(s.sample(i, t, 25, 100), PcStats { cpu: 38, mem: 75 });
    let (i, t) = sum(&[(150, 150), (175, 125)]); // no change -> keep last value
    assert_eq!(s.sample(i, t, 25, 100), PcStats { cpu: 38, mem: 75 });
}

#[test]
fn clamps_to_0_100() {
    let mut s = StatsSampler::default();
    s.sample(0, 0, 0, 100);
    assert_eq!(s.sample(0, 100, 0, 100), PcStats { cpu: 100, mem: 100 });
    // Idle grew more than total (counter glitch): clamps to 0 instead of going negative.
    assert_eq!(s.sample(200, 150, 100, 100), PcStats { cpu: 0, mem: 0 });
}

#[test]
fn counters_going_backwards_keep_the_last_value() {
    let mut s = StatsSampler::new();
    s.sample(100, 200, 50, 100);
    assert_eq!(s.sample(110, 300, 50, 100).cpu, 90);
    assert_eq!(s.sample(0, 0, 50, 100).cpu, 90);
}

#[test]
fn zero_total_memory_is_0() {
    let mut s = StatsSampler::new();
    assert_eq!(s.sample(0, 0, 0, 0), PcStats { cpu: 0, mem: 0 });
}
