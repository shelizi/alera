use alera_core::runtime::AutomationDefinition;

use super::*;

#[tokio::test]
async fn managed_workspace_preflight_does_not_block_status_or_stop_sessions() {
    let mut fixture = Fixture::new_with_workspace_id("blocked-preflight-workspace").await;
    let gate = crate::managed_workspace::managed_workspace_removal_test_gate::install(
        "blocked-preflight-workspace",
    );
    tokio::time::timeout(
        Duration::from_secs(1),
        fixture.actor.handle_line(
            1,
            json!({"id": 1, "type": "workspace.removeManaged", "payload": {"id": "blocked-preflight-workspace", "closeSessions": true}}).to_string(),
        ),
    )
    .await
    .expect("router must not await the removal preflight");
    tokio::time::timeout(Duration::from_secs(5), gate.wait_until_entered())
        .await
        .expect("the mutation worker should reach the removal preflight");
    assert!(fixture.actor.mutation_queue.has_runtime_mutations());
    assert!(fixture.actor.sessions["terminal"].running());
    assert!(fixture.commands.try_recv().is_err());

    fixture
        .actor
        .handle_line(
            1,
            json!({"id": 2, "type": "status.get", "payload": {}}).to_string(),
        )
        .await;
    let status = fixture.wait_for_response(2).await;
    assert_eq!(status["ok"], true, "{status}");

    gate.release();
    let response = fixture.wait_for_response(1).await;
    assert_eq!(response["ok"], true, "{response}");
    assert!(!std::path::Path::new(&fixture.workspace.path).exists());
    assert!(fixture
        .actor
        .runtime_store
        .find_workspace("blocked-preflight-workspace")
        .await
        .unwrap()
        .is_none());
    assert!(!fixture.actor.mutation_queue.has_runtime_mutations());
}

#[tokio::test]
async fn invalid_managed_storage_preflight_keeps_sessions_running() {
    let mut fixture = Fixture::new().await;
    let outside = fixture._root.path().join("outside-managed-root");
    std::fs::create_dir_all(&outside).unwrap();
    let mut workspace = fixture.workspace.clone();
    workspace.path = outside.to_string_lossy().into_owned();
    fixture
        .actor
        .runtime_store
        .upsert_workspace(workspace)
        .await
        .unwrap();

    let response = fixture
        .request(
            "workspace.removeManaged",
            json!({"id": "workspace", "closeSessions": true}),
        )
        .await;

    assert_eq!(response["ok"], false, "{response}");
    assert!(
        response["error"]
            .as_str()
            .unwrap()
            .contains("Workspace path is outside Alera-managed storage"),
        "{response}"
    );
    assert!(fixture.actor.sessions["terminal"].running());
    assert!(outside.exists());
    assert!(!fixture.actor.mutation_queue.has_runtime_mutations());
}

#[tokio::test]
async fn active_automation_preflight_keeps_sessions_running() {
    let mut fixture = Fixture::new().await;
    let now = Utc::now();
    let actor = json!({"kind": "localCli"});
    let definition: AutomationDefinition = serde_json::from_value(json!({
        "id": "cleanup-owner",
        "slug": "cleanup-owner",
        "name": "Cleanup Owner",
        "promptTemplate": "Run",
        "schedule": {"recurring": {"cron": "0 0 * * *", "timezone": "UTC"}},
        "target": {"existingTab": {"workspace_id": fixture.workspace.id, "tab_id": "tab-terminal"}},
        "state": "draft",
        "revision": 1,
        "createdBy": actor,
        "modifiedBy": actor,
        "createdAt": now.to_rfc3339(),
        "updatedAt": now.to_rfc3339(),
    }))
    .unwrap();
    let automation_actor = definition.created_by.clone();
    let stored = fixture
        .actor
        .runtime_store
        .upsert_automation(definition, automation_actor.clone())
        .await
        .unwrap();
    fixture
        .actor
        .runtime_store
        .approve_automation(&stored.id, stored.revision, automation_actor)
        .await
        .unwrap();

    let response = fixture
        .request(
            "workspace.removeManaged",
            json!({"id": "workspace", "closeSessions": true}),
        )
        .await;

    assert_eq!(response["ok"], false, "{response}");
    assert!(
        response["error"]
            .as_str()
            .unwrap()
            .contains("Workspace is owned by an active automation"),
        "{response}"
    );
    assert!(fixture.actor.sessions["terminal"].running());
    assert!(std::path::Path::new(&fixture.workspace.path).exists());
    assert!(!fixture.actor.mutation_queue.has_runtime_mutations());
}
