//! Operation-contract coverage for the `agentProfile.*`, `runtimeSettings.*`,
//! and `status.get` requests: the matrix in
//! `docs/agent-settings-operation-contract.md` names the replay and conflict
//! guarantees each operation keeps. These tests pin the cells the profile
//! store and autostart suites do not already cover.

use std::collections::HashMap;

use alera_core::runtime::WorkspaceTabRecord;
use chrono::Utc;
use serde_json::{json, Value};
use tokio::sync::mpsc::UnboundedReceiver;

use super::actor_test_harness::{local_client, test_actor};
use super::requests::idempotency_receipts::{
    payload_digest, prepare_receipt, settle_receipt, ReceiptPrepareOutcome,
};
use super::{ServerActor, ServerCommand};
use crate::terminal_host::client::{ClientFrame, ClientHandle};

pub(super) async fn wire_call(
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

pub(super) async fn read_response(
    responses: &mut UnboundedReceiver<ClientFrame>,
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

pub(super) async fn deferred_request(
    actor: &mut ServerActor,
    client_id: u64,
    request_id: i64,
    request_type: &str,
    payload: Value,
    responses: &mut UnboundedReceiver<ClientFrame>,
    inbox: &mut UnboundedReceiver<ServerCommand>,
) -> Value {
    wire_call(actor, client_id, request_id, request_type, payload).await;
    loop {
        tokio::select! {
            command = inbox.recv() => {
                actor.handle(command.expect("the command should be delivered")).await;
            }
            frame = responses.recv() => {
                let response = frame
                    .expect("the response should be delivered")
                    .as_json()
                    .expect("the response should be JSON");
                if response["id"] == request_id {
                    return response;
                }
            }
        }
    }
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
async fn agent_quota_consume_replays_a_settled_client_mutation_receipt() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let (inbox, mut inbox_receiver) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;

    let payload = json!({
        "offerRevision": "offer-1",
        "clientMutationId": "quota-consume-1",
    });
    let digest = payload_digest(&payload).unwrap();
    let stored_result = json!({
        "status": "consumed",
        "outcome": "reset",
        "snapshot": {
            "provider": "codex",
            "accountId": "default",
            "status": "ok",
            "windows": [],
        },
    });
    assert_eq!(
        prepare_receipt(
            &actor.runtime_store,
            "agentQuota.consumeCodexResetCredit",
            "local:cli",
            "quota-consume-1",
            &digest,
            None,
        )
        .await
        .unwrap(),
        ReceiptPrepareOutcome::Created
    );
    settle_receipt(
        &actor.runtime_store,
        "agentQuota.consumeCodexResetCredit",
        "local:cli",
        "quota-consume-1",
        &digest,
        &stored_result,
    )
    .await
    .unwrap();

    let first = deferred_request(
        &mut actor,
        1,
        50,
        "agentQuota.consumeCodexResetCredit",
        payload.clone(),
        &mut responses,
        &mut inbox_receiver,
    )
    .await;
    let retry = deferred_request(
        &mut actor,
        1,
        51,
        "agentQuota.consumeCodexResetCredit",
        payload,
        &mut responses,
        &mut inbox_receiver,
    )
    .await;
    assert_eq!(first["ok"], true, "{first}");
    assert_eq!(retry["ok"], true, "{retry}");
    assert_eq!(retry["payload"], first["payload"]);
    assert_eq!(retry["payload"]["status"], "consumed");
    assert_eq!(retry["payload"]["outcome"], "reset");

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM terminalHostIdempotencyReceipts
         WHERE operation = 'agentQuota.consumeCodexResetCredit'
           AND callerScope = 'local:cli' AND clientMutationId = 'quota-consume-1'",
    )
    .fetch_one(actor.runtime_store.pool())
    .await
    .unwrap();
    assert_eq!(count, 1, "a replay must not create a second receipt");

    let conflict = deferred_request(
        &mut actor,
        1,
        52,
        "agentQuota.consumeCodexResetCredit",
        json!({
            "offerRevision": "offer-2",
            "clientMutationId": "quota-consume-1",
        }),
        &mut responses,
        &mut inbox_receiver,
    )
    .await;
    assert_eq!(conflict["ok"], false, "{conflict}");
    assert!(
        conflict["error"].as_str().is_some_and(|error| {
            error.contains("different agentQuota.consumeCodexResetCredit payload")
        }),
        "{conflict}"
    );
    assert!(conflict.get("errorCode").is_none(), "{conflict}");
}

#[tokio::test]
async fn agent_quota_consume_without_a_client_mutation_id_keeps_legacy_validation() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let (inbox, mut inbox_receiver) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;

    let response = deferred_request(
        &mut actor,
        1,
        53,
        "agentQuota.consumeCodexResetCredit",
        json!({}),
        &mut responses,
        &mut inbox_receiver,
    )
    .await;
    assert_eq!(response["ok"], false, "{response}");
    assert_eq!(
        response["error"],
        "offerRevision must be a non-empty string"
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
