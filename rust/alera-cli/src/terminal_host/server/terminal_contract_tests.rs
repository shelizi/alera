//! Operation-contract coverage for the terminal request surface: the matrix in
//! `docs/terminal-operation-contract.md` names the replay, recovery, and
//! ownership guarantees each operation keeps. These tests pin the request-level
//! cells that the session, driver, output-resume, pulse, and tab-layout suites
//! do not already cover.

use std::collections::HashMap;

use alera_core::runtime::{AutomationRun, WorkspaceTabRecord};
use chrono::Utc;
use serde_json::{json, Value};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::terminal_host::client::{ClientFrame, ClientHandle};
use crate::terminal_host::protocol::decode_bytes;
use crate::terminal_host::session::{Session, SessionDriver};

use super::actor_test_harness::{local_client, mobile_client, test_actor};
use super::{ServerActor, ServerCommand};

fn terminal_tab(tab_id: &str, workspace_id: &str) -> WorkspaceTabRecord {
    let now = Utc::now();
    WorkspaceTabRecord {
        id: tab_id.to_string(),
        workspace_id: workspace_id.to_string(),
        kind: "terminal".to_string(),
        title: "Terminal".to_string(),
        created_at: now,
        updated_at: now,
        payload: json!({"terminalSessionId": "s1"}),
    }
}

fn automation_run(id: &str) -> AutomationRun {
    let now = Utc::now();
    serde_json::from_value(json!({
        "id": id,
        "automationId": "automation",
        "number": 1,
        "occurrenceKey": format!("manual|{id}"),
        "scheduledAt": now,
        "trigger": "manual",
        "actorKind": "managedAgent",
        "actorId": "profile",
        "status": "pending",
        "attemptCount": 0,
        "createdAt": now,
        "updatedAt": now,
    }))
    .unwrap()
}

async fn seed_automation_owned_tab(actor: &ServerActor, tab_id: &str, run_id: &str) {
    let mut tab = terminal_tab(tab_id, "workspace");
    tab.payload["automationOwned"] = Value::Bool(true);
    tab.payload["automationRunId"] = Value::String(run_id.to_string());
    actor.runtime_store.upsert_workspace_tab(tab).await.unwrap();
    actor
        .runtime_store
        .insert_automation_run(&automation_run(run_id))
        .await
        .unwrap();
}

async fn request(
    actor: &mut ServerActor,
    client_id: u64,
    request_id: i64,
    request_type: &str,
    payload: Value,
    receiver: &mut UnboundedReceiver<ClientFrame>,
    inbox_receiver: &mut UnboundedReceiver<ServerCommand>,
) -> Value {
    actor
        .handle_line(
            client_id,
            json!({
                "id": request_id,
                "type": request_type,
                "payload": payload,
            })
            .to_string(),
        )
        .await;
    loop {
        tokio::select! {
            command = inbox_receiver.recv() => {
                actor.handle(command.expect("the command should be delivered")).await;
            }
            frame = receiver.recv() => {
                let response = frame
                    .expect("the response should be delivered")
                    .as_json()
                    .expect("the response should be JSON");
                if response["id"] == request_id {
                    return response;
                }
            }
        }
    }
}

#[tokio::test]
async fn create_or_attach_to_a_live_session_attaches_without_revalidating_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let (handle_one, mut receiver_one) = ClientHandle::test_channels();
    let (handle_two, mut receiver_two) = ClientHandle::test_channels();
    let mut session = Session::driver_test_stub("s1", 120, 40);
    session.append_output(b"abc");
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle_one)), (2, local_client(handle_two))]),
        HashMap::from([("s1".to_string(), session)]),
    )
    .await;
    let (inbox, mut inbox_receiver) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;

    // The request's workspace/tab metadata does not match the live session;
    // the live PTY wins and the caller attaches anyway.
    let response = request(
        &mut actor,
        1,
        1,
        "createOrAttach",
        json!({
            "sessionId": "s1",
            "workspaceId": "other-workspace",
            "tabId": "other-tab",
            "workingDirectory": "/elsewhere",
        }),
        &mut receiver_one,
        &mut inbox_receiver,
    )
    .await;

    assert_eq!(response["ok"], true);
    let payload = &response["payload"];
    assert_eq!(payload["sessionId"], "s1");
    assert_eq!(payload["created"], false);
    assert_eq!(payload["running"], true);
    assert_eq!(payload["driver"]["kind"], "idle");
    assert_eq!(payload["snapshotCols"], 120);
    assert_eq!(payload["snapshotRows"], 40);
    assert_eq!(decode_bytes(payload.get("snapshotBase64")).unwrap(), b"abc");
    let session = &actor.sessions["s1"];
    assert_eq!(session.workspace_id, "workspace");
    assert_eq!(session.tab_id, "tab-s1");
    assert!(session.clients.contains(&1));
    // The attach seats the delivery cursor at the stream end.
    assert_eq!(session.delivered_output_cursor(1), Some(3));

    // A second attach replays to the same live PTY instead of respawning it.
    let instance_id = actor.sessions["s1"].instance_id();
    let response = request(
        &mut actor,
        2,
        2,
        "createOrAttach",
        json!({
            "sessionId": "s1",
            "workspaceId": "workspace",
            "tabId": "tab-s1",
            "workingDirectory": ".",
        }),
        &mut receiver_two,
        &mut inbox_receiver,
    )
    .await;
    assert_eq!(response["payload"]["created"], false);
    let session = &actor.sessions["s1"];
    assert_eq!(session.instance_id(), instance_id);
    assert!(session.clients.contains(&1) && session.clients.contains(&2));
}

