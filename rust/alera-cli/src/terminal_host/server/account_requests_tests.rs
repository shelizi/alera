use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;

use crate::terminal_host::alera_account::AuthProvider;
use crate::terminal_host::client::ClientHandle;
use crate::terminal_host::host_error::HostError;

use super::actor_test_harness::{local_client, mobile_client, test_actor};
use super::deferred_admission::{DeferredAdmission, DEFERRED_REQUEST_BACKPRESSURE_CODE};

#[tokio::test]
async fn authoritative_subscription_sync_updates_lifecycle_and_waiter() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut out_rx) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(7, mobile_client(handle, "phone"))]),
        HashMap::new(),
    )
    .await;
    actor.account_push.push_enabled = true;
    actor.account_push.subscription_sync_in_flight = true;
    actor.account_push.subscription_sync_waiters.push((7, 42));
    actor.account_push.cloud_jobs = 1;

    actor.handle_push_subscription_sync_finished(Ok(3));

    assert_eq!(actor.account_push.active_subscriptions, 3);
    assert!(!actor.account_push.subscription_sync_in_flight);
    assert_eq!(actor.account_push.cloud_jobs, 0);
    assert!(actor.account_push.subscription_sync_waiters.is_empty());
    let response = out_rx
        .try_recv()
        .expect("subscription sync response")
        .as_json()
        .expect("JSON response");
    assert_eq!(response["id"], 42);
    assert_eq!(response["payload"]["activeSubscriptions"], 3);
}

#[tokio::test]
async fn disabled_runtime_does_not_retain_authoritative_subscriptions() {
    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    actor.account_push.push_enabled = false;
    actor.account_push.subscription_sync_in_flight = true;
    actor.account_push.cloud_jobs = 1;

    actor.handle_push_subscription_sync_finished(Ok(2));

    assert_eq!(actor.account_push.active_subscriptions, 0);
    assert!(!actor.account_push.subscription_sync_in_flight);
    assert_eq!(actor.account_push.cloud_jobs, 0);
}

#[tokio::test]
async fn configuration_mobile_surface_never_exposes_cloud_credentials_or_settings_seeding() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut out_rx) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(7, mobile_client(handle, "phone"))]),
        HashMap::new(),
    )
    .await;
    for (id, request) in [
        "configuration.cloud.head",
        "configuration.cloud.publish",
        "configuration.settings.seed",
    ]
    .iter()
    .enumerate()
    {
        actor
            .handle_line(
                7,
                serde_json::json!({"id": id, "type": request, "payload": {"accountId": "a"}})
                    .to_string(),
            )
            .await;
        let response = out_rx.try_recv().unwrap().as_json().unwrap();
        assert!(response.get("error").is_some(), "{request}: {response}");
    }
    assert_eq!(actor.account_push.cloud_jobs, 0);
    assert!(actor
        .runtime_store
        .configuration_snapshot("a")
        .await
        .is_err());
}

#[tokio::test]
async fn configuration_snapshot_transfer_requires_owner_and_streams_store_snapshot() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use chrono::Utc;

    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    actor
        .runtime_store
        .set_alera_account(&alera_core::runtime::LocalAleraAccount {
            account_id: "a".into(),
            email: "a@example.test".into(),
            providers: vec![],
            runtime_id: "host".into(),
            cloud_base_url: "https://example.test".into(),
            signed_in_at: Utc::now(),
            access_token_expires_at: Utc::now(),
            push_subscription_count: 0,
        })
        .await
        .unwrap();
    actor
        .runtime_store
        .configuration_update_settings(json!({"terminal": {"fontSize": 23}}))
        .await
        .unwrap();
    let expected = actor
        .runtime_store
        .configuration_snapshot("a")
        .await
        .unwrap();

    let error = actor
        .handle_configuration_request(
            7,
            "configuration.transfer.start",
            &json!({"accountId":"b","action":"snapshot"}),
        )
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "The selected account does not own this runtime."
    );

    let start = actor
        .handle_configuration_request(
            7,
            "configuration.transfer.start",
            &json!({"accountId":"a","action":"snapshot"}),
        )
        .await
        .unwrap();
    let transfer_id = start["transferId"].as_str().unwrap();
    let total = start["size"].as_u64().unwrap() as usize;
    let mut bytes = Vec::with_capacity(total);
    while bytes.len() < total {
        let chunk = actor
            .handle_configuration_request(
                7,
                "configuration.transfer.read",
                &json!({
                    "accountId":"a",
                    "transferId":transfer_id,
                    "offset":bytes.len(),
                }),
            )
            .await
            .unwrap();
        bytes.extend(STANDARD.decode(chunk["data"].as_str().unwrap()).unwrap());
    }
    assert_eq!(bytes.len(), total);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
        expected
    );
}

