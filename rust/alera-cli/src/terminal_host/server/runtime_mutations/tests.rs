use alera_core::runtime::{
    Project, ProjectKind, RuntimeStore, Workspace, WorkspaceKind, WorkspaceStatus,
    WorkspaceTabRecord, LOCAL_HOST_ID,
};
use chrono::Utc;
use serde_json::json;

use super::{run_runtime_mutation, RuntimeMutationEffect, RuntimeMutationRequest};

#[tokio::test]
async fn removing_tab_releases_its_hosted_review_refs() {
    let dir = tempfile::tempdir().unwrap();
    let workspace_path = dir.path().join("workspace");
    let repo_path = workspace_path.join("nested/repo");
    std::fs::create_dir_all(&repo_path).unwrap();
    let repository = git2::Repository::init(&repo_path).unwrap();
    let object = repository.blob(b"review object").unwrap();
    let retention_id = "0123456789abcdef0123456789abcdef";
    for role in ["base", "head"] {
        repository
            .reference(
                &format!("refs/alera/hosted-reviews/tabs/{retention_id}/{role}"),
                object,
                true,
                "test",
            )
            .unwrap();
    }
    let store = RuntimeStore::open(&dir.path().join("runtime"))
        .await
        .unwrap();
    let now = Utc::now();
    store
        .upsert_project(Project {
            id: "project".into(),
            name: "Project".into(),
            repo_path: repo_path.to_string_lossy().into_owned(),
            created_at: now,
            updated_at: now,
            kind: ProjectKind::GitRepository,
        })
        .await
        .unwrap();
    store
        .upsert_workspace(Workspace {
            id: "workspace".into(),
            instance_id: "instance".into(),
            host_id: LOCAL_HOST_ID.into(),
            project_id: "project".into(),
            name: "Workspace".into(),
            branch: None,
            path: workspace_path.to_string_lossy().into_owned(),
            created_at: now,
            updated_at: now,
            kind: WorkspaceKind::Main,
            status: WorkspaceStatus::Active,
            source_branch: None,
            reuses_existing_branch: false,
            is_pinned: false,
            tag_ids: Vec::new(),
            tag_names: Vec::new(),
            parent_workspace_id: None,
            section_id: None,
            child_count: 0,
            archived_at: None,
        })
        .await
        .unwrap();
    store
        .upsert_workspace_tab(WorkspaceTabRecord {
            id: "diff-tab".into(),
            workspace_id: "workspace".into(),
            kind: "gitDiff".into(),
            title: "Pull Request Diff".into(),
            payload: json!({
                "gitDiffRoot": "nested/repo",
                "gitDiffHostedReviewRetentionId": retention_id,
            }),
            created_at: now,
            updated_at: now,
        })
        .await
        .unwrap();

    let outcome = run_runtime_mutation(
        store,
        RuntimeMutationRequest::RemoveTab {
            tab_id: "diff-tab".into(),
        },
    )
    .await;

    assert!(outcome.result.is_ok());
    for role in ["base", "head"] {
        assert!(repository
            .find_reference(&format!(
                "refs/alera/hosted-reviews/tabs/{retention_id}/{role}"
            ))
            .is_err());
    }
}

#[tokio::test]
async fn sleep_reports_committed_effect_when_activity_recording_fails() {
    let dir = tempfile::tempdir().unwrap();
    let store = RuntimeStore::open(dir.path()).await.unwrap();
    store
        .upsert_workspace_tab(WorkspaceTabRecord {
            id: "emulator-tab".into(),
            workspace_id: "force-activity-failure".into(),
            kind: "terminal".into(),
            title: "Android".into(),
            payload: json!({}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .await
        .unwrap();

    let outcome = run_runtime_mutation(
        store.clone(),
        RuntimeMutationRequest::SleepWorkspace {
            workspace_id: "force-activity-failure".into(),
        },
    )
    .await;

    assert!(outcome.result.is_err());
    assert!(outcome.committed_tab_ids.is_empty());
    assert!(matches!(
        outcome.effect_on_error,
        Some(RuntimeMutationEffect::WorkspaceSlept { workspace_id })
            if workspace_id == "force-activity-failure"
    ));
    assert!(store
        .find_workspace_tab("emulator-tab")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn feature_retirement_migration_resumes_after_midway_failure_and_restart() {
    let dir = tempfile::tempdir().unwrap();
    let store = RuntimeStore::open(dir.path()).await.unwrap();
    for statement in [
        "CREATE TABLE codexChatState (threadId TEXT PRIMARY KEY, tabId TEXT NOT NULL, revision INTEGER NOT NULL, stateJson TEXT NOT NULL)",
        "CREATE INDEX codexChatStateTab ON codexChatState(tabId)",
        "CREATE TRIGGER codexChatStateDeleteTab AFTER DELETE ON workspaceTabs BEGIN DELETE FROM codexChatState WHERE tabId = OLD.id; END",
    ] {
        sqlx::query(statement).execute(store.pool()).await.unwrap();
    }
    sqlx::query("INSERT INTO codexChatState VALUES ('thread-1', 'legacy-codex', 1, '{}')")
        .execute(store.pool())
        .await
        .unwrap();
    let now = Utc::now();
    store
        .upsert_workspace_tab(WorkspaceTabRecord {
            id: "legacy-codex".into(),
            workspace_id: "workspace-1".into(),
            kind: "codex".into(),
            title: "Pending Chat".into(),
            created_at: now,
            updated_at: now,
            payload: json!({"threadId": "thread-1"}),
        })
        .await
        .unwrap();
    sqlx::query(
        "CREATE TRIGGER failFeatureRetirement BEFORE DELETE ON workspaceTabs BEGIN SELECT RAISE(ABORT, 'blocked retirement'); END",
    )
    .execute(store.pool())
    .await
    .unwrap();

    let error = store.retire_removed_features().await.unwrap_err();
    assert!(error.to_string().contains("blocked retirement"));
    let objects: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_master WHERE name IN ('codexChatState', 'codexChatStateTab', 'codexChatStateDeleteTab')",
    )
    .fetch_one(store.pool())
    .await
    .unwrap();
    assert_eq!(objects, 3, "the failed migration must roll back its DDL");
    assert!(store
        .find_workspace_tab("legacy-codex")
        .await
        .unwrap()
        .is_some());

    sqlx::query("DROP TRIGGER failFeatureRetirement")
        .execute(store.pool())
        .await
        .unwrap();
    drop(store);

    let restarted = RuntimeStore::open(dir.path()).await.unwrap();
    restarted.retire_removed_features().await.unwrap();
    restarted.retire_removed_features().await.unwrap();
    assert!(restarted
        .find_workspace_tab("legacy-codex")
        .await
        .unwrap()
        .is_none());
    let remaining_objects: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_master WHERE name IN ('codexChatState', 'codexChatStateTab', 'codexChatStateDeleteTab')",
    )
    .fetch_one(restarted.pool())
    .await
    .unwrap();
    assert_eq!(remaining_objects, 0);
}
