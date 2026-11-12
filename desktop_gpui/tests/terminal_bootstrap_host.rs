//! Opt-in, isolated CLI/Runtime Host acceptance for Native's New Terminal.
//! It never operates on Classic's user's Runtime profile or files.
#![cfg(windows)]

use alera_core::child_process::windowless_command;
use alera_gpui_desktop::runtime_bridge::{RuntimeCliBridge, RuntimeStatus};
use alera_gpui_desktop::terminal::TerminalCreateFlow;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

struct IsolatedHost {
    cli: PathBuf,
    runtime: PathBuf,
}

impl Drop for IsolatedHost {
    fn drop(&mut self) {
        // Stop only this test's private Runtime profile. Never stop the
        // normal Alera / Classic Host, even if another window is running.
        let _ = windowless_command(&self.cli)
            .args(["runtime", "--json", "--runtime-dir"])
            .arg(&self.runtime)
            .args(["stop", "--force"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

#[test]
#[ignore = "requires ALERA_GPUI_TEST_CLI and starts an isolated temporary Alera Host"]
fn explicit_new_terminal_starts_host_registers_checkout_and_reads_pty() {
    let cli = std::env::var_os("ALERA_GPUI_TEST_CLI")
        .map(PathBuf::from)
        .expect("Set ALERA_GPUI_TEST_CLI to this branch's compatible alera.exe");
    assert!(cli.is_absolute() && cli.is_file());
    let temp = tempfile::tempdir().unwrap();
    let repo_dir = temp.path().join("repo");
    std::fs::create_dir_all(&repo_dir).unwrap();
    git2::Repository::init(&repo_dir).unwrap();
    let root = dunce::canonicalize(&repo_dir).unwrap();
    let runtime = temp.path().join("runtime");
    let _cleanup = IsolatedHost {
        cli: cli.clone(),
        runtime: runtime.clone(),
    };
    let project_add = windowless_command(&cli)
        .args(["project", "--json", "--runtime-dir"])
        .arg(&runtime)
        .args(["add", "--name", "Native Test", "--repo-path"])
        .arg(&root)
        .args(["--kind", "git-repository"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        project_add.status.success(),
        "isolated project registration failed"
    );
    let project: serde_json::Value = serde_json::from_slice(&project_add.stdout).unwrap();
    assert!(project["id"].is_string());

    let bridge = RuntimeCliBridge::new(&cli, &runtime);
    assert_eq!(bridge.probe().unwrap(), RuntimeStatus::Stopped);
    let created = bridge
        .create_terminal_for_workspace(&root)
        .expect("user-selected New Terminal must bootstrap existing project");
    assert!(matches!(
        bridge.probe().unwrap(),
        RuntimeStatus::Running { .. }
    ));
    let workspaces = bridge.list_registered_workspaces().unwrap();
    assert!(workspaces
        .iter()
        .any(|ws| ws.id == created.workspace_id && ws.path == root));

    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let sessions = bridge.list_terminals().unwrap();
        if sessions.iter().any(|session| {
            session.handle == created.handle
                && session.workspace_id == created.workspace_id
                && session.tab_id == created.tab_id
                && session.running
        }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "created PTY was never listed as running"
        );
        std::thread::sleep(Duration::from_millis(100));
    }

    let marker = "ALERA_GPUI_TERMINAL_BOOTSTRAP_OK";
    bridge
        .write_terminal(&created.handle, &format!("echo {marker}"), true)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let result = bridge.read_terminal(&created.handle, None).unwrap();
        if result.text.contains(marker) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Host PTY did not return marker output"
        );
        std::thread::sleep(Duration::from_millis(150));
    }

    // Simulate an isolated CLI-owned PTY exiting after the Host created
    // its tab but before GPUI confirms the new Terminal in a pane.
    bridge
        .write_terminal(&created.handle, "exit", true)
        .expect("isolated PTY should accept a normal shell exit");
    let deadline = Instant::now() + Duration::from_secs(20);
    let stopped_snapshot = loop {
        let sessions = bridge.list_terminals().unwrap();
        if !sessions
            .iter()
            .any(|session| session.handle == created.handle && session.running)
        {
            break sessions;
        }
        assert!(
            Instant::now() < deadline,
            "isolated Host still reported the exited PTY as running"
        );
        std::thread::sleep(Duration::from_millis(100));
    };

    let mut flow = TerminalCreateFlow::default();
    let request = flow.start_request().unwrap();
    assert!(flow.finish_request(request, created));
    assert!(
        flow.confirm_sessions(&stopped_snapshot).is_none(),
        "Host must never confirm a newly created Terminal after its PTY exited"
    );
    assert!(flow.is_waiting());
    assert!(flow.start_request().is_none());
    // Exercise the direct-Agent Host path with a controllable cmd.exe fixture.
    // This is NOT a genuine provider crash; it proves that a failed Agent PTY
    // does not masquerade as running or silently reuse its old Host identity.
    let shell = dunce::canonicalize(PathBuf::from(
        std::env::var_os("ComSpec").expect("Windows ComSpec"),
    ))
    .unwrap();
    let shell = shell.to_str().unwrap();
    let first_agent = bridge
        .create_agent_terminal_for_workspace(&root, "codex", shell)
        .expect("test Agent must use exact Host create-agent tab payload");
    let mut first_flow = TerminalCreateFlow::default();
    let request = first_flow.start_request().unwrap();
    assert!(first_flow.finish_request(request, first_agent.clone()));
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let sessions = bridge.list_terminals().unwrap();
        if first_flow
            .confirm_sessions_with_host_verified_agent(&sessions, "codex")
            .is_some()
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "test Agent never reached running PTY"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    bridge
        .write_terminal(&first_agent.handle, "exit /b 23", true)
        .expect("test Agent shell must accept a nonzero exit");
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let sessions = bridge.list_terminals().unwrap();
        if !sessions
            .iter()
            .any(|item| item.handle == first_agent.handle && item.running)
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "failed Agent PTY still marked running"
        );
        std::thread::sleep(Duration::from_millis(100));
    }

