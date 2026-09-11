use super::*;

#[test]
fn runtime_bootstrap_start_rejects_missing_target_before_job() {
    let dir = tempfile::tempdir().unwrap();
    let control_path = dir.path().join("runtime-host.json");
    let token = "ssh-missing-token";
    let (_guard, port) = spawn_host(dir.path(), &control_path, token);
    let (mut writer, mut reader) = connect(port);
    handshake(&mut writer, &mut reader, token);

    send(
        &mut writer,
        json!({
            "id": 1,
            "type": "sshTarget.bootstrap.start",
            "payload": {
                "targetId": "missing-target",
                "platform": "linux",
                "arch": "x64",
                "artifactPath": dir.path().join("unused.tar.gz")
            }
        }),
    );
    let response = read_response(&mut reader, 1);
    assert_eq!(
        response["ok"],
        json!(false),
        "bootstrap should fail: {response}"
    );
    assert!(
        response["error"]
            .as_str()
            .is_some_and(|error| error.contains("ssh target not found")),
        "error should explain the missing target: {response}"
    );

    send(
        &mut writer,
        json!({"id": 2, "type": "sshTarget.bootstrap.jobs", "payload": {}}),
    );
    let jobs = read_response(&mut reader, 2);
    assert_eq!(jobs["ok"], json!(true), "jobs failed: {jobs}");
    assert_eq!(jobs["payload"], json!([]));
}

#[test]
fn runtime_bootstrap_start_rejects_password_target_before_job() {
    let dir = tempfile::tempdir().unwrap();
    let control_path = dir.path().join("runtime-host.json");
    let token = "ssh-password-token";
    let (_guard, port) = spawn_host(dir.path(), &control_path, token);
    let (mut writer, mut reader) = connect(port);
    handshake(&mut writer, &mut reader, token);

    let mut target = ssh_target_payload("remote-password", "notInstalled");
    target["authKind"] = json!("password");
    send(
        &mut writer,
        json!({
            "id": 1,
            "type": "sshTarget.upsert",
            "payload": target
        }),
    );
    let saved = read_response(&mut reader, 1);
    assert_eq!(saved["ok"], json!(true), "upsert failed: {saved}");

    send(
        &mut writer,
        json!({
            "id": 2,
            "type": "sshTarget.bootstrap.start",
            "payload": {
                "targetId": "remote-password",
                "platform": "linux",
                "arch": "x64",
                "artifactPath": dir.path().join("unused.tar.gz")
            }
        }),
    );
    let response = read_response(&mut reader, 2);
    assert_eq!(
        response["ok"],
        json!(false),
        "bootstrap should fail: {response}"
    );
    assert!(
        response["error"]
            .as_str()
            .is_some_and(|error| error.contains("password SSH targets are not supported")),
        "error should explain the unsupported auth: {response}"
    );

    send(
        &mut writer,
        json!({"id": 3, "type": "sshTarget.bootstrap.jobs", "payload": {}}),
    );
    let jobs = read_response(&mut reader, 3);
    assert_eq!(jobs["ok"], json!(true), "jobs failed: {jobs}");
    assert_eq!(jobs["payload"], json!([]));
}

