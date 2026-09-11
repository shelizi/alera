use std::collections::HashMap;
use std::time::Duration;

use serde_json::json;

use super::actor_test_harness::{local_client, test_actor};
use crate::terminal_host::client::ClientHandle;

#[tokio::test]
async fn workspace_sidebar_snapshot_is_deferred_so_control_requests_can_advance() {
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
        .try_start_deferred_request(1, 11, "workspaceSidebar.snapshot", &json!({}))
        .await
        .unwrap();
    assert!(
        deferred,
        "sidebar snapshot must leave the actor loop before its reads finish"
    );

    actor
        .handle_line(
            1,
            json!({"id": 12, "type": "status.get", "payload": {}}).to_string(),
        )
        .await;
    let status = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("status response should not wait for the snapshot")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(status["id"], 12);
    assert_eq!(status["ok"], true);

    let completion = tokio::time::timeout(Duration::from_secs(1), commands.recv())
        .await
        .expect("snapshot worker should report completion")
        .unwrap();
    actor.handle(completion).await;
    let snapshot = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("snapshot completion should answer the original request")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(snapshot["id"], 11);
    assert_eq!(snapshot["ok"], true);
}

#[tokio::test]
async fn concurrent_workspace_sidebar_snapshots_share_one_background_load() {
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

    for request_id in [31, 32] {
        assert!(actor
            .try_start_deferred_request(1, request_id, "workspaceSidebar.snapshot", &json!({}),)
            .await
            .unwrap());
    }

    let completion = tokio::time::timeout(Duration::from_secs(1), commands.recv())
        .await
        .expect("the shared snapshot worker should report completion")
        .unwrap();
    actor.handle(completion).await;

    let mut response_ids = Vec::new();
    for _ in 0..2 {
        let response = tokio::time::timeout(Duration::from_secs(1), responses.recv())
            .await
            .expect("each coalesced request must receive a response")
            .unwrap()
            .as_json()
            .unwrap();
        assert_eq!(response["ok"], true);
        response_ids.push(response["id"].as_i64().unwrap());
    }
    response_ids.sort_unstable();
    assert_eq!(response_ids, vec![31, 32]);

    assert!(
        tokio::time::timeout(Duration::from_millis(100), commands.recv())
            .await
            .is_err(),
        "coalesced sidebar requests should not start a second background load"
    );

    assert!(actor
        .try_start_deferred_request(1, 33, "workspaceSidebar.snapshot", &json!({}))
        .await
        .unwrap());
    let next_completion = tokio::time::timeout(Duration::from_secs(1), commands.recv())
        .await
        .expect("a later refresh should start a fresh snapshot load")
        .unwrap();
    actor.handle(next_completion).await;
    let next_response = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("the later refresh should receive a response")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(next_response["id"], 33);
    assert_eq!(next_response["ok"], true);
}

#[tokio::test]
async fn workspace_sidebar_snapshot_completion_is_dropped_after_client_disconnects() {
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
        .try_start_deferred_request(1, 21, "workspaceSidebar.snapshot", &json!({}))
        .await
        .unwrap();
    assert!(deferred);
    let completion = tokio::time::timeout(Duration::from_secs(1), commands.recv())
        .await
        .expect("snapshot worker should report completion")
        .unwrap();

    actor.clients.remove(&1);
    actor.handle(completion).await;

    assert!(responses.try_recv().is_err());
}
