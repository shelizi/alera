use std::collections::HashSet;
use std::time::Instant;

use chrono::Utc;
use serde_json::{json, Value};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

#[cfg(test)]
mod cache_release_tests;
mod history;
mod host_metrics;
mod process_tree;
mod sampling_cadence;
#[cfg(windows)]
mod shell_identity;
mod snapshot_payload;
#[cfg(windows)]
pub use shell_identity::{recover_precise_shell, seal_shell_from_handle};
#[cfg(windows)]
mod windows_native;

use history::ResourceHistory;
use process_tree::{ProcessRow, SubtreeUsage};
use snapshot_payload::{
    accumulate, attach_rows, memory_usage_percent, process_json, session_json, APP_HISTORY_KEY,
    HOST_HISTORY_KEY,
};

pub use process_tree::{ProcessIndex, ShellProcess};
pub use sampling_cadence::{
    clamp_resource_interval, resource_idle_stop_for, RESOURCE_IDLE_STOP, RESOURCE_SAMPLE_INTERVAL,
};
pub use snapshot_payload::warming_snapshot;

/// `sysinfo` derives CPU from the delta between refreshes, so the first sweeps
/// after a (re)start report zero for every process.
///
/// Windows needs one sweep more than the others. Measured on sysinfo 0.39.6
/// against processes pegging a full core: Linux and macOS report ~100% on the
/// second sweep, Windows still reports 0.0 there and only reports ~96% on the
/// third. Treating the second sweep as valid on Windows would publish a
/// confident 0% for a machine that is actually saturated.
#[cfg(windows)]
const REFRESHES_BEFORE_CPU_IS_VALID: u32 = 3;
#[cfg(not(windows))]
const REFRESHES_BEFORE_CPU_IS_VALID: u32 = 2;

/// The attribution root for one terminal session.
#[derive(Debug, Clone)]
pub struct SessionPidRoot {
    pub session_id: String,
    pub workspace_id: String,
    pub tab_id: String,
    pub running: bool,
    pub shell: Option<ShellProcess>,
}

/// Observe a freshly spawned pid's start time, so later sweeps can tell that
/// process apart from whatever the OS puts at the same pid once it exits.
///
/// Refreshes only that pid. This runs on every terminal spawn, and a full
/// process-table walk there would be charged to every new tab.
///
/// `None` when the pid is already gone, which leaves the session unmeasured
/// rather than measuring a guess.
pub fn seal_shell_process(pid: u32) -> Option<ShellProcess> {
    let sysinfo_pid = Pid::from_u32(pid);
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[sysinfo_pid]),
        true,
        ProcessRefreshKind::nothing(),
    );
    system
        .process(sysinfo_pid)
        .map(|process| ShellProcess::coarse(pid, process.start_time()))
}

/// Index the whole process table for identity and parent links only.
///
/// Skips the cpu and memory refresh the sampler needs, so a caller that only
/// walks the tree (terminating a shell's descendants) pays for a much cheaper
/// sweep. The usage fields on those rows read zero, since they were never
/// collected.
///
/// `without_tasks` keeps threads out of the table. They would otherwise show up
/// as child processes, which makes a shell's own threads read as its
/// descendants: the kill walk would then signal the shell before the root killer
/// runs, and `descendants` would stop meaning what its name says.
#[cfg(unix)]
pub fn sweep_process_topology() -> ProcessIndex {
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().without_tasks(),
    );
    ProcessIndex::build(system.processes().iter().map(|(pid, process)| {
        ProcessRow::topology_only(
            pid.as_u32(),
            process.parent().map(|parent| parent.as_u32()),
            process.start_time(),
        )
    }))
}

/// A running child process under a terminal shell.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningProcessInfo {
    pub pid: u32,
    pub name: String,
}

/// Benign shell background/helper processes that should not prevent terminal closure.
pub fn is_ignored_helper_process(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let base = lower.trim_end_matches(".exe");
    matches!(base, "conhost" | "openconsole" | "gitstatusd")
}

/// Collects live descendant processes of a shell, filtering out benign helpers.
pub fn live_descendant_processes(shell: ShellProcess) -> Vec<RunningProcessInfo> {
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().without_tasks(),
    );
    let index = ProcessIndex::build(system.processes().iter().map(|(pid, process)| {
        ProcessRow::topology_only(
            pid.as_u32(),
            process.parent().map(|parent| parent.as_u32()),
            process.start_time(),
        )
    }));
    if !index.holds(shell) {
        return Vec::new();
    }
    let descendants = index.descendants(shell.pid);
    let mut result = Vec::new();
    for pid in descendants {
        let sysinfo_pid = Pid::from_u32(pid);
        if let Some(process) = system.process(sysinfo_pid) {
            let name = process.name().to_string_lossy().into_owned();
            if !is_ignored_helper_process(&name) {
                result.push(RunningProcessInfo { pid, name });
            }
        }
    }
    result
}

