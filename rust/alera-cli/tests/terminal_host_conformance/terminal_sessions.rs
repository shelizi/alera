use super::*;

#[cfg(windows)]
#[test]
fn windows_pty_starts_in_requested_working_directory() {
    let dir = tempfile::tempdir().unwrap();
    let working_directory = dir.path().join("workspace with spaces");
    std::fs::create_dir_all(&working_directory).unwrap();
    let control_path = dir.path().join("runtime-host.json");
    let token = "test-token";
    let (_guard, port) = spawn_host(dir.path(), &control_path, token);
    let (mut writer, mut reader) = connect(port);
    handshake(&mut writer, &mut reader, token);

    let mut launch = terminal_host_test_platform::marker_exit_launch("unused", 0);
    launch["arguments"] = json!(["/d", "/s", "/c", "cd"]);
    send(
        &mut writer,
        json!({
            "id": 1,
            "type": "createOrAttach",
            "payload": {
                "sessionId": "cwd-session",
                "workspaceId": "cwd-workspace",
                "tabId": "cwd-tab",
                "workingDirectory": working_directory.to_string_lossy(),
                "launch": launch,
                "cols": 80,
                "rows": 24
            }
        }),
    );

    let mut output = Vec::new();
    let mut created = false;
    let mut exit_code = None;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && exit_code.is_none() {
        let message = read_message(&mut reader);
        if message.get("id") == Some(&json!(1)) {
            assert_eq!(
                message["ok"],
                json!(true),
                "createOrAttach failed: {message}"
            );
            created = true;
            continue;
        }
        if collect_output(&message, &mut output) {
            continue;
        }
        if message.get("event").and_then(Value::as_str) == Some("exit") {
            exit_code = message["payload"]["exitCode"].as_i64();
        }
    }

    assert!(created, "createOrAttach response was not observed");
    assert_eq!(exit_code, Some(0));
    let expected = working_directory.to_string_lossy().to_ascii_lowercase();
    let actual = String::from_utf8_lossy(&output).to_ascii_lowercase();
    assert!(
        actual.contains(expected.as_str()),
        "PTY started outside requested cwd; expected {expected:?}, output was {actual:?}"
    );
}

#[test]
fn pauses_output_per_client_and_resumes_from_a_delta() {
    let dir = tempfile::tempdir().unwrap();
    let control_path = dir.path().join("host.json");
    let token = "pause-token";
    let (_guard, port) = spawn_host(dir.path(), &control_path, token);

    let (mut active_writer, mut active_reader) = connect(port);
    handshake(&mut active_writer, &mut active_reader, token);
    send(
        &mut active_writer,
        json!({
            "id": 1,
            "type": "createOrAttach",
            "payload": {
                "sessionId": "paused",
                "workspaceId": "w1",
                "tabId": "t1",
                "workingDirectory": terminal_host_test_platform::working_directory(),
                "launch": terminal_host_test_platform::input_echo_launch(),
                "cols": 80,
                "rows": 24
            }
        }),
    );
    let create = read_response(&mut active_reader, 1);
    assert_eq!(create["ok"], json!(true), "create failed: {create}");

    let mut active_output = Vec::new();
    read_output_until(&mut active_reader, &mut active_output, "READY");

    let (mut paused_writer, mut paused_reader) = connect(port);
    handshake(&mut paused_writer, &mut paused_reader, token);
    send(
        &mut paused_writer,
        json!({
            "id": 1,
            "type": "createOrAttach",
            "payload": {
                "sessionId": "paused",
                "workspaceId": "w1",
                "tabId": "t1",
                "workingDirectory": terminal_host_test_platform::working_directory(),
                "launch": terminal_host_test_platform::interactive_launch(),
                "cols": 80,
                "rows": 24
            }
        }),
    );
    let attached = read_response(&mut paused_reader, 1);
    assert_eq!(attached["ok"], json!(true), "attach failed: {attached}");
    assert_eq!(attached["payload"]["created"], json!(false));
    send(
        &mut paused_writer,
        json!({"id": 2, "type": "setOutputPaused", "payload": {"sessionId": "paused", "paused": true}}),
    );
    let paused = read_response(&mut paused_reader, 2);
    assert_eq!(paused["ok"], json!(true), "pause failed: {paused}");

    send(
        &mut active_writer,
        json!({"id": 2, "type": "write", "payload": {"sessionId": "paused", "dataBase64": STANDARD.encode(b"abc\r")}}),
    );
    let _ = read_response(&mut active_reader, 2);
    read_output_until(&mut active_reader, &mut active_output, "GOT:abc");

    let mut leaked = Vec::new();
    while let Some(message) =
        read_message_with_timeout(&mut paused_reader, Duration::from_millis(200))
    {
        assert!(!collect_output(&message, &mut leaked), "{message}");
    }

    send(
        &mut paused_writer,
        json!({"id": 3, "type": "setOutputPaused", "payload": {"sessionId": "paused", "paused": false}}),
    );
    // The missed bytes are queued on the terminal lane before the reply is on
    // the control lane, and the writer drains terminal frames first, so they
    // arrive ahead of the response instead of inside it.
    let mut resumed_output = Vec::new();
    let resumed = loop {
        let message = read_message(&mut paused_reader);
        if message.get("id") == Some(&json!(3)) {
            break message;
        }
        collect_output(&message, &mut resumed_output);
    };
    assert_eq!(resumed["ok"], json!(true), "resume failed: {resumed}");
    assert_eq!(resumed["payload"]["delta"], json!(true), "{resumed}");
    assert_eq!(resumed["payload"].get("snapshotBase64"), None, "{resumed}");
    assert!(
        String::from_utf8_lossy(&resumed_output).contains("GOT:abc"),
        "the resume delta should carry the output hidden while paused"
    );
}

