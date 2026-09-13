//! Live verification of binary output after a resync request.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

use crate::terminal_host::client::{
    connection_loop, ClientHandle, CLIENT_TERMINAL_OUT_QUEUE_CAPACITY,
};
use crate::terminal_host::protocol::{TerminalHostConfig, PROTOCOL_VERSION};
use crate::terminal_host::session::Session;

use super::super::actor_test_harness::{local_client, test_actor};
use super::super::wire_fixture_tests_lifecycle::wire_fixture;
use super::super::wire_fixture_tests_lifecycle::{assert_fixture, FRAME_TIMEOUT};

async fn read_binary_frame(
    reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>,
) -> (u8, Vec<u8>) {
    let mut header = [0u8; 5];
    timeout(FRAME_TIMEOUT, reader.read_exact(&mut header))
        .await
        .expect("timed out waiting for a binary frame")
        .unwrap();
    let length = u32::from_be_bytes([header[1], header[2], header[3], header[4]]) as usize;
    let mut payload = vec![0u8; length];
    timeout(FRAME_TIMEOUT, reader.read_exact(&mut payload))
        .await
        .expect("timed out waiting for a binary frame payload")
        .unwrap();
    (header[0], payload)
}

fn encode_binary_frame(kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut frame = vec![kind];
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(payload);
    frame
}

#[tokio::test]
async fn observe_binary_resync_and_terminal_control_order() {
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
    let mut session = Session::driver_test_stub("s1", 80, 24);
    session.attach(1);
    session.set_max_bytes(4);
    session.append_output(b"abcdefgh");
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle.clone()))]),
        HashMap::from([("s1".to_string(), session)]),
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
    timeout(FRAME_TIMEOUT, reader.read_line(&mut line))
        .await
        .expect("timed out waiting for the binary upgrade marker")
        .unwrap();
    let upgrade: Value = serde_json::from_str(line.trim_end()).unwrap();
    assert_fixture(&upgrade, &wire_fixture("event.binary_frames_enabled.json"));
    let (hello_kind, hello_payload) = read_binary_frame(&mut reader).await;
    assert_eq!(hello_kind, 1);
    let hello: Value = serde_json::from_slice(&hello_payload).unwrap();
    assert_fixture(&hello, &wire_fixture("response.ok.hello.binary.json"));

    actor
        .sessions
        .get_mut("s1")
        .unwrap()
        .mark_output_backpressured(1);
    actor.handle_output_resync_tick("s1".to_string(), 1);
    let (resync_kind, resync_payload) = read_binary_frame(&mut reader).await;
    assert_eq!(resync_kind, 1);
    assert_fixture(
        &serde_json::from_slice::<Value>(&resync_payload).unwrap(),
        &wire_fixture("event.output_resync_required.json"),
    );
    let resync_frame_fixture = wire_fixture("frame.output_resync_required.binary.json");
    assert_eq!(
        encode_binary_frame(resync_kind, &resync_payload),
        STANDARD
            .decode(resync_frame_fixture["frameBase64"].as_str().unwrap())
            .unwrap()
    );

    let mut resume_request = wire_fixture("request.set_output_paused.resume.json");
    resume_request["id"] = json!(2);
    actor.handle_line(1, resume_request.to_string()).await;
    let (resume_kind, resume_payload) = read_binary_frame(&mut reader).await;
    let resume: Value = serde_json::from_slice(&resume_payload).unwrap();
    assert_eq!(resume_kind, 1);
    assert_fixture(
        &resume,
        &wire_fixture("response.ok.output_resumed_snapshot.binary.json"),
    );
    let resume_frame_fixture = wire_fixture("frame.output_resumed_snapshot.binary.json");
    assert_eq!(
        encode_binary_frame(resume_kind, &resume_payload),
        STANDARD
            .decode(resume_frame_fixture["frameBase64"].as_str().unwrap())
            .unwrap()
    );

    let output_fixture = wire_fixture("terminal.output.binary.resync.json");
    let data = STANDARD
        .decode(output_fixture["dataBase64"].as_str().unwrap())
        .unwrap();
    actor.sessions.get_mut("s1").unwrap().append_output(&data);
    actor.flush_output_batch("s1");
    actor
        .handle_line(
            1,
            json!({
                "id": 3,
                "type": "configure",
                "payload": TerminalHostConfig::default().to_json(),
            })
            .to_string(),
        )
        .await;
    let (output_kind, output_payload) = read_binary_frame(&mut reader).await;
    assert_eq!(output_kind, 2);
    assert_eq!(
        output_payload,
        STANDARD
            .decode(output_fixture["payloadBase64"].as_str().unwrap())
            .unwrap()
    );
    let (control_kind, control_payload) = read_binary_frame(&mut reader).await;
    let control: Value = serde_json::from_slice(&control_payload).unwrap();
    assert_eq!(control_kind, 1);
    assert_fixture(
        &control,
        &wire_fixture("response.ok.empty.binary_interleaved.json"),
    );
    let control_frame_fixture = wire_fixture("frame.empty.binary_interleaved.json");
    assert_eq!(
        encode_binary_frame(control_kind, &control_payload),
        STANDARD
            .decode(control_frame_fixture["frameBase64"].as_str().unwrap())
            .unwrap()
    );
    let output_frame = encode_binary_frame(output_kind, &output_payload);
    assert_eq!(
        output_frame,
        STANDARD
            .decode(output_fixture["frameBase64"].as_str().unwrap())
            .unwrap()
    );

    drop(reader);
    writer.abort();
}
