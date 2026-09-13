use std::collections::HashMap;
use std::time::Duration;

use alera_core::runtime::{OrchestrationDispatchStatus, OrchestrationTaskStatus};

use super::actor_test_harness::test_actor;
use super::dispatch_context_install_tests::ready_task;

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