#[tokio::test]
async fn configuration_transfer_keeps_cas_and_account_checks_at_commit() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use chrono::Utc;
    use serde_json::{json, Value};
    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    actor
        .runtime_store
        .set_alera_account(&alera_core::runtime::LocalAleraAccount {
            account_id: "a".into(),
            email: "a@example.test".into(),
            providers: vec![],
            runtime_id: "host".into(),
            cloud_base_url: "https://example.test".into(),
            signed_in_at: Utc::now(),
            access_token_expires_at: Utc::now(),
            push_subscription_count: 0,
        })
        .await
        .unwrap();
    let before = actor
        .runtime_store
        .configuration_snapshot("a")
        .await
        .unwrap();
    let mut document = before["document"].clone();
    document["desktop"]["settings"]["terminal"] = json!({"fontSize":20});
    let payload = json!({"accountId":"a","expectedFingerprint":before["fingerprint"],"document":document,"base":null,"pending":null});
    let bytes = serde_json::to_vec(&payload).unwrap();
    for change_account in [false, true] {
        let start = actor
            .handle_configuration_request(
                7,
                "configuration.transfer.start",
                &json!({"accountId":"a","action":"apply","size":bytes.len()}),
            )
            .await
            .unwrap();
        let id = &start["transferId"];
        actor
            .handle_configuration_request(
                7,
                "configuration.transfer.chunk",
                &json!({"accountId":"a","transferId":id,"offset":0,"data":STANDARD.encode(&bytes)}),
            )
            .await
            .unwrap();
        if !change_account {
            assert_eq!(
                actor
                    .runtime_store
                    .configuration_snapshot("a")
                    .await
                    .unwrap()["document"],
                before["document"]
            );
        }
        if change_account {
            actor.runtime_store.clear_alera_account().await.unwrap();
        } else {
            actor
                .runtime_store
                .configuration_update_settings(json!({"terminal":{"fontSize":18}}))
                .await
                .unwrap();
        }
        let result = actor
            .handle_configuration_request(
                7,
                "configuration.transfer.commit",
                &json!({"accountId":"a","transferId":id}),
            )
            .await;
        assert!(result.is_err());
        assert!(actor
            .runtime_store
            .get_metadata("configuration.backup.a")
            .await
            .unwrap()
            .is_none());
    }
    assert_eq!(
        actor.runtime_store.configuration_settings().await.unwrap()["terminal"]["fontSize"],
        Value::from(18)
    );
}

#[tokio::test]
async fn configuration_import_validates_adapters_and_managed_launches_before_writing() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use chrono::Utc;
    use serde_json::json;
    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    actor
        .runtime_store
        .set_alera_account(&alera_core::runtime::LocalAleraAccount {
            account_id: "a".into(),
            email: "a@example.test".into(),
            providers: vec![],
            runtime_id: "host".into(),
            cloud_base_url: "https://example.test".into(),
            signed_in_at: Utc::now(),
            access_token_expires_at: Utc::now(),
            push_subscription_count: 0,
        })
        .await
        .unwrap();
    let before = actor
        .runtime_store
        .configuration_snapshot("a")
        .await
        .unwrap();
    for via_transfer in [false, true] {
        for profile in [
            json!({"id":"one","name":"One","agentType":"codex","command":"codex","launchMode":"managed",
                "managedConfig":{"bypassApprovalsAndSandbox":true,"sandbox":"read-only"}}),
            json!({"id":"one","name":"One","agentType":"unsupported","command":"unknown"}),
        ] {
            let mut document = before["document"].clone();
            document["shared"]["agentProfiles"] = json!({"items":{"one":profile},"order":["one"]});
            document["desktop"]["settings"]["terminal"] = json!({"fontSize":20});
            let payload = json!({"accountId":"a","expectedFingerprint":before["fingerprint"],
                "document":document,"base":null,"pending":{"operationId":"pending"}});
            let result = if via_transfer {
                let bytes = serde_json::to_vec(&payload).unwrap();
                let start = actor
                    .handle_configuration_request(
                        7,
                        "configuration.transfer.start",
                        &json!({"accountId":"a","action":"apply","size":bytes.len()}),
                    )
                    .await
                    .unwrap();
                actor.handle_configuration_request(7,"configuration.transfer.chunk",
                    &json!({"accountId":"a","transferId":start["transferId"],"offset":0,"data":STANDARD.encode(&bytes)})).await.unwrap();
                actor
                    .handle_configuration_request(
                        7,
                        "configuration.transfer.commit",
                        &json!({"accountId":"a","transferId":start["transferId"]}),
                    )
                    .await
            } else {
                actor
                    .handle_configuration_request(7, "configuration.apply", &payload)
                    .await
            };
            assert!(result.is_err());
            assert_eq!(
                actor
                    .runtime_store
                    .configuration_snapshot("a")
                    .await
                    .unwrap(),
                before
            );
            assert!(actor
                .runtime_store
                .get_metadata("configuration.backup.a")
                .await
                .unwrap()
                .is_none());
        }
    }
}

