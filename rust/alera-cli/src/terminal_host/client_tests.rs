use super::*;
use serde_json::json;
use tokio::io::AsyncBufReadExt as _;
use tokio::net::TcpListener;

#[test]
fn clean_socket_eof_and_read_failure_have_distinct_disconnect_causes() {
    assert_eq!(
        reason_for_read_termination(&Ok(None)),
        Some(DisconnectReason::PeerClosed)
    );
    assert_eq!(
        reason_for_read_termination(&Err(std::io::Error::other("no contents in logs"))),
        Some(DisconnectReason::TransportReadFailed)
    );
    assert_eq!(reason_for_read_termination(&Ok(Some(String::new()))), None);
}

#[tokio::test]
async fn control_response_stays_between_its_causal_terminal_frames() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let address = listener.local_addr().unwrap();
    let client = TcpStream::connect(address).await.unwrap();
    let (server, _) = listener.accept().await.unwrap();
    let (inbox, _inbox_rx) = tokio::sync::mpsc::unbounded_channel();
    let (control_tx, control_rx) = tokio::sync::mpsc::unbounded_channel();
    let (terminal_tx, terminal_rx) = tokio::sync::mpsc::channel(CLIENT_TERMINAL_OUT_QUEUE_CAPACITY);
    let handle = ClientHandle::new(control_tx, terminal_tx);

    handle
        .try_send_terminal(json!({"event": "output-a"}).into())
        .unwrap();
    handle
        .send_control(json!({"response": "attachment"}).into())
        .unwrap();
    handle
        .try_send_terminal(json!({"event": "output-b"}).into())
        .unwrap();
    let task = tokio::spawn(connection_loop(
        server,
        1,
        inbox,
        control_rx,
        terminal_rx,
        Arc::new(LocalClientIngressAdmission::new(
            LOCAL_CLIENT_PENDING_COMMAND_LIMIT,
            MAX_LOCAL_PENDING_REQUEST_BYTES,
        )),
    ));
    let mut lines = BufReader::new(client).lines();

    let first: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    let second: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    let third: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    assert_eq!(first["event"], json!("output-a"));
    assert_eq!(second["response"], json!("attachment"));
    assert_eq!(third["event"], json!("output-b"));

    drop(handle);
    task.await.unwrap();
}

