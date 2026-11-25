//! Opt-in Windows topology-first resource sampling.
//! Read a single process snapshot and query only selected descendants.
//! Never collect command lines or mutate inspected processes.

use std::collections::{HashMap, HashSet, VecDeque};
use std::mem::size_of;
#[cfg(test)]
use std::time::{Duration, Instant};

#[cfg(test)]
use sysinfo::{CpuRefreshKind, Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_NO_MORE_FILES, FILETIME, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::SystemInformation::GetSystemTimePreciseAsFileTime;
use windows::Win32::System::Threading::{
    GetProcessTimes, GetSystemTimes, OpenProcess, WaitForSingleObject,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
};

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn ticks(time: FILETIME) -> u64 {
    (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
}

struct Topology {
    started: u64,
    rows: Vec<(u32, u32)>,
}

fn native_topology() -> windows::core::Result<Topology> {
    // A PID opened after this snapshot must not have been created after its
    // beginning. Reject ambiguous arrivals/recycling in the prototype.
    let started = ticks(unsafe { GetSystemTimePreciseAsFileTime() });
    let handle = OwnedHandle(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }?);
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    unsafe { Process32FirstW(handle.0, &mut entry) }?;
    let mut rows = Vec::new();
    loop {
        rows.push((entry.th32ProcessID, entry.th32ParentProcessID));
        match unsafe { Process32NextW(handle.0, &mut entry) } {
            Ok(()) => {}
            Err(error) if error.code() == ERROR_NO_MORE_FILES.to_hresult() => break,
            Err(error) => return Err(error),
        }
    }
    Ok(Topology { started, rows })
}

fn select_targets(rows: &[(u32, u32)], roots: &[u32]) -> HashSet<u32> {
    let present: HashSet<u32> = rows.iter().map(|&(pid, _)| pid).collect();
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for &(pid, parent) in rows {
        if pid != 0 && parent != 0 {
            children.entry(parent).or_default().push(pid);
        }
    }
    let mut selected = HashSet::new();
    let mut queue = VecDeque::new();
    for &root in roots {
        if root != 0 && present.contains(&root) && selected.insert(root) {
            queue.push_back(root);
        }
    }
    while let Some(pid) = queue.pop_front() {
        if let Some(descendants) = children.get(&pid) {
            for &child in descendants {
                if selected.insert(child) {
                    queue.push_back(child);
                }
            }
        }
    }
    selected
}

#[derive(Debug, Clone, Copy)]
struct RawSample {
    creation: u64,
    cpu_ticks: u64,
    working_set: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MetricReadFailure {
    Open(i32),
    Times(i32),
    CreationMissing,
    CreatedAfterSnapshot { delta_ticks: u64 },
    Memory(i32),
    Exited,
    Wait(u32),
    Counter,
}

fn direct_sample(pid: u32, snapshot_started: u64) -> Result<RawSample, MetricReadFailure> {
    let handle = OwnedHandle(
        unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                false,
                pid,
            )
        }
        .map_err(|error| MetricReadFailure::Open(error.code().0))?,
    );
    let (mut creation, mut exit, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    unsafe { GetProcessTimes(handle.0, &mut creation, &mut exit, &mut kernel, &mut user) }
        .map_err(|error| MetricReadFailure::Times(error.code().0))?;
    let creation = ticks(creation);
    if creation == 0 {
        return Err(MetricReadFailure::CreationMissing);
    }
    if creation > snapshot_started {
        return Err(MetricReadFailure::CreatedAfterSnapshot {
            delta_ticks: creation - snapshot_started,
        });
    }
    let mut memory = PROCESS_MEMORY_COUNTERS {
        cb: size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };
    unsafe {
        GetProcessMemoryInfo(
            handle.0,
            &mut memory,
            size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )
    }
    .map_err(|error| MetricReadFailure::Memory(error.code().0))?;
    // The signaled state is unambiguous even for exit code STILL_ACTIVE.
    match unsafe { WaitForSingleObject(handle.0, 0) } {
        WAIT_TIMEOUT => {}
        WAIT_OBJECT_0 => return Err(MetricReadFailure::Exited),
        other => return Err(MetricReadFailure::Wait(other.0)),
    }
    Ok(RawSample {
        creation,
        cpu_ticks: ticks(kernel)
            .checked_add(ticks(user))
            .ok_or(MetricReadFailure::Counter)?,
        working_set: memory.WorkingSetSize as u64,
    })
}

fn global_ticks() -> Option<u64> {
    let (mut kernel, mut user) = (FILETIME::default(), FILETIME::default());
    // Kernel time already includes idle time.
    unsafe { GetSystemTimes(None, Some(&mut kernel), Some(&mut user)) }.ok()?;
    ticks(kernel).checked_add(ticks(user))
}

/// One-core percent using adjacent cumulative counters. A caller must key
/// samples by PID, enforce a minimum interval and discard missing generations.
/// Not a claim of exact equality with sysinfo's per-handle timing.
fn cpu_since(
    before: RawSample,
    after: RawSample,
    global_before: u64,
    global_after: u64,
    cores: usize,
) -> Option<f64> {
    if before.creation == 0 || before.creation != after.creation || cores == 0 {
        return None;
    }
    let global_delta = global_after.checked_sub(global_before)?;
    if global_delta == 0 {
        return None;
    }
    let delta = after.cpu_ticks.checked_sub(before.cpu_ticks)?;
    Some(100.0 * cores as f64 * delta as f64 / global_delta as f64)
}

#[cfg(test)]
fn refresh(system: &mut System, targets: ProcessesToUpdate<'_>, metrics: bool) {
    let kind = ProcessRefreshKind::nothing().without_tasks();
    system.refresh_processes_specifics(
        targets,
        true,
        if metrics {
            kind.with_cpu().with_memory()
        } else {
            kind
        },
    );
}

#[cfg(test)]
fn selected_sysinfo_pids(system: &System, roots: &[u32]) -> Vec<Pid> {
    let rows: Vec<_> = system
        .processes()
        .iter()
        .map(|(pid, process)| (pid.as_u32(), process.parent().map_or(0, |p| p.as_u32())))
        .collect();
    select_targets(&rows, roots)
        .into_iter()
        .map(Pid::from_u32)
        .collect()
}

#[test]
fn target_selection_covers_overlaps_cycles_and_new_children() {
    let mut rows = vec![
        (10, 1),
        (11, 10),
        (12, 11),
        (20, 0),
        (21, 20),
        (30, 31),
        (31, 30),
        (40, 99),
        (0, 0),
    ];
    assert_eq!(
        select_targets(&rows, &[10, 11, 20, 30, 99, 0]),
        HashSet::from([10, 11, 12, 20, 21, 30, 31])
    );
    rows.push((13, 12));
    assert_eq!(
        select_targets(&rows, &[10]),
        HashSet::from([10, 11, 12, 13])
    );
    rows.retain(|&(pid, _)| pid != 11);
    assert_eq!(select_targets(&rows, &[10]), HashSet::from([10]));
}

#[test]
fn cpu_delta_rejects_recycled_identity_and_counter_regression() {
    let before = RawSample {
        creation: 100,
        cpu_ticks: 1000,
        working_set: 1,
    };
    let after = RawSample {
        cpu_ticks: 1250,
        ..before
    };
    assert_eq!(cpu_since(before, after, 2000, 3000, 4), Some(100.0));
    assert_eq!(
        cpu_since(
            before,
            RawSample {
                creation: 101,
                ..after
            },
            2000,
            3000,
            4
        ),
        None
    );
    assert_eq!(cpu_since(after, before, 2000, 3000, 4), None);
    assert_eq!(cpu_since(before, after, 3000, 3000, 4), None);
    assert_eq!(cpu_since(before, after, 3000, 2000, 4), None);
}

#[test]
fn native_self_identity_and_working_set_match_expected_metrics() {
    const FILETIME_UNIX_EPOCH_SECONDS: u64 = 11_644_473_600;
    let pid = std::process::id();
    let topology = native_topology().expect("native process snapshot");
    assert!(select_targets(&topology.rows, &[pid]).contains(&pid));
    let sample = direct_sample(pid, topology.started).expect("own process metrics");
    assert!(sample.working_set > 0);
    assert!(matches!(
        direct_sample(pid, sample.creation - 1),
        Err(MetricReadFailure::CreatedAfterSnapshot { .. })
    ));
    let mut system = System::new();
    system.refresh_cpu_list(CpuRefreshKind::nothing());
    refresh(
        &mut system,
        ProcessesToUpdate::Some(&[Pid::from_u32(pid)]),
        true,
    );
    let expected = system
        .process(Pid::from_u32(pid))
        .expect("sysinfo own process");
    assert_eq!(
        sample.creation / 10_000_000 - FILETIME_UNIX_EPOCH_SECONDS,
        expected.start_time()
    );
    assert!(expected.memory() > 0);
    // Separate wall-clock observations may legitimately report different RSS.
    let a = global_ticks().expect("system CPU time");
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.max(Duration::from_millis(250)));
    let newer = native_topology().expect("fresh topology");
    let next = direct_sample(pid, newer.started).expect("next process metrics");
    let b = global_ticks().expect("next system CPU time");
    let percent =
        cpu_since(sample, next, a, b, system.cpus().len()).expect("monotonic CPU counters");
    assert!(percent.is_finite() && percent >= 0.0);
}

