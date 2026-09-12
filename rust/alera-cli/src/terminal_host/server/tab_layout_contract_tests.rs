//! Operation-contract coverage for `tab.*` and `layout.*` requests: the matrix
//! in `docs/tab-layout-operation-contract.md` names the replay and recovery
//! guarantees each operation keeps. These tests pin the cells the tab
//! compatibility suites do not already cover.

use std::collections::HashMap;

use alera_core::runtime::WorkspaceTabRecord;
use chrono::Utc;
use serde_json::json;

use super::actor_test_harness::test_actor;
use super::runtime_mutations::{run_runtime_mutation, RuntimeMutationRequest};

#[tokio::test]
async fn tab_remove_is_idempotent_for_retries() {
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
    let now = Utc::now();
    actor
        .runtime_store
        .upsert_workspace_tab(WorkspaceTabRecord {
            id: "tab-1".to_string(),
            workspace_id: registration.main_workspace.id.clone(),
            kind: "editor".to_string(),
            title: "File".to_string(),
            created_at: now,
            updated_at: now,
            payload: json!({}),
        })
        .await
        .unwrap();

    for _ in 0..2 {
        let outcome = run_runtime_mutation(
            actor.runtime_store.clone(),
            RuntimeMutationRequest::RemoveTab {
                tab_id: "tab-1".to_string(),
            },
        )
        .await;
        assert!(
            outcome.result.is_ok(),
            "repeated tab.remove should stay a no-op success"
        );
    }
    assert!(actor
        .runtime_store
        .find_workspace_tab("tab-1")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn tab_remove_for_workspace_is_idempotent_for_retries() {
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
            RuntimeMutationRequest::RemoveWorkspaceTabs {
                workspace_id: workspace_id.clone(),
            },
        )
        .await;
        assert!(
            outcome.result.is_ok(),
            "repeated tab.removeForWorkspace should stay a no-op success"
        );
    }
}

#[tokio::test]
async fn spawn_on_create_failure_rolls_back_the_tab_row() {
    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    let now = Utc::now();
    let result = actor
        .upsert_workspace_tab_and_spawn(WorkspaceTabRecord {
            id: "tab-spawn".to_string(),
            workspace_id: "missing-workspace".to_string(),
            kind: "terminal".to_string(),
            title: "Terminal".to_string(),
            created_at: now,
            updated_at: now,
            payload: json!({ "spawnOnCreate": true }),
        })
        .await;

    assert!(result.is_err(), "a missing workspace must fail the spawn");
    assert!(
        actor
            .runtime_store
            .find_workspace_tab("tab-spawn")
            .await
            .unwrap()
            .is_none(),
        "the tab row must be rolled back after a spawn failure"
    );
}
