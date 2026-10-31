use super::*;

async fn insert_tab(store: &RuntimeStore, id: &str, owner: &str, payload: Value) {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO workspaceTabs (id, workspaceId, kind, title, createdAt, updatedAt, payloadJson) \
         VALUES (?, ?, 'terminal', 'Terminal', ?, ?, ?)",
    )
    .bind(id)
    .bind(owner)
    .bind(&now)
    .bind(&now)
    .bind(payload.to_string())
    .execute(&store.pool)
    .await
    .unwrap();
}

fn single_pane(workspace_id: &str, tabs: &[&str]) -> Value {
    let group = format!("{workspace_id}/main");
    let mut layout = json!({
        "workspaceId": workspace_id,
        "root": {"type": "leaf", "groupId": group},
        "activeGroupId": group,
        "groups": {}
    });
    layout["groups"][&group] = json!({
        "id": group,
        "tabIds": tabs,
        "activeTabId": tabs.last().copied()
    });
    layout
}

async fn save_layout(store: &RuntimeStore, workspace: &str, layout: &Value) {
    sqlx::query("INSERT INTO workbenchLayouts (workspaceId, dataJson) VALUES (?, ?)")
        .bind(workspace)
        .bind(layout.to_string())
        .execute(&store.pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn ordinary_delete_preserves_null_and_exit_only_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let store = RuntimeStore::open(dir.path()).await.unwrap();
    insert_tab(
        &store,
        "corrupt",
        "owned",
        json!({"lastTerminalTabTransfer": null}),
    )
    .await;
    insert_tab(
        &store,
        "exit",
        "other",
        json!({"lastTerminalTabExit": null}),
    )
    .await;
    assert!(store.remove_workspace_tab("corrupt").await.is_err());
    assert!(store.remove_workspace_tab("exit").await.is_err());
    assert!(store
        .ensure_workspace_cleanup_preserves_terminal_transfer_evidence("owned")
        .await
        .is_err());
    assert!(store
        .ensure_workspace_cleanup_preserves_terminal_transfer_evidence("unrelated")
        .await
        .is_ok());
}

#[tokio::test]
async fn only_matching_original_pty_receipt_can_retire_tab_with_audit() {
    let dir = tempfile::tempdir().unwrap();
    let store = RuntimeStore::open(dir.path()).await.unwrap();
    let token = format!("{}:12", uuid::Uuid::new_v4());
    insert_tab(
        &store,
        "moved",
        "target",
        json!({
            "terminalSessionId": "moved",
            "terminalExecutionWorkspaceId": "source",
            "lastTerminalTabTransfer": {
                "id": "mutation-1", "from": "source", "to": "target",
                "ptyInstanceToken": token
            }
        }),
    )
    .await;
    save_layout(&store, "source", &single_pane("source", &[])).await;
    save_layout(&store, "target", &single_pane("target", &["moved"])).await;
    assert!(store
        .remove_transferred_terminal_after_verified_live_view(
            "moved",
            "moved",
            "target",
            "stale",
            &token,
            "my-private-view"
        )
        .await
        .is_err());
    assert!(store.find_workspace_tab("moved").await.unwrap().is_some());
    store
        .remove_transferred_terminal_after_verified_live_view(
            "moved",
            "moved",
            "target",
            "mutation-1",
            &token,
            "my-private-view",
        )
        .await
        .unwrap();
    assert!(store.find_workspace_tab("moved").await.unwrap().is_none());
    let raw: String =
        sqlx::query_scalar("SELECT dataJson FROM workbenchLayouts WHERE workspaceId = 'target'")
            .fetch_one(&store.pool)
            .await
            .unwrap();
    let target: Value = serde_json::from_str(&raw).unwrap();
    assert!(tab_ids_in_layout(&target, "target").unwrap().is_empty());
    let digest: String = sqlx::query_scalar(
        "SELECT viewLeaseSha256 FROM terminalTabRetirementAudits WHERE tabId = 'moved'",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(digest, hex::encode(Sha256::digest(b"my-private-view")));
    assert_ne!(digest, "my-private-view");
}

#[tokio::test]
async fn original_exited_pty_requires_exact_marker_and_audits_explicit_close() {
    let dir = tempfile::tempdir().unwrap();
    let store = RuntimeStore::open(dir.path()).await.unwrap();
    let token = format!("{}:42", uuid::Uuid::new_v4());
    let payload = json!({
        "terminalSessionId": "exited-session",
        "lastTerminalTabTransfer": {
            "id": "exit-move", "from": "source", "to": "target", "ptyInstanceToken": token
        },
        "lastTerminalTabExit": {
            "transferMutationId": "exit-move", "sessionId": "exited-session",
            "workspaceId": "target", "ptyInstanceToken": token,
            "exitCode": 7, "observedAtUtc": Utc::now().to_rfc3339()
        }
    });
    insert_tab(&store, "exited-tab", "target", payload).await;
    save_layout(&store, "source", &single_pane("source", &[])).await;
    save_layout(&store, "target", &single_pane("target", &["exited-tab"])).await;
    assert!(store
        .remove_transferred_terminal_after_verified_live_view(
            "exited-tab",
            "exited-session",
            "target",
            "exit-move",
            &token,
            "view"
        )
        .await
        .is_err());
    for (mutation, code) in [("stale-move", 7), ("exit-move", 0)] {
        assert!(store
            .remove_transferred_terminal_after_verified_original_exit(
                "exited-tab",
                "exited-session",
                "target",
                mutation,
                &token,
                "view",
                code
            )
            .await
            .is_err());
    }
    assert!(store
        .find_workspace_tab("exited-tab")
        .await
        .unwrap()
        .is_some());
    store
        .remove_transferred_terminal_after_verified_original_exit(
            "exited-tab",
            "exited-session",
            "target",
            "exit-move",
            &token,
            "view",
            7,
        )
        .await
        .unwrap();
    assert!(store
        .find_workspace_tab("exited-tab")
        .await
        .unwrap()
        .is_none());
    let audit: (String, String) = sqlx::query_as(
        "SELECT reason, viewLeaseSha256 FROM terminalTabRetirementAudits WHERE tabId = ?",
    )
    .bind("exited-tab")
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(audit.0, "explicit_verified_original_exited_pty_only_view");
    assert_eq!(audit.1, hex::encode(Sha256::digest(b"view")));
}

#[test]
fn retirement_keeps_other_destination_tab_and_activates_it() {
    let target = single_pane("target", &["moved", "editor"]);
    let next = layout_after_retiring_tab(&target, "target", "moved").unwrap();
    assert_eq!(next["groups"]["target/main"]["activeTabId"], "editor");
    assert!(tab_ids_in_layout(&next, "target")
        .unwrap()
        .contains("editor"));
}
