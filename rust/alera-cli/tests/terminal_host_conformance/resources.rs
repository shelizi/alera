use super::*;

#[test]
fn resource_snapshot_attributes_a_live_session_to_its_workspace_and_tab() {
    let dir = tempfile::tempdir().unwrap();
    let control_path = dir.path().join("runtime-host.json");
    let token = "test-token";
    let (_guard, port) = spawn_host(dir.path(), &control_path, token);
    let (mut writer, mut reader) = connect(port);
    handshake(&mut writer, &mut reader, token);

    send(
        &mut writer,
        json!({"id": 1, "type": "status.get", "payload": {}}),
    );
    let status = read_response(&mut reader, 1);
    let capabilities = status["payload"]["runtimeCapabilities"].as_array().unwrap();
    assert!(
        capabilities.contains(&json!("resourceMonitorV1")),
        "resource monitor capability missing: {capabilities:?}"
    );
    // The verb is additive, so the protocol version must not have moved.
    assert_eq!(
        status["payload"]["protocolVersion"],
        json!(PROTOCOL_VERSION)
    );

    create_long_running_session(&mut writer, &mut reader, 2, "s-resources", "w1", "t1");

    // The first call starts the sampler, so it answers with the warming
    // placeholder rather than blocking on a process sweep.
    send(
        &mut writer,
        json!({"id": 3, "type": "resources.snapshot", "payload": {"appPid": std::process::id()}}),
    );
    let warming = read_response(&mut reader, 3);
    assert_eq!(warming["ok"], json!(true), "snapshot failed: {warming}");
    assert_eq!(warming["payload"]["warming"], json!(true));

    // Poll until a real sweep lands. The sampler ticks every 2s and needs two
    // refreshes before its CPU deltas mean anything.
    let mut sampled = Value::Null;
    for id in 4..14 {
        std::thread::sleep(std::time::Duration::from_millis(1_500));
        send(
            &mut writer,
            json!({"id": id, "type": "resources.snapshot", "payload": {"appPid": std::process::id()}}),
        );
        let response = read_response(&mut reader, id);
        assert_eq!(response["ok"], json!(true), "snapshot failed: {response}");
        if response["payload"]["warming"] == json!(false) {
            sampled = response["payload"].clone();
            break;
        }
    }
    assert!(!sampled.is_null(), "no settled resource sample arrived");

    let sessions = sampled["sessions"].as_array().expect("sessions array");
    let session = sessions
        .iter()
        .find(|entry| entry["sessionId"] == json!("s-resources"))
        .expect("the live session is reported");
    assert_eq!(session["workspaceId"], json!("w1"));
    assert_eq!(session["tabId"], json!("t1"));
    assert_eq!(session["running"], json!(true));
    assert!(session["shellPid"].as_u64().unwrap() > 0);
    assert_eq!(session["measured"], json!(true));
    assert!(
        session["memoryBytes"].as_u64().unwrap() > 0,
        "the shell should report resident memory: {session}"
    );

    // The app pid was supplied, so the app row is measured too, and the host
    // row never swallows the PTY children it parents.
    assert!(sampled["processes"]["app"]["memoryBytes"].as_u64().unwrap() > 0);
    assert!(
        sampled["processes"]["host"]["memoryBytes"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(sampled["host"]["totalMemoryBytes"].as_u64().unwrap() > 0);
    assert!(sampled["host"]["cpuCoreCount"].as_u64().unwrap() > 0);
}
