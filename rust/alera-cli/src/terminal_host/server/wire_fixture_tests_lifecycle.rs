//! Live verification for the handshake, lifecycle, and binary-framing fixtures
//! under `test/fixtures/wire/`.
//!
//! Same contract as `wire_fixture_tests.rs`: the host must produce exactly
//! these frames and the clients must decode them. Two fixture shapes needed
//! special handling when they were pinned against a live actor:
//!
//! - `response.ok.mobile_hello.json` carries the live `device.lastSeenAt` the
//!   handshake stamps, so the fixture keeps a `"<lastSeenAt:0>"` placeholder
//!   that [`assert_fixture`] resolves against the actual value.
//! - `response.ok.mobile_hello.legacy.json` and
//!   `response.ok.host_shutdown.minimal.json` describe historical contract
//!   shapes the current host no longer emits verbatim (additive fields have
//!   been added since). They are verified as subsets of the live response via
//!   [`assert_wire_subset`] rather than byte-equal output.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use alera_core::runtime::{MobileDevice, MobileDevicePermission, WorkspaceTabRecord};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::timeout;

use crate::terminal_host::client::{
    connection_loop, ClientFrame, ClientHandle, CLIENT_TERMINAL_OUT_QUEUE_CAPACITY,
};
use crate::terminal_host::frame_codec::{encode_output_frame, encode_output_payload};
use crate::terminal_host::protocol::{TerminalHostConfig, PROTOCOL_VERSION};
use crate::terminal_host::session::Session;

use super::actor_test_harness::{local_client, mobile_client, test_actor};
use super::ServerActor;

const FRAME_TIMEOUT: Duration = Duration::from_secs(10);

fn wire_fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/fixtures/wire")
        .join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The stored record behind `request.unknown_fields.json`'s `tab.find`.
/// Identical to the tab fixture record in `wire_fixture_tests.rs` so the
/// projection it produces is the one `response.tab.find.desktop.json` pins.
fn stored_tab() -> WorkspaceTabRecord {
    let at: DateTime<Utc> = "2026-07-27T12:34:56.789Z".parse().unwrap();
    WorkspaceTabRecord {
        id: "terminal-1".to_string(),
        workspace_id: "workspace-1".to_string(),
        kind: "terminal".to_string(),
        title: "Flutter".to_string(),
        created_at: at,
        updated_at: at,
        payload: json!({
            "agentProfileLaunchV1": {
                "version": 1,
                "launch": {"kind": "command", "command": "fx"},
            },
            "initialPrompt": "durable bootstrap",
            "pendingAgentPrompt": {"agent": "fx", "prompt": "durable bootstrap"},
            "agentTitleStateV1": {
                "conversationId": "conv-1",
                "agent": null,
                "nativeId": null,
                "retired": [],
                "retiredPrompts": {},
                "initialPrompt": "durable bootstrap",
                "cursor": 3,
                "eligible": true,
                "attempted": true,
                "closed": false,
            },
            "agentTitleConversationId": "conv-1",
            "agentTitleRevision": "rev-1",
            "agentTitleStatus": "generating",
            "agentTitleSource": "agent",
            "terminalPulse": {"command": "r", "appendEnter": true, "delayMs": 2000},
            "shell": "zsh",
        }),
    }
}

/// The paired device `request.mobile_hello.json` authenticates as.
/// `token_hash` is `sha256("token-1")`, matching `mobile_access::sha256_hex`.
fn fixture_mobile_device() -> MobileDevice {
    let paired_at: DateTime<Utc> = "2000-01-01T00:00:01.000Z".parse().unwrap();
    MobileDevice {
        id: "device-1".to_string(),
        display_name: "Fixture Phone".to_string(),
        token_hash: hex::encode(Sha256::digest(b"token-1")),
        public_key_b64: None,
        permission: MobileDevicePermission::FullControl,
        paired_at,
        last_seen_at: Some(paired_at),
        revoked_at: None,
    }
}

