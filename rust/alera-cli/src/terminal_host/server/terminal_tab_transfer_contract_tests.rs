use std::collections::HashMap;

use alera_core::runtime::{WorkbenchLayoutRecord, WorkspaceTabRecord};
use chrono::Utc;
use serde_json::{json, Value};

use crate::project_management::register_project;
use crate::terminal_host::client::ClientHandle;
use crate::terminal_host::session::Session;

use super::super::actor_test_harness::{local_client, test_actor};
use super::invalid_transfer_receipt_marker;

fn layout(workspace: &str, tab_ids: &[&str]) -> Value {
    let group = format!("{workspace}/main");
    let mut data = json!({
        "workspaceId": workspace,
        "root": {"type": "leaf", "groupId": group},
        "activeGroupId": group,
        "groups": {}
    });
    data["groups"][&group] = json!({
        "id": group,
        "tabIds": tab_ids,
        "activeTabId": tab_ids.last().copied()
    });
    data
}

#[test]
fn transferred_terminal_rejects_malformed_owner_generation_and_exit_evidence() {
    let timestamp = Utc::now();
    let token = format!("{}:23", uuid::Uuid::new_v4());
    let make = |owner: &str, from: &str, to: &str, token: &str, exit: Option<Value>| {
        let mut payload = json!({
            "terminalSessionId": "session-moving",
            "lastTerminalTabTransfer": {
                "id": "move-1",
                "from": from,
                "to": to,
                "ptyInstanceToken": token,
            }
        });
        if let Some(exit) = exit {
            payload["lastTerminalTabExit"] = exit;
        }
        WorkspaceTabRecord {
            id: "tab-moving".to_string(),
            workspace_id: owner.to_string(),
            kind: "terminal".to_string(),
            title: "Terminal".to_string(),
            created_at: timestamp,
            updated_at: timestamp,
            payload,
        }
    };
    let valid_exit = json!({
        "transferMutationId": "move-1",
        "sessionId": "session-moving",
        "workspaceId": "target",
        "ptyInstanceToken": token,
        "exitCode": 0,
        "observedAtUtc": timestamp.to_rfc3339()
    });
    assert!(!invalid_transfer_receipt_marker(&make(
        "target", "source", "target", &token, None
    )));
    assert!(!invalid_transfer_receipt_marker(&make(
        "target",
        "source",
        "target",
        &token,
        Some(valid_exit.clone())
    )));
    assert!(invalid_transfer_receipt_marker(&make(
        "wrong-owner",
        "source",
        "target",
        &token,
        None
    )));
    assert!(invalid_transfer_receipt_marker(&make(
        "target", "target", "target", &token, None
    )));
    assert!(invalid_transfer_receipt_marker(&make(
        "target",
        "source",
        "target",
        "invalid-generation:23",
        None
    )));
    let mut bad_exit = valid_exit;
    bad_exit["observedAtUtc"] = json!("not-a-timestamp");
    assert!(invalid_transfer_receipt_marker(&make(
        "target",
        "source",
        "target",
        &token,
        Some(bad_exit)
    )));
}