#[test]
#[ignore = "spawned only by the targeted topology test"]
fn hold_test_child() {
    use std::io::Read;
    // Running all ignored tests must not block on interactive stdin.
    if std::env::var_os("ALERA_TEST_HOLD_PROCESS").as_deref() == Some(std::ffi::OsStr::new("1")) {
        if std::env::var_os("ALERA_TEST_BUSY_PROCESS").as_deref() == Some(std::ffi::OsStr::new("1"))
        {
            // Private, explicitly gated benchmark child. Bound the work even
            // if the parent disappears before closing its control pipe.
            use std::sync::{
                atomic::{AtomicBool, Ordering},
                Arc,
            };
            let stop = Arc::new(AtomicBool::new(false));
            let worker_stop = Arc::clone(&stop);
            let worker = std::thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(18);
                let mut value = 0u64;
                while !worker_stop.load(Ordering::Relaxed) && Instant::now() < deadline {
                    for _ in 0..1024 {
                        value = std::hint::black_box(value.wrapping_add(1));
                    }
                }
                std::hint::black_box(value);
            });
            let _ = std::io::stdin().read(&mut [0u8]);
            stop.store(true, Ordering::Relaxed);
            let _ = worker.join();
        } else {
            let _ = std::io::stdin().read(&mut [0u8]);
        }
    }
}