fn request_type_entry<'a>(
    snapshot: &'a serde_json::Value,
    request_type: &str,
) -> &'a serde_json::Value {
    snapshot["requestTypes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["requestType"] == request_type)
        .unwrap_or_else(|| panic!("missing requestType entry for {request_type}"))
}

#[tokio::test]
async fn push_subscription_sync_rejection_clears_in_flight_and_answers_waiters() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, mut responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    actor.deferred_admission = Arc::new(DeferredAdmission::paused_with_limits(1, 2, 0));
    actor.deferred_admission.add_test_permits(1);
    let (inbox, _commands) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;

    let (blocker_release_tx, blocker_release_rx) = tokio::sync::oneshot::channel::<()>();
    actor
        .start_deferred_request(1, 60, "test.fill", async move {
            let _ = blocker_release_rx.await;
            Ok(json!({}))
        })
        .unwrap();
    actor
        .start_deferred_request(1, 61, "test.fill", async { Ok(json!({})) })
        .unwrap();

    actor.start_push_subscription_sync(Some((1, 80)));

    assert!(!actor.account_push.subscription_sync_in_flight);
    assert_eq!(actor.account_push.cloud_jobs, 0);
    assert!(actor.account_push.subscription_sync_waiters.is_empty());

    let sync_rejected = tokio::time::timeout(std::time::Duration::from_secs(1), responses.recv())
        .await
        .expect("waiter should receive rejection response")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(sync_rejected["id"], 80);
    assert_eq!(sync_rejected["ok"], false);
    assert_eq!(
        sync_rejected["errorCode"],
        DEFERRED_REQUEST_BACKPRESSURE_CODE
    );
    assert_eq!(
        sync_rejected["errorDetails"]["requestType"],
        "mobile.cloudSubscriptions.refresh"
    );

    let snapshot = actor.deferred_admission.snapshot();
    assert_eq!(snapshot["rejected"], 1);
    let sync_entry = request_type_entry(&snapshot, "mobile.cloudSubscriptions.refresh");
    assert_eq!(sync_entry["rejected"], 1);
    assert_eq!(sync_entry["pending"], 0);

    blocker_release_tx.send(()).unwrap();
}

#[tokio::test]
async fn account_sign_in_rejects_under_saturation_without_starting() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, _responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    actor.deferred_admission = Arc::new(DeferredAdmission::paused_with_limits(1, 2, 0));
    actor.deferred_admission.add_test_permits(1);
    let (inbox, _commands) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;

    let (blocker_release_tx, blocker_release_rx) = tokio::sync::oneshot::channel::<()>();
    actor
        .start_deferred_request(1, 60, "test.fill", async move {
            let _ = blocker_release_rx.await;
            Ok(json!({}))
        })
        .unwrap();
    actor
        .start_deferred_request(1, 61, "test.fill", async { Ok(json!({})) })
        .unwrap();

    let error = actor
        .start_account_sign_in(1, 90, AuthProvider::Google, false)
        .expect_err("saturated admission must reject sign-in");
    let HostError::Conflict { code, details, .. } = &error else {
        panic!("expected a typed conflict, got {error:?}");
    };
    assert_eq!(code, DEFERRED_REQUEST_BACKPRESSURE_CODE);
    assert_eq!(details["requestType"], "account.signIn.start");
    assert!(actor.account_push.sign_in_cancel.is_none());
    assert_eq!(actor.account_push.cloud_jobs, 0);

    blocker_release_tx.send(()).unwrap();
}

#[tokio::test]
async fn account_sign_in_stays_single_flight() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, _responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([(1, local_client(handle))]),
        HashMap::new(),
    )
    .await;
    let (cancel_tx, _cancel_rx) = tokio::sync::oneshot::channel::<()>();
    actor.account_push.sign_in_cancel = Some(cancel_tx);

    let error = actor
        .start_account_sign_in(1, 91, AuthProvider::Google, false)
        .expect_err("a second sign-in must stay single-flight");
    assert!(error.wire_message().contains("already in progress"));
    assert!(actor.account_push.sign_in_cancel.is_some());
}
