use alera_core::runtime::{
    Project, ProjectKind, Workspace, WorkspaceKind, WorkspaceStatus, LOCAL_HOST_ID,
};
use chrono::Utc;
use serde_json::json;

use super::*;

#[tokio::test]
async fn direct_store_cascade_removals_release_hosted_review_refs() {
    for remove_project in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let repo_path = directory.path().join("project");
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
        let store = RuntimeStore::open(&directory.path().join("runtime"))
            .await
            .unwrap();
        insert_records(&store, &repo_path, &repo_path, retention_id).await;

        if remove_project {
            super::remove_project(store.clone(), "project")
                .await
                .unwrap();
            assert!(store.find_project("project").await.unwrap().is_none());
        } else {
            super::remove_workspace(store.clone(), "workspace")
                .await
                .unwrap();
            assert!(store.find_workspace("workspace").await.unwrap().is_none());
        }
        for role in ["base", "head"] {
            assert!(repository
                .find_reference(&format!(
                    "refs/alera/hosted-reviews/tabs/{retention_id}/{role}"
                ))
                .is_err());
        }
    }
}

#[tokio::test]
async fn release_falls_back_to_the_project_repo_after_worktree_removal() {
    let directory = tempfile::tempdir().unwrap();
    let project_repo_path = directory.path().join("project");
    let removed_worktree_path = directory.path().join("removed-worktree");
    let repository = git2::Repository::init(&project_repo_path).unwrap();
    let object = repository.blob(b"review object").unwrap();
    let retention_id = "abcdef0123456789abcdef0123456789";
    for role in ["base", "head"] {
        repository
            .reference(
                &format!("refs/alera/hosted-reviews/operations/{retention_id}/{role}"),
                object,
                true,
                "test",
            )
            .unwrap();
    }
    let store = RuntimeStore::open(&directory.path().join("runtime"))
        .await
        .unwrap();
    insert_records(
        &store,
        &project_repo_path,
        &removed_worktree_path,
        retention_id,
    )
    .await;

    reconcile(&store).await;
    for role in ["base", "head"] {
        assert!(repository
            .find_reference(&format!(
                "refs/alera/hosted-reviews/tabs/{retention_id}/{role}"
            ))
            .is_ok());
        assert!(repository
            .find_reference(&format!(
                "refs/alera/hosted-reviews/operations/{retention_id}/{role}"
            ))
            .is_err());
    }

    let retentions = for_tab(&store, "diff-tab").await;
    release(retentions);

    for role in ["base", "head"] {
        assert!(repository
            .find_reference(&format!(
                "refs/alera/hosted-reviews/tabs/{retention_id}/{role}"
            ))
            .is_err());
    }
}

#[tokio::test]
async fn reconcile_sweeps_journaled_operations_in_nested_repositories() {
    let directory = tempfile::tempdir().unwrap();
    let folder_path = directory.path().join("folder");
    let repo_path = folder_path.join("packages/app");
    std::fs::create_dir_all(&repo_path).unwrap();
    let repository = git2::Repository::init(&repo_path).unwrap();
    let object = repository.blob(b"review object").unwrap();
    let retention_id = uuid::Uuid::new_v4().simple().to_string();
    for role in ["base", "head"] {
        repository
            .reference(
                &format!("refs/alera/hosted-reviews/operations/{retention_id}/{role}"),
                object,
                true,
                "test",
            )
            .unwrap();
    }
    write_operation_marker(
        repo_path.to_string_lossy().as_ref(),
        &retention_id,
        u32::MAX,
    );
    let store = RuntimeStore::open(&directory.path().join("runtime"))
        .await
        .unwrap();
    insert_workspace_records(&store, &folder_path, &folder_path).await;

    reconcile(&store).await;

    for role in ["base", "head"] {
        assert!(repository
            .find_reference(&format!(
                "refs/alera/hosted-reviews/operations/{retention_id}/{role}"
            ))
            .is_err());
    }
    assert!(!alera_core::git::hosted_review::hosted_review_operations()
        .iter()
        .any(|operation| operation.retention_id == retention_id));
}

#[tokio::test]
async fn reconcile_preserves_an_operation_owned_by_a_live_process() {
    let directory = tempfile::tempdir().unwrap();
    let repo_path = directory.path().join("project");
    let repository = git2::Repository::init(&repo_path).unwrap();
    let object = repository.blob(b"review object").unwrap();
    let retention_id = uuid::Uuid::new_v4().simple().to_string();
    for role in ["base", "head"] {
        repository
            .reference(
                &format!("refs/alera/hosted-reviews/operations/{retention_id}/{role}"),
                object,
                true,
                "test",
            )
            .unwrap();
    }
    alera_core::git::hosted_review::record_hosted_review_operation(
        repo_path.to_string_lossy().as_ref(),
        &retention_id,
    )
    .unwrap();
    let store = RuntimeStore::open(&directory.path().join("runtime"))
        .await
        .unwrap();
    insert_workspace_records(&store, &repo_path, &repo_path).await;

    reconcile(&store).await;

    for role in ["base", "head"] {
        assert!(repository
            .find_reference(&format!(
                "refs/alera/hosted-reviews/operations/{retention_id}/{role}"
            ))
            .is_ok());
    }
    assert!(alera_core::git::hosted_review::hosted_review_operations()
        .iter()
        .any(|operation| operation.retention_id == retention_id));
    alera_core::git::hosted_review::release_hosted_review_range(
        repo_path.to_string_lossy().as_ref(),
        &retention_id,
    )
    .unwrap();
}

fn write_operation_marker(repo_path: &str, retention_id: &str, owner_pid: u32) {
    let path =
        std::env::temp_dir().join(format!("alera-hosted-review-operation-{retention_id}.json"));
    std::fs::write(
        path,
        serde_json::to_vec(&json!({
            "repoPath": repo_path,
            "retentionId": retention_id,
            "ownerPid": owner_pid,
        }))
        .unwrap(),
    )
    .unwrap();
}

async fn insert_records(
    store: &RuntimeStore,
    project_repo_path: &Path,
    workspace_path: &Path,
    retention_id: &str,
) {
    insert_workspace_records(store, project_repo_path, workspace_path).await;
    store
        .upsert_workspace_tab(WorkspaceTabRecord {
            id: "diff-tab".into(),
            workspace_id: "workspace".into(),
            kind: "gitDiff".into(),
            title: "Pull Request Diff".into(),
            payload: json!({RETENTION_ID_KEY: retention_id}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .await
        .unwrap();
}

async fn insert_workspace_records(
    store: &RuntimeStore,
    project_repo_path: &Path,
    workspace_path: &Path,
) {
    let now = Utc::now();
    store
        .upsert_project(Project {
            id: "project".into(),
            name: "Project".into(),
            repo_path: project_repo_path.to_string_lossy().into_owned(),
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
            kind: WorkspaceKind::Linked,
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
}