#[test]
fn runtime_ssh_target_upsert_distinguishes_omitted_and_null_install_dir() {
    let dir = tempfile::tempdir().unwrap();
    let control_path = dir.path().join("runtime-host.json");
    let token = "ssh-install-dir-token";
    let (_guard, port) = spawn_host(dir.path(), &control_path, token);
    let (mut writer, mut reader) = connect(port);
    handshake(&mut writer, &mut reader, token);

    let mut target = ssh_target_payload("remote-dir", "notInstalled");
    target["installDir"] = json!("/custom/alera/runtime");
    send(
        &mut writer,
        json!({
            "id": 1,
            "type": "sshTarget.upsert",
            "payload": target
        }),
    );
    let saved = read_response(&mut reader, 1);
    assert_eq!(saved["ok"], json!(true), "upsert failed: {saved}");
    assert_eq!(
        saved["payload"]["installDir"],
        json!("/custom/alera/runtime")
    );

    let mut omitted = ssh_target_payload("remote-dir", "notInstalled");
    omitted["host"] = json!("renamed.example.invalid");
    omitted.as_object_mut().unwrap().remove("installDir");
    send(
        &mut writer,
        json!({
            "id": 2,
            "type": "sshTarget.upsert",
            "payload": omitted
        }),
    );
    let preserved = read_response(&mut reader, 2);
    assert_eq!(
        preserved["payload"]["installDir"],
        json!("/custom/alera/runtime")
    );
    assert_eq!(
        preserved["payload"]["host"],
        json!("renamed.example.invalid")
    );

    let cleared = ssh_target_payload("remote-dir", "notInstalled");
    send(
        &mut writer,
        json!({
            "id": 3,
            "type": "sshTarget.upsert",
            "payload": cleared
        }),
    );
    let cleared = read_response(&mut reader, 3);
    assert_eq!(cleared["payload"]["installDir"], Value::Null);
}

#[test]
fn runtime_failed_bootstrap_preserves_previous_runtime_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let control_path = dir.path().join("runtime-host.json");
    let token = "ssh-failed-metadata-token";
    let path = fake_ready_ssh_path(dir.path());
    let (_guard, port) = spawn_host_with_env(dir.path(), &control_path, token, &[("PATH", &path)]);
    let (mut writer, mut reader) = connect(port);
    handshake(&mut writer, &mut reader, token);

    let mut target = ssh_target_payload("remote-installed", "installed");
    target["runtimeVersion"] = json!("1.2.2");
    target["runtimePlatform"] = json!("linux");
    target["runtimeArch"] = json!("x64");
    send(
        &mut writer,
        json!({
            "id": 1,
            "type": "sshTarget.upsert",
            "payload": target
        }),
    );
    let saved = read_response(&mut reader, 1);
    assert_eq!(saved["ok"], json!(true), "upsert failed: {saved}");

    send(
        &mut writer,
        json!({
            "id": 2,
            "type": "sshTarget.bootstrap.start",
            "payload": {
                "targetId": "remote-installed",
                "platform": "linux",
                "arch": "x64",
                "version": "1.2.3",
                "artifactPath": dir.path().join("missing-runtime.tar.gz")
            }
        }),
    );
    let started = read_response(&mut reader, 2);
    assert_eq!(
        started["ok"],
        json!(true),
        "bootstrap start failed: {started}"
    );

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            Instant::now() < deadline,
            "bootstrap failure event was never emitted"
        );
        let message = read_message(&mut reader);
        if message["event"] == json!("sshTargetBootstrapProgress")
            && message["payload"]["targetId"] == json!("remote-installed")
            && message["payload"]["status"] == json!("failed")
        {
            break;
        }
    }

    send(
        &mut writer,
        json!({"id": 3, "type": "sshTarget.list", "payload": {}}),
    );
    let listed = read_response(&mut reader, 3);
    assert_eq!(listed["ok"], json!(true), "list failed: {listed}");
    let target = listed["payload"]
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| item["id"] == json!("remote-installed"))
        })
        .expect("saved target should be listed");
    assert_eq!(target["bootstrapStatus"], json!("failed"));
    assert_eq!(target["runtimeVersion"], json!("1.2.2"));
    assert_eq!(target["runtimePlatform"], json!("linux"));
    assert_eq!(target["runtimeArch"], json!("x64"));
}

