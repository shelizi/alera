//! Operation-contract coverage for the `agentProfile.*`, `runtimeSettings.*`,
//! and `status.get` requests: the matrix in
//! `docs/agent-settings-operation-contract.md` names the replay and conflict
//! guarantees each operation keeps. These tests pin the cells the profile
//! store and autostart suites do not already cover.

use std::collections::HashMap;

use alera_core::runtime::WorkspaceTabRecord;
use chrono::Utc;
use serde_json::{json, Value};

use super::actor_test_harness::{local_client, mobile_client, test_actor};
use super::ServerActor;
use crate::terminal_host::client::ClientHandle;

async fn wire_call(
    actor: &mut ServerActor,
    client_id: u64,
    request_id: i64,
    request_type: &str,
    payload: Value,
) {
    actor
        .handle_line(
            client_id,
            json!({
                "id": request_id,
                "type": request_type,
                "payload": payload,
            })
            .to_string(),
        )
        .await
}

async fn read_response(
    responses: &mut tokio::sync::mpsc::UnboundedReceiver<crate::terminal_host::client::ClientFrame>,
    request_id: i64,
) -> Value {
    for _ in 0..8 {
        let response = tokio::time::timeout(std::time::Duration::from_secs(1), responses.recv())
            .await
            .expect("response should arrive")
            .unwrap()
            .as_json()
            .unwrap();
        if response["id"] == request_id {
            return response;
        }
    }
    panic!("no response for request {request_id}");
}

#[tokio::test]
async fn agent_profile_upsert_reports_a_typed_revision_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;

    wire_call(
        &mut actor,
        1,
        10,
        "agentProfile.upsert",
        json!({ "name": "Codex", "agentType": "codex", "command": "codex" }),
    )
    .await;
    let created = read_response(&mut responses, 10).await;
    assert_eq!(created["ok"], true, "{created}");
    assert_eq!(created["payload"]["revision"], 0);
    let profile_id = created["payload"]["id"].as_str().unwrap().to_string();

    wire_call(
        &mut actor,
        1,
        11,
        "agentProfile.upsert",
        json!({
            "id": profile_id,
            "name": "Codex Stale",
            "agentType": "codex",
            "command": "codex",
            "expectedRevision": 7,
        }),
    )
    .await;
    let conflict = read_response(&mut responses, 11).await;
    assert_eq!(conflict["ok"], false, "{conflict}");
    assert_eq!(conflict["errorCode"], "agent_profile_revision_conflict");
    assert_eq!(conflict["errorDetails"]["profileId"], json!(profile_id));
    assert_eq!(conflict["errorDetails"]["expectedRevision"], 7);
    assert_eq!(conflict["errorDetails"]["currentRevision"], 0);
    assert!(
        conflict["error"]
            .as_str()
            .is_some_and(|error| error.contains("revision conflict")),
        "the message stays available for older clients: {conflict}"
    );

    let stored = actor
        .runtime_store
        .find_agent_profile(&profile_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.name, "Codex", "a stale update must not overwrite");
    assert_eq!(stored.revision, 0);
}

#[tokio::test]
async fn agent_profile_remove_distinguishes_typed_conflicts_from_plain_errors() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;

    wire_call(
        &mut actor,
        1,
        20,
        "agentProfile.upsert",
        json!({ "name": "Codex", "agentType": "codex", "command": "codex" }),
    )
    .await;
    let created = read_response(&mut responses, 20).await;
    let profile_id = created["payload"]["id"].as_str().unwrap().to_string();

    // Missing `confirmed` is a plain format error, not a typed conflict.
    wire_call(
        &mut actor,
        1,
        21,
        "agentProfile.remove",
        json!({ "id": profile_id, "expectedRevision": 0 }),
    )
    .await;
    let unconfirmed = read_response(&mut responses, 21).await;
    assert_eq!(unconfirmed["ok"], false, "{unconfirmed}");
    assert!(
        unconfirmed["error"]
            .as_str()
            .is_some_and(|error| error.contains("explicit confirmation")),
        "{unconfirmed}"
    );
    assert!(unconfirmed.get("errorCode").is_none(), "{unconfirmed}");

    // A stale revision is the typed conflict a client refreshes on.
    wire_call(
        &mut actor,
        1,
        22,
        "agentProfile.remove",
        json!({ "id": profile_id, "expectedRevision": 3, "confirmed": true }),
    )
    .await;
    let conflict = read_response(&mut responses, 22).await;
    assert_eq!(conflict["ok"], false, "{conflict}");
    assert_eq!(conflict["errorCode"], "agent_profile_revision_conflict");
    assert_eq!(conflict["errorDetails"]["currentRevision"], 0);

    // The committed remove and its retry both succeed; the retry reports
    // `removed: false` instead of erroring.
    for request_id in [23, 24] {
        wire_call(
            &mut actor,
            1,
            request_id,
            "agentProfile.remove",
            json!({ "id": profile_id, "expectedRevision": 0, "confirmed": true }),
        )
        .await;
        let removed = read_response(&mut responses, request_id).await;
        assert_eq!(removed["ok"], true, "{removed}");
        assert_eq!(removed["payload"]["removed"], request_id == 23);
    }
}

