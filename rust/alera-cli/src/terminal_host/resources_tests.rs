use std::sync::{mpsc, Arc, Barrier};

use super::*;

#[cfg(windows)]
#[test]
fn native_windows_is_default_with_explicit_legacy_escape_hatch() {
    use std::ffi::OsStr;

    assert!(native_windows_enabled(None));
    assert!(native_windows_enabled(Some(OsStr::new("1"))));
    for disabled in ["0", "", "false", "unexpected"] {
        assert!(!native_windows_enabled(Some(OsStr::new(disabled))));
    }
    assert!(
        ResourceSampler::default().native.is_none(),
        "unit tests opt in explicitly"
    );
}

#[test]
fn a_sample_measures_this_process_and_reports_warming_until_cpu_is_valid() {
    let mut sampler = ResourceSampler::default();
    let self_pid = std::process::id();

    // Every sweep before the platform threshold is warming, because its CPU
    // numbers would all read zero.
    for _ in 1..REFRESHES_BEFORE_CPU_IS_VALID {
        let warming = sampler.sample(&[], self_pid, None);
        assert_eq!(warming["warming"], json!(true));
        assert!(
            warming["processes"]["host"]["memoryBytes"]
                .as_u64()
                .unwrap()
                > 0
        );
        assert_eq!(warming["processes"]["app"], Value::Null);
    }

    let settled = sampler.sample(&[], self_pid, None);
    assert_eq!(settled["warming"], json!(false));
    // One history point per sweep taken so far.
    assert_eq!(
        settled["processes"]["host"]["history"]
            .as_array()
            .unwrap()
            .len(),
        REFRESHES_BEFORE_CPU_IS_VALID as usize
    );
}

fn root(shell: Option<ShellProcess>, running: bool) -> SessionPidRoot {
    SessionPidRoot {
        session_id: "session-1".to_string(),
        workspace_id: "workspace-1".to_string(),
        tab_id: "tab-1".to_string(),
        running,
        shell,
    }
}

#[test]
fn a_session_without_a_live_pid_reports_zero_and_is_not_measured() {
    let mut sampler = ResourceSampler::default();
    let roots = vec![root(None, false)];

    let snapshot = sampler.sample(&roots, std::process::id(), None);

    let session = &snapshot["sessions"][0];
    assert_eq!(session["measured"], json!(false));
    assert_eq!(session["memoryBytes"], json!(0));
    assert_eq!(session["processCount"], json!(0));
}

#[test]
fn a_session_whose_pid_was_recycled_is_not_measured() {
    // The pid is live and running, but it no longer holds the shell this
    // session spawned. Attributing it would bill a stranger's memory to
    // this terminal, so the session reports unmeasured instead.
    let mut sampler = ResourceSampler::default();
    let self_pid = std::process::id();
    let sealed = seal_shell_process(self_pid).expect("this process is live");
    let recycled = ShellProcess::coarse(self_pid, sealed.start_time + 1);

    let snapshot = sampler.sample(&[root(Some(recycled), true)], self_pid, None);

    let session = &snapshot["sessions"][0];
    assert_eq!(session["shellPid"], json!(self_pid));
    assert_eq!(session["measured"], json!(false));
    assert_eq!(session["memoryBytes"], json!(0));
    assert_eq!(session["processCount"], json!(0));
}

#[test]
fn a_session_still_holding_its_shell_is_measured() {
    let mut sampler = ResourceSampler::default();
    let self_pid = std::process::id();
    let sealed = seal_shell_process(self_pid).expect("this process is live");

    // Measured against this process, standing in for a session's shell.
    let snapshot = sampler.sample(&[root(Some(sealed), true)], 1, None);

    let session = &snapshot["sessions"][0];
    assert_eq!(session["measured"], json!(true));
    assert!(session["memoryBytes"].as_u64().unwrap() > 0);
}

/// Sample once and read back the subtree measured for `pid`.
fn host_subtree(sampler: &mut ResourceSampler, pid: u32) -> (u64, u64) {
    let snapshot = sampler.sample(&[], pid, None);
    let row = &snapshot["processes"]["host"];
    (
        row["memoryBytes"].as_u64().expect("memory is a number"),
        row["processCount"].as_u64().expect("the count is a number"),
    )
}

