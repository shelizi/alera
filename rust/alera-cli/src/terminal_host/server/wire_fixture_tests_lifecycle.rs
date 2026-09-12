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
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::timeout;

use crate::terminal_host::client::{ClientFrame, ClientHandle};
use crate::terminal_host::protocol::TerminalHostConfig;
use crate::terminal_host::session::Session;

use super::actor_test_harness::{local_client, mobile_client, test_actor};
use super::ServerActor;

pub(super) const FRAME_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) fn wire_fixture(name: &str) -> Value {
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
pub(super) fn fixture_mobile_device() -> MobileDevice {
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
pub(super) fn frame_payload(frame: &ClientFrame) -> &ClientFrame {
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
pub(super) async fn exchange(
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
pub(super) fn assert_fixture(actual: &Value, expected: &Value) {
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
pub(super) fn assert_wire_subset(expected: &Value, actual: &Value) {
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