/// Asynchronously sweeps the session's shell tree on a blocking task.
pub async fn sweep_session_running_processes(
    shell: Option<ShellProcess>,
) -> Vec<RunningProcessInfo> {
    let Some(shell) = shell else {
        return Vec::new();
    };
    let sweep = tokio::task::spawn_blocking(move || live_descendant_processes(shell));
    tokio::time::timeout(std::time::Duration::from_secs(1), sweep)
        .await
        .ok()
        .and_then(|res| res.ok())
        .unwrap_or_default()
}

/// Owns the `sysinfo` handle across samples.
///
/// The handle has to outlive a single sample: CPU is a delta between two
/// refreshes, so a fresh `System` per request would always report 0%.
pub struct ResourceSampler {
    system: System,
    history: ResourceHistory,
    refreshes: u32,
    #[cfg(windows)]
    native: Option<windows_native::NativeSampler>,
}

impl Default for ResourceSampler {
    fn default() -> Self {
        ResourceSampler {
            system: System::new(),
            history: ResourceHistory::default(),
            refreshes: 0,
            #[cfg(windows)]
            native: if !cfg!(test)
                && std::env::var_os("ALERA_RESOURCE_NATIVE_WINDOWS").as_deref()
                    == Some(std::ffi::OsStr::new("1"))
            {
                Some(windows_native::NativeSampler::default())
            } else {
                None
            },
        }
    }
}

impl ResourceSampler {
    /// Forget the CPU baseline. Called when the ticker restarts after an idle
    /// gap, because a delta measured across that gap would describe minutes of
    /// history as if it were the last two seconds.
    pub fn reset_cpu_baseline(&mut self) {
        self.refreshes = 0;
        #[cfg(windows)]
        if let Some(native) = &mut self.native {
            native.reset();
        }
    }

    /// Drop process rows once nobody is reading resource snapshots.
    ///
    /// History stays available for the next viewing window, but the process
    /// table and its CPU baseline are useful only while sampling is active.
    pub fn release_process_cache(&mut self) {
        self.system = System::new();
        self.reset_cpu_baseline();
    }

    #[cfg(test)]
    pub fn sample(
        &mut self,
        roots: &[SessionPidRoot],
        host_pid: u32,
        app_pid: Option<u32>,
    ) -> Value {
        self.sample_with_app_identity(roots, host_pid, app_pid, None)
    }

