use std::collections::HashMap;

use serde_json::json;

use super::actor_test_harness::{local_client, mobile_client, test_actor};
use super::agent_settings_contract_tests::{read_response, wire_call};
use crate::terminal_host::client::ClientHandle;

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
