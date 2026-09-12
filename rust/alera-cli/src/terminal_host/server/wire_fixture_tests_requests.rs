//! Golden wire-contract coverage for the request-side fixtures shared with
//! the Flutter clients.
//!
//! `wire_fixture_tests.rs` pins host-produced frames for request types the
//! Dart side actively calls; this module drives the remaining request
//! fixtures under `test/fixtures/wire/` through a real `ServerActor` —
//! including the deferred `project.register`/`workspace.createManaged` paths
//! and the runtime-mutation barrier — then compares the emitted frames
//! field-for-field. Generated ids, timestamps, resolved paths, and the
//! deferred setup command are sanity-checked first, then pinned back to the
//! fixture's placeholder values so the whole `Value` still compares equal.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use alera_core::runtime::{
    Project, ProjectKind, SharedWorkbenchPrefsWriter, SharedWorkbenchViewPrefs,
};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::terminal_host::client::{ClientFrame, ClientHandle};
use crate::terminal_host::session::Session;

use super::actor_test_harness::{local_client, mobile_client, test_actor};
use super::{ServerActor, ServerCommand};

fn wire_fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/fixtures/wire")
        .join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// Sends one request frame and returns the response plus any event frames the
/// request pushed ahead of it.
///
/// Deferred requests park their reply behind a `ServerCommand` on the actor
/// inbox, so unlike the `exchange` helper in `wire_fixture_tests.rs` this loop
/// also pumps `commands` back through `actor.handle` until the response lands.
/// On the client lane a mutation's change broadcasts share the response lane,
/// so events can precede *or* trail the response depending on the handler.
async fn exchange(
    actor: &mut ServerActor,
    client_id: u64,
    request: &Value,
    receiver: &mut UnboundedReceiver<ClientFrame>,
    commands: &mut UnboundedReceiver<ServerCommand>,
) -> (Value, Vec<Value>) {
    let request_id = request["id"].as_i64().expect("request id");
    actor.handle_line(client_id, request.to_string()).await;
    tokio::time::timeout(Duration::from_secs(15), async {
        let mut events = Vec::new();
        loop {
            tokio::select! {
                command = commands.recv() => {
                    actor
                        .handle(command.expect("actor inbox should stay open"))
                        .await;
                }
                frame = receiver.recv() => {
                    let frame = frame
                        .expect("the host should write a response")
                        .as_json()
                        .expect("the frame should be JSON");
                    if frame["id"] == request_id {
                        return (frame, events);
                    }
                    events.push(frame);
                }
            }
        }
    })
    .await
    .expect("the request should finish")
}

async fn next_frame(receiver: &mut UnboundedReceiver<ClientFrame>) -> Value {
    receiver
        .recv()
        .await
        .expect("the host should emit an event")
        .as_json()
        .expect("the frame should be JSON")
}

/// Copies the fixture's value at each dotted `path` into `actual`. Fixtures
/// pin generated ids, timestamps, and machine paths as placeholders; the real
/// value is sanity-checked before it is masked so the rest of the frame still
/// compares field-for-field.
fn pin_fixture_placeholders(actual: &mut Value, fixture: &Value, paths: &[&str]) {
    for path in paths {
        let mut expected = &*fixture;
        let mut slot = &mut *actual;
        for segment in path.split('.') {
            expected = expected
                .get(segment)
                .unwrap_or_else(|| panic!("fixture is missing {path}"));
            slot = slot
                .get_mut(segment)
                .unwrap_or_else(|| panic!("response is missing {path}"));
        }
        *slot = expected.clone();
    }
}

fn uuid_string(value: &Value) -> String {
    let text = value.as_str().expect("id should be a string");
    uuid::Uuid::parse_str(text).unwrap_or_else(|_| panic!("{text} should be a uuid"));
    text.to_string()
}

fn rfc3339(value: &Value) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value.as_str().expect("timestamp should be a string"))
        .expect("timestamp should be RFC 3339")
        .with_timezone(&Utc)
}

/// A repository with one commit on `main`, enough for the managed-worktree
/// path to create `feature/*` worktrees without the git CLI.
fn seed_git_repo(parent: &Path) -> PathBuf {
    let repo_path = parent.join("repo");
    let repo = git2::Repository::init(&repo_path).unwrap();
    let tree_id = repo.index().unwrap().write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = git2::Signature::now("Test", "test@example.com").unwrap();
    repo.commit(
        Some("refs/heads/main"),
        &signature,
        &signature,
        "initial",
        &tree,
        &[],
    )
    .unwrap();
    repo.set_head("refs/heads/main").unwrap();
    repo_path
}