async fn recv_frame(receiver: &mut UnboundedReceiver<ClientFrame>) -> ClientFrame {
    timeout(FRAME_TIMEOUT, receiver.recv())
        .await
        .expect("timed out waiting for a host frame")
        .expect("the host lane should stay open")
}

/// The ordering envelopes (`OrderedControl`, `SequencedTerminal`, `Budgeted`)
/// are internal to one connection; this peers through them at the payload the
/// writer would emit.
fn frame_payload(frame: &ClientFrame) -> &ClientFrame {
    let mut frame = frame;
    loop {
        frame = match frame {
            ClientFrame::OrderedControl { frame, .. }
            | ClientFrame::SequencedTerminal { frame, .. }
            | ClientFrame::Budgeted { frame, .. } => frame,
            other => return other,
        };
    }
}

/// Sends one request frame and returns the response, the JSON event frames it
/// pushed ahead of itself, and the non-JSON control markers (frame upgrade,
/// restart/shutdown ordering sentinels) in arrival order.
async fn exchange(
    actor: &mut ServerActor,
    client_id: u64,
    request: &Value,
    receiver: &mut UnboundedReceiver<ClientFrame>,
) -> (Value, Vec<Value>, Vec<ClientFrame>) {
    let request_id = request["id"].as_i64().expect("request id");
    actor.handle_line(client_id, request.to_string()).await;
    let mut events = Vec::new();
    let mut markers = Vec::new();
    loop {
        let frame = recv_frame(receiver).await;
        match frame_payload(&frame) {
            ClientFrame::Json(_) | ClientFrame::Output { .. } => {
                let value = frame.as_json().expect("JSON-bearing frame");
                if value["id"] == request_id {
                    return (value, events, markers);
                }
                events.push(value);
            }
            _ => markers.push(frame),
        }
    }
}

/// Asserts a full fixture match. A fixture string of the form `"<name:N>"` is
/// a dynamic slot: the actual value only has to be a non-null value of the
/// same JSON kind, and it is copied into the expectation before comparing.
fn assert_fixture(actual: &Value, expected: &Value) {
    assert_eq!(&resolve_placeholders(expected, actual), actual);
}