#[tokio::test]
async fn desktop_app_attach_marks_an_automation_owned_tab_run_taken_over() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, _receiver) = ClientHandle::test_channels();
    let mut session = Session::driver_test_stub("s1", 120, 40);
    session.workspace_id = "workspace".into();
    session.tab_id = "tab-takeover".into();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, super::ClientState::local(handle, true))]),
        HashMap::from([("s1".to_string(), session)]),
    )
    .await;
    seed_automation_owned_tab(&actor, "tab-takeover", "run-takeover").await;

    actor
        .create_or_attach(
            1,
            &json!({
                "sessionId": "s1",
                "workspaceId": "workspace",
                "tabId": "tab-takeover",
                "workingDirectory": ".",
            }),
        )
        .await
        .unwrap();

    assert!(
        actor
            .runtime_store
            .find_automation_run("run-takeover")
            .await
            .unwrap()
            .unwrap()
            .taken_over
    );
}

#[tokio::test]
async fn local_cli_attach_does_not_mark_an_automation_owned_tab_run_taken_over() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, _receiver) = ClientHandle::test_channels();
    let mut session = Session::driver_test_stub("s1", 120, 40);
    session.workspace_id = "workspace".into();
    session.tab_id = "tab-cli".into();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::from([("s1".to_string(), session)]),
    )
    .await;
    seed_automation_owned_tab(&actor, "tab-cli", "run-cli").await;

    actor
        .create_or_attach(
            1,
            &json!({
                "sessionId": "s1",
                "workspaceId": "workspace",
                "tabId": "tab-cli",
                "workingDirectory": ".",
            }),
        )
        .await
        .unwrap();

    assert!(
        !actor
            .runtime_store
            .find_automation_run("run-cli")
            .await
            .unwrap()
            .unwrap()
            .taken_over
    );
}

#[tokio::test]
async fn create_or_attach_resumes_retained_state_from_an_absolute_output_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let (control_tx, _control_rx) = tokio::sync::mpsc::unbounded_channel();
    let (terminal_tx, mut terminal_rx) = tokio::sync::mpsc::channel(8);
    let handle = ClientHandle::new(control_tx, terminal_tx);
    let mut session = Session::driver_test_stub("s1", 120, 40);
    session.append_output(b"hello world");
    let mut actor = test_actor(
        &dir,
        HashMap::from([(2, local_client(handle))]),
        HashMap::from([("s1".to_string(), session)]),
    )
    .await;

    let payload = actor
        .create_or_attach(
            2,
            &json!({
                "sessionId": "s1",
                "workspaceId": "workspace",
                "tabId": "tab-s1",
                "workingDirectory": ".",
                "resumeCursor": 5,
            }),
        )
        .await
        .unwrap();

    assert_eq!(payload["delta"], true);
    assert_eq!(payload["resumed"], true);
    assert_eq!(payload["outputCursor"], 11);
    assert_eq!(payload.get("snapshotBase64"), None);
    let frame = terminal_rx.recv().await.expect("resume delta");
    assert!(matches!(
        frame,
        ClientFrame::SequencedTerminal { frame, .. }
            if matches!(*frame, ClientFrame::Output { ref data, .. } if data == b" world")
    ));
    assert_eq!(actor.sessions["s1"].delivered_output_cursor(2), Some(11));
}

#[tokio::test]
async fn create_or_attach_falls_back_to_full_snapshot_for_a_stale_retained_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, _receiver) = ClientHandle::test_channels();
    let mut session = Session::driver_test_stub("s1", 120, 40);
    session.set_max_bytes(4);
    session.append_output(b"abcdefgh");
    let mut actor = test_actor(
        &dir,
        HashMap::from([(2, local_client(handle))]),
        HashMap::from([("s1".to_string(), session)]),
    )
    .await;

    let payload = actor
        .create_or_attach(
            2,
            &json!({
                "sessionId": "s1",
                "workspaceId": "workspace",
                "tabId": "tab-s1",
                "workingDirectory": ".",
                "resumeCursor": 0,
            }),
        )
        .await
        .unwrap();

    assert_eq!(payload["delta"], false);
    assert_eq!(payload["outputCursor"], 8);
    assert_eq!(
        decode_bytes(payload.get("snapshotBase64")).unwrap(),
        b"efgh"
    );
    assert_eq!(actor.sessions["s1"].delivered_output_cursor(2), Some(8));
}

