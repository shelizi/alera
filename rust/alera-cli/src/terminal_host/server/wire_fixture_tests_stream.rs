//! Golden wire-contract fixtures for the terminal output stream.
//!
//! Same shared contract as `wire_fixture_tests.rs`, covering the output lane:
//! `setOutputPaused` resume replies (delta and snapshot resend), the `output`
//! and `outputResyncRequired` push events, and the unscoped broadcast change
//! events that share a client's control lane. The JSON documents under
//! `test/fixtures/wire/` are compared field-for-field as `serde_json::Value`s.

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::{json, Value};
use tokio::sync::mpsc::{Receiver, UnboundedReceiver};

use crate::terminal_host::client::{ClientFrame, ClientHandle, CLIENT_TERMINAL_OUT_QUEUE_CAPACITY};
use crate::terminal_host::protocol::encode_bytes;
use crate::terminal_host::session::Session;

use super::actor_test_harness::{local_client, mobile_client, test_actor};
use super::ServerActor;

fn wire_fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/fixtures/wire")
        .join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// A client whose control and terminal receivers both stay alive. The resume
/// flow needs the pair: the reply travels on the control lane while the missed
/// bytes and resync nudges travel on the terminal lane. Neither
/// `test_channels` nor `test_terminal_channels` keeps both, so the channels are
/// built here.
fn two_lane_client() -> (
    ClientHandle,
    UnboundedReceiver<ClientFrame>,
    Receiver<ClientFrame>,
) {
    let (control_out, control_rx) = tokio::sync::mpsc::unbounded_channel();
    let (terminal_out, terminal_rx) =
        tokio::sync::mpsc::channel(CLIENT_TERMINAL_OUT_QUEUE_CAPACITY);
    (
        ClientHandle::new(control_out, terminal_out),
        control_rx,
        terminal_rx,
    )
}

/// Sends one request frame and returns the response plus any event frames the
/// request pushed ahead of it: broadcasts share the client's lane, so a
/// mutation's change event always lands before its response.
async fn exchange(
    actor: &mut ServerActor,
    client_id: u64,
    request: &Value,
    receiver: &mut UnboundedReceiver<ClientFrame>,
) -> (Value, Vec<Value>) {
    let request_id = request["id"].as_i64().expect("request id");
    actor.handle_line(client_id, request.to_string()).await;
    let mut events = Vec::new();
    loop {
        let frame = receiver
            .recv()
            .await
            .expect("the host should write a response")
            .as_json()
            .expect("the frame should be JSON");
        if frame["id"] == request_id {
            return (frame, events);
        }
        events.push(frame);
    }
}

async fn next_frame(receiver: &mut UnboundedReceiver<ClientFrame>) -> Value {
    receiver
        .recv()
        .await
        .expect("the host should emit an event")
        .as_json()
        .expect("the frame should be JSON")
}

async fn next_terminal_frame(receiver: &mut Receiver<ClientFrame>) -> Value {
    receiver
        .recv()
        .await
        .expect("the host should emit a terminal frame")
        .as_json()
        .expect("the frame should be JSON")
}

/// A resume the host can still place in the stream answers `delta: true` and
/// pushes the missed bytes on the terminal lane ahead of the control-lane
/// reply, so they stay ordered against live output.
#[tokio::test]
async fn output_resumed_delta_response_matches_shared_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut control_rx, mut terminal_rx) = two_lane_client();
    let mut session = Session::driver_test_stub("s1", 80, 24);
    session.attach(1);
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::from([("s1".to_string(), session)]),
    )
    .await;

    // "hello" is delivered, then the client pauses and " world" piles up.
    actor
        .sessions
        .get_mut("s1")
        .unwrap()
        .append_output(b"hello");
    actor.flush_output_batch("s1");
    assert_eq!(
        next_terminal_frame(&mut terminal_rx).await["payload"]["dataBase64"],
        json!(encode_bytes(b"hello"))
    );
    actor
        .sessions
        .get_mut("s1")
        .unwrap()
        .set_output_paused(1, true);
    actor
        .sessions
        .get_mut("s1")
        .unwrap()
        .append_output(b" world");

    let request = wire_fixture("request.set_output_paused.resume.json");
    let (response, events) = exchange(&mut actor, 1, &request, &mut control_rx).await;

    assert!(events.is_empty());
    assert_eq!(
        response,
        wire_fixture("response.ok.output_resumed_delta.json")
    );
    // The gap goes out as an ordinary output event, not inside the reply.
    assert_eq!(
        next_terminal_frame(&mut terminal_rx).await,
        json!({
            "event": "output",
            "payload": {
                "sessionId": "s1",
                "dataBase64": encode_bytes(b" world"),
            },
        })
    );
}

