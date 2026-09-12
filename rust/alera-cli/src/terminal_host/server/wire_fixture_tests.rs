//! Golden wire-contract fixtures shared with the Flutter clients.
//!
//! The JSON documents under `test/fixtures/wire/` are the cross-language
//! contract: the host must produce exactly these frames and both Dart clients
//! must decode them. Comparing whole `serde_json::Value`s keeps the check
//! field-for-field while remaining insensitive to key order or whitespace.

use std::collections::HashMap;
use std::path::PathBuf;

use alera_core::runtime::{AgentProfile, AgentProfileLaunchMode, WorkspaceTabRecord};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::terminal_host::client::{ClientFrame, ClientHandle};
use crate::terminal_host::protocol::PROTOCOL_VERSION;

use super::actor_test_harness::{local_client, mobile_client, test_actor};
use super::ServerActor;

fn wire_fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/fixtures/wire")
        .join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The stored record behind the tab fixtures. Host-owned recovery keys
/// (`initialPrompt`, `pendingAgentPrompt`, `agentTitleStateV1`) and the
/// `generating` title status exist only in storage; the client projections in
/// the fixtures show what is supposed to survive onto the wire.
fn stored_tab() -> WorkspaceTabRecord {
    let at: DateTime<Utc> = "2026-07-27T12:34:56.789Z".parse().unwrap();
    WorkspaceTabRecord {
        id: "terminal-1".to_string(),
        workspace_id: "workspace-1".to_string(),
        kind: "terminal".to_string(),
        title: "Flutter".to_string(),
        created_at: at,
        updated_at: at,
        payload: json!({
            "agentProfileLaunchV1": {
                "version": 1,
                "launch": {"kind": "command", "command": "fx"},
            },
            "initialPrompt": "durable bootstrap",
            "pendingAgentPrompt": {"agent": "fx", "prompt": "durable bootstrap"},
            "agentTitleStateV1": {
                "conversationId": "conv-1",
                "agent": null,
                "nativeId": null,
                "retired": [],
                "retiredPrompts": {},
                "initialPrompt": "durable bootstrap",
                "cursor": 3,
                "eligible": true,
                "attempted": true,
                "closed": false,
            },
            "agentTitleConversationId": "conv-1",
            "agentTitleRevision": "rev-1",
            "agentTitleStatus": "generating",
            "agentTitleSource": "agent",
            "terminalPulse": {"command": "r", "appendEnter": true, "delayMs": 2000},
            "shell": "zsh",
        }),
    }
}

/// Sends one request frame and returns the response plus any event frames the
/// request pushed ahead of it: broadcasts share the client's lane, so a
/// mutation's change event always lands before its response.
async fn exchange(
    actor: &mut ServerActor,
    client_id: u64,
    request: &Value,
    receiver: &mut UnboundedReceiver<ClientFrame>,
) -> (Value, Vec<Value>) {
    let request_id = request["id"].as_i64().expect("request id");
    actor.handle_line(client_id, request.to_string()).await;
    let mut events = Vec::new();
    loop {
        let frame = receiver
            .recv()
            .await
            .expect("the host should write a response")
            .as_json()
            .expect("the frame should be JSON");
        if frame["id"] == request_id {
            return (frame, events);
        }
        events.push(frame);
    }
}

async fn next_frame(receiver: &mut UnboundedReceiver<ClientFrame>) -> Value {
    receiver
        .recv()
        .await
        .expect("the host should emit an event")
        .as_json()
        .expect("the frame should be JSON")
}

#[tokio::test]
async fn hello_response_matches_shared_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;

    let (response, events) = exchange(
        &mut actor,
        1,
        &json!({
            "id": 0,
            "type": "hello",
            "payload": {
                "protocolVersion": PROTOCOL_VERSION,
                "token": "token",
                "clientKind": "app",
            },
        }),
        &mut receiver,
    )
    .await;

    assert!(events.is_empty());
    assert_eq!(response, wire_fixture("response.ok.hello.json"));
}