/// The regression case for counting threads as processes.
///
/// A thread costs a stack, not an address space, so a batch of them must
/// barely move a subtree total. When the process refresh leaves `tasks` on,
/// every thread instead enters the table as a child process whose `statm`
/// repeats the whole process's RSS, and the total scales with the thread
/// count: this process measured 26x its real memory at 97 threads.
#[test]
fn a_subtree_does_not_scale_with_the_thread_count() {
    const THREADS: usize = 64;
    let mut sampler = ResourceSampler::default();
    let self_pid = std::process::id();

    let (memory_before, count_before) = host_subtree(&mut sampler, self_pid);

    // Each thread reports in before parking on the gate, so the sample below
    // cannot race a thread that has not started yet. The gate then holds all
    // of them alive until the measurement is taken.
    let (ready_tx, ready_rx) = mpsc::channel();
    let gate = Arc::new(Barrier::new(THREADS + 1));
    let threads: Vec<_> = (0..THREADS)
        .map(|_| {
            let gate = Arc::clone(&gate);
            let ready = ready_tx.clone();
            std::thread::spawn(move || {
                ready.send(()).expect("the test is still listening");
                gate.wait();
            })
        })
        .collect();
    drop(ready_tx);
    for _ in 0..THREADS {
        ready_rx.recv().expect("every thread reports in");
    }

    let (memory_after, count_after) = host_subtree(&mut sampler, self_pid);

    gate.wait();
    for thread in threads {
        thread.join().expect("the gated thread returns");
    }

    // The process count is the sharp signal, because a thread row simply is
    // an extra row. Bounds are loose on purpose: the other cases in this
    // binary run alongside this one and spawn their own children, and the
    // whole-binary RSS moves under them. The bug multiplied this subtree by
    // 8.6x when it was measured, so it clears both by a wide margin.
    assert!(
        count_after < count_before + 16,
        "the subtree gained {} processes after spawning {THREADS} threads \
         ({count_before} -> {count_after}), so thread rows are being counted",
        count_after.saturating_sub(count_before),
    );
    assert!(
        memory_after < memory_before.saturating_mul(4),
        "subtree memory scaled with the thread count ({memory_before} -> \
         {memory_after} after spawning {THREADS} threads), so thread rows are \
         being summed"
    );
}

/// Guards the two invariants a real machine cannot break. Neither is a tight
/// bound, which is the point: they only trip on double counting, and a
/// version bump that changes what the refresh returns trips them here rather
/// than in the panel.
#[test]
fn the_totals_stay_within_what_the_machine_has() {
    let mut sampler = ResourceSampler::default();
    let self_pid = std::process::id();
    // CPU is a delta between refreshes, so it only carries a reading once
    // the baseline has settled.
    for _ in 0..REFRESHES_BEFORE_CPU_IS_VALID {
        sampler.sample(&[], self_pid, None);
    }

    let snapshot = sampler.sample(&[], self_pid, None);

    let machine_memory = snapshot["host"]["totalMemoryBytes"]
        .as_u64()
        .expect("the machine reports its memory");
    let attributed_memory = snapshot["totals"]["memoryBytes"]
        .as_u64()
        .expect("the totals carry memory");
    assert!(
        attributed_memory <= machine_memory,
        "attributed {attributed_memory} bytes, more than the {machine_memory} \
         the machine has"
    );

    let cores = snapshot["host"]["cpuCoreCount"]
        .as_u64()
        .expect("the machine reports its cores");
    let attributed_cpu = snapshot["totals"]["cpuPercent"]
        .as_f64()
        .expect("the totals carry cpu");
    // `sysinfo` already caps a single row at `cores * 100`, so a total above
    // it can only come from summing the same work twice.
    let ceiling = (cores * 100) as f64;
    assert!(
        attributed_cpu <= ceiling,
        "attributed {attributed_cpu}% cpu, more than the {ceiling}% \
         {cores} cores can do"
    );
}

#[test]
fn resetting_the_baseline_marks_the_next_sample_as_warming() {
    let mut sampler = ResourceSampler::default();
    let self_pid = std::process::id();
    for _ in 0..REFRESHES_BEFORE_CPU_IS_VALID {
        sampler.sample(&[], self_pid, None);
    }
    assert_eq!(
        sampler.sample(&[], self_pid, None)["warming"],
        json!(false),
        "the sampler should have settled before the reset"
    );

    sampler.reset_cpu_baseline();

    assert_eq!(sampler.sample(&[], self_pid, None)["warming"], json!(true));
}

#[test]
fn helper_processes_are_ignored() {
    assert!(is_ignored_helper_process("conhost.exe"));
    assert!(is_ignored_helper_process("Conhost"));
    assert!(is_ignored_helper_process("openconsole.exe"));
    assert!(is_ignored_helper_process("gitstatusd"));
    assert!(!is_ignored_helper_process("cargo.exe"));
    assert!(!is_ignored_helper_process("node"));
    assert!(!is_ignored_helper_process("python"));
}

#[tokio::test]
async fn empty_shell_reports_no_running_processes() {
    let none = sweep_session_running_processes(None).await;
    assert!(none.is_empty());

    let absent = ShellProcess::coarse(u32::MAX, 1);
    let none = sweep_session_running_processes(Some(absent)).await;
    assert!(none.is_empty());
}