fn resolve_placeholders(expected: &Value, actual: &Value) -> Value {
    match expected {
        Value::String(slot) if slot.starts_with('<') && slot.ends_with('>') => {
            assert!(
                same_json_kind(expected, actual),
                "placeholder {slot} got {actual}"
            );
            actual.clone()
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        resolve_placeholders(value, actual.get(key).unwrap_or(&Value::Null)),
                    )
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    resolve_placeholders(value, actual.get(index).unwrap_or(&Value::Null))
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

fn same_json_kind(expected: &Value, actual: &Value) -> bool {
    matches!(
        (expected, actual),
        (Value::Null, Value::Null)
            | (Value::Bool(_), Value::Bool(_))
            | (Value::Number(_), Value::Number(_))
            | (Value::String(_), Value::String(_))
            | (Value::Array(_), Value::Array(_))
            | (Value::Object(_), Value::Object(_))
    )
}

/// For fixtures that pin a historical contract shape: every leaf the fixture
/// carries must still appear with that value in the live response, while the
/// live response is free to carry more (additive) fields and capabilities.
fn assert_wire_subset(expected: &Value, actual: &Value) {
    match (expected, actual) {
        (Value::Object(expected), Value::Object(actual)) => {
            for (key, value) in expected {
                let Some(actual_value) = actual.get(key) else {
                    panic!("live frame dropped fixture key {key}: {actual:?}");
                };
                assert_wire_subset(value, actual_value);
            }
        }
        (Value::Array(expected), Value::Array(actual)) => {
            for value in expected {
                assert!(
                    actual.contains(value),
                    "live frame no longer carries {value}: {actual:?}"
                );
            }
        }
        (expected, actual) => assert_eq!(expected, actual),
    }
}

#[tokio::test]
async fn binary_hello_upgrades_in_band_and_frames_everything_after() {
    let dir = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let client_stream = TcpStream::connect(listener.local_addr().unwrap())
        .await
        .unwrap();
    let (server_stream, _) = listener.accept().await.unwrap();
    let (control_tx, control_rx) = tokio::sync::mpsc::unbounded_channel();
    let (terminal_tx, terminal_rx) = tokio::sync::mpsc::channel(CLIENT_TERMINAL_OUT_QUEUE_CAPACITY);
    let handle = ClientHandle::new(control_tx, terminal_tx);
    let (inbox, _inbox_rx) = tokio::sync::mpsc::unbounded_channel();
    let writer = tokio::spawn(connection_loop(
        server_stream,
        1,
        inbox,
        control_rx,
        terminal_rx,
    ));
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle.clone()))]),
        HashMap::new(),
    )
    .await;
    let (read_half, _write_half) = client_stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();

    actor
        .handle_line(
            1,
            json!({
                "id": 0,
                "type": "hello",
                "payload": {
                    "protocolVersion": PROTOCOL_VERSION,
                    "token": "token",
                    "clientKind": "app",
                    "binaryFrames": true,
                },
            })
            .to_string(),
        )
        .await;

    // The upgrade marker is queued on the control lane ahead of the response,
    // so the wire starts with the sentinel line and the hello response itself
    // already arrives framed.
    timeout(FRAME_TIMEOUT, reader.read_line(&mut line))
        .await
        .expect("timed out waiting for the upgrade line")
        .expect("failed to read the upgrade line");
    let upgrade: Value = serde_json::from_str(line.trim_end()).unwrap();
    assert_fixture(&upgrade, &wire_fixture("event.binary_frames_enabled.json"));

    let mut header = [0u8; 5];
    timeout(FRAME_TIMEOUT, reader.read_exact(&mut header))
        .await
        .expect("timed out waiting for the framed hello response")
        .unwrap();
    assert_eq!(header[0], 1, "the hello response arrives as a JSON frame");
    let length = u32::from_be_bytes([header[1], header[2], header[3], header[4]]) as usize;
    let mut payload = vec![0u8; length];
    reader.read_exact(&mut payload).await.unwrap();
    let response: Value = serde_json::from_slice(&payload).unwrap();
    assert_fixture(&response, &wire_fixture("response.ok.hello.binary.json"));

    // Terminal output negotiated this way crosses as a kind-2 frame carrying
    // the raw PTY bytes; `terminal.output.binary.json` pins the exact bytes.
    let output_fixture = wire_fixture("terminal.output.binary.json");
    let data = STANDARD
        .decode(output_fixture["dataBase64"].as_str().unwrap())
        .unwrap();
    handle
        .try_send_terminal(ClientFrame::Output {
            session_id: output_fixture["sessionId"].as_str().unwrap().to_string(),
            data,
        })
        .unwrap();

    let expected_frame = STANDARD
        .decode(output_fixture["frameBase64"].as_str().unwrap())
        .unwrap();
    let mut framed = vec![0u8; expected_frame.len()];
    timeout(FRAME_TIMEOUT, reader.read_exact(&mut framed))
        .await
        .expect("timed out waiting for the binary output frame")
        .unwrap();
    assert_eq!(framed, expected_frame);

    // A shutdown on the negotiated connection answers framed, then queues the
    // disposal sentinel the writer turns into ServerCommand::RequestedShutdown.
    actor
        .handle_line(1, wire_fixture("request.host_shutdown.json").to_string())
        .await;
    let mut header = [0u8; 5];
    timeout(FRAME_TIMEOUT, reader.read_exact(&mut header))
        .await
        .expect("timed out waiting for the framed shutdown response")
        .unwrap();
    assert_eq!(header[0], 1, "a post-upgrade response stays a JSON frame");
    let length = u32::from_be_bytes([header[1], header[2], header[3], header[4]]) as usize;
    let mut payload = vec![0u8; length];
    reader.read_exact(&mut payload).await.unwrap();
    let response: Value = serde_json::from_slice(&payload).unwrap();
    assert_fixture(&response, &wire_fixture("response.ok.host_shutdown.json"));

    drop(reader);
    writer.abort();
}

