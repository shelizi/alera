use super::*;

#[test]
fn runtime_tab_remove_terminates_active_session() {
    let dir = tempfile::tempdir().unwrap();
    let control_path = dir.path().join("runtime-host.json");
    let token = "test-token";
    let (_guard, port) = spawn_host(dir.path(), &control_path, token);
    let (mut writer, mut reader) = connect(port);
    handshake(&mut writer, &mut reader, token);

    create_long_running_session(&mut writer, &mut reader, 1, "s-runtime-tab", "w1", "t1");

    send(
        &mut writer,
        json!({
            "id": 2,
            "type": "tab.remove",
            "payload": {"id": "t1"}
        }),
    );
    let removed = read_response(&mut reader, 2);
    assert_eq!(removed["ok"], json!(true), "tab.remove failed: {removed}");

    assert_session_is_not_attached(&mut writer, &mut reader, 3, "s-runtime-tab");
}

#[test]
fn runtime_workspace_remove_terminates_active_sessions() {
    let dir = tempfile::tempdir().unwrap();
    let control_path = dir.path().join("runtime-host.json");
    let token = "test-token";
    let (_guard, port) = spawn_host(dir.path(), &control_path, token);
    let (mut writer, mut reader) = connect(port);
    handshake(&mut writer, &mut reader, token);

    create_long_running_session(
        &mut writer,
        &mut reader,
        1,
        "s-runtime-workspace",
        "w1",
        "t1",
    );

    send(
        &mut writer,
        json!({
            "id": 2,
            "type": "workspace.remove",
            "payload": {"id": "w1", "cascadeTabs": true}
        }),
    );
    let removed = read_response(&mut reader, 2);
    assert_eq!(
        removed["ok"],
        json!(true),
        "workspace.remove failed: {removed}"
    );

    assert_session_is_not_attached(&mut writer, &mut reader, 3, "s-runtime-workspace");
}

#[test]
fn runtime_linked_review_persists_and_cascades_on_workspace_remove() {
    let dir = tempfile::tempdir().unwrap();
    let control_path = dir.path().join("runtime-host.json");
    let token = "linked-review-token";
    let (_guard, port) = spawn_host(dir.path(), &control_path, token);
    let (mut writer, mut reader) = connect(port);
    handshake(&mut writer, &mut reader, token);

    // No review linked yet.
    send(
        &mut writer,
        json!({"id": 1, "type": "linkedReview.find", "payload": {"workspaceId": "w1"}}),
    );
    let missing = read_response(&mut reader, 1);
    assert_eq!(missing["ok"], json!(true), "find failed: {missing}");
    assert_eq!(missing["payload"], Value::Null);

    // Link a review to the workspace.
    send(
        &mut writer,
        json!({
            "id": 2,
            "type": "linkedReview.upsert",
            "payload": {
                "workspaceId": "w1",
                "provider": "github",
                "number": 123,
                "url": "https://github.com/o/r/pull/123",
                "linkedAt": "2026-07-09T00:00:00Z"
            }
        }),
    );
    let saved = read_response(&mut reader, 2);
    assert_eq!(saved["ok"], json!(true), "upsert failed: {saved}");
    assert_eq!(saved["payload"]["number"], json!(123));

    send(
        &mut writer,
        json!({"id": 3, "type": "linkedReview.find", "payload": {"workspaceId": "w1"}}),
    );
    let found = read_response(&mut reader, 3);
    assert_eq!(found["payload"]["provider"], json!("github"));
    assert_eq!(
        found["payload"]["url"],
        json!("https://github.com/o/r/pull/123")
    );

    // Removing the workspace cascades the linked review.
    send(
        &mut writer,
        json!({
            "id": 4,
            "type": "workspace.remove",
            "payload": {"id": "w1", "cascadeTabs": true}
        }),
    );
    let removed = read_response(&mut reader, 4);
    assert_eq!(removed["ok"], json!(true), "remove failed: {removed}");

    send(
        &mut writer,
        json!({"id": 5, "type": "linkedReview.find", "payload": {"workspaceId": "w1"}}),
    );
    let after = read_response(&mut reader, 5);
    assert_eq!(
        after["payload"],
        Value::Null,
        "linked review should be cascaded on workspace remove: {after}"
    );
}