#[cfg(windows)]
#[test]
fn precise_shell_resource_attribution_rejects_same_second_reuse() {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    };
    let pid = std::process::id();
    let raw = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            false,
            pid,
        )
    }
    .expect("self process");
    let handle = unsafe { OwnedHandle::from_raw_handle(raw.0) };
    let sealed = seal_shell_from_handle(pid, handle.as_raw_handle()).expect("exact seal");
    let creation = sealed.creation_filetime().unwrap();
    let other = if creation % 10_000_000 == 9_999_999 {
        creation - 1
    } else {
        creation + 1
    };
    let wrong = ShellProcess::from_windows_creation_time(pid, other).unwrap();
    assert_eq!(sealed.start_time, wrong.start_time);
    let mut sampler = ResourceSampler::default();
    let snapshot = sampler.sample(&[root(Some(sealed), true)], pid, None);
    assert_eq!(snapshot["sessions"][0]["measured"], json!(true));
    assert!(snapshot["sessions"][0]["memoryBytes"].as_u64().unwrap() > 0);
    let snapshot = sampler.sample(&[root(Some(wrong), true)], pid, None);
    assert_eq!(snapshot["sessions"][0]["measured"], json!(false));
    assert_eq!(snapshot["sessions"][0]["memoryBytes"], json!(0));
    assert_eq!(snapshot["sessions"][0]["processCount"], json!(0));
}

#[cfg(windows)]
#[test]
fn opted_in_sampler_keeps_bare_app_pid_on_legacy_backend() {
    let mut sampler = ResourceSampler::default();
    sampler.native = Some(windows_native::NativeSampler::default());
    let pid = std::process::id();
    let first = sampler.sample_with_app_identity(&[], pid, Some(pid), None);
    assert_eq!(first["warming"], json!(true));
    assert!(!sampler.system.processes().is_empty());
    assert_eq!(sampler.refreshes, 1);
    sampler.sample_with_app_identity(&[], pid, Some(pid), None);
    // Staying on legacy must not reset its CPU baseline every time.
    assert_eq!(sampler.refreshes, 2);
    sampler.release_process_cache();
    assert!(sampler.system.processes().is_empty());
    assert_eq!(sampler.refreshes, 0);
}

#[cfg(windows)]
#[test]
#[ignore = "opt-in native resource payload and fallback on a responsive single-group host"]
fn opted_in_sampler_warms_falls_back_and_keeps_history_across_idle() {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    };
    let pid = std::process::id();
    let raw = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            false,
            pid,
        )
    }
    .expect("self process");
    let handle = unsafe { OwnedHandle::from_raw_handle(raw.0) };
    let seal = seal_shell_from_handle(pid, handle.as_raw_handle()).expect("exact seal");
    let creation = seal.creation_filetime().unwrap();
    let mut sampler = ResourceSampler::default();
    sampler.native = Some(windows_native::NativeSampler::default());
    let roots = [root(Some(seal), true)];
    for step in 0..3 {
        let snapshot = sampler.sample_with_app_identity(&roots, pid, Some(pid), Some(creation));
        assert_eq!(snapshot["warming"], json!(step < 2));
        assert_eq!(snapshot["sessions"][0]["measured"], json!(true));
        assert!(snapshot["sessions"][0]["memoryBytes"].as_u64().unwrap() > 0);
        assert_eq!(
            snapshot["totals"]["memoryBytes"],
            snapshot["sessions"][0]["memoryBytes"]
        );
        assert!(
            sampler.system.processes().is_empty(),
            "native must not retain full-table rows"
        );
        assert_eq!(
            snapshot["sessions"][0]["history"].as_array().unwrap().len(),
            step + 1
        );
        if step < 2 {
            std::thread::sleep(
                sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.max(std::time::Duration::from_millis(250))
                    + std::time::Duration::from_millis(30),
            );
        }
    }
    // The exact same PID and second, but wrong full FILETIME.
    let wrong = if creation % 10_000_000 == 9_999_999 {
        creation - 1
    } else {
        creation + 1
    };
    let snapshot = sampler.sample_with_app_identity(&roots, pid, Some(pid), Some(wrong));
    assert_eq!(snapshot["warming"], json!(true));
    assert_eq!(snapshot["processes"]["app"]["memoryBytes"], json!(0));
    assert!(
        !sampler.system.processes().is_empty(),
        "wrong generation must fall back"
    );
    assert_eq!(sampler.refreshes, 1);
    sampler.sample_with_app_identity(&roots, pid, Some(pid), Some(creation));
    assert_eq!(
        sampler.refreshes, 2,
        "cooldown must retain legacy CPU baseline"
    );
    sampler.release_process_cache();
    assert!(sampler.system.processes().is_empty());
    let snapshot = sampler.sample_with_app_identity(&roots, pid, Some(pid), Some(creation));
    assert_eq!(snapshot["warming"], json!(true));
    assert!(
        sampler.system.processes().is_empty(),
        "idle release must clear native cooldown"
    );
    assert_eq!(
        snapshot["sessions"][0]["history"].as_array().unwrap().len(),
        6
    );
}