#[tokio::test]
async fn terminal_restart_fails_closed_when_metadata_disagrees_with_the_live_session() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut session = Session::driver_test_stub("s1", 120, 40);
    session.attach(1);
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::from([("s1".to_string(), session)]),
    )
    .await;
    let (inbox, mut inbox_receiver) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;
    let instance_id = actor.sessions["s1"].instance_id();

    let response = request(
        &mut actor,
        1,
        1,
        "terminal.restart",
        json!({
            "sessionId": "s1",
            "workspaceId": "other-workspace",
            "tabId": "tab-s1",
            "workingDirectory": ".",
            "launch": {"shell": "/bin/sh"},
        }),
        &mut receiver,
        &mut inbox_receiver,
    )
    .await;

    assert_eq!(response["ok"], false);
    assert_eq!(
        response["error"].as_str().unwrap(),
        "Terminal restart metadata does not match the live session."
    );
    // The failed restart must not have touched the live session.
    let session = &actor.sessions["s1"];
    assert!(session.running());
    assert_eq!(session.instance_id(), instance_id);
    assert!(session.clients.contains(&1));
}

#[tokio::test]
async fn write_reports_missing_exited_and_writerless_sessions_and_empty_writes_noop() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::from([("s1".to_string(), Session::driver_test_stub("s1", 80, 24))]),
    )
    .await;
    let (inbox, mut inbox_receiver) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;

    // A handle that is not in the live sessions map fails `require_session`.
    let response = request(
        &mut actor,
        1,
        1,
        "write",
        json!({"sessionId": "ghost", "dataBase64": "aGk="}),
        &mut receiver,
        &mut inbox_receiver,
    )
    .await;
    assert_eq!(response["ok"], false);
    assert_eq!(
        response["error"].as_str().unwrap(),
        "Terminal session is not attached: ghost"
    );

    // An empty write without deferredEnter is an immediate no-op ok that
    // never reaches the writer queue.
    let response = request(
        &mut actor,
        1,
        2,
        "write",
        json!({"sessionId": "s1", "dataBase64": ""}),
        &mut receiver,
        &mut inbox_receiver,
    )
    .await;
    assert_eq!(response["ok"], true);
    assert_eq!(response["payload"], json!({}));

    // The stub is running but has no PTY writer, matching the post-exit drain
    // window on Windows: the write fails instead of parking.
    let response = request(
        &mut actor,
        1,
        3,
        "write",
        json!({"sessionId": "s1", "dataBase64": "aGk="}),
        &mut receiver,
        &mut inbox_receiver,
    )
    .await;
    assert_eq!(response["ok"], false);
    assert_eq!(
        response["error"].as_str().unwrap(),
        "terminal input writer is unavailable"
    );

    // After exit the session stays in the map but rejects input.
    actor.sessions.get_mut("s1").unwrap().handle_exit(0);
    let response = request(
        &mut actor,
        1,
        4,
        "write",
        json!({"sessionId": "s1", "dataBase64": "aGk="}),
        &mut receiver,
        &mut inbox_receiver,
    )
    .await;
    assert_eq!(response["ok"], false);
    assert_eq!(
        response["error"].as_str().unwrap(),
        "Terminal session is not running."
    );
}

#[tokio::test]
async fn detach_keeps_the_session_and_terminate_deletes_tab_and_history() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut session = Session::driver_test_stub("s1", 120, 40);
    session.attach(1);
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::from([("s1".to_string(), session)]),
    )
    .await;
    actor
        .runtime_store
        .upsert_workspace_tab(terminal_tab("tab-s1", "workspace"))
        .await
        .unwrap();
    let (inbox, mut inbox_receiver) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;

    // detach checkpoints the session and drops the client's cursor, but the
    // PTY stays alive in the sessions map.
    let response = request(
        &mut actor,
        1,
        1,
        "detach",
        json!({"sessionId": "s1"}),
        &mut receiver,
        &mut inbox_receiver,
    )
    .await;
    assert_eq!(response["ok"], true);
    let session = &actor.sessions["s1"];
    assert!(session.running());
    assert!(session.clients.is_empty());
    assert_eq!(session.delivered_output_cursor(1), None);
    let checkpoint = actor.history.read("s1", usize::MAX).await.unwrap().unwrap();
    assert!(checkpoint.running);

    // terminate removes the tab row, the live session, and the persisted
    // history before the reply goes out.
    let response = request(
        &mut actor,
        1,
        2,
        "terminate",
        json!({"sessionId": "s1"}),
        &mut receiver,
        &mut inbox_receiver,
    )
    .await;
    assert_eq!(response["ok"], true);
    assert!(!actor.sessions.contains_key("s1"));
    assert!(actor
        .runtime_store
        .find_workspace_tab("tab-s1")
        .await
        .unwrap()
        .is_none());
    assert!(
        actor
            .history
            .read("s1", usize::MAX)
            .await
            .unwrap()
            .is_none(),
        "explicit termination deletes the session's history"
    );

    // A retry reports the same typed not-attached error as a handle that
    // never existed.
    let response = request(
        &mut actor,
        1,
        3,
        "terminate",
        json!({"sessionId": "s1"}),
        &mut receiver,
        &mut inbox_receiver,
    )
    .await;
    assert_eq!(response["ok"], false);
    assert_eq!(
        response["error"].as_str().unwrap(),
        "Terminal session is not attached: s1"
    );
}

