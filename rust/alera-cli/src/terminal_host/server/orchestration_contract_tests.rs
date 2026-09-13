//! Operation-contract coverage for `orchestration.*` requests: the matrix in
//! `docs/orchestration-operation-contract.md` names replay, ownership, and
//! restart guarantees each operation must keep. These tests pin the cells that
//! the lifecycle and install suites do not already cover.

use std::collections::HashMap;

use alera_core::runtime::{NewOrchestrationTask, OrchestrationDispatchStatus};
use serde_json::{json, Value};

use super::actor_test_harness::{local_client, test_actor};
use super::orchestration_dispatch_requests::DispatchPreparation;
use super::ServerActor;
use crate::terminal_host::client::ClientHandle;

pub(super) async fn ready_task(actor: &ServerActor, coordinator: &str) -> String {
    actor
        .runtime_store
        .create_orchestration_task(NewOrchestrationTask {
            spec: "do the work".to_string(),
            task_title: None,
            display_name: None,
            deps: Vec::new(),
            parent_id: None,
            created_by_terminal_handle: Some(coordinator.to_string()),
            run_id: None,
            workspace_id: "w1".to_string(),
            coordinator_handle: coordinator.to_string(),
            result_schema: None,
        })
        .await
        .unwrap()
        .id
}

pub(super) async fn wire_call(
    actor: &mut ServerActor,
    client_id: u64,
    request_id: i64,
    request_type: &str,
    payload: Value,
) {
    actor
        .handle_line(
            client_id,
            json!({
                "id": request_id,
                "type": request_type,
                "payload": payload,
            })
            .to_string(),
        )
        .await
}

pub(super) async fn read_response(
    responses: &mut tokio::sync::mpsc::UnboundedReceiver<crate::terminal_host::client::ClientFrame>,
    request_id: i64,
) -> Value {
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            let response = responses
                .recv()
                .await
                .expect("response should arrive")
                .as_json()
                .unwrap();
            if response["id"].as_i64() == Some(request_id) {
                return response;
            }
        }
    })
    .await
    .expect("response should arrive")
}

#[tokio::test]
async fn dispatch_retry_after_commit_fails_closed_instead_of_double_dispatching() {
    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    let task_id = ready_task(&actor, "coordinator").await;

    let first = actor
        .prepare_orchestration_dispatch(&json!({
            "task": task_id,
            "to": "worker-1",
            "from": "coordinator",
        }))
        .await
        .unwrap();
    assert!(matches!(first, DispatchPreparation::Ready(_)));

    let retry = actor
        .prepare_orchestration_dispatch(&json!({
            "task": task_id,
            "to": "worker-1",
            "from": "coordinator",
        }))
        .await;
    let error = match retry {
        Ok(_) => panic!("dispatch retry should fail on the committed task state"),
        Err(error) => error.to_string(),
    };
    assert!(
        error.contains("not ready"),
        "dispatch retry should fail on the committed task state, got: {error}"
    );
}

#[tokio::test]
async fn dispatch_accept_is_replayable_and_keeps_the_first_acceptance() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let task_id = ready_task(&actor, "coordinator").await;
    let dispatch = actor
        .runtime_store
        .create_scoped_orchestration_dispatch(
            &task_id,
            "worker-1",
            None,
            "w1",
            "coordinator",
            None,
            "return-immediately",
            "keep-open",
        )
        .await
        .unwrap();

    wire_call(
        &mut actor,
        1,
        10,
        "orchestration.dispatchAccept",
        json!({ "terminal": "worker-1" }),
    )
    .await;
    assert_eq!(read_response(&mut responses, 10).await["ok"], true);
    let first_accepted_at = actor
        .runtime_store
        .orchestration_dispatch_by_id(&dispatch.id)
        .await
        .unwrap()
        .unwrap()
        .accepted_at;
    assert!(first_accepted_at.is_some());

    wire_call(
        &mut actor,
        1,
        11,
        "orchestration.dispatchAccept",
        json!({ "terminal": "worker-1" }),
    )
    .await;
    assert_eq!(read_response(&mut responses, 11).await["ok"], true);
    let accepted = actor
        .runtime_store
        .orchestration_dispatch_by_id(&dispatch.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(accepted.status, OrchestrationDispatchStatus::Dispatched);
    assert_eq!(accepted.accepted_at, first_accepted_at);
}

