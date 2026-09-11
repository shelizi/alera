use super::*;

#[test]
fn cli_mutations_use_running_runtime_host() {
    let dir = tempfile::tempdir().unwrap();
    let control_path = dir.path().join("host.json");
    let token = "cli-token";
    let (_guard, port) = spawn_host(dir.path(), &control_path, token);

    let (mut writer, mut reader) = connect(port);
    handshake(&mut writer, &mut reader, token);

    let output = alera_core::child_process::windowless_command(env!("CARGO_BIN_EXE_alera"))
        .args([
            "project",
            "--runtime-dir",
            dir.path().to_str().unwrap(),
            "--json",
            "add",
            "--id",
            "cli-project",
            "--name",
            "CLI Project",
            "--repo-path",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .expect("failed to run alera project add");
    assert!(
        output.status.success(),
        "project add failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let project: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(project["id"], json!("cli-project"));

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "never observed projectsChanged");
        let message = read_message(&mut reader);
        if message.get("event").and_then(Value::as_str) == Some("projectsChanged") {
            break;
        }
    }
}

#[test]
fn cli_mutations_use_alternate_runtime_host_control_after_legacy_host_json() {
    let dir = tempfile::tempdir().unwrap();
    let runtime_control_path = dir.path().join("runtime-host.json");
    let token = "alternate-cli-token";
    let (_guard, port) = spawn_host(dir.path(), &runtime_control_path, token);
    std::fs::write(
        dir.path().join("host.json"),
        json!({
            "protocolVersion": 2,
            "port": 1,
            "token": "legacy-token"
        })
        .to_string(),
    )
    .unwrap();

    let (mut writer, mut reader) = connect(port);
    handshake(&mut writer, &mut reader, token);

    let output = alera_core::child_process::windowless_command(env!("CARGO_BIN_EXE_alera"))
        .args([
            "project",
            "--runtime-dir",
            dir.path().to_str().unwrap(),
            "--json",
            "add",
            "--id",
            "alternate-cli-project",
            "--name",
            "Alternate CLI Project",
            "--repo-path",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .expect("failed to run alera project add");
    assert!(
        output.status.success(),
        "project add failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let project: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(project["id"], json!("alternate-cli-project"));

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "never observed projectsChanged");
        let message = read_message(&mut reader);
        if message.get("event").and_then(Value::as_str) == Some("projectsChanged") {
            break;
        }
    }
}

#[test]
fn cli_rejects_unknown_tab_kind() {
    let dir = tempfile::tempdir().unwrap();

    let output = alera_core::child_process::windowless_command(env!("CARGO_BIN_EXE_alera"))
        .args([
            "tab",
            "--runtime-dir",
            dir.path().to_str().unwrap(),
            "--json",
            "create",
            "--workspace-id",
            "workspace-1",
            "--title",
            "Broken",
            "--kind",
            "typo",
        ])
        .output()
        .expect("failed to run alera tab create");

    assert_eq!(
        output.status.code(),
        Some(64),
        "tab create should fail as a usage error: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "invalid tab creation should not print JSON"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Unsupported tab kind"),
        "stderr should explain the invalid tab kind: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !dir.path().join("runtime.sqlite").exists(),
        "invalid tab creation should not open or mutate the runtime store"
    );
}

#[test]
fn cli_bootstrap_cancel_requires_runtime_host_job() {
    let dir = tempfile::tempdir().unwrap();

    let output = alera_core::child_process::windowless_command(env!("CARGO_BIN_EXE_alera"))
        .args([
            "ssh-target",
            "--runtime-dir",
            dir.path().to_str().unwrap(),
            "bootstrap-cancel",
            "--id",
            "target-1",
        ])
        .output()
        .expect("failed to run alera ssh-target bootstrap-cancel");

    assert!(
        !output.status.success(),
        "cancel should fail without a runtime host: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("no active runtime host bootstrap job"),
        "stderr should explain the missing active job: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
