use super::*;

/// Default isolated Workers retain the same PTY across a Host crash.
/// Replaying the Agent launch or a one-shot bootstrap would issue duplicate
/// actions. Legacy PTY-restart tests use the explicitly host-owned test mode.
#[test]
fn isolated_worker_preserves_shell_across_host_restart_without_relaunch() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("isolated-worker-starts.txt");
    let token = "worker-restart-contract-token";
    let (host, port) = spawn_host(dir.path(), token);
    let (mut writer, mut reader) = connect(port, token);
    send(
        &mut writer,
        json!({"id": 1, "type": "workspace.upsert", "payload": workspace_payload("worker-ws", dir.path())}),
    );
    assert_eq!(read_response(&mut reader, 1)["ok"], json!(true));

    let launch = json!({
        "sessionId": "persistent-worker",
        "workspaceId": "worker-ws",
        "tabId": "persistent-worker",
        "workingDirectory": dir.path().to_string_lossy(),
        "launch": {
            "shell": "/bin/sh",
            "arguments": ["-c", format!("printf X >> {}; sleep 30", marker.display())],
            "environment": {"PATH": "/usr/bin:/bin", "TERM": "xterm"}
        },
        "cols": 80,
        "rows": 24
    });
    send(
        &mut writer,
        json!({"id": 2, "type": "createOrAttach", "payload": launch}),
    );
    let first = read_response(&mut reader, 2);
    assert_eq!(first["ok"], json!(true), "{first}");
    let started = Instant::now();
    while !marker.exists() {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "isolated worker did not start its shell"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    assert_eq!(std::fs::read_to_string(&marker).unwrap(), "X");

    drop(reader);
    drop(writer);
    drop(host);
    std::fs::remove_file(dir.path().join("runtime-host.json")).unwrap();

    let (_restarted, port) = spawn_host(dir.path(), token);
    let (mut writer, mut reader) = connect(port, token);
    send(
        &mut writer,
        json!({"id": 3, "type": "createOrAttach", "payload": launch}),
    );
    let attached = read_response(&mut reader, 3);
    assert_eq!(attached["ok"], json!(true), "{attached}");
    std::thread::sleep(Duration::from_millis(250));
    assert_eq!(
        std::fs::read_to_string(&marker).unwrap(),
        "X",
        "Host reconnect must not spawn a second shell"
    );
    send(
        &mut writer,
        json!({"id": 4, "type": "terminate", "payload": {"sessionId": "persistent-worker"}}),
    );
    let ended = read_response(&mut reader, 4);
    assert_eq!(ended["ok"], json!(true), "{ended}");
}
