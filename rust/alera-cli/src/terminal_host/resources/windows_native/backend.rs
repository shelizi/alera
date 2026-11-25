//! Transactional CPU baseline and backend selection for Windows accounting.
//! Reject an entire frame on ambiguity; never publish partially sampled trees.
use super::super::process_tree::{ProcessIndex, ProcessRow, ShellProcess};
use super::{
    cpu_since, direct_sample, global_ticks, native_topology, select_targets, ticks, RawSample,
    Topology,
};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use windows::Win32::System::SystemInformation::GetSystemTimePreciseAsFileTime;
use windows::Win32::System::Threading::{
    GetActiveProcessorCount, GetActiveProcessorGroupCount, ALL_PROCESSOR_GROUPS,
};

const EPOCH_SECONDS: u64 = 11_644_473_600;
const TICKS_PER_SECOND: u64 = 10_000_000;
const MAX_SPAN: Duration = Duration::from_millis(500);
const MAX_GAP: Duration = Duration::from_secs(150);
const CLOCK_TOLERANCE_TICKS: u64 = 2_500_000; // 250 ms
const WARMUP_SWEEPS: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Reject {
    Topology,
    UnsupportedCores,
    MissingMetric,
    ShellGeneration,
    UnsealedRoot,
    ParentGeneration,
    Clock,
    Counter,
    SlowSweep,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Root {
    pid: u32,
    sealed: Option<ShellProcess>,
    // An additive marker, not a change to ShellProcess's persisted Unix seconds.
    creation: Option<u64>,
}

impl Root {
    pub(super) fn plain(pid: u32) -> Self {
        Self {
            pid,
            sealed: None,
            creation: None,
        }
    }
    pub(super) fn shell(shell: ShellProcess) -> Self {
        Self {
            pid: shell.pid,
            sealed: Some(shell),
            creation: shell.creation_filetime(),
        }
    }

    #[cfg(test)]
    fn exact(pid: u32, creation: u64) -> Self {
        Self {
            pid,
            sealed: None,
            creation: Some(creation),
        }
    }

    #[cfg(test)]
    fn exact_shell(shell: ShellProcess, creation: u64) -> Self {
        Self {
            pid: shell.pid,
            sealed: Some(shell),
            creation: Some(creation),
        }
    }
}

fn unix_start(creation: u64) -> Option<u64> {
    (creation / TICKS_PER_SECOND).checked_sub(EPOCH_SECONDS)
}

fn consistent_clock(wall_before: u64, wall_after: u64, elapsed: Duration) -> bool {
    let Some(delta) = wall_after.checked_sub(wall_before) else {
        return false;
    };
    let expected = elapsed.as_nanos() / 100;
    u64::try_from(expected).is_ok_and(|expected| delta.abs_diff(expected) <= CLOCK_TOLERANCE_TICKS)
}

fn single_group_cores() -> Result<usize, Reject> {
    if unsafe { GetActiveProcessorGroupCount() } != 1 {
        return Err(Reject::UnsupportedCores);
    }
    let count = unsafe { GetActiveProcessorCount(ALL_PROCESSOR_GROUPS) } as usize;
    if !(1..=64).contains(&count) {
        return Err(Reject::UnsupportedCores);
    }
    Ok(count)
}

pub(super) struct Frame {
    samples: HashMap<u32, RawSample>,
    parents: HashMap<u32, u32>,
    cores: usize,
    system_ticks: u64,
    at: Instant,
    wall_at: u64,
}

impl Frame {
    pub(super) fn contains_pid(&self, pid: u32) -> bool {
        self.samples.contains_key(&pid)
    }

    // Validate exact closure, process generations and parent links before
    // building rows. Missing roots are allowed; missing *selected* rows are
    // not, because losing an intermediate node can hide an entire subtree.
    fn validate(
        topology: &Topology,
        roots: &[Root],
        samples: HashMap<u32, RawSample>,
        cores: usize,
        system_ticks: u64,
        at: Instant,
        wall_at: u64,
    ) -> Result<Self, Reject> {
        if !(1..=64).contains(&cores) {
            return Err(Reject::UnsupportedCores);
        }
        if topology.started == 0 || wall_at < topology.started || system_ticks == 0 {
            return Err(Reject::Clock);
        }
        let mut parents = HashMap::with_capacity(topology.rows.len());
        for &(pid, parent) in &topology.rows {
            if parents.insert(pid, parent).is_some() {
                return Err(Reject::Topology);
            }
        }
        let root_ids: Vec<_> = roots.iter().map(|root| root.pid).collect();
        let selected = select_targets(&topology.rows, &root_ids);
        if samples.len() != selected.len() || !selected.iter().all(|pid| samples.contains_key(pid))
        {
            return Err(Reject::MissingMetric);
        }
        for sample in samples.values() {
            if sample.creation == 0
                || sample.creation > topology.started
                || unix_start(sample.creation).is_none()
            {
                return Err(Reject::Clock);
            }
        }
        for root in roots {
            if let (Some(creation), Some(sample)) = (root.creation, samples.get(&root.pid)) {
                if creation == 0 || creation != sample.creation {
                    return Err(Reject::ShellGeneration);
                }
            }
            if let (Some(sealed), Some(sample)) = (root.sealed, samples.get(&root.pid)) {
                if sealed.pid != root.pid || unix_start(sample.creation) != Some(sealed.start_time)
                {
                    return Err(Reject::ShellGeneration);
                }
            }
        }
        for (&pid, sample) in &samples {
            let parent = parents[&pid];
            if let Some(parent_sample) = samples.get(&parent) {
                // Windows parent PID can outlive/refer to a recycled parent.
                if parent_sample.creation > sample.creation {
                    return Err(Reject::ParentGeneration);
                }
            }
            let mut chain = HashSet::new();
            let mut cursor = pid;
            while samples.contains_key(&cursor) {
                if !chain.insert(cursor) {
                    return Err(Reject::Topology);
                }
                cursor = parents[&cursor];
            }
        }
        parents.retain(|pid, _| selected.contains(pid));
        Ok(Self {
            samples,
            parents,
            cores,
            system_ticks,
            at,
            wall_at,
        })
    }
}

pub(super) fn collect(roots: &[Root], expected_cores: usize) -> Result<Frame, Reject> {
    let start = Instant::now();
    let cores = single_group_cores()?;
    if cores != expected_cores {
        return Err(Reject::UnsupportedCores);
    }
    let topology = native_topology().map_err(|_| Reject::Topology)?;
    let root_ids: Vec<_> = roots.iter().map(|root| root.pid).collect();
    let selected = select_targets(&topology.rows, &root_ids);
    // All raw metrics are from one topology and query-limited handles. Keep
    // the collection bounded in elapsed time rather than claiming a frozen
    // view of a live, changing process table.
    let at = Instant::now();
    let wall_at = ticks(unsafe { GetSystemTimePreciseAsFileTime() });
    let system_ticks = global_ticks().ok_or(Reject::Counter)?;
    let mut samples = HashMap::with_capacity(selected.len());
    for pid in selected {
        // Stop starting further queries after the budget is spent. An
        // individual OS call is still not interruptible by this check.
        if start.elapsed() > MAX_SPAN {
            return Err(Reject::SlowSweep);
        }
        let sample = direct_sample(pid, topology.started).ok_or(Reject::MissingMetric)?;
        samples.insert(pid, sample);
    }
    let elapsed = start.elapsed();
    if elapsed > MAX_SPAN {
        return Err(Reject::SlowSweep);
    }
    if !consistent_clock(
        topology.started,
        ticks(unsafe { GetSystemTimePreciseAsFileTime() }),
        elapsed,
    ) {
        return Err(Reject::Clock);
    }
    if single_group_cores()? != cores {
        return Err(Reject::UnsupportedCores);
    }
    Frame::validate(&topology, roots, samples, cores, system_ticks, at, wall_at)
}

pub(super) struct Measurement {
    pub(super) index: ProcessIndex,
    pub(super) warming: bool,
    #[cfg(test)]
    measured_count: usize,
}

#[derive(Default)]
struct Baseline {
    previous: Option<Frame>,
    consecutive: u32,
}

impl Baseline {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn accept(&mut self, result: Result<Frame, Reject>) -> Result<Measurement, Reject> {
        // Transactional: a rejected frame cannot leave any prior or partial
        // generation in the baseline. The caller may fall back exactly once.
        let result = result.and_then(|frame| self.apply(frame));
        if result.is_err() {
            self.reset();
        }
        result
    }

    fn apply(&mut self, frame: Frame) -> Result<Measurement, Reject> {
        let mut adjacent = false;
        if let Some(previous) = &self.previous {
            let elapsed = frame
                .at
                .checked_duration_since(previous.at)
                .ok_or(Reject::Clock)?;
            if !consistent_clock(previous.wall_at, frame.wall_at, elapsed) {
                return Err(Reject::Clock);
            }
            adjacent = elapsed >= sysinfo::MINIMUM_CPU_UPDATE_INTERVAL
                && elapsed <= MAX_GAP
                && frame.cores == previous.cores;
            if adjacent && frame.system_ticks <= previous.system_ticks {
                return Err(Reject::Counter);
            }
        }
        let consecutive = if adjacent {
            self.consecutive.saturating_add(1)
        } else {
            1
        };
        let warming = consecutive < WARMUP_SWEEPS;
        let mut rows = Vec::with_capacity(frame.samples.len());
        for (&pid, &sample) in &frame.samples {
            let mut cpu_percent = 0.0;
            if adjacent {
                let previous = self.previous.as_ref().expect("adjacent frame");
                if let Some(&old) = previous.samples.get(&pid) {
                    if old.creation == sample.creation {
                        let cpu = cpu_since(
                            old,
                            sample,
                            previous.system_ticks,
                            frame.system_ticks,
                            frame.cores,
                        )
                        .ok_or(Reject::Counter)?;
                        if !cpu.is_finite() || cpu < 0.0 || cpu > f32::MAX as f64 {
                            return Err(Reject::Counter);
                        }
                        if !warming {
                            cpu_percent = cpu as f32;
                        }
                    }
                    // A new generation must not inherit the old PID's CPU.
                }
            }
            let parent = frame.parents[&pid];
            rows.push(ProcessRow {
                pid,
                parent_pid: (parent != 0).then_some(parent),
                start_time: unix_start(sample.creation).ok_or(Reject::Clock)?,
                cpu_percent,
                memory_bytes: sample.working_set,
            });
        }
        #[cfg(test)]
        let measured_count = rows.len();
        let index = ProcessIndex::build(rows);
        self.previous = Some(frame); // replaces and evicts missing generations
        self.consecutive = consecutive;
        Ok(Measurement {
            index,
            warming,
            #[cfg(test)]
            measured_count,
        })
    }
}

#[cfg(test)]
enum Outcome<T> {
    Native(Measurement),
    Legacy(T),
}

#[cfg(test)]
fn sample_or<T>(
    baseline: &mut Baseline,
    candidate: Result<Frame, Reject>,
    legacy: impl FnOnce(Reject) -> T,
) -> Outcome<T> {
    match baseline.accept(candidate) {
        Ok(measurement) => Outcome::Native(measurement),
        Err(reason) => Outcome::Legacy(legacy(reason)),
    }
}

// Keep backend selection independent from persisted PTY identities. A
// production caller must supply creation markers captured at authoritative
// spawn/attachment, never manufacture trust by reading an arbitrary PID.
const RETRY_DELAY: Duration = Duration::from_secs(30);

#[derive(Default)]
pub(super) struct BackendController {
    native: Baseline,
    legacy_active: bool,
    retry_at: Option<Instant>,
}

pub(super) enum BackendResult<T> {
    Native(Measurement),
    Legacy {
        value: T,
        reason: Option<Reject>,
        reset: bool,
    },
}

impl BackendController {
    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(super) fn sample<T>(
        &mut self,
        now: Instant,
        finished_at: impl FnOnce() -> Instant,
        roots: &[Root],
        native: impl FnOnce() -> Result<Frame, Reject>,
        legacy: impl FnOnce(bool) -> T,
    ) -> BackendResult<T> {
        // Recovered/coarse-only seals must not be upgraded from a fresh PID
        // observation. The original backend retains its existing semantics.
        let unsealed = roots
            .iter()
            .any(|root| root.creation.is_none_or(|t| t == 0));
        let cooling_down = self.retry_at.is_some_and(|retry| now < retry);
        let reason = if unsealed {
            Some(Reject::UnsealedRoot)
        } else if cooling_down {
            None
        } else {
            match self.native.accept(native()) {
                Ok(measurement) => {
                    self.legacy_active = false;
                    self.retry_at = None;
                    return BackendResult::Native(measurement);
                }
                Err(reason) => Some(reason),
            }
        };
        self.native.reset();
        if reason.is_some() && !cooling_down {
            self.retry_at = finished_at().checked_add(RETRY_DELAY);
        }
        let reset = !self.legacy_active;
        self.legacy_active = true;
        BackendResult::Legacy {
            value: legacy(reset),
            reason,
            reset,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{hold_test_child, refresh};
    use super::*;
    use std::process::{Child, Command, Stdio};
    use sysinfo::{CpuRefreshKind, Pid, ProcessesToUpdate, System};

    const START: u64 = (EPOCH_SECONDS + 1_000) * TICKS_PER_SECOND;

    fn raw(offset: u64, cpu: u64, memory: u64) -> RawSample {
        RawSample {
            creation: START + offset,
            cpu_ticks: cpu,
            working_set: memory,
        }
    }

    fn frame(
        origin: Instant,
        step: u64,
        rows: &[(u32, u32)],
        samples: &[(u32, RawSample)],
        roots: &[Root],
    ) -> Result<Frame, Reject> {
        let wall = START + (10 + step) * TICKS_PER_SECOND;
        Frame::validate(
            &Topology {
                started: wall,
                rows: rows.to_vec(),
            },
            roots,
            samples.iter().copied().collect(),
            4,
            (step + 1) * 4 * TICKS_PER_SECOND,
            origin + Duration::from_secs(step),
            wall,
        )
    }

    #[test]
    fn rejected_frames_are_atomic_and_invoke_one_fallback() {
        let origin = Instant::now();
        let roots = [Root::plain(10)];
        let rows = [(10, 1), (11, 10), (12, 11)];
        let mut baseline = Baseline::default();
        let samples = [
            (10, raw(1, 100, 100)),
            (11, raw(2, 200, 200)),
            (12, raw(3, 300, 300)),
        ];
        let first = frame(origin, 0, &rows, &samples, &roots);
        assert!(matches!(
            sample_or(&mut baseline, first, |_| panic!("unexpected fallback")),
            Outcome::<()>::Native(measurement) if measurement.warming && measurement.measured_count == 3
        ));
        let mut calls = 0;
        let rejected = frame(origin, 1, &rows, &samples[..2], &roots);
        assert!(matches!(
            sample_or(&mut baseline, rejected, |reason| {
                calls += 1;
                assert_eq!(reason, Reject::MissingMetric);
                42
            }),
            Outcome::Legacy(42)
        ));
        assert_eq!(calls, 1);
        assert!(baseline.previous.is_none());
        assert_eq!(baseline.consecutive, 0);
        let next = baseline
            .accept(frame(origin, 2, &rows, &samples, &roots))
            .unwrap();
        assert!(next.warming);
        assert_eq!(next.measured_count, 3);
        assert_eq!(
            next.index
                .collect_subtree(10, &mut HashSet::new())
                .memory_bytes,
            600
        );
    }

    #[test]
    fn rejects_reused_shell_parent_cycles_and_inconsistent_snapshot() {
        let now = Instant::now();
        let sealed = Root::shell(ShellProcess::coarse(10, 999));
        assert!(matches!(
            frame(now, 0, &[(10, 1)], &[(10, raw(1, 1, 1))], &[sealed]),
            Err(Reject::ShellGeneration)
        ));
        let roots = [Root::plain(10)];
        assert!(matches!(
            frame(
                now,
                0,
                &[(10, 1), (11, 10)],
                &[(10, raw(2, 1, 1)), (11, raw(1, 1, 1))],
                &roots
            ),
            Err(Reject::ParentGeneration)
        ));
        assert!(matches!(
            frame(
                now,
                0,
                &[(10, 11), (11, 10)],
                &[(10, raw(1, 1, 1)), (11, raw(1, 1, 1))],
                &roots
            ),
            Err(Reject::Topology)
        ));
        assert!(matches!(
            frame(now, 0, &[(10, 1), (10, 2)], &[(10, raw(1, 1, 1))], &roots),
            Err(Reject::Topology)
        ));
        assert!(matches!(
            frame(
                now,
                0,
                &[(10, 1)],
                &[(10, raw(11 * TICKS_PER_SECOND, 1, 1))],
                &roots
            ),
            Err(Reject::Clock)
        ));
        assert!(frame(now, 0, &[(11, 10)], &[], &roots).is_ok()); // absent root
    }

    #[test]
    fn adjacent_baselines_warm_reset_evict_and_do_not_inherit_recycled_pid_cpu() {
        let origin = Instant::now();
        let roots = [Root::plain(10)];
        let rows = [(10, 1), (11, 10)];
        let mut baseline = Baseline::default();
        for step in 0..3 {
            let samples = [
                (10, raw(1, step * TICKS_PER_SECOND, 100)),
                (11, raw(2, step * TICKS_PER_SECOND / 2, 200)),
            ];
            let result = baseline
                .accept(frame(origin, step, &rows, &samples, &roots))
                .unwrap();
            assert_eq!(result.warming, step < 2);
            let usage = result.index.collect_subtree(10, &mut HashSet::new());
            assert_eq!(usage.memory_bytes, 300);
            assert_eq!(usage.process_count, 2);
            assert_eq!(usage.cpu_percent, if step < 2 { 0.0 } else { 150.0 });
        }
        let changed = baseline
            .accept(frame(
                origin,
                3,
                &rows,
                &[
                    (10, raw(1, 3 * TICKS_PER_SECOND, 100)),
                    (11, raw(3, 0, 300)),
                ],
                &roots,
            ))
            .unwrap();
        assert_eq!(
            changed
                .index
                .collect_subtree(10, &mut HashSet::new())
                .cpu_percent,
            100.0
        );
        let gone = baseline
            .accept(frame(
                origin,
                4,
                &[(10, 1)],
                &[(10, raw(1, 4 * TICKS_PER_SECOND, 100))],
                &roots,
            ))
            .unwrap();
        assert_eq!(gone.measured_count, 1);
        assert!(!baseline
            .previous
            .as_ref()
            .unwrap()
            .samples
            .contains_key(&11));
        let returned = baseline
            .accept(frame(
                origin,
                5,
                &rows,
                &[
                    (10, raw(1, 5 * TICKS_PER_SECOND, 100)),
                    (11, raw(3, 5 * TICKS_PER_SECOND, 300)),
                ],
                &roots,
            ))
            .unwrap();
        assert_eq!(
            returned
                .index
                .collect_subtree(10, &mut HashSet::new())
                .cpu_percent,
            100.0
        );
        let gap = baseline
            .accept(frame(
                origin,
                200,
                &rows,
                &[
                    (10, raw(1, 6 * TICKS_PER_SECOND, 100)),
                    (11, raw(3, 6 * TICKS_PER_SECOND, 300)),
                ],
                &roots,
            ))
            .unwrap();
        assert!(gap.warming);
        baseline.reset();
        assert!(baseline.previous.is_none());
    }

    #[test]
    fn rejects_clock_and_counter_regression_without_partial_update() {
        let origin = Instant::now();
        let roots = [Root::plain(10)];
        let rows = [(10, 1)];
        let mut baseline = Baseline::default();
        baseline
            .accept(frame(origin, 0, &rows, &[(10, raw(1, 100, 10))], &roots))
            .unwrap();
        let mut next = frame(origin, 1, &rows, &[(10, raw(1, 200, 10))], &roots).unwrap();
        next.wall_at += TICKS_PER_SECOND;
        assert!(matches!(baseline.accept(Ok(next)), Err(Reject::Clock)));
        assert!(baseline.previous.is_none());
        baseline
            .accept(frame(origin, 2, &rows, &[(10, raw(1, 100, 10))], &roots))
            .unwrap();
        assert!(matches!(
            baseline.accept(frame(origin, 3, &rows, &[(10, raw(1, 99, 10))], &roots)),
            Err(Reject::Counter)
        ));
        assert!(baseline.previous.is_none());
        baseline
            .accept(frame(origin, 4, &rows, &[(10, raw(1, 100, 10))], &roots))
            .unwrap();
        let mut next = frame(origin, 5, &rows, &[(10, raw(1, 200, 10))], &roots).unwrap();
        next.system_ticks = 1;
        assert!(matches!(baseline.accept(Ok(next)), Err(Reject::Counter)));
        assert!(baseline.previous.is_none());
    }

    struct ChildGuard(Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            // Closing our pipe also lets the child exit if the parent dies.
            drop(self.0.stdin.take());
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn children(count: usize) -> Vec<ChildGuard> {
        let parent = module_path!().rsplit_once("::backend::tests").unwrap().0;
        let parent = parent.split_once("::").unwrap().1;
        let child_test = format!("{parent}::hold_test_child");
        // Keep a direct reference to the helper so this spelling stays tied to
        // the test module containing the original gated child.
        let _ = hold_test_child as fn();
        (0..count)
            .map(|_| {
                ChildGuard(
                    Command::new(std::env::current_exe().unwrap())
                        .args(["--ignored", "--exact", &child_test, "--nocapture"])
                        .env("ALERA_TEST_HOLD_PROCESS", "1")
                        .stdin(Stdio::piped())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()
                        .expect("spawn test-only holder"),
                )
            })
            .collect()
    }

    fn pause() {
        std::thread::sleep(
            sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.max(Duration::from_millis(250))
                + Duration::from_millis(30),
        );
    }

    #[test]
    #[ignore = "opt-in live process-table regression; run serialized on a responsive host"]
    fn live_candidate_preserves_session_claiming_and_discards_exited_children() {
        let Ok(cores) = single_group_cores() else {
            return;
        };
        let mut holders = children(4);
        let parent = std::process::id();
        let child_pids: Vec<_> = holders.iter().map(|child| child.0.id()).collect();
        let mut system = System::new();
        system.refresh_cpu_list(CpuRefreshKind::nothing());
        assert_eq!(system.cpus().len(), cores);
        let ids: Vec<_> = std::iter::once(parent)
            .chain(child_pids.iter().copied())
            .map(Pid::from_u32)
            .collect();
        refresh(&mut system, ProcessesToUpdate::Some(&ids), true);
        let shells: Vec<_> = child_pids
            .iter()
            .map(|&pid| {
                ShellProcess::coarse(
                    pid,
                    system
                        .process(Pid::from_u32(pid))
                        .expect("live child")
                        .start_time(),
                )
            })
            .collect();
        let roots: Vec<_> = std::iter::once(Root::plain(parent))
            .chain(shells.iter().copied().map(Root::shell))
            .collect();
        let mut baseline = Baseline::default();
        let mut positive_cpu = false;
        for step in 0..4 {
            let measured = baseline.accept(collect(&roots, cores)).unwrap();
            assert_eq!(measured.warming, step < 2);
            let mut claimed = HashSet::new();
            for shell in &shells {
                assert!(measured.index.holds(*shell));
                let usage = measured.index.collect_subtree(shell.pid, &mut claimed);
                assert_eq!(usage.process_count, 1);
                assert!(usage.memory_bytes > 0);
            }
            let host = measured.index.collect_subtree(parent, &mut claimed);
            assert!(host.process_count >= 1 && host.memory_bytes > 0);
            if !measured.warming {
                positive_cpu |= host.cpu_percent > 0.0;
            }
            if step < 3 {
                let until = Instant::now()
                    + sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.max(Duration::from_millis(250))
                    + Duration::from_millis(40);
                let mut value = 0u64;
                while Instant::now() < until {
                    value = std::hint::black_box(value.wrapping_add(1));
                }
                std::hint::black_box(value);
            }
        }
        assert!(
            positive_cpu,
            "native adjacent samples measure real CPU work"
        );
        let exited = holders.pop().unwrap();
        let dead_pid = exited.0.id();
        drop(exited);
        pause();
        let measured = baseline.accept(collect(&roots, cores)).unwrap();
        let dead_shell = shells.iter().find(|shell| shell.pid == dead_pid).unwrap();
        assert!(!measured.index.holds(*dead_shell));
        assert!(!baseline
            .previous
            .as_ref()
            .unwrap()
            .samples
            .contains_key(&dead_pid));
    }

    #[test]
    #[ignore = "opt-in 8/24 private child process stress and phase benchmark"]
    fn candidate_multi_process_benchmark() {
        let cores = single_group_cores().expect("single processor group required");
        let parent = std::process::id();
        for count in [8, 24] {
            let holders = children(count);
            let pids: Vec<_> = holders.iter().map(|h| h.0.id()).collect();
            let mut system = System::new();
            system.refresh_cpu_list(CpuRefreshKind::nothing());
            assert_eq!(system.cpus().len(), cores);
            let ids: Vec<_> = pids.iter().copied().map(Pid::from_u32).collect();
            refresh(&mut system, ProcessesToUpdate::Some(&ids), true);
            let shells: Vec<_> = pids
                .iter()
                .map(|&pid| {
                    ShellProcess::coarse(
                        pid,
                        system.process(Pid::from_u32(pid)).unwrap().start_time(),
                    )
                })
                .collect();
            let roots: Vec<_> = std::iter::once(Root::plain(parent))
                .chain(shells.iter().copied().map(Root::shell))
                .collect();
            let mut baseline = Baseline::default();
            for round in 0..4 {
                for legacy in if round % 2 == 0 {
                    [true, false]
                } else {
                    [false, true]
                } {
                    let start = Instant::now();
                    let (index, measured_count) = if legacy {
                        refresh(&mut system, ProcessesToUpdate::All, true);
                        (
                            ProcessIndex::build(system.processes().iter().map(|(pid, p)| {
                                ProcessRow {
                                    pid: pid.as_u32(),
                                    parent_pid: p.parent().map(|v| v.as_u32()),
                                    start_time: p.start_time(),
                                    cpu_percent: p.cpu_usage(),
                                    memory_bytes: p.memory(),
                                }
                            })),
                            system.processes().len(),
                        )
                    } else {
                        let measured = baseline.accept(collect(&roots, cores)).unwrap();
                        (measured.index, measured.measured_count)
                    };
                    let mut claimed = HashSet::new();
                    for shell in &shells {
                        assert!(index.holds(*shell));
                        let usage = index.collect_subtree(shell.pid, &mut claimed);
                        assert_eq!(usage.process_count, 1);
                        assert!(usage.memory_bytes > 0);
                    }
                    let host = index.collect_subtree(parent, &mut claimed);
                    assert_eq!(host.process_count, 1);
                    assert!(host.memory_bytes > 0);
                    println!("CANDIDATE_BENCH children={count} round={round} version={} rows={measured_count} elapsed_us={}",if legacy {"before"}else{"after"},start.elapsed().as_micros());
                }
                pause();
            }
            drop(holders);
        }
    }

    #[test]
    fn precise_root_rejects_same_second_pid_reuse_without_changing_coarse_seal() {
        let origin = Instant::now();
        let shell = ShellProcess::coarse(10, 1000);
        let root = Root::exact_shell(shell, START + 1);
        let rows = [(10, 1)];
        assert!(frame(origin, 0, &rows, &[(10, raw(1, 10, 10))], &[root]).is_ok());
        // The persisted Unix-second seal still matches both generations.
        assert_eq!(unix_start(START + 1), unix_start(START + 2));
        assert!(frame(
            origin,
            0,
            &rows,
            &[(10, raw(2, 10, 10))],
            &[Root::shell(shell)]
        )
        .is_ok());
        assert!(matches!(
            frame(origin, 0, &rows, &[(10, raw(2, 10, 10))], &[root]),
            Err(Reject::ShellGeneration)
        ));
        assert!(frame(origin, 0, &[], &[], &[root]).is_ok()); // exited root
    }

    #[test]
    fn backend_fallback_resets_once_and_skips_native_during_cooldown() {
        let now = Instant::now();
        let mut backend = BackendController::default();
        let roots = [Root::exact(10, START + 1)];
        let rows = [(10, 1)];
        let samples = [(10, raw(1, 10, 20))];
        let first = backend.sample(
            now,
            || now,
            &roots,
            || frame(now, 0, &rows, &samples, &roots),
            |_| panic!("unexpected fallback"),
        );
        match first {
            BackendResult::<()>::Native(m) => assert!(m.warming),
            _ => panic!("expected native"),
        }
        let failure = backend.sample(
            now + Duration::from_secs(2),
            || now + Duration::from_secs(7),
            &roots,
            || Err(Reject::MissingMetric),
            |reset| {
                assert!(reset);
                42
            },
        );
        assert!(matches!(
            failure,
            BackendResult::Legacy {
                value: 42,
                reason: Some(Reject::MissingMetric),
                reset: true
            }
        ));
        assert!(backend.native.previous.is_none());
        for second in [8, 10, 32, 36] {
            let legacy = backend.sample(
                now + Duration::from_secs(second),
                || now + Duration::from_secs(second),
                &roots,
                || panic!("must not retry during cooldown"),
                |reset| {
                    assert!(!reset);
                    43
                },
            );
            assert!(matches!(
                legacy,
                BackendResult::Legacy {
                    value: 43,
                    reason: None,
                    reset: false
                }
            ));
        }
        let resumed = backend.sample(
            now + Duration::from_secs(37),
            || now + Duration::from_secs(37),
            &roots,
            || frame(now, 37, &rows, &samples, &roots),
            |_| panic!("unexpected fallback"),
        );
        assert!(matches!(resumed,BackendResult::<()>::Native(m) if m.warming));
        let coarse = [Root::shell(ShellProcess::coarse(10, 1000))];
        let result = backend.sample(
            now + Duration::from_secs(39),
            || now + Duration::from_secs(39),
            &coarse,
            || panic!("unsealed root must not query native"),
            |reset| reset,
        );
        assert!(matches!(
            result,
            BackendResult::Legacy {
                value: true,
                reason: Some(Reject::UnsealedRoot),
                reset: true
            }
        ));
        backend.reset();
        assert!(
            backend.retry_at.is_none()
                && !backend.legacy_active
                && backend.native.previous.is_none()
        );
    }

    fn busy_children(count: usize) -> Vec<ChildGuard> {
        let parent = module_path!().rsplit_once("::backend::tests").unwrap().0;
        let parent = parent.split_once("::").unwrap().1;
        let child_test = format!("{parent}::hold_test_child");
        (0..count)
            .map(|_| {
                ChildGuard(
                    Command::new(std::env::current_exe().unwrap())
                        .args(["--ignored", "--exact", &child_test, "--nocapture"])
                        .env("ALERA_TEST_HOLD_PROCESS", "1")
                        .env("ALERA_TEST_BUSY_PROCESS", "1")
                        .stdin(Stdio::piped())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()
                        .expect("spawn bounded CPU worker"),
                )
            })
            .collect()
    }

    // Compare cumulative per-process CPU ticks over their independently
    // observed monotonic intervals, rather than treating the sysinfo and
    // native sampling instants as identical.
    fn reference_sample(pid: u32) -> (Instant, RawSample) {
        let wall = ticks(unsafe { GetSystemTimePreciseAsFileTime() });
        let start = Instant::now();
        let raw = direct_sample(pid, wall).expect("private worker metrics");
        (start + start.elapsed() / 2, raw)
    }

    #[test]
    #[ignore = "opt-in sustained CPU accuracy comparison on a responsive single-group Windows host"]
    fn sustained_cpu_accuracy_comparison() {
        let cores = single_group_cores().expect("single processor group");
        let holders = busy_children(2);
        let pids: Vec<_> = holders.iter().map(|h| h.0.id()).collect();
        let parent = std::process::id();
        std::thread::sleep(Duration::from_millis(350));
        let parent_raw = reference_sample(parent).1;
        let shells: Vec<_> = pids
            .iter()
            .map(|&pid| {
                let raw = reference_sample(pid).1;
                (
                    ShellProcess::coarse(pid, unix_start(raw.creation).unwrap()),
                    raw.creation,
                )
            })
            .collect();
        let roots: Vec<_> = std::iter::once(Root::exact(parent, parent_raw.creation))
            .chain(
                shells
                    .iter()
                    .map(|&(shell, creation)| Root::exact_shell(shell, creation)),
            )
            .collect();
        let ids: Vec<_> = pids.iter().copied().map(Pid::from_u32).collect();
        let mut system = System::new();
        system.refresh_cpu_list(CpuRefreshKind::nothing());
        assert_eq!(cores, system.cpus().len());
        let mut baseline = Baseline::default();
        let mut refs: HashMap<u32, (Instant, RawSample)> = HashMap::new();
        let mut native_errors = Vec::new();
        let mut legacy_errors = Vec::new();
        let mut reference_values = Vec::new();
        let mut total_native = 0.0;
        let mut total_reference = 0.0;
        for round in 0..6 {
            let measurement = if round % 2 == 0 {
                let m = baseline.accept(collect(&roots, cores)).unwrap();
                refresh(&mut system, ProcessesToUpdate::Some(&ids), true);
                m
            } else {
                refresh(&mut system, ProcessesToUpdate::Some(&ids), true);
                baseline.accept(collect(&roots, cores)).unwrap()
            };
            assert_eq!(measurement.warming, round < 2);
            for (worker, &pid) in pids.iter().enumerate() {
                let (at, raw) = reference_sample(pid);
                let previous = refs.insert(pid, (at, raw));
                if measurement.warming {
                    continue;
                }
                let (old_at, old) = previous.expect("reference baseline");
                assert_eq!(old.creation, raw.creation);
                let reference = (raw.cpu_ticks.checked_sub(old.cpu_ticks).unwrap() as f64
                    / TICKS_PER_SECOND as f64)
                    / at.duration_since(old_at).as_secs_f64()
                    * 100.0;
                let usage = measurement.index.collect_subtree(pid, &mut HashSet::new());
                assert_eq!(usage.process_count, 1);
                assert!(usage.memory_bytes > 0);
                let native = usage.cpu_percent;
                let legacy = system.process(Pid::from_u32(pid)).unwrap().cpu_usage() as f64;
                assert!(reference.is_finite() && native.is_finite() && legacy.is_finite());
                assert!(reference >= 0.0 && native >= 0.0 && legacy >= 0.0);
                native_errors.push((native - reference).abs());
                legacy_errors.push((legacy - reference).abs());
                reference_values.push(reference);
                total_native += native;
                total_reference += reference;
                println!("CPU_COMPARE round={round} worker={worker} reference={reference:.3} native={native:.3} legacy={legacy:.3} native_abs_error={:.3} legacy_abs_error={:.3}",(native-reference).abs(),(legacy-reference).abs());
            }
            if round < 5 {
                std::thread::sleep(Duration::from_secs(2));
            }
        }
        let average = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
        assert_eq!(native_errors.len(), 8);
        println!("CPU_COMPARE_SUMMARY samples={} reference_avg={:.3} native_avg={:.3} native_mae={:.3} legacy_mae={:.3} native_max_error={:.3}",native_errors.len(),average(&reference_values),total_native/native_errors.len() as f64,average(&native_errors),average(&legacy_errors),native_errors.iter().copied().fold(0.0,f64::max));
        // Gate adoption on a useful workload and agreement with measured
        // CPU-time deltas; do not assume a fully scheduled core on shared hosts.
        assert!(
            average(&reference_values) > 20.0,
            "workload starved; rerun on a responsive host"
        );
        assert!(
            average(&native_errors) <= 10.0,
            "candidate mean error too high"
        );
        assert!(
            (total_native - total_reference).abs() / total_reference <= 0.15,
            "candidate aggregate error too high"
        );
    }
}