#[test]
fn runtime_remove_aborts_active_ssh_bootstrap_job() {
    let dir = tempfile::tempdir().unwrap();
    let control_path = dir.path().join("runtime-host.json");
    let token = "ssh-remove-token";
    let path = fake_blocking_ssh_path(dir.path());
    let (_guard, port) = spawn_host_with_env(dir.path(), &control_path, token, &[("PATH", &path)]);
    let (mut writer, mut reader) = connect(port);
    handshake(&mut writer, &mut reader, token);

    send(
        &mut writer,
        json!({
            "id": 1,
            "type": "sshTarget.upsert",
            "payload": ssh_target_payload("remote-active", "notInstalled")
        }),
    );
    let saved = read_response(&mut reader, 1);
    assert_eq!(saved["ok"], json!(true), "upsert failed: {saved}");

    send(
        &mut writer,
        json!({
            "id": 2,
            "type": "sshTarget.bootstrap.start",
            "payload": {
                "targetId": "remote-active",
                "platform": "linux",
                "arch": "x64",
                "artifactPath": dir.path().join("unused.tar.gz")
            }
        }),
    );
    let started = read_response(&mut reader, 2);
    assert_eq!(
        started["ok"],
        json!(true),
        "bootstrap start failed: {started}"
    );
    assert_eq!(started["payload"]["status"], json!("installing"));

    send(
        &mut writer,
        json!({"id": 3, "type": "sshTarget.list", "payload": {}}),
    );
    let installing_targets = read_response(&mut reader, 3);
    assert_eq!(
        installing_targets["ok"],
        json!(true),
        "list failed: {installing_targets}"
    );
    assert_eq!(
        installing_targets["payload"][0]["bootstrapStatus"],
        json!("installing")
    );

    send(
        &mut writer,
        json!({
            "id": 4,
            "type": "sshTarget.remove",
            "payload": {"id": "remote-active"}
        }),
    );
    let removed = read_response(&mut reader, 4);
    assert_eq!(removed["ok"], json!(true), "remove failed: {removed}");

    send(
        &mut writer,
        json!({"id": 5, "type": "sshTarget.bootstrap.jobs", "payload": {}}),
    );
    let jobs = read_response(&mut reader, 5);
    assert_eq!(jobs["ok"], json!(true), "jobs failed: {jobs}");
    assert_eq!(jobs["payload"], json!([]));

    send(
        &mut writer,
        json!({"id": 6, "type": "sshTarget.list", "payload": {}}),
    );
    let targets = read_response(&mut reader, 6);
    assert_eq!(targets["ok"], json!(true), "list failed: {targets}");
    assert_eq!(targets["payload"], json!([]));
}

#[test]
fn runtime_cancel_clears_stale_installing_ssh_bootstrap_state() {
    let dir = tempfile::tempdir().unwrap();
    let control_path = dir.path().join("runtime-host.json");
    let token = "ssh-stale-token";

    {
        let (_guard, port) = spawn_host(dir.path(), &control_path, token);
        let (mut writer, mut reader) = connect(port);
        handshake(&mut writer, &mut reader, token);
        send(
            &mut writer,
            json!({
                "id": 1,
                "type": "sshTarget.upsert",
                "payload": ssh_target_payload("remote-stale", "installing")
            }),
        );
        let saved = read_response(&mut reader, 1);
        assert_eq!(saved["ok"], json!(true), "upsert failed: {saved}");
    }

    std::fs::remove_file(&control_path).ok();
    let (_guard, port) = spawn_host(dir.path(), &control_path, token);
    let (mut writer, mut reader) = connect(port);
    handshake(&mut writer, &mut reader, token);

    send(
        &mut writer,
        json!({
            "id": 1,
            "type": "sshTarget.bootstrap.cancel",
            "payload": {"id": "remote-stale"}
        }),
    );
    let cancelled = read_response(&mut reader, 1);
    assert_eq!(
        cancelled["ok"],
        json!(true),
        "stale cancel failed: {cancelled}"
    );
    assert_eq!(cancelled["payload"]["bootstrapStatus"], json!("cancelled"));
    assert_eq!(cancelled["payload"]["lastError"], Value::Null);

    send(
        &mut writer,
        json!({"id": 2, "type": "sshTarget.list", "payload": {}}),
    );
    let targets = read_response(&mut reader, 2);
    assert_eq!(targets["ok"], json!(true), "list failed: {targets}");
    assert_eq!(targets["payload"][0]["bootstrapStatus"], json!("cancelled"));
    assert_eq!(targets["payload"][0]["lastError"], Value::Null);
}
