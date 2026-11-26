//! Collect only the host metrics used by a resource snapshot.
use sysinfo::System;

/// Windows sysinfo 0.39.6 computes per-process CPU from GetProcessTimes and
/// GetSystemTimes, using the cached logical CPU count. Its separate global
/// CPU usage refresh opens and reads PDH counters which this payload does not
/// expose. Likewise, swap usage is not part of the payload. Keep the Unix
/// refresh unchanged because process CPU accounting differs by platform.
#[cfg(windows)]
pub(super) fn refresh_host_metrics(system: &mut System) {
    use sysinfo::{CpuRefreshKind, MemoryRefreshKind};
    system.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
    if system.cpus().is_empty() {
        system.refresh_cpu_list(CpuRefreshKind::nothing());
    }
}

#[cfg(not(windows))]
pub(super) fn refresh_host_metrics(system: &mut System) {
    system.refresh_memory();
    system.refresh_cpu_usage();
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::hint::black_box;
    use std::time::{Duration, Instant};
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, MINIMUM_CPU_UPDATE_INTERVAL};

    fn refresh_processes(system: &mut System, pids: ProcessesToUpdate<'_>) {
        system.refresh_processes_specifics(
            pids,
            true,
            ProcessRefreshKind::nothing()
                .without_tasks()
                .with_cpu()
                .with_memory(),
        );
    }

    fn legacy_host_metrics(system: &mut System) {
        system.refresh_memory();
        system.refresh_cpu_usage();
    }

    #[test]
    fn windows_host_metrics_keep_ram_and_core_count_without_global_cpu_refresh() {
        let mut before = System::new();
        let mut after = System::new();
        legacy_host_metrics(&mut before);
        refresh_host_metrics(&mut after);
        assert!(!after.cpus().is_empty());
        assert_eq!(after.cpus().len(), before.cpus().len());
        assert!(after.total_memory() > 0);
        assert_eq!(after.total_memory(), before.total_memory());
        assert!(after.available_memory() > 0);
        assert!(after.available_memory() <= after.total_memory());

        let self_pid = Pid::from_u32(std::process::id());
        let pids = [self_pid];
        let mut positive_cpu = false;
        let mut original_positive_cpu = false;
        // Leave enough time for the original Windows warm-up semantics.
        // Exercise our own process only; never launch or inspect user agents.
        for i in 0..5 {
            legacy_host_metrics(&mut before);
            refresh_processes(&mut before, ProcessesToUpdate::Some(&pids));
            refresh_host_metrics(&mut after);
            refresh_processes(&mut after, ProcessesToUpdate::Some(&pids));
            let original = before.process(self_pid).expect("test process");
            let optimized = after.process(self_pid).expect("test process");
            assert_eq!(optimized.start_time(), original.start_time());
            assert!(optimized.memory() > 0);
            assert!(optimized.cpu_usage().is_finite() && optimized.cpu_usage() >= 0.0);
            if i >= 2 {
                positive_cpu |= optimized.cpu_usage() > 0.0;
                original_positive_cpu |= original.cpu_usage() > 0.0;
            }
            if positive_cpu && original_positive_cpu {
                break;
            }
            if i < 4 {
                let busy_until = Instant::now()
                    + MINIMUM_CPU_UPDATE_INTERVAL.max(Duration::from_millis(250))
                    + Duration::from_millis(50);
                let mut value = 0u64;
                while Instant::now() < busy_until {
                    value = black_box(value.wrapping_add(1));
                }
                black_box(value);
            }
        }
        assert!(
            original_positive_cpu,
            "baseline process CPU must become positive"
        );
        assert!(
            positive_cpu,
            "per-process CPU must remain available without PDH"
        );
    }

    #[test]
    #[ignore = "opt-in real Windows resource refresh phase benchmark"]
    fn windows_resource_refresh_phase_benchmark() {
        use super::super::process_tree::{ProcessIndex, ProcessRow};
        use std::collections::HashSet;
        let self_pid = std::process::id();
        let mut before = System::new();
        let mut after = System::new();
        let interval =
            MINIMUM_CPU_UPDATE_INTERVAL.max(Duration::from_millis(250)) + Duration::from_millis(30);
        let measure = |system: &mut System, legacy: bool, phase: &str, round: usize| {
            let start = Instant::now();
            if legacy {
                legacy_host_metrics(system);
            } else {
                refresh_host_metrics(system);
            }
            let host_us = start.elapsed().as_micros();
            let t = Instant::now();
            refresh_processes(system, ProcessesToUpdate::All);
            let process_us = t.elapsed().as_micros();
            let t = Instant::now();
            let index =
                ProcessIndex::build(system.processes().iter().map(|(pid, process)| ProcessRow {
                    pid: pid.as_u32(),
                    parent_pid: process.parent().map(|p| p.as_u32()),
                    start_time: process.start_time(),
                    cpu_percent: process.cpu_usage(),
                    memory_bytes: process.memory(),
                }));
            let usage = index.collect_subtree(self_pid, &mut HashSet::new());
            assert!(usage.memory_bytes > 0);
            black_box(usage);
            let index_us = t.elapsed().as_micros();
            assert!(system.total_memory() > 0 && !system.cpus().is_empty());
            println!("REFRESH_BENCH phase={phase} round={round} version={} host_us={host_us} process_us={process_us} index_us={index_us} total_us={}",
                if legacy {"before"} else {"after"}, start.elapsed().as_micros());
        };
        // Cold: each pass starts with a new System. Windows PDH may share
        // machine-level caches, so use interleaved order, not first-ever boot.
        for (round, legacy) in [
            true, false, false, true, true, false, false, true, true, false,
        ]
        .into_iter()
        .enumerate()
        {
            let mut fresh = System::new();
            measure(&mut fresh, legacy, "cold", round);
        }
        // Warm: preserve one System per version and a valid CPU baseline.
        for _ in 0..3 {
            legacy_host_metrics(&mut before);
            refresh_processes(&mut before, ProcessesToUpdate::All);
            refresh_host_metrics(&mut after);
            refresh_processes(&mut after, ProcessesToUpdate::All);
            std::thread::sleep(interval);
        }
        for round in 0..10 {
            for legacy in if round % 2 == 0 {
                [true, false]
            } else {
                [false, true]
            } {
                measure(
                    if legacy { &mut before } else { &mut after },
                    legacy,
                    "warm",
                    round,
                );
            }
            std::thread::sleep(interval);
        }
    }
}
