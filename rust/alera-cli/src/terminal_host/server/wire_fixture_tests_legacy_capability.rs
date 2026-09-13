//! Live verification of the text transport retained for old clients.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

use crate::terminal_host::client::{
    connection_loop, ClientHandle, CLIENT_TERMINAL_OUT_QUEUE_CAPACITY,
};
use crate::terminal_host::protocol::PROTOCOL_VERSION;

use super::super::actor_test_harness::{local_client, test_actor};
use super::super::wire_fixture_tests_lifecycle::{
    assert_fixture, assert_wire_subset, wire_fixture, FRAME_TIMEOUT,
};

#[tokio::test]
async fn legacy_hello_keeps_output_as_a_base64_json_event() {
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
    let mut session = crate::terminal_host::session::Session::driver_test_stub("s1", 80, 24);
    session.attach(1);
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
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
                    "clientKind": "app"
                },
            })
            .to_string(),
        )
        .await;
    timeout(FRAME_TIMEOUT, reader.read_line(&mut line))
        .await
        .expect("timed out waiting for the legacy hello response")
        .unwrap();
    let hello: Value = serde_json::from_str(line.trim_end()).unwrap();
    assert_fixture(&hello, &wire_fixture("response.ok.hello.json"));

    let output_fixture = wire_fixture("event.output.legacy.json");
    let data = STANDARD
        .decode(output_fixture["payload"]["dataBase64"].as_str().unwrap())
        .unwrap();
    actor.sessions.get_mut("s1").unwrap().append_output(&data);
    actor.flush_output_batch("s1");

    line.clear();
    timeout(FRAME_TIMEOUT, reader.read_line(&mut line))
        .await
        .expect("timed out waiting for the legacy output event")
        .unwrap();
    let output: Value = serde_json::from_str(line.trim_end()).unwrap();
    assert_wire_subset(&output_fixture, &output);

    drop(reader);
    writer.abort();
}
