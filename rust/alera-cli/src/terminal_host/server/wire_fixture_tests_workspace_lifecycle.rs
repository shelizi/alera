//! Live verification of the workspace create, rename, and remove event order.

use std::collections::HashMap;

use alera_core::runtime::{Project, ProjectKind};
use serde_json::json;

use crate::terminal_host::client::ClientHandle;

use super::super::actor_test_harness::{local_client, test_actor};
use super::super::wire_fixture_tests_lifecycle::{assert_fixture, exchange, wire_fixture};
use super::super::wire_fixture_tests_requests::exchange as deferred_exchange;

#[tokio::test]
async fn workspace_lifecycle_events_and_responses_match_shared_fixtures() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    actor
        .runtime_store
        .upsert_project(Project {
            id: "project-1".to_string(),
            name: "Fixture Project".to_string(),
            repo_path: dir.path().join("repo").to_string_lossy().into_owned(),
            created_at: "2000-01-01T00:00:01Z".parse().unwrap(),
            updated_at: "2000-01-01T00:00:01Z".parse().unwrap(),
            kind: ProjectKind::Folder,
        })
        .await
        .unwrap();

    let path = dir.path().join("workspace-lifecycle");
    let mut create = wire_fixture("request.workspace_lifecycle_create.json");
    create["payload"]["path"] = json!(path.to_string_lossy());
    let (create_response, create_events, create_markers) =
        exchange(&mut actor, 1, &create, &mut receiver).await;
    assert_fixture(
        &create_response,
        &wire_fixture("response.ok.workspace_lifecycle_create.json"),
    );
    assert_eq!(create_events.len(), 1);
    assert_fixture(
        &create_events[0],
        &wire_fixture("event.workspaces_changed.workspace_lifecycle_scoped.json"),
    );
    assert!(create_markers.is_empty());

    let rename = wire_fixture("request.workspace_lifecycle_rename.json");
    let (rename_response, rename_events, rename_markers) =
        exchange(&mut actor, 1, &rename, &mut receiver).await;
    assert_fixture(
        &rename_response,
        &wire_fixture("response.ok.workspace_lifecycle_rename.json"),
    );
    assert_eq!(rename_events.len(), 1);
    assert_fixture(
        &rename_events[0],
        &wire_fixture("event.workspaces_changed.workspace_lifecycle_rename.json"),
    );
    assert!(rename_markers.is_empty());

    let (inbox, mut commands) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;
    let remove = wire_fixture("request.workspace_lifecycle_remove.json");
    let (remove_response, remove_events) =
        deferred_exchange(&mut actor, 1, &remove, &mut receiver, &mut commands).await;
    assert_fixture(
        &remove_response,
        &wire_fixture("response.ok.workspace_lifecycle_remove.json"),
    );
    assert_eq!(remove_events.len(), 2);
    assert_fixture(
        &remove_events[0],
        &wire_fixture("event.workspaces_changed.workspace_lifecycle_remove.json"),
    );
    assert_fixture(
        &remove_events[1],
        &wire_fixture("event.workspace_tabs_changed.workspace_lifecycle_remove.json"),
    );
}