/// A session that exited under one host process must be restorable from the
/// SQLite checkpoint store by a freshly started host sharing the runtime dir.
/// This is the persistence guarantee that makes incremental migration safe.
#[test]
fn remints_session_from_disk_after_restart_with_prior_scrollback() {
    let dir = tempfile::tempdir().unwrap();
    let control_path = dir.path().join("host.json");
    let token = "restart-token";

    // Host A: run a session that prints a marker and exits.
    {
        let (_guard, port) = spawn_host(dir.path(), &control_path, token);
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
                    "launch": terminal_host_test_platform::delayed_markers_launch(),
                    "cols": 80,
                    "rows": 24
                }
            }),
        );
        // Drain until the exit event, which triggers an immediate checkpoint.
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            assert!(Instant::now() < deadline, "never observed session exit");
            let message = read_message(&mut reader);
            if message.get("event").and_then(Value::as_str) == Some("exit") {
                assert_eq!(message["payload"]["exitCode"], json!(3));
                break;
            }
        }
        // Round-trip a detach: the actor processes commands in order, so its OK
        // response guarantees the exit checkpoint has been flushed to SQLite
        // before we kill host A by dropping the guard.
        send(
            &mut writer,
            json!({"id": 2, "type": "detach", "payload": {"sessionId": "s1"}}),
        );
        let detached = read_message(&mut reader);
        assert_eq!(detached["id"], json!(2));
        assert_eq!(detached["ok"], json!(true));
        // Dropping _guard kills host A, leaving the checkpoint on disk.
    }

    // A killed host does not delete its control file. The app treats a stale
    // file as unusable and removes it before relaunching; do the same so we
    // wait for host B's freshly published port rather than host A's dead one.
    std::fs::remove_file(&control_path).ok();

    // Host B: remint the session, seed it with the previous output, then add a
    // new output chunk whose sequence must follow Host A's chunks.
    {
        let (_guard, port) = spawn_host(dir.path(), &control_path, token);
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
                    "launch": terminal_host_test_platform::marker_exit_launch("AFTER", 0),
                    "cols": 80,
                    "rows": 24
                }
            }),
        );
        let restored = read_message(&mut reader);
        assert_eq!(restored["id"], json!(1));
        assert_eq!(restored["ok"], json!(true), "restore failed: {restored}");
        assert_eq!(restored["payload"]["created"], json!(true));
        assert_eq!(restored["payload"]["running"], json!(true));
        assert_eq!(restored["payload"]["exitCode"], Value::Null);
        let snapshot = STANDARD
            .decode(restored["payload"]["snapshotBase64"].as_str().unwrap())
            .unwrap();
        let snapshot = String::from_utf8_lossy(&snapshot);
        assert!(snapshot.contains("FIRST"));
        assert!(snapshot.contains("SECOND"));

        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            assert!(Instant::now() < deadline, "never observed reminted exit");
            let message = read_message(&mut reader);
            if message.get("event").and_then(Value::as_str) == Some("exit") {
                assert_eq!(message["payload"]["exitCode"], json!(0));
                break;
            }
        }
        send(
            &mut writer,
            json!({"id": 2, "type": "detach", "payload": {"sessionId": "s1"}}),
        );
        let detached = read_message(&mut reader);
        assert_eq!(detached["id"], json!(2));
        assert_eq!(detached["ok"], json!(true));
    }

    std::fs::remove_file(&control_path).ok();

    // Host C observes the persisted chunks in chronological order.
    let (_guard, port) = spawn_host(dir.path(), &control_path, token);
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
                "launch": terminal_host_test_platform::interactive_launch(),
                "cols": 80,
                "rows": 24
            }
        }),
    );
    let restored = read_message(&mut reader);
    assert_eq!(restored["id"], json!(1));
    assert_eq!(restored["ok"], json!(true), "restore failed: {restored}");
    let snapshot = STANDARD
        .decode(restored["payload"]["snapshotBase64"].as_str().unwrap())
        .unwrap();
    let snapshot = String::from_utf8_lossy(&snapshot);
    let first = snapshot.find("FIRST").expect("FIRST output");
    let second = snapshot.find("SECOND").expect("SECOND output");
    let after = snapshot.find("AFTER").expect("AFTER output");
    assert!(
        first < second && second < after,
        "scrollback order: {snapshot}"
    );
}
