use std::collections::HashMap;

use alera_core::runtime::{WorkbenchLayoutRecord, WorkspaceTabRecord};
use chrono::Utc;
use serde_json::{json, Value};

use crate::project_management::register_project;
use crate::terminal_host::client::ClientHandle;
use crate::terminal_host::session::Session;

use super::super::actor_test_harness::{local_client, test_actor};
use super::super::runtime_mutations::RuntimeMutationRequest;
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

    // Another native window may share the same Host but holds its own
    // original View lease. Neither explicit termination nor Tab removal
    // can close the PTY while that window remains attached.
    let live = actor.sessions.get_mut("session-moving").unwrap();
    live.attach(2);
    assert!(live.register_view_lease(2, "second-window"));
    let close = json!({
        "expectedWorkspaceId": target,
        "expectedTabId": "tab-moving",
        "expectedTransferMutationId": "move-one",
        "expectedViewLeaseId": "original-window"
    });
    assert!(actor
        .verify_terminal_termination_owner(1, "session-moving", &close)
        .await
        .is_err());
    let remove = RuntimeMutationRequest::RemoveTab {
        tab_id: "tab-moving".to_string(),
        expected_workspace_id: Some(target.clone()),
        expected_transfer_mutation_id: Some("move-one".to_string()),
        expected_view_lease_id: Some("original-window".to_string()),
    };
    assert!(actor.prepare_runtime_mutation(1, &remove).await.is_err());
    assert!(actor
        .sessions
        .get_mut("session-moving")
        .unwrap()
        .release_view_lease(2, "second-window"));
    assert!(actor.prepare_runtime_mutation(1, &remove).await.is_ok());

    // Undo is a real reverse transaction, never a close+remint. The same
    // original process instance, view token and Session must survive both
    // legs and the original mutation must become stale.
    let reverse = json!({
        "tabId": "tab-moving",
        "sessionId": "session-moving",
        "sourceWorkspaceId": target,
        "targetWorkspaceId": source,
        "clientMutationId": "move-back",
        "expectedSourceLayout": layout(&target, &["tab-moving"]),
        "expectedTargetLayout": layout(&source, &[]),
        "sourceLayout": layout(&target, &[]),
        "targetLayout": layout(&source, &["tab-moving"])
    });
    actor.transfer_terminal_tab(1, &reverse).await.unwrap();
    let back = actor.sessions.get("session-moving").unwrap();
    assert_eq!(back.instance_id(), original_instance);
    assert_eq!(back.workspace_id, source);
    assert!(back.has_view_lease(1, "original-window"));
    assert!(back.running());
    let reverse_status = actor
        .terminal_tab_transfer_status(
            1,
            &json!({
                "tabId": "tab-moving", "sessionId": "session-moving",
                "sourceWorkspaceId": target, "targetWorkspaceId": source,
                "clientMutationId": "move-back"
            }),
        )
        .await
        .unwrap();
    assert_eq!(reverse_status["applied"], true);
    let stale = actor
        .terminal_tab_transfer_status(
            1,
            &json!({
                "tabId": "tab-moving", "sessionId": "session-moving",
                "sourceWorkspaceId": source, "targetWorkspaceId": target,
                "clientMutationId": "move-one"
            }),
        )
        .await
        .unwrap();
    assert_eq!(stale["applied"], false);
    assert_eq!(stale["recoveryRequired"], true);
    assert_eq!(stale["reason"], "receipt_conflict");

    let return_to_target = json!({
        "tabId": "tab-moving", "sessionId": "session-moving",
        "sourceWorkspaceId": source, "targetWorkspaceId": target,
        "clientMutationId": "move-two",
        "expectedSourceLayout": layout(&source, &["tab-moving"]),
        "expectedTargetLayout": layout(&target, &[]),
        "sourceLayout": layout(&source, &[]),
        "targetLayout": layout(&target, &["tab-moving"])
    });
    actor
        .transfer_terminal_tab(1, &return_to_target)
        .await
        .unwrap();
    assert_eq!(
        actor.sessions.get("session-moving").unwrap().instance_id(),
        original_instance
    );
    assert_eq!(
        actor.sessions.get("session-moving").unwrap().workspace_id,
        target
    );

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
                "clientMutationId": "move-two"
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
        "move-two"
    );
}

