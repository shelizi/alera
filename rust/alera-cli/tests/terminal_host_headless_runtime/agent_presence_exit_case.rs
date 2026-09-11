use super::*;

#[test]
#[cfg(unix)]
fn terminating_terminal_broadcasts_agent_presence_removal() {
    let dir = tempfile::tempdir().unwrap();
    let token = "terminal-presence-exit-token";
    let (_guard, port) = spawn_host(dir.path(), token);
    let (mut writer, mut reader) = connect(port, token);

    send(
        &mut writer,
        json!({"id": 1, "type": "createOrAttach", "payload": {
            "sessionId": "presence-exit-session", "workspaceId": "workspace-1", "tabId": "presence-exit-tab",
            "workingDirectory": "/tmp", "launch": {"shell": "/bin/sh", "arguments": ["-c", "sleep 30"],
            "environment": {"PATH": "/usr/bin:/bin", "TERM": "xterm"}}, "cols": 80, "rows": 24
        }}),
    );
    assert_eq!(read_response(&mut reader, 1)["ok"], json!(true));

    send(
        &mut writer,
        json!({"id": 2, "type": "orchestration.agentStatus", "payload": {"entries": [{
            "terminalSessionId": "presence-exit-session",
            "workspaceId": "workspace-1",
            "tabId": "presence-exit-tab",
            "agentType": "codex",
            "state": "working",
            "stateStartedAt": "2026-09-09T13:00:00Z",
            "updatedAt": "2026-09-09T13:00:01Z"
        }]}}),
    );
    assert_eq!(read_response(&mut reader, 2)["ok"], json!(true));

    send(
        &mut writer,
        json!({"id": 3, "type": "terminate", "payload": {"sessionId": "presence-exit-session"}}),
    );
    let mut saw_presence_change = false;
    let terminate_response = loop {
        let message = read_message(&mut reader);
        if message.get("event").and_then(Value::as_str) == Some("agentPresenceChanged") {
            saw_presence_change = true;
            assert_eq!(
                message["payload"]["changes"][0]["terminalSessionId"],
                json!("presence-exit-session")
            );
            assert_eq!(message["payload"]["changes"][0]["removed"], json!(true));
        }
        if message.get("id") == Some(&json!(3)) {
            break message;
        }
    };
    assert_eq!(terminate_response["ok"], json!(true));
    assert!(
        saw_presence_change,
        "terminating a background terminal did not broadcast presence removal"
    );
}