#[tokio::test]
async fn runtime_settings_update_reapplies_the_same_values_idempotently() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;

    for request_id in [30, 31] {
        wire_call(
            &mut actor,
            1,
            request_id,
            "runtimeSettings.update",
            json!({ "confirmProjectRemoval": false }),
        )
        .await;
        let response = read_response(&mut responses, request_id).await;
        assert_eq!(response["ok"], true, "{response}");
        // The reply is the re-assembled settings, not an acknowledgement.
        assert_eq!(response["payload"]["confirmProjectRemoval"], false);
        assert!(response["payload"]["automation"].is_object(), "{response}");
    }
    assert_eq!(
        actor.runtime_store.confirm_project_removal().await.unwrap(),
        false
    );
}

#[tokio::test]
async fn agent_profile_launch_rejects_same_mutation_id_with_an_altered_payload() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let now = Utc::now();
    let tab = WorkspaceTabRecord {
        id: "recorded-launch-tab".to_string(),
        workspace_id: "workspace-1".to_string(),
        kind: "terminal".to_string(),
        title: "Recorded launch".to_string(),
        created_at: now,
        updated_at: now,
        payload: json!({"spawnOnCreate": true}),
    };
    actor
        .runtime_store
        .record_agent_profile_launch(
            "local:cli",
            "workspace-1",
            "mutation-1",
            "original-payload-digest",
            &json!({"tab": {"id": tab.id.clone()}}),
            &tab,
        )
        .await
        .unwrap();

    wire_call(
        &mut actor,
        1,
        50,
        "agentProfile.launchIdempotent",
        json!({
            "workspaceId": "workspace-1",
            "profileId": "profile-1",
            "prompt": "changed prompt",
            "clientMutationId": "mutation-1"
        }),
    )
    .await;
    let conflict = read_response(&mut responses, 50).await;
    assert_eq!(conflict["ok"], false, "{conflict}");
    assert!(
        conflict["error"]
            .as_str()
            .is_some_and(|error| error.contains("different agentProfile.launch payload")),
        "{conflict}"
    );
    // This operation's contract deliberately exposes a plain state error,
    // unlike OCC conflicts that carry an errorCode and errorDetails.
    assert!(conflict.get("errorCode").is_none(), "{conflict}");
    assert!(actor
        .runtime_store
        .find_workspace_tab("recorded-launch-tab")
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn runtime_settings_update_validates_the_full_payload_before_writing() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;

    wire_call(
        &mut actor,
        1,
        60,
        "runtimeSettings.update",
        json!({ "confirmProjectRemoval": false }),
    )
    .await;
    let initial = read_response(&mut responses, 60).await;
    assert_eq!(initial["ok"], true, "{initial}");
    assert_eq!(initial["payload"]["revision"], 1);

    // The valid key must not be committed when the same payload contains an
    // unsupported key.
    wire_call(
        &mut actor,
        1,
        61,
        "runtimeSettings.update",
        json!({ "confirmProjectRemoval": true, "notASetting": false }),
    )
    .await;
    let rejected = read_response(&mut responses, 61).await;
    assert_eq!(rejected["ok"], false, "{rejected}");
    assert!(
        rejected["error"]
            .as_str()
            .is_some_and(|error| error.contains("Unsupported runtime setting")),
        "{rejected}"
    );
    assert_eq!(
        actor.runtime_store.confirm_project_removal().await.unwrap(),
        false,
        "an invalid later key must not partially commit the valid key"
    );
    assert_eq!(
        actor.runtime_settings_snapshot().await.unwrap()["revision"],
        1
    );
}