#[tokio::test]
async fn multi_client_local_ingress_is_bounded_without_blocking_control_or_shutdown() {
    use tokio::time::{timeout, Duration};
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let address = listener.local_addr().unwrap();
    let admission = Arc::new(LocalClientIngressAdmission::new(
        2,
        MAX_LOCAL_PENDING_REQUEST_BYTES,
    ));
    let (inbox, mut inbox_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut clients = Vec::new();
    let mut controls = Vec::new();
    let mut terminals = Vec::new();
    let mut tasks = Vec::new();
    for id in 1..=2 {
        let client = TcpStream::connect(address).await.unwrap();
        let (server, _) = listener.accept().await.unwrap();
        let (control_tx, control_rx) = tokio::sync::mpsc::unbounded_channel();
        let (terminal_tx, terminal_rx) =
            tokio::sync::mpsc::channel(CLIENT_TERMINAL_OUT_QUEUE_CAPACITY);
        let task = tokio::spawn(connection_loop(
            server,
            id,
            inbox.clone(),
            control_rx,
            terminal_rx,
            admission.clone(),
        ));
        clients.push(client);
        controls.push(control_tx);
        terminals.push(terminal_tx);
        tasks.push(task);
    }

    clients[0].write_all(b"one\ntwo\nthree\n").await.unwrap();
    clients[1].write_all(b"four\n").await.unwrap();
    let first = timeout(Duration::from_secs(2), inbox_rx.recv())
        .await
        .unwrap()
        .unwrap();
    let second = timeout(Duration::from_secs(2), inbox_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(&first, ServerCommand::AdmittedClientLine { .. }));
    assert!(matches!(&second, ServerCommand::AdmittedClientLine { .. }));
    assert_eq!(admission.commands.available_permits(), 0);
    tokio::task::yield_now().await;
    assert!(
        inbox_rx.try_recv().is_err(),
        "inbound queue exceeded the shared 2-slot cap"
    );

    // An inbox command from another producer is never blocked by this
    // local request budget; this must also hold when all slots are full.
    inbox.send(ServerCommand::RequestedShutdown).unwrap();
    assert!(matches!(
        timeout(Duration::from_secs(2), inbox_rx.recv())
            .await
            .unwrap(),
        Some(ServerCommand::RequestedShutdown),
    ));

    // Even while waiting for a slot the socket must keep writing queued
    // control responses rather than deadlocking the client transport.
    controls[0]
        .send(json!({"response":"still-alive"}).into())
        .unwrap();
    let mut reader = BufReader::new(clients.remove(0)).lines();
    let response = timeout(Duration::from_secs(2), reader.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(response.contains("still-alive"));

    drop(first);
    let third = timeout(Duration::from_secs(2), inbox_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(third, ServerCommand::AdmittedClientLine { .. }));
    drop(second);
    for task in tasks {
        task.abort();
    }
}

#[test]
fn request_byte_units_charge_utf8_and_round_up_with_a_nonzero_floor() {
    assert_eq!(local_request_byte_permits("".len()), 1);
    assert_eq!(local_request_byte_permits("a".len()), 1);
    assert_eq!(local_request_byte_permits("a".repeat(4096).len()), 1);
    assert_eq!(local_request_byte_permits("a".repeat(4097).len()), 2);
    assert_eq!(local_request_byte_permits("😀".len()), 1);
    assert_eq!(local_request_byte_permits("😀".repeat(1025).len()), 2);
    assert_eq!(
        local_request_byte_permits(client_line_reader::MAX_LOCAL_REQUEST_LINE_BYTES),
        (client_line_reader::MAX_LOCAL_REQUEST_LINE_BYTES / LOCAL_REQUEST_BYTE_UNIT_BYTES) as u32,
    );
}

#[tokio::test]
async fn local_multi_client_byte_budget_blocks_second_request_without_blocking_controls() {
    use tokio::time::{timeout, Duration};
    let admission = Arc::new(LocalClientIngressAdmission::new(3, 4096));
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (inbox, mut inbox_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut clients = Vec::new();
    let mut tasks = Vec::new();
    let mut controls = Vec::new();
    let mut terminals = Vec::new();
    for id in 1..=2 {
        let client = TcpStream::connect(addr).await.unwrap();
        let (server, _) = listener.accept().await.unwrap();
        let (control_tx, control_rx) = tokio::sync::mpsc::unbounded_channel();
        let (terminal_tx, terminal_rx) =
            tokio::sync::mpsc::channel(CLIENT_TERMINAL_OUT_QUEUE_CAPACITY);
        tasks.push(tokio::spawn(connection_loop(
            server,
            id,
            inbox.clone(),
            control_rx,
            terminal_rx,
            admission.clone(),
        )));
        controls.push(control_tx);
        terminals.push(terminal_tx);
        clients.push(client);
    }
    clients[0].write_all(b"first\n").await.unwrap();
    let first = timeout(Duration::from_secs(2), inbox_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(first, ServerCommand::AdmittedClientLine { .. }));
    clients[1].write_all(b"second\n").await.unwrap();
    assert_eq!(admission.commands.available_permits(), 2);
    assert_eq!(admission.bytes.available_permits(), 0);
    tokio::task::yield_now().await;
    assert!(inbox_rx.try_recv().is_err());
    inbox.send(ServerCommand::RequestedShutdown).unwrap();
    assert!(matches!(
        inbox_rx.recv().await,
        Some(ServerCommand::RequestedShutdown)
    ));
    controls[1]
        .send(json!({"response":"alive"}).into())
        .unwrap();
    let mut reader = BufReader::new(clients.remove(1)).lines();
    let reply = timeout(Duration::from_secs(2), reader.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(reply.contains("alive"));
    drop(first);
    let second = timeout(Duration::from_secs(2), inbox_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(second, ServerCommand::AdmittedClientLine { .. }));
    drop(second);
    for task in tasks {
        task.abort();
    }
}
