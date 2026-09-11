use super::*;

#[test]
fn orchestration_completion_replays_after_a_committed_response_is_lost() {
    let dir = tempfile::tempdir().unwrap();
    let control_path = dir.path().join("host.json");
    let token = "completion-replay-token";
    let (_guard, port) = spawn_host(dir.path(), &control_path, token);
    let (mut writer, mut reader) = connect(port);
    handshake(&mut writer, &mut reader, token);
    create_long_running_session(
        &mut writer,
        &mut reader,
        100,
        "replay-worker",
        "w-replay",
        "t-replay",
    );

    send(
        &mut writer,
        json!({
            "id": 101,
            "type": "orchestration.taskCreate",
            "payload": {
                "spec": "complete once",
                "workspace": "w-replay",
                "coordinator": "coord"
            }
        }),
    );
    let task = read_response(&mut reader, 101);
    assert_eq!(task["ok"], json!(true), "taskCreate failed: {task}");
    let task_id = task["payload"]["id"].clone();

    send(
        &mut writer,
        json!({
            "id": 102,
            "type": "orchestration.dispatch",
            "payload": {
                "task": task_id,
                "to": "replay-worker",
                "from": "coord",
                "terminalPolicy": "keep-open"
            }
        }),
    );
    let dispatched = read_response(&mut reader, 102);
    assert_eq!(
        dispatched["ok"],
        json!(true),
        "dispatch failed: {dispatched}"
    );
    let context_token = dispatched["payload"]["contextToken"].clone();

    send(
        &mut writer,
        json!({
            "id": 103,
            "type": "orchestration.dispatchAccept",
            "payload": {
                "terminal": "replay-worker",
                "contextToken": context_token
            }
        }),
    );
    let accepted = read_response(&mut reader, 103);
    assert_eq!(accepted["ok"], json!(true), "accept failed: {accepted}");

    let result = json!({
        "summary": "done",
        "completionKind": "success",
        "artifacts": [],
        "filesModified": [],
        "validation": []
    });
    send(
        &mut writer,
        json!({
            "id": 104,
            "type": "orchestration.complete",
            "payload": {
                "terminal": "replay-worker",
                "contextToken": context_token,
                "result": result
            }
        }),
    );
    let first = read_response(&mut reader, 104);
    assert_eq!(first["ok"], json!(true), "first completion failed: {first}");

    // Model a lost response: the client retries the exact completion after the
    // store has already committed it. The protocol should preserve the store's
    // idempotency instead of rejecting the retry as "no active dispatch".
    send(
        &mut writer,
        json!({
            "id": 105,
            "type": "orchestration.complete",
            "payload": {
                "terminal": "replay-worker",
                "contextToken": context_token,
                "result": result
            }
        }),
    );
    let replay = read_response(&mut reader, 105);
    assert_eq!(
        replay["ok"],
        json!(true),
        "completion replay failed: {replay}"
    );
    assert_eq!(
        replay["payload"]["dispatchId"],
        first["payload"]["dispatchId"]
    );
    assert_eq!(replay["payload"]["dispatchStatus"], json!("completed"));

    send(
        &mut writer,
        json!({
            "id": 106,
            "type": "orchestration.taskCreate",
            "payload": {
                "spec": "new dispatch wins",
                "workspace": "w-replay",
                "coordinator": "coord"
            }
        }),
    );
    let next_task = read_response(&mut reader, 106);
    assert_eq!(
        next_task["ok"],
        json!(true),
        "second taskCreate failed: {next_task}"
    );
    send(
        &mut writer,
        json!({
            "id": 107,
            "type": "orchestration.dispatch",
            "payload": {
                "task": next_task["payload"]["id"],
                "to": "replay-worker",
                "from": "coord",
                "terminalPolicy": "keep-open"
            }
        }),
    );
    let next_dispatch = read_response(&mut reader, 107);
    assert_eq!(
        next_dispatch["ok"],
        json!(true),
        "second dispatch failed: {next_dispatch}"
    );

    send(
        &mut writer,
        json!({
            "id": 108,
            "type": "orchestration.complete",
            "payload": {
                "terminal": "replay-worker",
                "contextToken": context_token,
                "result": result
            }
        }),
    );
    let stale_replay = read_response(&mut reader, 108);
    assert_eq!(stale_replay["ok"], json!(false), "{stale_replay}");
    assert!(
        stale_replay["error"]
            .as_str()
            .is_some_and(|error| error.contains("invalid or stale")),
        "unexpected stale replay response: {stale_replay}"
    );
}