    // A new explicit user action after checking the Host may start a fresh
    // PTY; it must have its own handle rather than revive the exited Agent.
    let second_agent = bridge
        .create_agent_terminal_for_workspace(&root, "codex", shell)
        .expect("explicit second Agent launch should succeed");
    assert_ne!(first_agent.handle, second_agent.handle);
    assert_ne!(first_agent.tab_id, second_agent.tab_id);
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let sessions = bridge.list_terminals().unwrap();
        let active = sessions
            .iter()
            .filter(|item| item.running)
            .collect::<Vec<_>>();
        if active
            .iter()
            .any(|item| second_agent.matches_live_session(item))
        {
            assert_eq!(active.len(), 1, "retry created duplicate running PTYs");
            assert!(!active.iter().any(|item| item.handle == first_agent.handle));
            break;
        }
        assert!(
            Instant::now() < deadline,
            "second Agent PTY never became ready"
        );
        std::thread::sleep(Duration::from_millis(100));
    }

    // A short-lived direct Agent process must exit its wrapper shell without
    // the user typing 'exit'. Otherwise Host falsely reports an Agent as live.
    let quick_exit = PathBuf::from(std::env::var_os("SystemRoot").unwrap())
        .join("System32")
        .join("where.exe");
    assert!(
        quick_exit.is_file(),
        "Windows short-lived CLI fixture missing"
    );
    let quick_agent = bridge
        .create_agent_terminal_for_workspace(&root, "claude", quick_exit.to_str().unwrap())
        .expect("short-lived direct Agent launch must create a Host tab");
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let sessions = bridge.list_terminals().unwrap();
        if !sessions
            .iter()
            .any(|item| item.handle == quick_agent.handle && item.running)
        {
            assert!(
                sessions
                    .iter()
                    .any(|item| second_agent.matches_live_session(item)),
                "short-lived Agent exit must not terminate another live PTY"
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "short-lived direct Agent left an orphaned running shell"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}
