use std::collections::HashMap;
use std::time::Duration;

use serde_json::json;

use super::actor_test_harness::{local_client, test_actor};
use crate::terminal_host::client::ClientHandle;

#[tokio::test]
async fn deferred_requests_bound_background_concurrency() {
    const EXPECTED_LIMIT: usize = super::DEFERRED_REQUEST_CONCURRENCY;
    let dir = tempfile::tempdir().unwrap();
    let actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    let gate = std::sync::Arc::new(tokio::sync::Semaphore::new(0));
    let (started_tx, mut started_rx) = tokio::sync::mpsc::unbounded_channel();
    let (finished_tx, mut finished_rx) = tokio::sync::mpsc::unbounded_channel();

    for request_id in 0..=EXPECTED_LIMIT as i64 {
        let gate = gate.clone();
        let started_tx = started_tx.clone();
        let finished_tx = finished_tx.clone();
        actor.start_deferred_request(1, request_id, async move {
            started_tx.send(request_id).unwrap();
            let permit = gate.acquire().await.unwrap();
            permit.forget();
            finished_tx.send(request_id).unwrap();
            Ok(json!({}))
        });
    }
    drop(started_tx);
    drop(finished_tx);

    for _ in 0..EXPECTED_LIMIT {
        tokio::time::timeout(Duration::from_secs(1), started_rx.recv())
            .await
            .expect("the admitted requests should start")
            .expect("started channel should remain open");
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(100), started_rx.recv())
            .await
            .is_err(),
        "a request beyond the background concurrency budget started early"
    );

    gate.add_permits(EXPECTED_LIMIT);
    tokio::time::timeout(Duration::from_secs(1), started_rx.recv())
        .await
        .expect("the queued request should start after capacity is released")
        .expect("started channel should remain open");
    gate.add_permits(1);

    for _ in 0..=EXPECTED_LIMIT {
        tokio::time::timeout(Duration::from_secs(1), finished_rx.recv())
            .await
            .expect("all deferred requests should finish")
            .expect("finished channel should remain open");
    }
}

#[tokio::test]
async fn prompt_file_uploads_respect_the_deferred_io_budget() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    actor.deferred_request_slots = std::sync::Arc::new(tokio::sync::Semaphore::new(0));
    let (inbox, mut commands) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;

    assert!(actor
        .try_start_deferred_request(
            1,
            9,
            "mobile.promptFile.start",
            &json!({"name": "budget.txt", "sizeBytes": 1}),
        )
        .await
        .unwrap());
    assert!(
        tokio::time::timeout(Duration::from_millis(100), commands.recv())
            .await
            .is_err(),
        "prompt file I/O started without deferred budget capacity"
    );

    actor.deferred_request_slots.add_permits(1);
    let completion = tokio::time::timeout(Duration::from_secs(1), commands.recv())
        .await
        .expect("prompt file worker should resume after budget capacity is released")
        .unwrap();
    actor.handle(completion).await;
    let response = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("prompt file start should answer after admission")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(response["id"], 9);
    assert_eq!(response["ok"], true);
}

#[tokio::test]
async fn acknowledged_prompt_image_upload_is_cancelled_on_disconnect() {
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

    assert!(actor
        .try_start_deferred_request(
            1,
            10,
            "mobile.promptImage.start",
            &json!({"format": "png", "sizeBytes": 16}),
        )
        .await
        .unwrap());
    let completion = tokio::time::timeout(Duration::from_secs(1), commands.recv())
        .await
        .expect("prompt image start should report completion")
        .unwrap();
    actor.handle(completion).await;
    let response = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("prompt image start should answer the request")
        .unwrap()
        .as_json()
        .unwrap();
    let upload_id = response["payload"]["uploadId"]
        .as_str()
        .unwrap()
        .to_string();

    actor.deferred_request_slots = std::sync::Arc::new(tokio::sync::Semaphore::new(0));
    actor.dispose_client(1).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let store = super::prompt_image_store::PromptImageStore::in_runtime_dir(&actor.runtime_dir);
    assert_eq!(
        store
            .append_chunk(&upload_id, 0, b"\x89PNG\r\n\x1a\n")
            .expect("disconnect cleanup should wait for shared I/O admission"),
        8
    );

    actor.deferred_request_slots.add_permits(1);
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if matches!(
                store.append_chunk(&upload_id, 8, b"x"),
                Err(super::prompt_image_store::PromptImageStoreError::Missing)
            ) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("acknowledged prompt image reservation should be cancelled after disconnect");
}