/// A client whose cursor fell out of the ring - or was never recorded - gets a
/// snapshot resend instead: `delta: false` with the capped scrollback tail and
/// the dims those bytes were written at.
#[tokio::test]
async fn output_resumed_snapshot_response_matches_shared_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut control_rx, mut terminal_rx) = two_lane_client();
    let mut session = Session::driver_test_stub("s1", 80, 24);
    session.set_max_bytes(4);
    session.attach(1);
    session.set_output_paused(1, true);
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::from([("s1".to_string(), session)]),
    )
    .await;

    // The ring trims past the paused client's cursor, so the gap "abcd" can no
    // longer be replayed and only a snapshot resynchronises it.
    actor
        .sessions
        .get_mut("s1")
        .unwrap()
        .append_output(b"abcdefgh");

    let request = wire_fixture("request.set_output_paused.resume.json");
    let (response, events) = exchange(&mut actor, 1, &request, &mut control_rx).await;

    assert!(events.is_empty());
    assert_eq!(
        response,
        wire_fixture("response.ok.output_resumed_snapshot.json")
    );
    assert!(
        terminal_rx.try_recv().is_err(),
        "a snapshot resume carries its bytes inside the reply"
    );
}

/// The `.legacy` pin is the shape a pre-snapshot-metadata host produced: the
/// session id and the bytes, nothing else. The current host always adds
/// `delta`, `resetInteractionModes`, and the snapshot dims, so the fixture is
/// asserted as a strict subset of the live reply rather than regenerated.
#[tokio::test]
async fn output_resumed_snapshot_legacy_fixture_is_a_prefix_of_the_live_shape() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut control_rx, mut terminal_rx) = two_lane_client();
    let mut session = Session::driver_test_stub("s1", 80, 24);
    session.set_max_bytes(4);
    session.attach(1);
    session.set_output_paused(1, true);
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::from([("s1".to_string(), session)]),
    )
    .await;
    actor
        .sessions
        .get_mut("s1")
        .unwrap()
        .append_output(b"abcdefgh");

    let request = wire_fixture("request.set_output_paused.resume.json");
    let (response, _) = exchange(&mut actor, 1, &request, &mut control_rx).await;

    let legacy = wire_fixture("response.ok.output_resumed_snapshot.legacy.json");
    assert_eq!(response["id"], legacy["id"]);
    assert_eq!(response["ok"], legacy["ok"]);
    for (key, value) in legacy["payload"].as_object().unwrap() {
        assert_eq!(
            &response["payload"][key], value,
            "legacy field {key} drifted from the live reply"
        );
    }
    assert!(
        terminal_rx.try_recv().is_err(),
        "a snapshot resume carries its bytes inside the reply"
    );
}

/// `require_session` rejects a handle that is not in the live sessions map
/// before the pause state is even looked at.
#[tokio::test]
async fn session_not_attached_error_matches_shared_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let mut request = wire_fixture("request.set_output_paused.resume.json");
    request["id"] = json!(0);
    request["payload"]["sessionId"] = json!("s-missing");

    let (response, events) = exchange(&mut actor, 1, &request, &mut receiver).await;

    assert!(events.is_empty());
    assert_eq!(
        response,
        wire_fixture("response.error.session_not_attached.json")
    );
}