    /// Collect resources on a blocking thread. Both backends share the same
    /// attribution, history, and wire payload path.
    pub fn sample_with_app_identity(
        &mut self,
        roots: &[SessionPidRoot],
        host_pid: u32,
        app_pid: Option<u32>,
        app_creation: Option<u64>,
    ) -> Value {
        #[cfg(feature = "isolated-host-conformance")]
        let sample_started = Instant::now();
        let (index, warming, native_used) =
            self.collect_process_index(roots, host_pid, app_pid, app_creation);

        let now = Instant::now();
        let mut claimed: HashSet<u32> = HashSet::new();
        let mut totals = SubtreeUsage::default();

        // Sessions are claimed first, and deliberately so: every PTY shell is a
        // child of the host process, so measuring the host first would swallow
        // all of them into one unattributed row.
        let mut sessions = Vec::with_capacity(roots.len());
        for root in roots {
            // Identity, not just presence: a pid the OS has already recycled
            // would otherwise bill a stranger's memory to this terminal.
            let measured = root.shell.is_some_and(|shell| {
                if !index.holds(shell) {
                    return false;
                }
                if native_used {
                    // Native validated all exact generations in this frame.
                    return true;
                }
                #[cfg(windows)]
                {
                    shell_identity::matches_live_creation(shell)
                }
                #[cfg(not(windows))]
                {
                    true
                }
            });
            let usage = match root.shell {
                Some(shell) if root.running && measured => {
                    index.collect_subtree(shell.pid, &mut claimed)
                }
                _ => SubtreeUsage::default(),
            };
            accumulate(&mut totals, usage);
            let history = self
                .history
                .record(&root.session_id, usage.memory_bytes, now);
            sessions.push(session_json(root, measured, usage, history));
        }

        let host = index.collect_subtree(host_pid, &mut claimed);
        accumulate(&mut totals, host);
        let host_history = self
            .history
            .record(HOST_HISTORY_KEY, host.memory_bytes, now);
        let host_row = process_json(host_pid, host, host_history);

        let app = app_pid
            .filter(|&pid| {
                #[cfg(windows)]
                if let Some(creation) = app_creation {
                    // A supplied precise marker must not degrade to a bare
                    // PID even if native collection rejects the frame.
                    return ShellProcess::from_windows_creation_time(pid, creation).is_some_and(
                        |seal| {
                            index.holds(seal)
                                && (native_used || shell_identity::matches_live_creation(seal))
                        },
                    );
                }
                #[cfg(not(windows))]
                let _ = (pid, app_creation);
                true
            })
            .map(|pid| index.collect_subtree(pid, &mut claimed))
            .unwrap_or_default();
        accumulate(&mut totals, app);
        let app_history = self.history.record(APP_HISTORY_KEY, app.memory_bytes, now);
        let app_row = app_pid.map(|pid| process_json(pid, app, app_history));

        self.history.evict_stale(now);

        let total_memory = self.system.total_memory();
        let available_memory = self.system.available_memory();
        let load = System::load_average();
        let snapshot = json!({
            "collectedAt": Utc::now().timestamp_millis(),
            "warming": warming,
            "host": {
                "totalMemoryBytes": total_memory,
                "availableMemoryBytes": available_memory,
                "usedMemoryBytes": total_memory.saturating_sub(available_memory),
                "memoryUsagePercent": memory_usage_percent(total_memory, available_memory),
                "cpuCoreCount": self.system.cpus().len(),
                // Zero on Windows: the platform has no load average.
                "loadAverage1m": load.one,
            },
            "totals": {
                "cpuPercent": totals.cpu_percent,
                "memoryBytes": totals.memory_bytes,
            },
        });
        let snapshot = attach_rows(snapshot, host_row, app_row, sessions);
        // Test-build-only phase telemetry; no production logging or wire
        // fields, no PID/command data, and no timing overhead in normal builds.
        #[cfg(feature = "isolated-host-conformance")]
        tracing::debug!(
            target: "resource_conformance",
            backend = if native_used { "native" } else { "sysinfo" },
            warming,
            elapsed_us = sample_started.elapsed().as_micros() as u64,
            collected_at = snapshot["collectedAt"].as_i64().unwrap_or_default(),
            sessions = roots.len(),
            "resource snapshot completed"
        );
        snapshot
    }

    fn collect_process_index(
        &mut self,
        roots: &[SessionPidRoot],
        host_pid: u32,
        app_pid: Option<u32>,
        app_creation: Option<u64>,
    ) -> (ProcessIndex, bool, bool) {
        host_metrics::refresh_host_metrics(&mut self.system);
        #[cfg(not(windows))]
        let _ = (roots, host_pid, app_pid, app_creation);
        #[cfg(windows)]
        if let Some(native) = &mut self.native {
            match native.sample(
                roots,
                host_pid,
                app_pid,
                app_creation,
                self.system.cpus().len(),
            ) {
                windows_native::SampleOutcome::Native(index, warming) => {
                    // Release the prior backend's rows/handles after a
                    // successful switch. Keep history and host metrics.
                    if !self.system.processes().is_empty() {
                        self.system = System::new();
                        host_metrics::refresh_host_metrics(&mut self.system);
                    }
                    self.refreshes = 0;
                    return (index, warming, true);
                }
                windows_native::SampleOutcome::Legacy { reset } => {
                    if reset {
                        self.system = System::new();
                        self.refreshes = 0;
                        host_metrics::refresh_host_metrics(&mut self.system);
                    }
                }
            }
        }
        // `nothing()` is not nothing: it defaults `tasks` on, and on Linux that
        // puts every thread in the table as a child process of its own. Each one
        // reports the whole process's RSS, because they share the address space,
        // so a subtree total scales with the thread count instead of measuring
        // memory: the app read 26x its real size at 97 threads. Thread CPU
        // double counts the same way, since the leader's `/proc/<pid>/stat` is
        // already the thread-group aggregate.
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .without_tasks()
                .with_cpu()
                .with_memory(),
        );
        self.refreshes = self.refreshes.saturating_add(1);
        let warming = self.refreshes < REFRESHES_BEFORE_CPU_IS_VALID;

        let index =
            ProcessIndex::build(
                self.system
                    .processes()
                    .iter()
                    .map(|(pid, process)| ProcessRow {
                        pid: pid.as_u32(),
                        parent_pid: process.parent().map(|parent| parent.as_u32()),
                        start_time: process.start_time(),
                        cpu_percent: process.cpu_usage(),
                        memory_bytes: process.memory(),
                    }),
            );
        (index, warming, false)
    }
}

#[cfg(test)]
#[path = "resources_tests.rs"]
mod tests;