#[tokio::test]
async fn unauthenticated_response_matches_shared_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut client = local_client(handle);
    client.authenticated = false;
    let mut actor = test_actor(&dir, HashMap::from([(1, client)]), HashMap::new()).await;

    let (response, events) = exchange(
        &mut actor,
        1,
        &json!({"id": 1, "type": "tab.list", "payload": {"workspaceId": "workspace-1"}}),
        &mut receiver,
    )
    .await;

    assert!(events.is_empty());
    assert_eq!(response, wire_fixture("response.error.unauthenticated.json"));
}

#[tokio::test]
async fn format_error_response_matches_shared_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;

    let (response, events) = exchange(
        &mut actor,
        1,
        &json!({"id": 1, "type": "tab.list", "payload": {}}),
        &mut receiver,
    )
    .await;

    assert!(events.is_empty());
    assert_eq!(response, wire_fixture("response.error.format.json"));
}

#[tokio::test]
async fn typed_conflict_response_matches_shared_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let now: DateTime<Utc> = "2026-07-27T12:34:56.789Z".parse().unwrap();
    actor
        .runtime_store
        .upsert_agent_profile(
            AgentProfile {
                id: "profile-1".to_string(),
                name: "Fixture Profile".to_string(),
                sort_order: 0,
                agent_type: "claude".to_string(),
                command: "fx".to_string(),
                launch_mode: AgentProfileLaunchMode::Command,
                managed_config: None,
                custom_prompt: String::new(),
                description: String::new(),
                quota_group: None,
                revision: 0,
                created_at: now,
                updated_at: now,
            },
            None,
        )
        .await
        .unwrap();
    let request = wire_fixture("request.agent_profile_remove.conflict.json");

    let (response, events) = exchange(&mut actor, 1, &request, &mut receiver).await;

    assert!(events.is_empty());
    assert_eq!(response, wire_fixture("response.error.conflict.json"));
    assert_eq!(response["errorCode"], "agent_profile_revision_conflict");
}

#[tokio::test]
async fn tab_list_projections_match_shared_fixtures() {
    let dir = tempfile::tempdir().unwrap();
    let (local_handle, mut local_rx) = ClientHandle::test_channels();
    let (mobile_handle, mut mobile_rx) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([
            (1, local_client(local_handle)),
            (2, mobile_client(mobile_handle, "phone")),
        ]),
        HashMap::new(),
    )
    .await;
    actor
        .runtime_store
        .upsert_workspace_tab(stored_tab())
        .await
        .unwrap();

    let (desktop, _) = exchange(
        &mut actor,
        1,
        &json!({"id": 1, "type": "tab.list", "payload": {"workspaceId": "workspace-1"}}),
        &mut local_rx,
    )
    .await;
    let (mobile, _) = exchange(
        &mut actor,
        2,
        &json!({"id": 1, "type": "tab.list", "payload": {"workspaceId": "workspace-1"}}),
        &mut mobile_rx,
    )
    .await;

    assert_eq!(desktop, wire_fixture("response.tab.list.desktop.json"));
    assert_eq!(mobile, wire_fixture("response.tab.list.mobile.json"));
}

