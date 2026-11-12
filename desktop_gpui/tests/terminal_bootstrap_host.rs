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
}