#[tokio::test]
async fn terminate_replays_a_client_mutation_receipt_after_teardown() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut session = Session::driver_test_stub("s1", 120, 40);
    session.attach(1);
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::from([("s1".to_string(), session)]),
    )
    .await;
    actor
        .runtime_store
        .upsert_workspace_tab(terminal_tab("tab-s1", "workspace"))
        .await
        .unwrap();
    let (inbox, mut inbox_receiver) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;

    let payload = json!({
        "sessionId": "s1",
        "clientMutationId": "terminate-1",
    });
    let first = request(
        &mut actor,
        1,
        10,
        "terminate",
        payload.clone(),
        &mut receiver,
        &mut inbox_receiver,
    )
    .await;
    assert_eq!(first["ok"], true, "{first}");
    assert_eq!(first["payload"], json!({}));
    assert!(!actor.sessions.contains_key("s1"));
    let receipt_state: String = sqlx::query_scalar(
        "SELECT state FROM terminalHostIdempotencyReceipts
         WHERE operation = 'terminal.terminate'
           AND callerScope = 'local:cli' AND clientMutationId = 'terminate-1'",
    )
    .fetch_one(actor.runtime_store.pool())
    .await
    .unwrap();
    assert_eq!(receipt_state, "settled");

    let retry = request(
        &mut actor,
        1,
        11,
        "terminate",
        payload,
        &mut receiver,
        &mut inbox_receiver,
    )
    .await;
    assert_eq!(retry["ok"], true, "{retry}");
    assert_eq!(retry["payload"], first["payload"]);

    let conflict = request(
        &mut actor,
        1,
        12,
        "terminate",
        json!({
            "sessionId": "different-session",
            "clientMutationId": "terminate-1",
        }),
        &mut receiver,
        &mut inbox_receiver,
    )
    .await;
    assert_eq!(conflict["ok"], false, "{conflict}");
    assert!(
        conflict["error"]
            .as_str()
            .is_some_and(|error| error.contains("different terminal.terminate payload")),
        "{conflict}"
    );
    assert!(conflict.get("errorCode").is_none(), "{conflict}");
}

#[tokio::test]
async fn disconnect_detaches_without_killing_the_pty_and_idle_shutdown_preserves_history() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, _receiver) = ClientHandle::test_channels();
    let mut session = Session::driver_test_stub("s1", 120, 40);
    session.attach(2);
    let mut actor = test_actor(
        &dir,
        HashMap::from([(2, mobile_client(handle, "phone-1"))]),
        HashMap::from([("s1".to_string(), session)]),
    )
    .await;
    actor.claim_mobile_driver(2, "s1", Some((48, 22)));

    actor.dispose_client(2).await;

    // Disconnect detaches the client everywhere but leaves the PTY alone.
    let session = &actor.sessions["s1"];
    assert!(session.running());
    assert!(session.clients.is_empty());
    assert_eq!(session.delivered_output_cursor(2), None);
    // The mobile seat was released and the desktop dims restored.
    assert_eq!(session.driver, SessionDriver::Idle);
    assert_eq!(session.current_dims, (120, 40));
    assert!(!actor.disposed);

    // A shutdown tick from a superseded generation is ignored.
    let generation = actor.shutdown_gen;
    actor.handle_shutdown_tick(generation.wrapping_sub(1)).await;
    assert!(!actor.disposed);
    assert!(actor.sessions.contains_key("s1"));

    // The current generation's tick disposes the host: sessions terminate
    // with history preserved so the next host start can restore them.
    actor.handle_shutdown_tick(generation).await;
    assert!(actor.disposed);
    assert!(actor.sessions.is_empty());
    let checkpoint = actor.history.read("s1", usize::MAX).await.unwrap().unwrap();
    assert!(!checkpoint.running);
    assert!(checkpoint.ended_at.is_some());
}