#[test]
fn fresh_topology_discovers_a_new_live_child() {
    use std::process::{Child, Command, Stdio};

    struct ChildGuard(Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            // Only the test binary created by this test; never a user process.
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    let name = module_path!().split_once("::").expect("test module path").1;
    let child_test = format!("{name}::hold_test_child");
    let child = Command::new(std::env::current_exe().expect("test executable"))
        .args(["--ignored", "--exact", &child_test, "--nocapture"])
        .env("ALERA_TEST_HOLD_PROCESS", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn private test child");
    let mut guard = ChildGuard(child);
    let child_pid = guard.0.id();
    let parent = std::process::id();
    let topology = native_topology().expect("child topology");
    assert!(guard.0.try_wait().expect("child status").is_none());
    assert!(topology.rows.contains(&(child_pid, parent)));
    assert!(select_targets(&topology.rows, &[parent]).contains(&child_pid));
    let raw = direct_sample(child_pid, topology.started).expect("live child metrics");
    assert!(raw.creation > 0);
    // Explicitly exit our test child before the next topology read.
    drop(guard);
    let after = native_topology().expect("post-exit topology");
    assert!(!select_targets(&after.rows, &[parent]).contains(&child_pid));
}

#[test]
#[ignore = "opt-in real process-collection timing; shared host load can cause large outliers"]
fn windows_targeted_collection_benchmark() {
    let root = std::process::id();
    let mut full = System::new();
    let mut double = System::new();
    let mut native = System::new();
    for system in [&mut full, &mut double, &mut native] {
        system.refresh_cpu_list(CpuRefreshKind::nothing());
    }
    // Limit this exploratory run. Retain the actual observations (including
    // outliers); the direct variant has not established sampler equivalence.
    for round in 0..6 {
        for offset in 0..4 {
            let mode = (round + offset) % 4;
            let start = Instant::now();
            let mut topology_us = 0;
            let mut selected_count = 0;
            let mut missing = 0;
            let (mode_name, count) = match mode {
                0 => {
                    refresh(&mut full, ProcessesToUpdate::All, true);
                    assert!(full.process(Pid::from_u32(root)).unwrap().memory() > 0);
                    ("full", full.processes().len())
                }
                1 => {
                    refresh(&mut double, ProcessesToUpdate::All, false);
                    let ids = selected_sysinfo_pids(&double, &[root]);
                    selected_count = ids.len();
                    topology_us = start.elapsed().as_micros();
                    refresh(&mut double, ProcessesToUpdate::Some(&ids), true);
                    assert!(double.process(Pid::from_u32(root)).unwrap().memory() > 0);
                    ("double", double.processes().len())
                }
                2 => {
                    let topology = native_topology().unwrap();
                    let ids: Vec<_> = select_targets(&topology.rows, &[root])
                        .into_iter()
                        .map(Pid::from_u32)
                        .collect();
                    selected_count = ids.len();
                    topology_us = start.elapsed().as_micros();
                    refresh(&mut native, ProcessesToUpdate::Some(&ids), true);
                    assert!(native.process(Pid::from_u32(root)).unwrap().memory() > 0);
                    ("native_target", topology.rows.len())
                }
                _ => {
                    let topology = native_topology().unwrap();
                    let ids = select_targets(&topology.rows, &[root]);
                    selected_count = ids.len();
                    topology_us = start.elapsed().as_micros();
                    let system_ticks = global_ticks().expect("system times");
                    std::hint::black_box(system_ticks);
                    let mut saw_root = false;
                    for pid in ids {
                        if let Ok(sample) = direct_sample(pid, topology.started) {
                            if pid == root {
                                saw_root = sample.working_set > 0;
                            }
                            std::hint::black_box(sample);
                        } else {
                            missing += 1;
                        }
                    }
                    assert!(saw_root);
                    ("native_direct", topology.rows.len())
                }
            };
            println!("TARGETED_PROBE round={round} mode={mode_name} table_count={count} selected={selected_count} missing={missing} topology_us={topology_us} total_us={}", start.elapsed().as_micros());
        }
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.max(Duration::from_millis(250)));
    }
}

mod backend;

/// Optional production backend. Captures only the Host's own authoritative
/// identity; the App and Session generations must come from their owners.
#[derive(Default)]
pub(super) struct NativeSampler {
    controller: backend::BackendController,
    host: Option<super::ShellProcess>,
}

pub(super) enum SampleOutcome {
    Native(super::ProcessIndex, bool),
    Legacy { reset: bool },
}

impl NativeSampler {
    pub(super) fn reset(&mut self) {
        self.controller.reset();
        self.host = None;
    }

    pub(super) fn sample(
        &mut self,
        sessions: &[super::SessionPidRoot],
        host_pid: u32,
        app_pid: Option<u32>,
        app_creation: Option<u64>,
        cores: usize,
    ) -> SampleOutcome {
        use backend::{BackendResult, Reject, Root};

        if self.host.is_none() && host_pid == std::process::id() {
            // A running process cannot recycle its own PID while making this
            // call. Open an owned, query-only handle, not a guessed App PID.
            if let Ok(raw) = unsafe {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                    false,
                    host_pid,
                )
            } {
                let handle = OwnedHandle(raw);
                self.host = super::seal_shell_from_handle(host_pid, handle.0 .0);
            }
        }
        let host = self.host.filter(|seal| seal.pid == host_pid);
        let mut roots = Vec::with_capacity(sessions.len() + 2);
        roots.push(host.map_or_else(|| Root::plain(host_pid), Root::shell));
        if let Some(pid) = app_pid {
            let seal = app_creation.and_then(|creation| {
                super::ShellProcess::from_windows_creation_time(pid, creation)
            });
            roots.push(seal.map_or_else(|| Root::plain(pid), Root::shell));
        }
        for session in sessions {
            if let Some(shell) = session.shell {
                roots.push(Root::shell(shell));
            } else if session.running {
                // Unknown live Session identity must stay on the old path.
                roots.push(Root::plain(0));
            }
        }
        match self.controller.sample(
            std::time::Instant::now(),
            std::time::Instant::now,
            &roots,
            || {
                let frame = backend::collect(&roots, cores)?;
                if !frame.contains_pid(host_pid) {
                    return Err(Reject::MissingMetric);
                }
                Ok(frame)
            },
            |reset| reset,
        ) {
            BackendResult::Native(measurement) => {
                SampleOutcome::Native(measurement.index, measurement.warming)
            }
            BackendResult::Legacy {
                value,
                reason,
                reset,
            } => {
                debug_assert_eq!(value, reset);
                if reset {
                    tracing::debug!(?reason, "resource sampler falling back to sysinfo");
                }
                SampleOutcome::Legacy { reset }
            }
        }
    }
}
