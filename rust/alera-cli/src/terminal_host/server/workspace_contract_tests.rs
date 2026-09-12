//! Operation-contract coverage for `workspace.*` requests: the matrix in
//! `docs/workspace-operation-contract.md` names the replay and recovery
//! guarantees each operation keeps. These tests pin the cells the managed
//! workspace and section suites do not already cover.

use std::collections::HashMap;

use alera_core::runtime::WorkbenchLayoutRecord;
use serde_json::json;

use super::actor_test_harness::test_actor;
use super::runtime_mutations::{run_runtime_mutation, RuntimeMutationRequest};

#[tokio::test]
async fn workspace_remove_is_idempotent_for_retries() {
    let dir = tempfile::tempdir().unwrap();
    let project_dir = tempfile::tempdir().unwrap();
    let actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    let registration = crate::project_management::register_project(
        &actor.runtime_store,
        project_dir.path().to_str().unwrap(),
        Some("Demo"),
    )
    .await
    .unwrap();
    let workspace_id = registration.main_workspace.id.clone();

    for _ in 0..2 {
        let outcome = run_runtime_mutation(
            actor.runtime_store.clone(),
            RuntimeMutationRequest::RemoveWorkspace {
                workspace_id: workspace_id.clone(),
                cascade_tabs: true,
            },
        )
        .await;
        assert!(
            outcome.result.is_ok(),
            "repeated workspace.remove should stay a no-op success"
        );
    }
}

#[tokio::test]
async fn workspace_sleep_is_idempotent_for_retries() {
    let dir = tempfile::tempdir().unwrap();
    let project_dir = tempfile::tempdir().unwrap();
    let actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    let registration = crate::project_management::register_project(
        &actor.runtime_store,
        project_dir.path().to_str().unwrap(),
        Some("Demo"),
    )
    .await
    .unwrap();
    let workspace_id = registration.main_workspace.id.clone();

    for _ in 0..2 {
        let outcome = run_runtime_mutation(
            actor.runtime_store.clone(),
            RuntimeMutationRequest::SleepWorkspace {
                workspace_id: workspace_id.clone(),
            },
        )
        .await;
        assert!(
            outcome.result.is_ok(),
            "repeated workspace.sleep should stay a no-op success"
        );
    }
}

#[tokio::test]
async fn layout_upsert_last_write_wins_for_the_same_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let project_dir = tempfile::tempdir().unwrap();
    let actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    let registration = crate::project_management::register_project(
        &actor.runtime_store,
        project_dir.path().to_str().unwrap(),
        Some("Demo"),
    )
    .await
    .unwrap();
    let workspace_id = registration.main_workspace.id.clone();

    for marker in ["first", "second"] {
        actor
            .runtime_store
            .upsert_workbench_layout(WorkbenchLayoutRecord {
                workspace_id: workspace_id.clone(),
                data: json!({ "marker": marker }),
            })
            .await
            .unwrap();
    }

    let layout = actor
        .runtime_store
        .find_workbench_layout(&workspace_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(layout.data["marker"], "second");
}