async fn seed_git_project(actor: &ServerActor, repo: &Path) {
    let workspace_root = repo.parent().unwrap().join("workspaces");
    actor
        .runtime_store
        .set_workspace_directory(Some(&workspace_root.to_string_lossy()))
        .await
        .unwrap();
    let now = Utc::now();
    actor
        .runtime_store
        .upsert_project(Project {
            id: "project-1".to_string(),
            name: "Fixture Project".to_string(),
            repo_path: repo.to_string_lossy().into_owned(),
            created_at: now,
            updated_at: now,
            kind: ProjectKind::GitRepository,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn project_register_response_matches_shared_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let project_dir = dir.path().join("fixture-project");
    std::fs::create_dir(&project_dir).unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let (inbox, mut commands) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;
    let mut request = wire_fixture("request.project_register.json");
    // The fixture's <repo-path> stands in for the machine-local directory the
    // host canonicalizes before registering it.
    request["payload"]["path"] = json!(project_dir.to_string_lossy());

    let (mut response, events) = exchange(
        &mut actor,
        1,
        &request,
        &mut receiver,
        &mut commands,
    )
    .await;

    // Registration is deferred: the commit broadcasts all three wildcard
    // change events ahead of the response on the same lane.
    assert_eq!(
        events,
        vec![
            json!({"event": "projectsChanged", "payload": {}}),
            json!({"event": "workspacesChanged", "payload": {}}),
            json!({"event": "workspaceTabsChanged", "payload": {}}),
        ]
    );
    let canonical = std::fs::canonicalize(&project_dir)
        .unwrap()
        .to_string_lossy()
        .to_string();
    let project = &response["payload"]["project"];
    let workspace = &response["payload"]["mainWorkspace"];
    let project_id = uuid_string(&project["id"]);
    assert_eq!(project["repoPath"], canonical);
    assert_eq!(workspace["projectId"], project_id);
    assert_eq!(workspace["path"], project["repoPath"]);
    assert_ne!(uuid_string(&workspace["id"]), project_id);
    assert_ne!(uuid_string(&workspace["instanceId"]), project_id);
    assert_eq!(project["createdAt"], project["updatedAt"]);
    assert_eq!(project["createdAt"], workspace["createdAt"]);
    assert_eq!(rfc3339(&project["createdAt"]), rfc3339(&workspace["updatedAt"]));

    let fixture = wire_fixture("response.ok.project_register.json");
    pin_fixture_placeholders(
        &mut response,
        &fixture,
        &[
            "payload.project.id",
            "payload.project.repoPath",
            "payload.project.createdAt",
            "payload.project.updatedAt",
            "payload.mainWorkspace.id",
            "payload.mainWorkspace.instanceId",
            "payload.mainWorkspace.projectId",
            "payload.mainWorkspace.path",
            "payload.mainWorkspace.createdAt",
            "payload.mainWorkspace.updatedAt",
        ],
    );
    assert_eq!(response, fixture);
}

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

    let (mut response, events) = exchange(
        &mut actor,
        1,
        &request,
        &mut receiver,
        &mut commands,
    )
    .await;

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

    let (mut response, events) = exchange(
        &mut actor,
        1,
        &request,
        &mut receiver,
        &mut commands,
    )
    .await;

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
    assert!(
        std::fs::read_to_string(&script)
            .unwrap()
            .contains("echo fixture")
    );

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

#[tokio::test]
async fn host_busy_response_matches_shared_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::from([(
            "session-1".to_string(),
            Session::driver_test_stub("session-1", 80, 24),
        )]),
    )
    .await;
    let (inbox, mut commands) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;

    let (response, events) = exchange(
        &mut actor,
        1,
        &json!({"id": 26, "type": "host.shutdown", "payload": {}}),
        &mut receiver,
        &mut commands,
    )
    .await;

    assert!(events.is_empty());
    assert_eq!(response, wire_fixture("response.error.host_busy.json"));
}