#[tokio::test]
async fn runtime_settings_update_checks_expected_revision_and_preserves_lww_without_it() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, mobile_client(handle, "phone"))]),
        HashMap::new(),
    )
    .await;

    wire_call(
        &mut actor,
        1,
        70,
        "mobile.runtimeSettings.update",
        json!({ "confirmProjectRemoval": false }),
    )
    .await;
    let initial = read_response(&mut responses, 70).await;
    assert_eq!(initial["ok"], true, "{initial}");
    assert_eq!(initial["payload"]["revision"], 1);

    wire_call(
        &mut actor,
        1,
        71,
        "mobile.runtimeSettings.update",
        json!({ "confirmProjectRemoval": true, "expectedRevision": 0 }),
    )
    .await;
    let conflict = read_response(&mut responses, 71).await;
    assert_eq!(conflict["ok"], false, "{conflict}");
    assert_eq!(conflict["errorCode"], "runtime_settings_revision_conflict");
    assert_eq!(conflict["errorDetails"]["expectedRevision"], 0);
    assert_eq!(conflict["errorDetails"]["actualRevision"], 1);
    assert_eq!(
        actor.runtime_store.confirm_project_removal().await.unwrap(),
        false,
        "a stale revision must not write any keys"
    );

    wire_call(
        &mut actor,
        1,
        72,
        "mobile.runtimeSettings.update",
        json!({ "confirmProjectRemoval": true, "expectedRevision": 1 }),
    )
    .await;
    let committed = read_response(&mut responses, 72).await;
    assert_eq!(committed["ok"], true, "{committed}");
    assert_eq!(committed["payload"]["confirmProjectRemoval"], true);
    assert_eq!(committed["payload"]["revision"], 2);

    // Omitting the additive field retains the legacy last-write-wins path.
    wire_call(
        &mut actor,
        1,
        73,
        "mobile.runtimeSettings.update",
        json!({ "confirmProjectRemoval": false }),
    )
    .await;
    let lww = read_response(&mut responses, 73).await;
    assert_eq!(lww["ok"], true, "{lww}");
    assert_eq!(lww["payload"]["confirmProjectRemoval"], false);
    assert_eq!(lww["payload"]["revision"], 3);
}

#[tokio::test]
async fn mobile_runtime_settings_update_rejects_desktop_only_keys() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, mobile_client(handle, "phone"))]),
        HashMap::new(),
    )
    .await;

    wire_call(
        &mut actor,
        1,
        40,
        "mobile.runtimeSettings.update",
        json!({ "aiTextGeneration": { "enabled": true }, "confirmProjectRemoval": false }),
    )
    .await;
    let response = read_response(&mut responses, 40).await;
    assert_eq!(response["ok"], false, "{response}");
    assert!(
        response["error"]
            .as_str()
            .is_some_and(|error| error.contains("Unsupported mobile setting")),
        "{response}"
    );
    // The allowlist rejects the payload before any key is applied.
    assert_eq!(
        actor.runtime_store.confirm_project_removal().await.unwrap(),
        true
    );
}

#[tokio::test]
async fn status_get_reports_a_stable_shape() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;

    wire_call(&mut actor, 1, 50, "status.get", json!({})).await;
    let status = read_response(&mut responses, 50).await;
    assert_eq!(status["ok"], true, "{status}");
    let payload = &status["payload"];
    assert_eq!(
        payload["protocolVersion"],
        crate::terminal_host::protocol::PROTOCOL_VERSION
    );
    assert_eq!(payload["runtime"], "alera");
    assert_eq!(payload["authenticated"], true);
    assert!(payload["runtimeHostVersion"].is_string(), "{status}");
    assert!(payload["activeSessions"].is_number(), "{status}");
    assert!(payload["activeAgents"].is_number(), "{status}");
    let capabilities = payload["runtimeCapabilities"].as_array().unwrap();
    for capability in [
        "orchestrationAgentProfilesV1",
        "orchestrationAgentProfileRevisionsV1",
        "orchestrationAgentProfileRemovalV1",
        "agentProfileLaunchIdempotencyV1",
        "runtimeAgentStatusV1",
        "runtimeHostLifecycleV1",
    ] {
        assert!(
            capabilities.iter().any(|value| value == capability),
            "status.get must advertise {capability}: {status}"
        );
    }
}
