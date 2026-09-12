use std::collections::HashMap;
use std::time::Duration;

use alera_core::runtime::{OrchestrationDispatchStatus, OrchestrationTaskStatus};
use serde_json::json;

use super::actor_test_harness::{local_client, test_actor};
use super::deferred_admission::DeferredAdmission;
use super::dispatch_context_install::DispatchContextContinuation;
use crate::terminal_host::client::ClientHandle;

async fn ready_task(actor: &super::ServerActor, workspace_id: &str) -> String {
    actor
        .runtime_store
        .create_orchestration_task(alera_core::runtime::NewOrchestrationTask {
            spec: "do the work".to_string(),
            task_title: None,
            display_name: None,
            deps: Vec::new(),
            parent_id: None,
            created_by_terminal_handle: Some("coordinator".to_string()),
            run_id: None,
            workspace_id: workspace_id.to_string(),
            coordinator_handle: "coordinator".to_string(),
            result_schema: None,
        })
        .await
        .unwrap()
        .id
}

#[tokio::test]
async fn dispatch_context_cleanup_is_deferred_and_budgeted() {
    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    actor.deferred_admission = std::sync::Arc::new(DeferredAdmission::paused_with_limits(
        usize::MAX,
        usize::MAX,
        0,
    ));
    let context_dir = actor.runtime_dir.join("orchestration-contexts");
    std::fs::create_dir_all(&context_dir).unwrap();
    let context_path = context_dir.join("cleanup.json");
    std::fs::write(&context_path, b"context").unwrap();

    actor.remove_dispatch_context("cleanup");
    assert!(
        context_path.exists(),
        "dispatch context cleanup ran synchronously without background admission"
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        context_path.exists(),
        "dispatch context cleanup bypassed the deferred I/O budget"
    );

    actor.deferred_admission.add_test_permits(1);
    tokio::time::timeout(Duration::from_secs(1), async {
        while context_path.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("dispatch context cleanup should run after budget capacity is released");
}

#[tokio::test]
async fn dispatch_context_install_is_deferred_from_the_actor_mailbox() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    actor.deferred_admission = std::sync::Arc::new(DeferredAdmission::paused_with_limits(
        usize::MAX,
        usize::MAX,
        0,
    ));
    let (inbox, mut commands) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;
    let task_id = ready_task(&actor, "w1").await;

    actor
        .handle_line(
            1,
            json!({
                "id": 30,
                "type": "orchestration.dispatch",
                "payload": {"task": task_id, "to": "worker-1", "from": "coordinator"},
            })
            .to_string(),
        )
        .await;
    assert!(
        tokio::time::timeout(Duration::from_millis(100), responses.recv())
            .await
            .is_err(),
        "dispatch answered before the context install ran"
    );

    actor
        .handle_line(
            1,
            json!({"id": 31, "type": "status.get", "payload": {}}).to_string(),
        )
        .await;
    let status = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("status should not wait for a gated dispatch context install")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(status["id"], 31);

    actor.deferred_admission.add_test_permits(1);
    let completion = tokio::time::timeout(Duration::from_secs(1), commands.recv())
        .await
        .expect("dispatch context install should report completion")
        .unwrap();
    actor.handle(completion).await;
    let response = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("dispatch should answer after the context install completes")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(response["id"], 30);
    assert_eq!(response["ok"], true);
    assert!(response["payload"]["contextToken"].as_str().is_some());

    let context_path = actor
        .runtime_dir
        .join("orchestration-contexts")
        .join("worker-1.json");
    let context: serde_json::Value =
        serde_json::from_slice(&std::fs::read(context_path).unwrap()).unwrap();
    assert_eq!(
        context["token"].as_str().unwrap(),
        response["payload"]["contextToken"].as_str().unwrap()
    );
}

#[tokio::test]
async fn stale_dispatch_context_install_cannot_overwrite_a_newer_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    actor.deferred_admission = std::sync::Arc::new(DeferredAdmission::paused_with_limits(
        usize::MAX,
        usize::MAX,
        0,
    ));
    actor
        .start_dispatch_context_install(
            "reused",
            "old-dispatch",
            "old-token",
            DispatchContextContinuation::Detached,
        )
        .unwrap();

    actor.remove_dispatch_context("reused");
    actor
        .start_dispatch_context_install(
            "reused",
            "new-dispatch",
            "new-token",
            DispatchContextContinuation::Detached,
        )
        .unwrap();
    actor.deferred_admission.add_test_permits(3);
    let context_path = actor
        .runtime_dir
        .join("orchestration-contexts")
        .join("reused.json");
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if let Ok(bytes) = std::fs::read(&context_path) {
                let context: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                if context["dispatchId"] == "new-dispatch" {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the newest dispatch context install should win");
    tokio::time::sleep(Duration::from_millis(100)).await;
    let context: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&context_path).unwrap()).unwrap();
    assert_eq!(context["dispatchId"], "new-dispatch");
    assert_eq!(context["token"], "new-token");
}