#[tokio::test]
async fn runtime_mutation_busy_response_matches_shared_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let (inbox, mut commands) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;
    // Park the queue so the serialized `tab.remove` stays in flight instead of
    // spawning a mutation task; any conflicting request now hits the barrier.
    actor.park_runtime_mutations();
    actor
        .handle_line(
            1,
            json!({"id": 30, "type": "tab.remove", "payload": {"id": "missing-tab"}})
                .to_string(),
        )
        .await;

    let (response, events) = exchange(
        &mut actor,
        1,
        &json!({"id": 0, "type": "tab.upsert", "payload": {}}),
        &mut receiver,
        &mut commands,
    )
    .await;

    assert!(events.is_empty());
    assert_eq!(
        response,
        wire_fixture("response.error.runtime_mutation_busy.json")
    );
    assert!(actor.mutation_queue.has_runtime_mutations());
}

#[tokio::test]
async fn tab_not_found_response_matches_shared_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, mobile_client(handle, "phone"))]),
        HashMap::new(),
    )
    .await;
    let (inbox, mut commands) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;

    let (response, events) = exchange(
        &mut actor,
        1,
        &json!({"id": 0, "type": "terminal.attach", "payload": {"tabId": "terminal-1"}}),
        &mut receiver,
        &mut commands,
    )
    .await;

    assert!(events.is_empty());
    assert_eq!(response, wire_fixture("response.error.tab_not_found.json"));
}

#[tokio::test]
async fn workspace_not_found_response_matches_shared_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, mobile_client(handle, "phone"))]),
        HashMap::new(),
    )
    .await;
    let (inbox, mut commands) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;

    let (response, events) = exchange(
        &mut actor,
        1,
        &json!({"id": 0, "type": "terminal.create", "payload": {"workspaceId": "workspace-1"}}),
        &mut receiver,
        &mut commands,
    )
    .await;

    assert!(events.is_empty());
    assert_eq!(
        response,
        wire_fixture("response.error.workspace_not_found.json")
    );
}

#[tokio::test]
async fn legacy_mobile_view_prefs_request_is_accepted_and_backfilled() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, mobile_client(handle, "phone"))]),
        HashMap::new(),
    )
    .await;
    let (inbox, mut commands) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;
    // The fixture's expectedRevision: 7 only passes the mobile revision check
    // when the shared record already sits at 7.
    for _ in 0..7 {
        actor
            .runtime_store
            .update_shared_workbench_view_prefs(
                SharedWorkbenchViewPrefs::default(),
                None,
                SharedWorkbenchPrefsWriter::Runtime,
            )
            .await
            .unwrap();
    }
    let request = wire_fixture("request.workbench_view_prefs.update.mobile.legacy.json");

    let (response, events) = exchange(&mut actor, 1, &request, &mut receiver, &mut commands).await;

    assert_eq!(
        events,
        vec![json!({"event": "workbenchViewPrefsChanged", "payload": {}})]
    );
    assert_eq!(response["id"], 7);
    assert_eq!(response["ok"], true);
    let payload = &response["payload"];
    assert_eq!(payload["revision"], 8);
    assert_eq!(payload["lastWriter"], "mobile");
    // The legacy payload omits the section fields; the host backfills them
    // from the stored record before applying the update.
    let mut expected_prefs = request["payload"]["prefs"].clone();
    expected_prefs["sectionSort"] = json!("name");
    expected_prefs["collapsedSectionIds"] = json!([]);
    expected_prefs["othersSectionCollapsed"] = json!(false);
    assert_eq!(payload["prefs"], expected_prefs);
}

/// `*.legacy` fixtures pin payload shapes the current host no longer emits;
/// they stay valid decode targets for the Dart clients without being rewritten
/// to the modern frame.
#[test]
fn legacy_response_fixtures_keep_their_historical_shape() {
    let busy = wire_fixture("response.error.host_busy.legacy.json");
    assert_eq!(busy["ok"], false);
    assert!(busy["error"]
        .as_str()
        .expect("error message")
        .contains("active terminal session(s)"));

    let created = wire_fixture("response.ok.workspace_create_managed.legacy.json");
    assert_eq!(created["ok"], true);
    let workspace = &created["payload"]["workspace"];
    // The slim historical workspace only carried the fields the client's
    // tolerant decoder still requires; instanceId/hostId/kind/status arrived
    // later and stay absent here on purpose.
    for key in ["id", "projectId", "name", "path", "createdAt", "updatedAt"] {
        assert!(workspace.get(key).is_some(), "legacy workspace is missing {key}");
    }
}