#[tokio::test]
async fn orphaned_prompt_image_start_is_cancelled_after_disconnect() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, _responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let (inbox, mut commands) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;

    assert!(actor
        .try_start_deferred_request(
            1,
            15,
            "mobile.promptImage.start",
            &json!({"format": "png", "sizeBytes": 16}),
        )
        .await
        .unwrap());
    let completion = tokio::time::timeout(Duration::from_secs(1), commands.recv())
        .await
        .expect("prompt image start should report completion")
        .unwrap();
    let upload_id = match &completion {
        super::ServerCommand::MobilePromptImageFinished {
            result: Ok(value), ..
        } => value["uploadId"].as_str().unwrap().to_string(),
        _ => panic!("unexpected prompt image completion"),
    };

    actor.dispose_client(1).await;
    actor.handle(completion).await;
    let store = super::prompt_image_store::PromptImageStore::in_runtime_dir(&actor.runtime_dir);
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if matches!(
                store.append_chunk(&upload_id, 0, b"\x89PNG\r\n\x1a\n"),
                Err(super::prompt_image_store::PromptImageStoreError::Missing)
            ) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("orphaned prompt image start should be cleaned after disconnect");
}

#[tokio::test]
async fn prompt_image_upload_io_is_deferred_from_the_actor_mailbox() {
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

    assert!(actor
        .try_start_deferred_request(
            1,
            11,
            "mobile.promptImage.start",
            &json!({"format": "png", "sizeBytes": 8}),
        )
        .await
        .unwrap());

    actor
        .handle_line(
            1,
            json!({"id": 12, "type": "status.get", "payload": {}}).to_string(),
        )
        .await;
    let status = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("status should not wait for prompt image filesystem work")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(status["id"], 12);

    let start_completion = tokio::time::timeout(Duration::from_secs(1), commands.recv())
        .await
        .expect("prompt image start should report completion")
        .unwrap();
    actor.handle(start_completion).await;
    let start = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("prompt image start should answer the request")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(start["id"], 11);
    assert_eq!(start["ok"], true);
    let upload_id = start["payload"]["uploadId"].as_str().unwrap().to_string();

    assert!(actor
        .try_start_deferred_request(
            1,
            13,
            "mobile.promptImage.chunk",
            &json!({
                "uploadId": upload_id,
                "offset": 0,
                "dataBase64": "iVBORw0KGgo=",
            }),
        )
        .await
        .unwrap());
    let chunk_completion = tokio::time::timeout(Duration::from_secs(1), commands.recv())
        .await
        .expect("prompt image chunk should report completion")
        .unwrap();
    actor.handle(chunk_completion).await;
    let chunk = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("prompt image chunk should answer the request")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(chunk["id"], 13);
    assert_eq!(chunk["ok"], true);
    assert_eq!(chunk["payload"]["nextOffset"], 8);

    assert!(actor
        .try_start_deferred_request(
            1,
            14,
            "mobile.promptImage.complete",
            &json!({"uploadId": upload_id}),
        )
        .await
        .unwrap());
    let complete_completion = tokio::time::timeout(Duration::from_secs(1), commands.recv())
        .await
        .expect("prompt image completion should report completion")
        .unwrap();
    actor.handle(complete_completion).await;
    let complete = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("prompt image completion should answer the request")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(complete["id"], 14);
    assert_eq!(complete["ok"], true);
    assert!(std::path::Path::new(complete["payload"]["path"].as_str().unwrap()).is_file());
}

#[tokio::test]
async fn project_registration_preparation_is_deferred_before_runtime_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let project_path = dir.path().join("registered-project");
    std::fs::create_dir_all(&project_path).unwrap();
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
            21,
            "project.register",
            &json!({"path": project_path.to_string_lossy(), "name": "Deferred Project"}),
        )
        .await
        .unwrap();
    assert!(
        deferred,
        "project registration preparation should leave the actor mailbox"
    );
    assert!(
        actor
            .runtime_store
            .list_projects()
            .await
            .unwrap()
            .is_empty(),
        "background preparation must not mutate the runtime store"
    );

    actor
        .handle_line(
            1,
            json!({"id": 22, "type": "status.get", "payload": {}}).to_string(),
        )
        .await;
    let status = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("status response should not wait for project path preparation")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(status["id"], 22);
    assert_eq!(status["ok"], true);

    let completion = tokio::time::timeout(Duration::from_secs(1), commands.recv())
        .await
        .expect("project registration preparation should report completion")
        .unwrap();
    actor.handle(completion).await;
    let mut saw_projects_changed = false;
    let mut registration = None;
    for _ in 0..4 {
        let frame = tokio::time::timeout(Duration::from_secs(1), responses.recv())
            .await
            .expect("project registration should emit its broadcasts and RPC response")
            .unwrap()
            .as_json()
            .unwrap();
        saw_projects_changed |= frame["event"] == "projectsChanged";
        if frame["id"] == 21 {
            registration = Some(frame);
            break;
        }
    }
    assert!(saw_projects_changed);
    let registration = registration.expect("project registration RPC response should be delivered");
    assert_eq!(registration["id"], 21);
    assert_eq!(registration["ok"], true);
    assert_eq!(registration["payload"]["created"], true);
    assert_eq!(
        registration["payload"]["project"]["name"],
        "Deferred Project"
    );
    assert_eq!(actor.runtime_store.list_projects().await.unwrap().len(), 1);
}
