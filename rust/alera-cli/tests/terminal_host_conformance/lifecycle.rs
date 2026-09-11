use super::*;

#[test]
fn full_protocol_sequence() {
    let dir = tempfile::tempdir().unwrap();
    let control_path = dir.path().join("host.json");
    let token = "test-token";

    let child = alera_command_with_isolated_home(dir.path())
        .args([
            "terminal-host",
            "--runtime-dir",
            dir.path().to_str().unwrap(),
            "--control-file",
            control_path.to_str().unwrap(),
            "--token",
            token,
            "--empty-shutdown-delay-seconds",
            "60",
            "--detached-session-shutdown-delay-seconds",
            "60",
        ])
        .spawn()
        .expect("failed to spawn alera terminal-host");
    let _guard = HostGuard(child);

    // Wait for the host to bind and publish its control file.
    let deadline = Instant::now() + Duration::from_secs(10);
    let (port, file_token) = loop {
        if let Some(control) = read_control(&control_path) {
            break control;
        }
        assert!(Instant::now() < deadline, "control file was never written");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(file_token, token);

    // First client: create a session that prints a marker and exits with code 7.
    let (mut writer, mut reader) = connect(port);
    handshake(&mut writer, &mut reader, token);
    send(
        &mut writer,
        json!({
            "id": 1,
            "type": "createOrAttach",
            "payload": {
                "sessionId": "s1",
                "workspaceId": "w1",
                "tabId": "t1",
                "workingDirectory": terminal_host_test_platform::working_directory(),
                "launch": terminal_host_test_platform::marker_exit_launch("MARKER", 7),
                "cols": 80,
                "rows": 24
            }
        }),
    );

    let mut output: Vec<u8> = Vec::new();
    let mut created = None;
    let mut exit_code = None;
    let event_deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < event_deadline {
        let message = read_message(&mut reader);
        if message.get("id") == Some(&json!(1)) {
            assert_eq!(
                message["ok"],
                json!(true),
                "createOrAttach failed: {message}"
            );
            created = message["payload"]["created"].as_bool();
            assert_eq!(message["payload"]["running"], json!(true));
            continue;
        }
        match message.get("event").and_then(Value::as_str) {
            Some("output") => {
                collect_output(&message, &mut output);
            }
            Some("exit") => {
                exit_code = message["payload"]["exitCode"].as_i64();
                break;
            }
            Some("error") => panic!("unexpected error event: {message}"),
            _ => {}
        }
    }

    assert_eq!(
        created,
        Some(true),
        "first attach should create the session"
    );
    assert_eq!(exit_code, Some(7), "child exit code should propagate");
    let text = String::from_utf8_lossy(&output);
    assert!(text.contains("MARKER"), "PTY output was: {text:?}");

    // Second client: remint the exited session under the same handle while
    // preserving its prior snapshot.
    let (mut writer2, mut reader2) = connect(port);
    handshake(&mut writer2, &mut reader2, token);
    send(
        &mut writer2,
        json!({
            "id": 1,
            "type": "createOrAttach",
            "payload": {
                "sessionId": "s1",
                "workspaceId": "w1",
                "tabId": "t1",
                "workingDirectory": terminal_host_test_platform::working_directory(),
                "launch": terminal_host_test_platform::interactive_launch(),
                "cols": 80,
                "rows": 24
            }
        }),
    );
    let reattach = read_message(&mut reader2);
    assert_eq!(reattach["id"], json!(1));
    assert_eq!(reattach["ok"], json!(true));
    assert_eq!(
        reattach["payload"]["created"],
        json!(true),
        "reopen must remint the exited session"
    );
    assert_eq!(reattach["payload"]["running"], json!(true));
    assert_eq!(reattach["payload"]["exitCode"], Value::Null);
    let snapshot = STANDARD
        .decode(reattach["payload"]["snapshotBase64"].as_str().unwrap())
        .unwrap();
    assert!(
        String::from_utf8_lossy(&snapshot).contains("MARKER"),
        "snapshot should replay prior output"
    );

    // Terminating the session must succeed and remove its history.
    send(
        &mut writer2,
        json!({"id": 2, "type": "terminate", "payload": {"sessionId": "s1"}}),
    );
    let terminated = read_response(&mut reader2, 2);
    assert_eq!(terminated["id"], json!(2));
    assert_eq!(
        terminated["ok"],
        json!(true),
        "terminate failed: {terminated}"
    );
}

#[test]
fn rejects_bad_token() {
    let dir = tempfile::tempdir().unwrap();
    let control_path = dir.path().join("host.json");
    let token = "good-token";

    let child = alera_command_with_isolated_home(dir.path())
        .args([
            "terminal-host",
            "--runtime-dir",
            dir.path().to_str().unwrap(),
            "--control-file",
            control_path.to_str().unwrap(),
            "--token",
            token,
            "--empty-shutdown-delay-seconds",
            "60",
            "--detached-session-shutdown-delay-seconds",
            "60",
        ])
        .spawn()
        .expect("failed to spawn alera terminal-host");
    let _guard = HostGuard(child);

    let deadline = Instant::now() + Duration::from_secs(10);
    let port = loop {
        if let Some((port, _)) = read_control(&control_path) {
            break port;
        }
        assert!(Instant::now() < deadline, "control file was never written");
        std::thread::sleep(Duration::from_millis(50));
    };

    let (mut writer, mut reader) = connect(port);
    send(
        &mut writer,
        json!({"id": 0, "type": "hello", "payload": {"protocolVersion": PROTOCOL_VERSION, "token": "wrong"}}),
    );
    let response = read_message(&mut reader);
    assert_eq!(response["id"], json!(0));
    assert_eq!(response["ok"], json!(false));
    assert_eq!(
        response["error"],
        json!("Terminal host authentication failed.")
    );
}
