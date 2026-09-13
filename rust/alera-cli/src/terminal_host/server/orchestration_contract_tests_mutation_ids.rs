//! `clientMutationId` receipt coverage for `orchestration.send` / `reply` /
//! `escalate` / `ask`: replay, payload-conflict, and in-progress cells of the
//! contract matrix.

use std::collections::HashMap;

use alera_core::runtime::{
    NewOrchestrationMessage, OrchestrationMessagePriority, OrchestrationMessageType,
};
use serde_json::json;

use super::actor_test_harness::{local_client, test_actor};
use super::orchestration_contract_tests::{read_response, ready_task, wire_call};
use crate::terminal_host::client::ClientHandle;

#[tokio::test]
async fn send_client_mutation_id_replays_without_duplicate_and_conflicts_on_payload_change() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let payload = json!({
        "from": "worker",
        "to": "coordinator",
        "subject": "status",
        "body": "ready",
        "clientMutationId": "send-1",
    });

    wire_call(&mut actor, 1, 70, "orchestration.send", payload.clone()).await;
    let first = read_response(&mut responses, 70).await;
    assert_eq!(first["ok"], true);

    wire_call(&mut actor, 1, 71, "orchestration.send", payload.clone()).await;
    let replay = read_response(&mut responses, 71).await;
    assert_eq!(replay["payload"], first["payload"]);
    assert_eq!(
        actor
            .runtime_store
            .all_orchestration_messages_for_handle("coordinator", None, 100)
            .await
            .unwrap()
            .len(),
        1
    );

    wire_call(
        &mut actor,
        1,
        72,
        "orchestration.send",
        json!({
            "from": "worker",
            "to": "coordinator",
            "subject": "status",
            "body": "changed",
            "clientMutationId": "send-1",
        }),
    )
    .await;
    let conflict = read_response(&mut responses, 72).await;
    assert_eq!(conflict["ok"], false);
    assert!(conflict["error"]
        .as_str()
        .is_some_and(|error| error.contains("different orchestration.send payload")));

    for request_id in [73, 74] {
        wire_call(
            &mut actor,
            1,
            request_id,
            "orchestration.send",
            json!({
                "from": "worker",
                "to": "coordinator",
                "subject": "without key",
                "body": "still inserts",
            }),
        )
        .await;
        assert_eq!(read_response(&mut responses, request_id).await["ok"], true);
    }
    assert_eq!(
        actor
            .runtime_store
            .all_orchestration_messages_for_handle("coordinator", None, 100)
            .await
            .unwrap()
            .len(),
        3
    );
}

#[tokio::test]
async fn reply_client_mutation_id_replays_without_duplicate_reply() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let original = actor
        .runtime_store
        .insert_orchestration_message(NewOrchestrationMessage {
            from_handle: "worker".to_string(),
            to_handle: "coordinator".to_string(),
            subject: "Question".to_string(),
            body: "Which path?".to_string(),
            message_type: OrchestrationMessageType::DecisionGate,
            priority: OrchestrationMessagePriority::High,
            thread_id: None,
            payload: None,
            run_id: None,
            workspace_id: None,
            task_id: None,
            dispatch_id: None,
            expires_at: None,
        })
        .await
        .unwrap();
    let payload = json!({
        "id": original.id,
        "body": "src/main.rs",
        "clientMutationId": "reply-1",
    });

    wire_call(&mut actor, 1, 80, "orchestration.reply", payload.clone()).await;
    let first = read_response(&mut responses, 80).await;
    assert_eq!(first["ok"], true);

    wire_call(&mut actor, 1, 81, "orchestration.reply", payload).await;
    let replay = read_response(&mut responses, 81).await;
    assert_eq!(replay["payload"], first["payload"]);
    let replies = actor
        .runtime_store
        .all_orchestration_messages_for_handle("worker", None, 100)
        .await
        .unwrap();
    assert_eq!(replies.len(), 1);
    assert_eq!(replies[0].body, "src/main.rs");
}

#[tokio::test]
async fn escalate_client_mutation_id_replays_without_duplicate_message() {
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
    let payload = json!({
        "terminal": "worker-1",
        "subject": "Blocked",
        "body": "Need coordinator help",
        "clientMutationId": "escalate-1",
    });

    wire_call(&mut actor, 1, 90, "orchestration.escalate", payload.clone()).await;
    let first = read_response(&mut responses, 90).await;
    assert_eq!(first["ok"], true);

    wire_call(&mut actor, 1, 91, "orchestration.escalate", payload).await;
    let replay = read_response(&mut responses, 91).await;
    assert_eq!(replay["payload"], first["payload"]);
    let messages = actor
        .runtime_store
        .all_orchestration_messages_for_handle("coordinator", None, 100)
        .await
        .unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].message_type,
        OrchestrationMessageType::Escalation
    );
}

#[tokio::test]
async fn ask_client_mutation_id_stays_in_progress_then_replays_answer() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let payload = json!({
        "from": "worker",
        "to": "coordinator",
        "question": "Which path?",
        "timeoutMs": 10_000,
        "clientMutationId": "ask-answer-1",
    });

    wire_call(&mut actor, 1, 100, "orchestration.ask", payload.clone()).await;
    wire_call(&mut actor, 1, 101, "orchestration.ask", payload.clone()).await;
    let in_progress = read_response(&mut responses, 101).await;
    assert_eq!(in_progress["ok"], false);
    assert!(in_progress["error"]
        .as_str()
        .is_some_and(|error| error.contains("already in progress")));

    let questions = actor
        .runtime_store
        .all_orchestration_messages_for_handle("coordinator", None, 100)
        .await
        .unwrap();
    assert_eq!(questions.len(), 1);
    let question_id = questions[0].id.clone();
    wire_call(
        &mut actor,
        1,
        102,
        "orchestration.reply",
        json!({ "id": question_id, "body": "src/main.rs" }),
    )
    .await;
    let answered = read_response(&mut responses, 100).await;
    assert_eq!(answered["ok"], true);
    assert_eq!(answered["payload"]["answered"], true);
    assert_eq!(answered["payload"]["reply"]["body"], "src/main.rs");
    assert_eq!(read_response(&mut responses, 102).await["ok"], true);

    wire_call(&mut actor, 1, 103, "orchestration.ask", payload).await;
    let replay = read_response(&mut responses, 103).await;
    assert_eq!(replay["payload"], answered["payload"]);
    assert_eq!(
        actor
            .runtime_store
            .all_orchestration_messages_for_handle("coordinator", None, 100)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn ask_timeout_settles_client_mutation_id_for_replay() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let payload = json!({
        "from": "worker",
        "to": "coordinator",
        "question": "Which path?",
        "timeoutMs": 10_000,
        "clientMutationId": "ask-timeout-1",
    });

    wire_call(&mut actor, 1, 110, "orchestration.ask", payload.clone()).await;
    actor.handle_orchestration_wait_timeout(1, 77).await;
    let timed_out = read_response(&mut responses, 110).await;
    assert_eq!(timed_out["ok"], true);
    assert_eq!(timed_out["payload"]["answered"], false);
    assert_eq!(timed_out["payload"]["timedOut"], true);

    wire_call(&mut actor, 1, 111, "orchestration.ask", payload).await;
    let replay = read_response(&mut responses, 111).await;
    assert_eq!(replay["payload"], timed_out["payload"]);
    assert_eq!(
        actor
            .runtime_store
            .all_orchestration_messages_for_handle("coordinator", None, 100)
            .await
            .unwrap()
            .len(),
        1
    );
}
