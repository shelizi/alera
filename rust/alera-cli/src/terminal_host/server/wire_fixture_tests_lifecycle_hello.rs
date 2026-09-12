//! Live verification for the handshake and binary-framing fixtures under
//! `test/fixtures/wire/`. Split from `wire_fixture_tests_lifecycle.rs` to keep
//! each module under the line budget; the shared helpers live there.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

use crate::terminal_host::client::{
    connection_loop, ClientFrame, ClientHandle, CLIENT_TERMINAL_OUT_QUEUE_CAPACITY,
};
use crate::terminal_host::frame_codec::{encode_output_frame, encode_output_payload};
use crate::terminal_host::protocol::PROTOCOL_VERSION;

use super::actor_test_harness::{local_client, mobile_client, test_actor};
use super::wire_fixture_tests_lifecycle::{
    assert_fixture, assert_wire_subset, exchange, fixture_mobile_device, frame_payload,
    wire_fixture, FRAME_TIMEOUT,
};

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