#[tokio::test]
async fn terminal_output_binary_fixture_matches_the_frame_codec() {
    // The fixture pins all three views of one output chunk: the raw PTY bytes,
    // the session-prefixed payload the mobile WebSocket transport uses, and
    // the full length-prefixed frame the desktop socket uses.
    let fixture = wire_fixture("terminal.output.binary.json");
    let session_id = fixture["sessionId"].as_str().unwrap();
    let data = STANDARD
        .decode(fixture["dataBase64"].as_str().unwrap())
        .unwrap();

    assert_eq!(
        encode_output_payload(session_id, &data),
        STANDARD
            .decode(fixture["payloadBase64"].as_str().unwrap())
            .unwrap(),
    );
    assert_eq!(
        encode_output_frame(session_id, &data),
        STANDARD
            .decode(fixture["frameBase64"].as_str().unwrap())
            .unwrap(),
    );

    // A client that never negotiated frames gets the same chunk as the base64
    // JSON event instead.
    assert_eq!(
        ClientFrame::Output {
            session_id: session_id.to_string(),
            data,
        }
        .as_json()
        .unwrap(),
        json!({
            "event": "output",
            "payload": {
                "sessionId": session_id,
                "dataBase64": fixture["dataBase64"],
            },
        }),
    );
}

#[tokio::test]
async fn mobile_hello_matches_shared_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, mobile_client(handle, "phone"))]),
        HashMap::new(),
    )
    .await;
    actor
        .runtime_store
        .upsert_mobile_device(fixture_mobile_device())
        .await
        .unwrap();
    let request = wire_fixture("request.mobile_hello.json");

    let (response, events, markers) = exchange(&mut actor, 1, &request, &mut receiver).await;

    // The request asked for binaryFrames, so the in-band upgrade marker is
    // queued ahead of the events and response on the same lane.
    assert_eq!(markers.len(), 1);
    assert!(matches!(
        frame_payload(&markers[0]),
        ClientFrame::UpgradeToBinary
    ));
    assert_eq!(
        events,
        vec![json!({"event": "mobileDevicesChanged", "payload": {}})]
    );
    assert_fixture(&response, &wire_fixture("response.ok.mobile_hello.json"));
}

#[tokio::test]
async fn mobile_hello_legacy_fixture_remains_a_subset_of_the_live_response() {
    // The fixture describes the handshake shape an early mobile protocol
    // client received: today's host answers a strict superset (more
    // capabilities, additive device fields), which is what keeps an old phone
    // decodable. It is not byte-equal current output.
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, mobile_client(handle, "phone"))]),
        HashMap::new(),
    )
    .await;
    actor
        .runtime_store
        .upsert_mobile_device(fixture_mobile_device())
        .await
        .unwrap();

    let (response, _, _) = exchange(
        &mut actor,
        1,
        &wire_fixture("request.mobile_hello.json"),
        &mut receiver,
    )
    .await;

    assert_wire_subset(
        &wire_fixture("response.ok.mobile_hello.legacy.json"),
        &response,
    );
}

#[tokio::test]
async fn mobile_hello_version_mismatch_matches_shared_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, mobile_client(handle, "phone"))]),
        HashMap::new(),
    )
    .await;

    let (response, events, markers) = exchange(
        &mut actor,
        1,
        &json!({
            "id": 23,
            "type": "mobile.hello",
            "payload": {
                "protocolVersion": 0,
                "deviceId": "device-1",
                "deviceToken": "token-1",
            },
        }),
        &mut receiver,
    )
    .await;

    assert!(events.is_empty());
    assert!(markers.is_empty());
    assert_fixture(
        &response,
        &wire_fixture("response.error.mobile_version_mismatch.json"),
    );
}

#[tokio::test]
async fn host_shutdown_matches_shared_fixture_then_queues_disposal() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let request = wire_fixture("request.host_shutdown.json");

    let (response, events, markers) = exchange(&mut actor, 1, &request, &mut receiver).await;

    assert!(events.is_empty());
    assert!(markers.is_empty());
    assert_fixture(&response, &wire_fixture("response.ok.host_shutdown.json"));
    // The disposal sentinel is written after the response on the same lane so
    // the caller sees the acknowledgement before the connection drops.
    let marker = recv_frame(&mut receiver).await;
    assert!(matches!(
        frame_payload(&marker),
        ClientFrame::ShutdownRuntimeAfterWrite { .. }
    ));
}