#[tokio::test]
async fn live_moved_pty_cannot_be_retired_when_durable_tab_row_disappears() {
    let dir = tempfile::tempdir().unwrap();
    let source_repo = tempfile::tempdir().unwrap();
    let target_repo = tempfile::tempdir().unwrap();
    let (handle, _replies) = ClientHandle::test_channels();
    let mut clients = HashMap::new();
    clients.insert(1, local_client(handle));
    let mut actor = test_actor(&dir, clients, HashMap::new()).await;
    let source = register_project(
        &actor.runtime_store,
        source_repo.path().to_str().unwrap(),
        Some("Original"),
    )
    .await
    .unwrap()
    .main_workspace
    .id;
    let target = register_project(
        &actor.runtime_store,
        target_repo.path().to_str().unwrap(),
        Some("Current"),
    )
    .await
    .unwrap()
    .main_workspace
    .id;
    let mut moved = Session::driver_test_stub("missing-row-session", 80, 24);
    moved.workspace_id = target.clone();
    moved.tab_id = "missing-row-tab".to_owned();
    moved.attach(1);
    assert!(moved.register_view_lease(1, "current-window"));
    moved.moved_workspace_fence.insert(source.clone());
    moved.moved_workspace_fence.insert(target.clone());
    actor.sessions.insert(moved.id.clone(), moved);

    // Simulate an out-of-band SQLite row loss while the original PTY remains
    // live. A missing row cannot promote the PTY into a never-moved session.
    let status_query = json!({
        "tabId": "missing-row-tab",
        "sessionId": "missing-row-session",
        "sourceWorkspaceId": source,
        "targetWorkspaceId": target,
        "clientMutationId": "previous-move"
    });
    let missing = actor
        .terminal_tab_transfer_status(1, &status_query)
        .await
        .unwrap();
    assert_eq!(missing["applied"], false);
    assert_eq!(missing["recoveryRequired"], true);
    assert_eq!(missing["reason"], "tab_missing");
    let closing = json!({
        "expectedWorkspaceId": target,
        "expectedTabId": "missing-row-tab",
        "expectedViewLeaseId": "current-window",
        "expectedTransferMutationId": "previous-move"
    });
    assert!(actor
        .verify_terminal_termination_owner(1, "missing-row-session", &closing)
        .await
        .is_err());
    assert!(actor
        .verify_terminal_attachment_owner("missing-row-session", &target, "missing-row-tab",)
        .await
        .is_err());
    for workspace_id in [&source, &target] {
        assert!(actor
            .prepare_runtime_mutation(
                1,
                &RuntimeMutationRequest::SleepWorkspace {
                    workspace_id: workspace_id.to_owned(),
                },
            )
            .await
            .is_err());
    }
    // Even replacing the row with a never-moved-looking Tab cannot erase
    // the current Host's earlier successful move of this still-live PTY.
    let now = Utc::now();
    actor
        .runtime_store
        .upsert_workspace_tab(WorkspaceTabRecord {
            id: "missing-row-tab".to_owned(),
            workspace_id: target.clone(),
            kind: "terminal".to_owned(),
            title: "Original PTY".to_owned(),
            created_at: now,
            updated_at: now,
            payload: json!({"terminalSessionId": "missing-row-session"}),
        })
        .await
        .unwrap();
    let erased_receipt = actor
        .terminal_tab_transfer_status(1, &status_query)
        .await
        .unwrap();
    assert_eq!(erased_receipt["applied"], false);
    assert_eq!(erased_receipt["recoveryRequired"], true);
    assert_eq!(erased_receipt["reason"], "transfer_receipt_missing");
    assert!(actor
        .verify_terminal_termination_owner(1, "missing-row-session", &closing)
        .await
        .is_err());
    assert!(actor
        .verify_terminal_attachment_owner("missing-row-session", &target, "missing-row-tab")
        .await
        .is_err());
    assert!(actor
        .sessions
        .get("missing-row-session")
        .is_some_and(|session| session.running()));
}
