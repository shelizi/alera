use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use alera_core::runtime::{Project, ProjectKind, Workspace};
use chrono::Utc;
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

#[tokio::test]
async fn automation_policy_show_is_deferred_before_repository_declaration_read() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("automation-repo");
    std::fs::create_dir_all(&repo_path).unwrap();
    std::fs::write(repo_path.join("alera.toml"), "automation_declared = true\n").unwrap();
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
            id: "automation-project".into(),
            name: "Automation Project".into(),
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
            81,
            "automation.policy",
            &json!({"kind": "show", "projectId": "automation-project"}),
        )
        .await
        .unwrap();
    assert!(deferred);

    actor
        .handle_line(
            1,
            json!({"id": 82, "type": "status.get", "payload": {}}).to_string(),
        )
        .await;
    let status = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("status response should not wait for automation policy repository read")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(status["id"], 82);

    let completion = tokio::time::timeout(Duration::from_secs(1), commands.recv())
        .await
        .expect("automation policy worker should report completion")
        .unwrap();
    actor.handle(completion).await;
    let policy_response = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("automation policy completion should answer the original request")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(policy_response["id"], 81);
    assert_eq!(policy_response["ok"], true);
    assert_eq!(policy_response["payload"]["project"]["repoDeclared"], true);
}

#[tokio::test]
async fn automation_policy_mutations_and_run_bound_show_remain_actor_owned() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, _responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;

    for payload in [
        json!({"kind": "agent", "profileId": "profile", "policy": {}}),
        json!({"kind": "project", "projectId": "project", "policy": {}}),
        json!({"kind": "show", "projectId": "project", "run": "run-1"}),
    ] {
        assert!(
            !actor
                .try_start_deferred_request(1, 91, "automation.policy", &payload)
                .await
                .unwrap(),
            "actor-owned policy request was unexpectedly deferred: {payload}"
        );
    }
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