#[tokio::test]
async fn dispatch_context_completion_is_dropped_when_the_dispatch_owner_died() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    actor.deferred_admission = std::sync::Arc::new(DeferredAdmission::paused_with_limits(
        usize::MAX,
        usize::MAX,
        0,
    ));
    let (inbox, mut commands) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;
    let task_id = ready_task(&actor, "w1").await;

    actor
        .handle_line(
            1,
            json!({
                "id": 40,
                "type": "orchestration.dispatch",
                "payload": {"task": task_id, "to": "worker-2", "from": "coordinator"},
            })
            .to_string(),
        )
        .await;
    let dispatch = actor
        .runtime_store
        .active_orchestration_dispatch_for_handle("worker-2")
        .await
        .unwrap()
        .expect("the dispatch record commits before the install lands");
    actor
        .runtime_store
        .fail_orchestration_startup(&dispatch.id, "owner invalidated")
        .await
        .unwrap();

    actor.deferred_admission.add_test_permits(1);
    let completion = tokio::time::timeout(Duration::from_secs(1), commands.recv())
        .await
        .expect("dispatch context install should still report completion")
        .unwrap();
    actor.handle(completion).await;
    assert!(
        tokio::time::timeout(Duration::from_millis(100), responses.recv())
            .await
            .is_err(),
        "a dead dispatch owner must not get a late install commit"
    );
}

#[tokio::test]
async fn dispatch_context_install_jumps_ahead_of_queued_bulk_work() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    actor.deferred_admission =
        std::sync::Arc::new(DeferredAdmission::paused_with_limits(1, usize::MAX, 0));
    actor.deferred_admission.add_test_permits(1);
    let (inbox, mut commands) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;
    let task_id = ready_task(&actor, "w1").await;

    let (blocker_started_tx, blocker_started_rx) = tokio::sync::oneshot::channel();
    let (blocker_release_tx, blocker_release_rx) = tokio::sync::oneshot::channel();
    actor
        .start_deferred_request(1, 60, "test.blocker", async move {
            let _ = blocker_started_tx.send(());
            let _ = blocker_release_rx.await;
            Ok(json!({}))
        })
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), blocker_started_rx)
        .await
        .expect("the bulk blocker should hold the only deferred slot")
        .unwrap();

    actor
        .start_deferred_request(1, 61, "test.bulk", async { Ok(json!({})) })
        .unwrap();

    actor
        .handle_line(
            1,
            json!({
                "id": 62,
                "type": "orchestration.dispatch",
                "payload": {"task": task_id, "to": "worker-jump", "from": "coordinator"},
            })
            .to_string(),
        )
        .await;

    let _ = blocker_release_tx.send(());
    let install = loop {
        let command = tokio::time::timeout(Duration::from_secs(1), commands.recv())
            .await
            .expect("the dispatch context install should complete")
            .unwrap();
        match command {
            super::ServerCommand::DispatchContextInstalled { .. } => break command,
            super::ServerCommand::DeferredRequestFinished { request_id, .. } => {
                assert_ne!(
                    request_id, 61,
                    "the queued bulk request started before the dispatch context install"
                );
            }
            _ => panic!("unexpected deferred completion"),
        }
    };
    actor.handle(install).await;
    let response = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("the dispatch should answer after its context install")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(response["id"], 62);
    assert_eq!(response["ok"], true);
}

#[tokio::test]
async fn orphaned_awaiting_acceptance_dispatch_is_recovered_on_startup() {
    let dir = tempfile::tempdir().unwrap();
    let first = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    let task_id = ready_task(&first, "w1").await;
    let dispatch = first
        .runtime_store
        .create_scoped_orchestration_dispatch(
            &task_id,
            "orphaned-worker",
            None,
            "w1",
            "coordinator",
            Some("hash"),
            "return-immediately",
            "keep-open",
        )
        .await
        .unwrap();
    // The committed dispatch and its context file outlive the host; the
    // in-memory install continuation does not.
    let context_dir = first.runtime_dir.join("orchestration-contexts");
    std::fs::create_dir_all(&context_dir).unwrap();
    let context_path = context_dir.join("orphaned-worker.json");
    std::fs::write(&context_path, br#"{"dispatchId":"d","token":"t"}"#).unwrap();
    drop(first);

    let mut restarted = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    restarted.recover_orphaned_dispatch_startups().await;

    let recovered = restarted
        .runtime_store
        .orchestration_dispatch_by_id(&dispatch.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recovered.status, OrchestrationDispatchStatus::StartupFailed);
    let task = restarted
        .runtime_store
        .orchestration_task_by_id(&task_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(task.status, OrchestrationTaskStatus::Ready);
    tokio::time::timeout(Duration::from_secs(1), async {
        while context_path.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the orphaned context file must be removed");
}

#[tokio::test]
async fn startup_recovery_leaves_accepted_dispatches_for_stall_detection() {
    let dir = tempfile::tempdir().unwrap();
    let first = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    let task_id = ready_task(&first, "w1").await;
    let dispatch = first
        .runtime_store
        .create_scoped_orchestration_dispatch(
            &task_id,
            "accepted-worker",
            None,
            "w1",
            "coordinator",
            Some("hash"),
            "return-immediately",
            "keep-open",
        )
        .await
        .unwrap();
    first
        .runtime_store
        .accept_orchestration_dispatch(&dispatch.id, "accepted-worker", "hash")
        .await
        .unwrap();
    let context_dir = first.runtime_dir.join("orchestration-contexts");
    std::fs::create_dir_all(&context_dir).unwrap();
    let context_path = context_dir.join("accepted-worker.json");
    std::fs::write(&context_path, br#"{"dispatchId":"d","token":"t"}"#).unwrap();
    drop(first);

    let mut restarted = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    restarted.recover_orphaned_dispatch_startups().await;

    let recovered = restarted
        .runtime_store
        .orchestration_dispatch_by_id(&dispatch.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recovered.status, OrchestrationDispatchStatus::Dispatched);
    assert!(
        context_path.exists(),
        "an accepted dispatch keeps its context; stall detection owns it"
    );
}
