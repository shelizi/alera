use std::collections::HashMap;
use std::time::Duration;

use serde_json::json;

use super::actor_test_harness::{local_client, test_actor};
use crate::terminal_host::client::ClientHandle;

#[tokio::test]
async fn host_directory_list_is_deferred_so_control_requests_can_advance() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("item.txt"), "item").unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let (inbox, mut commands) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;

    let deferred = actor
        .try_start_deferred_request(
            1,
            31,
            "hostDirectory.list",
            &json!({"path": dir.path().to_string_lossy()}),
        )
        .await
        .unwrap();
    assert!(deferred);

    actor
        .handle_line(
            1,
            json!({"id": 32, "type": "status.get", "payload": {}}).to_string(),
        )
        .await;
    let status = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("status response should not wait for directory listing")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(status["id"], 32);
    assert_eq!(status["ok"], true);

    let completion = tokio::time::timeout(Duration::from_secs(1), commands.recv())
        .await
        .expect("directory worker should report completion")
        .unwrap();
    actor.handle(completion).await;
    let listing = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("directory completion should answer the original request")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(listing["id"], 31);
    assert_eq!(listing["ok"], true);
}

#[tokio::test]
async fn deferred_blocking_completion_is_dropped_after_client_disconnects() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let (inbox, mut commands) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;

    let deferred = actor
        .try_start_deferred_request(
            1,
            41,
            "hostDirectory.list",
            &json!({"path": dir.path().to_string_lossy()}),
        )
        .await
        .unwrap();
    assert!(deferred);
    let completion = tokio::time::timeout(Duration::from_secs(1), commands.recv())
        .await
        .expect("directory worker should report completion")
        .unwrap();

    actor.clients.remove(&1);
    actor.handle(completion).await;

    assert!(responses.try_recv().is_err());
}