#[tokio::test]
async fn tab_find_projections_match_shared_fixtures() {
    let dir = tempfile::tempdir().unwrap();
    let (local_handle, mut local_rx) = ClientHandle::test_channels();
    let (mobile_handle, mut mobile_rx) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([
            (1, local_client(local_handle)),
            (2, mobile_client(mobile_handle, "phone")),
        ]),
        HashMap::new(),
    )
    .await;
    actor
        .runtime_store
        .upsert_workspace_tab(stored_tab())
        .await
        .unwrap();

    let (desktop, _) = exchange(
        &mut actor,
        1,
        &json!({"id": 1, "type": "tab.find", "payload": {"id": "terminal-1"}}),
        &mut local_rx,
    )
    .await;
    let (mobile, _) = exchange(
        &mut actor,
        2,
        &json!({"id": 1, "type": "tab.find", "payload": {"id": "terminal-1"}}),
        &mut mobile_rx,
    )
    .await;
    let (missing, _) = exchange(
        &mut actor,
        1,
        &json!({"id": 1, "type": "tab.find", "payload": {"id": "missing-1"}}),
        &mut local_rx,
    )
    .await;

    assert_eq!(desktop, wire_fixture("response.tab.find.desktop.json"));
    assert_eq!(mobile, wire_fixture("response.tab.find.mobile.json"));
    assert_eq!(missing, wire_fixture("response.tab.find.missing.json"));
}

#[tokio::test]
async fn tab_upsert_round_trip_matches_shared_fixtures() {
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
        .upsert_workspace_tab(stored_tab())
        .await
        .unwrap();
    let request = wire_fixture("request.tab.upsert.json");

    let (response, events) = exchange(&mut actor, 1, &request, &mut receiver).await;

    assert_eq!(response, wire_fixture("response.tab.upsert.desktop.json"));
    assert_eq!(events, vec![wire_fixture("event.workspace_tabs_changed.json")]);
    // Projection redaction is wire-only: storage must keep the recovery keys
    // the client could never have sent back.
    let stored = actor
        .runtime_store
        .find_workspace_tab("terminal-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.title, "Flutter (renamed)");
    assert_eq!(stored.payload["initialPrompt"], "durable bootstrap");
    assert_eq!(
        stored.payload["pendingAgentPrompt"]["prompt"],
        "durable bootstrap"
    );
    assert_eq!(stored.payload["agentTitleStateV1"]["cursor"], 3);
}

#[tokio::test]
async fn mobile_tab_upsert_denial_matches_shared_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, mobile_client(handle, "phone"))]),
        HashMap::new(),
    )
    .await;
    let request = wire_fixture("request.tab.upsert.json");

    let (response, events) = exchange(&mut actor, 1, &request, &mut receiver).await;

    assert!(events.is_empty());
    assert_eq!(
        response,
        wire_fixture("response.error.mobile_tab_upsert_denied.json")
    );
}

#[tokio::test]
async fn mobile_workbench_view_prefs_conflict_matches_shared_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, mobile_client(handle, "phone"))]),
        HashMap::new(),
    )
    .await;
    let request = wire_fixture("request.workbench_view_prefs.update.mobile.json");

    let (response, events) = exchange(&mut actor, 1, &request, &mut receiver).await;

    assert!(events.is_empty());
    // This conflict is deliberately pinned as an untyped state error: the
    // store reports it as RuntimeStoreError::Message and the request path maps
    // it through state_error, so no errorCode/errorDetails reach the wire.
    assert_eq!(
        response,
        wire_fixture("response.error.mobile_prefs_conflict.json")
    );
}

#[tokio::test]
async fn scoped_change_events_match_shared_fixtures() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut receiver) = ClientHandle::test_channels();
    let actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;

    actor.broadcast_workspaces_changed(Some("project-1"));
    actor.broadcast_workspaces_changed(None);
    actor.broadcast_workspace_tabs_changed(Some("workspace-1"));
    actor.handle_project_clone_changed("job-1".to_string());

    assert_eq!(
        next_frame(&mut receiver).await,
        wire_fixture("event.workspaces_changed.json")
    );
    assert_eq!(
        next_frame(&mut receiver).await,
        wire_fixture("event.workspaces_changed.wildcard.json")
    );
    assert_eq!(
        next_frame(&mut receiver).await,
        wire_fixture("event.workspace_tabs_changed.json")
    );
    assert_eq!(
        next_frame(&mut receiver).await,
        wire_fixture("event.project_clone_jobs_changed.json")
    );
}