#[tokio::test]
async fn existing_pty_moves_atomically_but_unknown_second_view_rejects_it() {
    let dir = tempfile::tempdir().unwrap();
    let source_repo = tempfile::tempdir().unwrap();
    let target_repo = tempfile::tempdir().unwrap();
    let (handle, _replies) = ClientHandle::test_channels();
    let mut clients = HashMap::new();
    clients.insert(1, local_client(handle));
    let mut actor = test_actor(&dir, clients, HashMap::new()).await;
    let source_project = register_project(
        &actor.runtime_store,
        source_repo.path().to_str().unwrap(),
        Some("Source"),
    )
    .await
    .unwrap();
    let target_project = register_project(
        &actor.runtime_store,
        target_repo.path().to_str().unwrap(),
        Some("Target"),
    )
    .await
    .unwrap();
    let source = source_project.main_workspace.id;
    let target = target_project.main_workspace.id;
    let now = Utc::now();
    actor
        .runtime_store
        .upsert_workspace_tab(WorkspaceTabRecord {
            id: "tab-moving".to_string(),
            workspace_id: source.clone(),
            kind: "terminal".to_string(),
            title: "Original session".to_string(),
            created_at: now,
            updated_at: now,
            payload: json!({"terminalSessionId": "session-moving"}),
        })
        .await
        .unwrap();
    let before_source = layout(&source, &["tab-moving"]);
    let before_target = layout(&target, &[]);
    for (workspace_id, data) in [
        (source.clone(), before_source.clone()),
        (target.clone(), before_target.clone()),
    ] {
        actor
            .runtime_store
            .upsert_workbench_layout(WorkbenchLayoutRecord { workspace_id, data })
            .await
            .unwrap();
    }
    let mut session = Session::driver_test_stub("session-moving", 80, 24);
    session.workspace_id = source.clone();
    session.tab_id = "tab-moving".to_string();
    session.attach(1);
    assert!(session.register_view_lease(1, "original-window"));
    let original_instance = session.instance_id();
    session.attach(2);
    session.mark_unscoped_attachment(2);
    actor.sessions.insert("session-moving".to_string(), session);
    let transfer = json!({
        "tabId": "tab-moving",
        "sessionId": "session-moving",
        "sourceWorkspaceId": source,
        "targetWorkspaceId": target,
        "clientMutationId": "move-one",
        "expectedSourceLayout": before_source,
        "expectedTargetLayout": before_target,
        "sourceLayout": layout(&source, &[]),
        "targetLayout": layout(&target, &["tab-moving"])
    });
    assert!(actor.transfer_terminal_tab(1, &transfer).await.is_err());
    assert_eq!(
        actor
            .runtime_store
            .find_workspace_tab("tab-moving")
            .await
            .unwrap()
            .unwrap()
            .workspace_id,
        source
    );
    actor.sessions.get_mut("session-moving").unwrap().detach(2);
    actor.transfer_terminal_tab(1, &transfer).await.unwrap();
    let live = actor.sessions.get("session-moving").unwrap();
    assert!(live.running());
    assert_eq!(live.instance_id(), original_instance);
    assert_eq!(live.workspace_id, target);
    assert!(live.has_view_lease(1, "original-window"));
    let moved = actor
        .runtime_store
        .find_workspace_tab("tab-moving")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(moved.workspace_id, target);
    assert_eq!(moved.payload["lastTerminalTabTransfer"]["id"], "move-one");
    let status = actor
        .terminal_tab_transfer_status(
            1,
            &json!({
                "tabId": "tab-moving",
                "sessionId": "session-moving",
                "sourceWorkspaceId": source,
                "targetWorkspaceId": target,
                "clientMutationId": "move-one"
            }),
        )
        .await
        .unwrap();
    assert_eq!(status["applied"], true);

    // Simulate the narrow commit/PTY-loss window: the persisted receipt and
    // both layouts survived, but this actor no longer owns the original
    // process. A matching mutation must NOT be acknowledged from SQLite
    // alone, and CreateOrAttach must not remint a new shell under the old PTY
    // identity. Preserve the durable owner for manual recovery.
    actor.sessions.remove("session-moving");
    let unresolved = actor
        .terminal_tab_transfer_status(
            1,
            &json!({
                "tabId": "tab-moving",
                "sessionId": "session-moving",
                "sourceWorkspaceId": source,
                "targetWorkspaceId": target,
                "clientMutationId": "move-one"
            }),
        )
        .await
        .unwrap();
    assert_eq!(unresolved["applied"], false);
    assert_eq!(unresolved["recoveryRequired"], true);
    assert_eq!(unresolved["reason"], "pty_missing");
    assert!(actor
        .verify_terminal_attachment_owner("session-moving", &target, "tab-moving")
        .await
        .is_err());
    assert_eq!(
        actor
            .runtime_store
            .find_workspace_tab("tab-moving")
            .await
            .unwrap()
            .unwrap()
            .payload["lastTerminalTabTransfer"]["id"],
        "move-one"
    );
}