/// PTY output reaches an attached, unpaused client as an `output` event on the
/// terminal lane, base64 on a text transport.
#[tokio::test]
async fn output_event_matches_shared_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, _control_rx, mut terminal_rx) = two_lane_client();
    let mut session = Session::driver_test_stub("s1", 80, 24);
    session.attach(1);
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::from([("s1".to_string(), session)]),
    )
    .await;

    actor
        .sessions
        .get_mut("s1")
        .unwrap()
        .append_output(b"\x1b[0m\xff\x00");
    actor.flush_output_batch("s1");

    assert_eq!(
        next_terminal_frame(&mut terminal_rx).await,
        wire_fixture("event.output.json")
    );
}

/// When a client's terminal queue filled up, the session flags it as
/// backpressured; the resync tick then nudges it to resume with an
/// `outputResyncRequired` event on the terminal lane.
#[tokio::test]
async fn output_resync_required_event_matches_shared_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, _control_rx, mut terminal_rx) = two_lane_client();
    let mut session = Session::driver_test_stub("s1", 80, 24);
    session.attach(1);
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::from([("s1".to_string(), session)]),
    )
    .await;

    // The state a full terminal queue leaves behind through
    // `send_terminal_output`: paused plus a pending resync flag.
    actor
        .sessions
        .get_mut("s1")
        .unwrap()
        .mark_output_backpressured(1);
    actor.handle_output_resync_tick("s1".to_string(), 1);

    assert_eq!(
        next_terminal_frame(&mut terminal_rx).await,
        wire_fixture("event.output_resync_required.json")
    );
}

/// Disconnecting an authenticated mobile client broadcasts
/// `mobileDevicesChanged` to the remaining clients.
#[tokio::test]
async fn mobile_devices_changed_event_matches_shared_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let (local_handle, mut local_rx) = ClientHandle::test_channels();
    let (mobile_handle, _mobile_rx) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([
            (1, local_client(local_handle)),
            (2, mobile_client(mobile_handle, "phone")),
        ]),
        HashMap::new(),
    )
    .await;

    actor.dispose_client(2).await;

    assert_eq!(
        next_frame(&mut local_rx).await,
        wire_fixture("event.mobile_devices_changed.json")
    );
}

/// A project mutation broadcasts the unscoped `projectsChanged` ahead of its
/// own response on the requester's lane.
#[tokio::test]
async fn projects_changed_event_matches_shared_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;

    let (response, events) = exchange(
        &mut actor,
        1,
        &json!({
            "id": 1,
            "type": "project.upsert",
            "payload": {
                "id": "project-1",
                "name": "Fixture Project",
                "repoPath": "/repo/project-1",
                "createdAt": "2026-07-27T12:34:56.789Z",
                "updatedAt": "2026-07-27T12:34:56.789Z",
                "kind": "gitRepository",
            },
        }),
        &mut receiver,
    )
    .await;

    assert_eq!(response["ok"], json!(true));
    assert_eq!(events, vec![wire_fixture("event.projects_changed.json")]);
}

/// Removing a project config broadcasts the unscoped `projectConfigsChanged`.
#[tokio::test]
async fn project_configs_changed_event_matches_shared_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;

    let (response, events) = exchange(
        &mut actor,
        1,
        &json!({
            "id": 1,
            "type": "projectConfig.remove",
            "payload": {"projectId": "project-1"},
        }),
        &mut receiver,
    )
    .await;

    assert_eq!(response["ok"], json!(true));
    assert_eq!(
        events,
        vec![wire_fixture("event.project_configs_changed.json")]
    );
}

/// A mutation broader than one workspace sends the wildcard
/// `workspaceTabsChanged`: an empty payload, never a guessed scope.
#[tokio::test]
async fn workspace_tabs_changed_wildcard_event_matches_shared_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;

    actor.broadcast_workspace_tabs_changed(None);

    assert_eq!(
        next_frame(&mut receiver).await,
        wire_fixture("event.workspace_tabs_changed.wildcard.json")
    );
}