#[tokio::test]
async fn task_cancel_is_idempotent_for_repeated_retries() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let task_id = ready_task(&actor, "coordinator").await;

    for request_id in [20, 21] {
        wire_call(
            &mut actor,
            1,
            request_id,
            "orchestration.taskCancel",
            json!({
                "id": task_id,
                "reason": "no longer needed",
                "actor": "coordinator",
            }),
        )
        .await;
        let response = read_response(&mut responses, request_id).await;
        assert_eq!(response["ok"], true, "cancel {request_id}: {response}");
        assert_eq!(response["payload"]["task"]["status"], "cancelled");
    }
}

#[tokio::test]
async fn gate_resolve_rejects_a_second_resolution() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let task_id = ready_task(&actor, "coordinator").await;
    let gate = actor
        .runtime_store
        .create_orchestration_gate(&task_id, "proceed?", &[])
        .await
        .unwrap();

    wire_call(
        &mut actor,
        1,
        30,
        "orchestration.gateResolve",
        json!({ "id": gate.id, "resolution": "yes" }),
    )
    .await;
    assert_eq!(read_response(&mut responses, 30).await["ok"], true);

    wire_call(
        &mut actor,
        1,
        31,
        "orchestration.gateResolve",
        json!({ "id": gate.id, "resolution": "no" }),
    )
    .await;
    let second = read_response(&mut responses, 31).await;
    assert_eq!(second["ok"], false);
    assert!(
        second["error"]
            .as_str()
            .is_some_and(|error| error.contains("not pending")),
        "second resolve should fail with a typed error, got: {second}"
    );
}

#[tokio::test]
async fn heartbeat_is_rejected_once_the_dispatch_is_inactive() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let task_id = ready_task(&actor, "coordinator").await;
    let dispatch = actor
        .runtime_store
        .create_scoped_orchestration_dispatch(
            &task_id,
            "worker-1",
            None,
            "w1",
            "coordinator",
            None,
            "return-immediately",
            "keep-open",
        )
        .await
        .unwrap();
    actor
        .runtime_store
        .accept_orchestration_dispatch(&dispatch.id, "worker-1", "")
        .await
        .unwrap();
    actor
        .runtime_store
        .complete_orchestration_dispatch(
            &dispatch.id,
            "worker-1",
            "{\"summary\":\"done\",\"completionKind\":\"success\",\"artifacts\":[],\"filesModified\":[],\"validation\":[]}",
        )
        .await
        .unwrap();

    wire_call(
        &mut actor,
        1,
        40,
        "orchestration.heartbeat",
        json!({ "terminal": "worker-1", "phase": "work" }),
    )
    .await;
    let response = read_response(&mut responses, 40).await;
    assert_eq!(response["ok"], false);
    assert!(
        response["error"]
            .as_str()
            .is_some_and(|error| error.contains("no active dispatch")),
        "heartbeat on an inactive dispatch should fail, got: {response}"
    );
}

#[tokio::test]
async fn task_recover_rejects_a_task_that_is_not_stalled() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let task_id = ready_task(&actor, "coordinator").await;

    wire_call(
        &mut actor,
        1,
        50,
        "orchestration.taskRecover",
        json!({
            "id": task_id,
            "status": "ready",
            "reason": "retry",
            "actor": "coordinator",
        }),
    )
    .await;
    let response = read_response(&mut responses, 50).await;
    assert_eq!(response["ok"], false);
    assert!(
        response["error"]
            .as_str()
            .is_some_and(|error| error.contains("not stalled")),
        "recover on a ready task should fail, got: {response}"
    );
}

#[tokio::test]
async fn worker_done_rejects_an_assignee_mismatch() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let task_id = ready_task(&actor, "coordinator").await;
    let dispatch = actor
        .runtime_store
        .create_scoped_orchestration_dispatch(
            &task_id,
            "worker-1",
            None,
            "w1",
            "coordinator",
            None,
            "return-immediately",
            "keep-open",
        )
        .await
        .unwrap();

    wire_call(
        &mut actor,
        1,
        60,
        "orchestration.workerDone",
        json!({
            "terminal": "worker-2",
            "task": task_id,
            "dispatch": dispatch.id,
            "result": {
                "summary": "done",
                "completionKind": "success",
                "artifacts": [],
                "filesModified": [],
                "validation": []
            },
        }),
    )
    .await;
    let response = read_response(&mut responses, 60).await;
    assert_eq!(response["ok"], false);
    assert!(
        response["error"]
            .as_str()
            .is_some_and(|error| error.contains("authority rejected")),
        "workerDone from the wrong terminal should fail, got: {response}"
    );
}
