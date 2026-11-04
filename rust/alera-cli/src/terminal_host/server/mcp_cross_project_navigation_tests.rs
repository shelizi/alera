//! Host authorization and UI receipt tests for cross-project MCP navigation.
//! Navigation cannot borrow another project's PTY identity or bypass App ACK.
use std::collections::HashMap;

use alera_core::runtime::{Project, ProjectKind, Workspace, WorkspaceTabRecord};
use chrono::Utc;
use serde_json::json;
use tokio::sync::mpsc;

use crate::terminal_host::client::{ClientFrame, ClientHandle};
use crate::terminal_host::session::Session;

use super::actor_test_harness::test_actor;
use super::{ClientState, ServerActor};

async fn navigation_actor(
    dir: &tempfile::TempDir,
) -> (ServerActor, mpsc::UnboundedReceiver<ClientFrame>, String) {
    let (app_tx, app_rx) = mpsc::unbounded_channel();
    let (app_pty, _) = mpsc::channel(4);
    let (cli_tx, _) = mpsc::unbounded_channel();
    let (cli_pty, _) = mpsc::channel(4);
    let mut actor = test_actor(
        dir,
        HashMap::from([
            (
                1,
                ClientState::local(ClientHandle::new(app_tx, app_pty), true),
            ),
            (
                2,
                ClientState::local(ClientHandle::new(cli_tx, cli_pty), false),
            ),
        ]),
        HashMap::from([(
            "source-agent".to_owned(),
            Session::driver_test_stub("source-agent", 80, 24),
        )]),
    )
    .await;
    let now = Utc::now();
    actor
        .runtime_store
        .upsert_project(Project {
            id: "project-b".into(),
            name: "Other Project".into(),
            repo_path: dir.path().to_string_lossy().into_owned(),
            created_at: now,
            updated_at: now,
            kind: ProjectKind::Folder,
        })
        .await
        .unwrap();
    let workspace: Workspace = serde_json::from_value(json!({
        "id":"workspace-b", "projectId":"project-b",
        "instanceId":"instance-b", "hostId":"local",
        "name":"Main", "path":dir.path().to_string_lossy(),
        "createdAt":now, "updatedAt":now,
        "kind":"main", "status":"active", "reusesExistingBranch":false
    }))
    .unwrap();
    actor
        .runtime_store
        .upsert_workspace(workspace)
        .await
        .unwrap();
    actor
        .runtime_store
        .upsert_workspace_tab(WorkspaceTabRecord {
            id: "tab-b".into(),
            workspace_id: "workspace-b".into(),
            kind: "terminal".into(),
            title: "Other Terminal".into(),
            created_at: now,
            updated_at: now,
            payload: json!({"terminalSessionId":"foreign-terminal"}),
        })
        .await
        .unwrap();
    let token = actor.mcp_ui.mint_token("source-agent");
    actor
        .handle_mcp_ui_request(1, "uiControl.register", &json!({"windowId":"main"}))
        .await
        .unwrap();
    (actor, app_rx, token)
}

