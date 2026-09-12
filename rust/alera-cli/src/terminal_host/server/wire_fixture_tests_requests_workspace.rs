//! Live verification for the managed-workspace creation fixtures under
//! `test/fixtures/wire/`. Split from `wire_fixture_tests_requests.rs` to keep
//! each module under the line budget; the shared helpers live there.

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::json;

use crate::terminal_host::client::ClientHandle;

use super::actor_test_harness::{local_client, test_actor};
use super::wire_fixture_tests_requests::{
    exchange, next_frame, pin_fixture_placeholders, rfc3339, seed_git_project, seed_git_repo,
    uuid_string, wire_fixture,
};

#[tokio::test]
async fn managed_workspace_create_response_matches_shared_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let repo = seed_git_repo(dir.path());
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let (inbox, mut commands) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;
    seed_git_project(&actor, &repo).await;
    let request = wire_fixture("request.workspace_create_managed.json");

    let (mut response, events) =
        exchange(&mut actor, 1, &request, &mut receiver, &mut commands).await;

    assert!(events.is_empty());
    // The response lands first; the trailing broadcast is a wildcard because
    // `projectId` lives under `payload.workspace`, not at the top level the
    // scope lookup reads.
    assert_eq!(
        next_frame(&mut receiver).await,
        wire_fixture("event.workspaces_changed.wildcard.json")
    );
    let workspace = &response["payload"]["workspace"];
    let workspace_id = uuid_string(&workspace["id"]);
    uuid_string(&workspace["instanceId"]);
    assert_eq!(
        workspace["path"],
        dir.path()
            .join("workspaces")
            .join("repo-project-1")
            .join("feature-workspace")
            .to_string_lossy()
            .as_ref()
    );
    assert_eq!(workspace["createdAt"], workspace["updatedAt"]);
    rfc3339(&workspace["createdAt"]);
    // `deferSetup` only produces a command when the project config declares
    // copy or setup actions; this repo has neither.
    assert!(response["payload"].get("deferredSetupCommand").is_none());
    assert_eq!(response["payload"]["setupReport"], json!({"steps": []}));

    let fixture = wire_fixture("response.ok.workspace_create_managed.json");
    pin_fixture_placeholders(
        &mut response,
        &fixture,
        &[
            "payload.workspace.id",
            "payload.workspace.instanceId",
            "payload.workspace.path",
            "payload.workspace.createdAt",
            "payload.workspace.updatedAt",
        ],
    );
    assert_eq!(response, fixture, "workspace id {workspace_id}");
}

#[tokio::test]
async fn managed_workspace_create_deferred_response_matches_shared_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let repo = seed_git_repo(dir.path());
    // A setup command in `alera.toml` is what makes `deferSetup` emit a
    // `deferredSetupCommand` instead of running it inline.
    std::fs::write(
        repo.join("alera.toml"),
        "[worktree]\nsetup = [\"echo fixture\"]\n",
    )
    .unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let (inbox, mut commands) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;
    seed_git_project(&actor, &repo).await;
    let request = wire_fixture("request.workspace_create_managed.json");

    let (mut response, events) =
        exchange(&mut actor, 1, &request, &mut receiver, &mut commands).await;

    assert!(events.is_empty());
    assert_eq!(
        next_frame(&mut receiver).await,
        wire_fixture("event.workspaces_changed.wildcard.json")
    );
    let workspace = &response["payload"]["workspace"];
    let workspace_id = uuid_string(&workspace["id"]);
    uuid_string(&workspace["instanceId"]);
    assert_eq!(workspace["createdAt"], workspace["updatedAt"]);
    // The command is a launcher plus one quoted script path, the only shape
    // that survives every supported interactive shell.
    let command = response["payload"]["deferredSetupCommand"]
        .as_str()
        .expect("a deferred setup command should be emitted");
    let (launcher, extension) = if cfg!(windows) {
        ("cmd /d /c \"", "cmd")
    } else {
        ("/bin/sh \"", "sh")
    };
    let script = command
        .strip_prefix(launcher)
        .and_then(|rest| rest.strip_suffix('"'))
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("unexpected deferred command shape: {command}"));
    assert_eq!(script.parent(), Some(dir.path()));
    assert_eq!(
        script.file_name().and_then(|name| name.to_str()),
        Some(format!("worktree-setup-{}.{extension}", workspace_id).as_str())
    );
    assert!(script.exists(), "{}", script.display());
    assert!(std::fs::read_to_string(&script)
        .unwrap()
        .contains("echo fixture"));

    let fixture = wire_fixture("response.ok.workspace_create_managed.deferred.json");
    pin_fixture_placeholders(
        &mut response,
        &fixture,
        &[
            "payload.workspace.id",
            "payload.workspace.instanceId",
            "payload.workspace.path",
            "payload.workspace.createdAt",
            "payload.workspace.updatedAt",
            "payload.deferredSetupCommand",
        ],
    );
    assert_eq!(response, fixture);
}