#[tokio::test]
async fn forced_host_shutdown_counts_the_running_session() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::from([("s1".to_string(), Session::driver_test_stub("s1", 80, 24))]),
    )
    .await;
    let request = wire_fixture("request.host_shutdown.force.json");

    let (response, events, markers) = exchange(&mut actor, 1, &request, &mut receiver).await;

    assert!(events.is_empty());
    assert!(markers.is_empty());
    assert_fixture(
        &response,
        &wire_fixture("response.ok.host_shutdown.force.json"),
    );
    let marker = recv_frame(&mut receiver).await;
    assert!(matches!(
        frame_payload(&marker),
        ClientFrame::ShutdownRuntimeAfterWrite { .. }
    ));
}

#[tokio::test]
async fn host_shutdown_minimal_fixture_is_a_subset_of_the_live_response() {
    // The fixture keeps the two-field shutdown acknowledgement an early
    // lifecycle client decoded: today's host adds the active-* counters. It
    // remains a valid (strict subset) response rather than current output.
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let request = wire_fixture("request.host_shutdown.force.json");

    let (response, _, _) = exchange(&mut actor, 1, &request, &mut receiver).await;

    assert_wire_subset(
        &wire_fixture("response.ok.host_shutdown.minimal.json"),
        &response,
    );
}

#[tokio::test]
async fn host_restart_matches_shared_fixture_then_queues_restart() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let request = wire_fixture("request.host_restart.json");

    let (response, events, markers) = exchange(&mut actor, 1, &request, &mut receiver).await;

    assert!(events.is_empty());
    assert!(markers.is_empty());
    assert_fixture(&response, &wire_fixture("response.ok.host_restart.json"));
    let marker = recv_frame(&mut receiver).await;
    assert!(matches!(
        frame_payload(&marker),
        ClientFrame::RestartRuntimeAfterWrite { .. }
    ));
}

#[tokio::test]
async fn mobile_shutdown_denial_matches_shared_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, mobile_client(handle, "phone"))]),
        HashMap::new(),
    )
    .await;

    let (response, events, markers) = exchange(
        &mut actor,
        1,
        &wire_fixture("request.host_shutdown.json"),
        &mut receiver,
    )
    .await;

    assert!(events.is_empty());
    assert!(markers.is_empty());
    assert_fixture(
        &response,
        &wire_fixture("response.error.mobile_shutdown_denied.json"),
    );
}

#[tokio::test]
async fn empty_ok_response_matches_shared_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;

    let (response, events, markers) = exchange(
        &mut actor,
        1,
        &json!({
            "id": 0,
            "type": "configure",
            "payload": TerminalHostConfig::default().to_json(),
        }),
        &mut receiver,
    )
    .await;

    assert!(events.is_empty());
    assert!(markers.is_empty());
    assert_fixture(&response, &wire_fixture("response.ok.empty.json"));
}

#[tokio::test]
async fn unknown_request_fields_are_ignored_end_to_end() {
    // Forward compatibility: a newer client may send fields this host build
    // does not know, both at the envelope and the payload level, and the
    // request must still parse and answer normally.
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    actor
        .runtime_store
        .upsert_workspace_tab(stored_tab())
        .await
        .unwrap();
    let request = wire_fixture("request.unknown_fields.json");

    let (response, events, markers) = exchange(&mut actor, 1, &request, &mut receiver).await;

    assert!(events.is_empty());
    assert!(markers.is_empty());
    assert_eq!(response["id"], 25);
    assert_eq!(response["ok"], true);
    // The projection is the pinned desktop shape; only the echoed id differs.
    assert_eq!(
        response["payload"],
        wire_fixture("response.tab.find.desktop.json")["payload"],
    );
}