#[tokio::test]
async fn cross_project_navigation_is_authorized_only_for_matching_active_project_and_tab() {
    let dir = tempfile::tempdir().unwrap();
    let (mut actor, mut app_events, token) = navigation_actor(&dir).await;
    let mut request = json!({
        "sessionId":"source-agent", "workspaceId":"workspace",
        "capability":token, "action":"select_tab",
        "arguments":{
            "workspaceId":"workspace-b","projectId":"project-b","tabId":"tab-b"
        }
    });
    let accepted = actor
        .handle_mcp_ui_request(2, "uiControl.request", &request)
        .await
        .unwrap();
    assert_eq!(accepted["status"], "accepted");
    let delivered = app_events.try_recv().unwrap().as_json().unwrap();
    assert_eq!(delivered["payload"]["workspaceId"], "workspace-b");
    assert_eq!(delivered["payload"]["arguments"]["tabId"], "tab-b");
    assert_eq!(
        actor.sessions.len(),
        1,
        "navigation must never spawn a target PTY"
    );

    let request_id = accepted["requestId"].as_str().unwrap();
    assert!(
        actor
            .handle_mcp_ui_request(
                2,
                "uiControl.complete",
                &json!({"requestId":request_id,"applied":true,"tabId":"tab-b"})
            )
            .await
            .is_err(),
        "a CLI cannot acknowledge any window command"
    );
    assert!(
        actor
            .handle_mcp_ui_request(
                1,
                "uiControl.complete",
                &json!({"requestId":request_id,"applied":true,"tabId":"wrong-tab"})
            )
            .await
            .is_err(),
        "a trusted App cannot acknowledge a different Tab"
    );
    actor
        .handle_mcp_ui_request(
            1,
            "uiControl.complete",
            &json!({"requestId":request_id,"applied":true,"tabId":"tab-b"}),
        )
        .await
        .unwrap();
    let status = actor
        .handle_mcp_ui_request(
            2,
            "uiControl.status",
            &json!({
                "sessionId":"source-agent","workspaceId":"workspace",
                "capability":token,"requestId":request_id
            }),
        )
        .await
        .unwrap();
    assert_eq!(status["applied"], true);
    assert_eq!(status["tabId"], "tab-b");
    assert_eq!(status["workspaceId"], "workspace-b");

    request["arguments"]["projectId"] = json!("forged-project");
    assert!(actor
        .handle_mcp_ui_request(2, "uiControl.request", &request)
        .await
        .is_err());
    request["arguments"]["projectId"] = json!("project-b");
    request["arguments"]["tabId"] = json!("nonexistent-tab");
    assert!(actor
        .handle_mcp_ui_request(2, "uiControl.request", &request)
        .await
        .is_err());
    request["arguments"]["tabId"] = json!("tab-b");
    request["arguments"]
        .as_object_mut()
        .unwrap()
        .remove("projectId");
    assert!(
        actor
            .handle_mcp_ui_request(2, "uiControl.request", &request)
            .await
            .is_err(),
        "cross-workspace navigation needs explicit project identity"
    );
}

#[tokio::test]
async fn cross_project_open_file_and_focus_are_ui_only_and_archived_targets_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let (mut actor, mut app_events, token) = navigation_actor(&dir).await;
    let mut request = json!({
        "sessionId":"source-agent","workspaceId":"workspace","capability":token,
        "action":"open_file",
        "arguments":{
            "relativePath":"lib/main.dart","workspaceId":"workspace-b",
            "projectId":"project-b","openMode":"new"
        }
    });
    actor
        .handle_mcp_ui_request(2, "uiControl.request", &request)
        .await
        .unwrap();
    let event = app_events.try_recv().unwrap().as_json().unwrap();
    assert_eq!(event["payload"]["workspaceId"], "workspace-b");
    assert_eq!(event["payload"]["arguments"]["openMode"], "new");
    assert_eq!(actor.sessions.len(), 1);
    request["action"] = json!("focus_workspace");
    request["arguments"] = json!({"workspaceId":"workspace-b","projectId":"project-b"});
    actor
        .handle_mcp_ui_request(2, "uiControl.request", &request)
        .await
        .unwrap();
    assert_eq!(
        app_events.try_recv().unwrap().as_json().unwrap()["payload"]["workspaceId"],
        "workspace-b"
    );
    request["action"] = json!("focus_agent");
    request["arguments"] =
        json!({"handle":"foreign-terminal","workspaceId":"workspace-b","projectId":"project-b"});
    assert!(
        actor
            .handle_mcp_ui_request(2, "uiControl.request", &request)
            .await
            .is_err(),
        "UI navigation cannot expand the Agent-control schema"
    );

    let mut archived = actor
        .runtime_store
        .find_workspace("workspace-b")
        .await
        .unwrap()
        .unwrap();
    archived.archived_at = Some(Utc::now());
    actor
        .runtime_store
        .upsert_workspace(archived)
        .await
        .unwrap();
    request["action"] = json!("focus_workspace");
    request["arguments"] = json!({"workspaceId":"workspace-b","projectId":"project-b"});
    assert!(
        actor
            .handle_mcp_ui_request(2, "uiControl.request", &request)
            .await
            .is_err(),
        "archived workspace cannot be activated by MCP"
    );
}
