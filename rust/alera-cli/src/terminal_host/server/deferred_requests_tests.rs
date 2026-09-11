use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use alera_core::runtime::{Project, ProjectKind, Workspace};
use chrono::Utc;
use serde_json::json;

use super::actor_test_harness::{local_client, test_actor};
use crate::terminal_host::client::ClientHandle;

#[tokio::test]
async fn host_directory_list_is_deferred_so_control_requests_can_advance() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("item.txt"), "item").unwrap();
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
            31,
            "hostDirectory.list",
            &json!({"path": dir.path().to_string_lossy()}),
        )
        .await
        .unwrap();
    assert!(deferred);

    actor
        .handle_line(
            1,
            json!({"id": 32, "type": "status.get", "payload": {}}).to_string(),
        )
        .await;
    let status = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("status response should not wait for directory listing")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(status["id"], 32);
    assert_eq!(status["ok"], true);

    let completion = tokio::time::timeout(Duration::from_secs(1), commands.recv())
        .await
        .expect("directory worker should report completion")
        .unwrap();
    actor.handle(completion).await;
    let listing = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("directory completion should answer the original request")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(listing["id"], 31);
    assert_eq!(listing["ok"], true);
}

#[tokio::test]
async fn deferred_blocking_completion_is_dropped_after_client_disconnects() {
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
        .try_start_deferred_request(
            1,
            41,
            "hostDirectory.list",
            &json!({"path": dir.path().to_string_lossy()}),
        )
        .await
        .unwrap();
    assert!(deferred);
    let completion = tokio::time::timeout(Duration::from_secs(1), commands.recv())
        .await
        .expect("directory worker should report completion")
        .unwrap();

    actor.clients.remove(&1);
    actor.handle(completion).await;

    assert!(responses.try_recv().is_err());
}

#[tokio::test]
async fn project_branch_listing_is_deferred_before_git_access() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    init_git_repository(&repo_path);
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let now = Utc::now();
    actor
        .runtime_store
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
    let (inbox, mut commands) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;

    let deferred = actor
        .try_start_deferred_request(
            1,
            51,
            "project.branches.list",
            &json!({"projectId": "project"}),
        )
        .await
        .unwrap();
    assert!(deferred);

    actor
        .handle_line(
            1,
            json!({"id": 52, "type": "status.get", "payload": {}}).to_string(),
        )
        .await;
    let status = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("status response should not wait for git branch listing")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(status["id"], 52);

    let completion = tokio::time::timeout(Duration::from_secs(1), commands.recv())
        .await
        .expect("git branch worker should report completion")
        .unwrap();
    actor.handle(completion).await;
    let branch_response = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("git branch completion should answer the original request")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(branch_response["id"], 51);
    assert_eq!(branch_response["ok"], true);
    assert!(branch_response["payload"]["branches"]
        .as_array()
        .unwrap()
        .iter()
        .any(|branch| branch == "feature"));
    assert!(branch_response["payload"]["localBranches"]
        .as_array()
        .unwrap()
        .iter()
        .any(|branch| branch == "feature"));
}

#[tokio::test]
async fn workspace_repository_url_is_deferred_before_git_access() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    init_git_repository(&repo_path);
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let now = Utc::now();
    actor
        .runtime_store
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
    let workspace: Workspace = serde_json::from_value(json!({
        "id": "workspace",
        "instanceId": "instance",
        "hostId": "local",
        "projectId": "project",
        "name": "Workspace",
        "path": repo_path.to_string_lossy(),
        "createdAt": now,
        "updatedAt": now,
        "kind": "main",
        "status": "active",
        "reusesExistingBranch": false
    }))
    .unwrap();
    actor
        .runtime_store
        .upsert_workspace(workspace)
        .await
        .unwrap();
    let (inbox, mut commands) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;

    let deferred = actor
        .try_start_deferred_request(
            1,
            61,
            "workspace.repositoryWebUrl",
            &json!({"workspaceId": "workspace"}),
        )
        .await
        .unwrap();
    assert!(deferred);

    actor
        .handle_line(
            1,
            json!({"id": 62, "type": "status.get", "payload": {}}).to_string(),
        )
        .await;
    let status = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("status response should not wait for git remote lookup")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(status["id"], 62);

    let completion = tokio::time::timeout(Duration::from_secs(1), commands.recv())
        .await
        .expect("git remote worker should report completion")
        .unwrap();
    actor.handle(completion).await;
    let remote_response = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("git remote completion should answer the original request")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(remote_response["id"], 61);
    assert_eq!(remote_response["ok"], true);
    assert_eq!(
        remote_response["payload"]["remoteUrl"],
        "https://example.com/alera.git"
    );
}

#[tokio::test]
async fn effective_project_config_is_deferred_before_repository_file_read() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    std::fs::create_dir_all(&repo_path).unwrap();
    std::fs::write(
        repo_path.join("alera.toml"),
        "[new_workspace]\nprompt_append = \"From repository\"\n",
    )
    .unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let now = Utc::now();
    actor
        .runtime_store
        .upsert_project(Project {
            id: "project-config".into(),
            name: "Project Config".into(),
            repo_path: repo_path.to_string_lossy().into_owned(),
            created_at: now,
            updated_at: now,
            kind: ProjectKind::Folder,
        })
        .await
        .unwrap();
    let (inbox, mut commands) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;

    let deferred = actor
        .try_start_deferred_request(
            1,
            71,
            "projectConfig.effective",
            &json!({"projectId": "project-config"}),
        )
        .await
        .unwrap();
    assert!(deferred);

    actor
        .handle_line(
            1,
            json!({"id": 72, "type": "status.get", "payload": {}}).to_string(),
        )
        .await;
    let status = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("status response should not wait for alera.toml")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(status["id"], 72);

    let completion = tokio::time::timeout(Duration::from_secs(1), commands.recv())
        .await
        .expect("effective config worker should report completion")
        .unwrap();
    actor.handle(completion).await;
    let config_response = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("effective config completion should answer the original request")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(config_response["id"], 71);
    assert_eq!(config_response["ok"], true);
    assert_eq!(config_response["payload"]["origin"], "repoFile");
    assert_eq!(
        config_response["payload"]["config"]["newWorkspace"]["promptAppend"],
        "From repository"
    );
}

fn init_git_repository(path: &Path) {
    std::fs::create_dir_all(path).unwrap();
    let repository = git2::Repository::init(path).unwrap();
    let signature = git2::Signature::now("Alera Test", "alera@example.test").unwrap();
    let tree_id = repository.index().unwrap().write_tree().unwrap();
    let tree = repository.find_tree(tree_id).unwrap();
    let commit_id = repository
        .commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
        .unwrap();
    drop(tree);
    let commit = repository.find_commit(commit_id).unwrap();
    repository.branch("feature", &commit, false).unwrap();
    drop(commit);
    repository
        .remote("origin", "https://example.com/alera.git")
        .unwrap();
}
