use std::collections::HashMap;

use serde_json::json;

use super::actor_test_harness::{local_client, mobile_client, test_actor};
use super::agent_settings_contract_tests::{read_response, wire_call};
use crate::terminal_host::client::ClientHandle;

#[tokio::test]
async fn classic_and_gpui_history_sync_share_revisioned_host_state() {
    let dir = tempfile::tempdir().unwrap();
    let (gpui_handle, mut gpui_responses) = ClientHandle::test_channels();
    let (classic_handle, mut classic_responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([
            (1, local_client(gpui_handle)),
            (2, local_client(classic_handle)),
        ]),
        HashMap::new(),
    )
    .await;

    let before = actor.runtime_settings_snapshot().await.unwrap();
    assert_eq!(before["revision"], 0);
    assert_eq!(before["agentQuotas"]["historySync"]["mode"], "all");

    let mut quotas = before["agentQuotas"].clone();
    quotas["historySync"] = json!({
        "mode": "groups",
        "accountGroups": {
            "codex:default": "work",
            "codex:profile-b": "work",
            "claude:default": "private"
        }
    });
    wire_call(
        &mut actor,
        1,
        201,
        "runtimeSettings.update",
        json!({ "expectedRevision": 0, "agentQuotas": quotas }),
    )
    .await;
    let saved = read_response(&mut gpui_responses, 201).await;
    assert_eq!(saved["ok"], true, "{saved}");
    assert_eq!(saved["payload"]["revision"], 1, "{saved}");
    assert_eq!(
        saved["payload"]["agentQuotas"]["historySync"]["mode"],
        "groups"
    );

    wire_call(&mut actor, 2, 202, "runtimeSettings.get", json!({})).await;
    let seen_in_classic = read_response(&mut classic_responses, 202).await;
    assert_eq!(seen_in_classic["ok"], true, "{seen_in_classic}");
    assert_eq!(
        seen_in_classic["payload"]["agentQuotas"]["historySync"]["accountGroups"]
            ["codex:profile-b"],
        "work",
        "{seen_in_classic}"
    );

    // Old Classic edit based on its original revision must not overwrite GPUI.
    quotas["historySync"] = json!({"mode": "separate"});
    wire_call(
        &mut actor,
        2,
        203,
        "runtimeSettings.update",
        json!({ "expectedRevision": 0, "agentQuotas": quotas.clone() }),
    )
    .await;
    let rejected = read_response(&mut classic_responses, 203).await;
    assert_eq!(rejected["ok"], false, "{rejected}");
    assert_eq!(rejected["errorCode"], "runtime_settings_revision_conflict");
    assert_eq!(
        actor.runtime_settings_snapshot().await.unwrap()["agentQuotas"]["historySync"]["mode"],
        "groups"
    );

    wire_call(
        &mut actor,
        2,
        204,
        "runtimeSettings.update",
        json!({ "expectedRevision": 1, "agentQuotas": quotas }),
    )
    .await;
    let committed = read_response(&mut classic_responses, 204).await;
    assert_eq!(committed["ok"], true, "{committed}");
    assert_eq!(committed["payload"]["revision"], 2);
    assert_eq!(
        committed["payload"]["agentQuotas"]["historySync"]["mode"],
        "separate"
    );
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
    assert!(!actor.runtime_store.confirm_project_removal().await.unwrap());
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
    assert!(
        !actor.runtime_store.confirm_project_removal().await.unwrap(),
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
    assert!(
        !actor.runtime_store.confirm_project_removal().await.unwrap(),
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

/// Two isolated authenticated app clients exercise the actual Host request
/// router and SQLite expectedRevision transaction; no production Host runs.
#[tokio::test]
async fn agent_hook_host_cas_rejects_stale_classic_native_writers_and_preserves_others() {
    let dir = tempfile::tempdir().unwrap();
    let (classic_handle, mut classic_responses) = ClientHandle::test_channels();
    let (native_handle, mut native_responses) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &dir,
        HashMap::from([
            (1, local_client(classic_handle)),
            (2, local_client(native_handle)),
        ]),
        HashMap::new(),
    )
    .await;

    // Both clients observe the same Host-owned snapshot.
    for (client_id, request_id) in [(1, 100), (2, 200)] {
        wire_call(
            &mut actor,
            client_id,
            request_id,
            "runtimeSettings.get",
            json!({}),
        )
        .await;
    }
    let classic_before = read_response(&mut classic_responses, 100).await;
    let native_before = read_response(&mut native_responses, 200).await;
    assert_eq!(classic_before["ok"], true, "{classic_before}");
    assert_eq!(native_before["ok"], true, "{native_before}");
    assert_eq!(
        classic_before["payload"]["revision"],
        native_before["payload"]["revision"]
    );
    let observed = classic_before["payload"]["revision"].as_i64().unwrap();

    // Classic changes an unrelated field, which must also invalidate a
    // pending Native Hook CAS that was based on the old global revision.
    wire_call(
        &mut actor,
        1,
        101,
        "runtimeSettings.update",
        json!({"expectedRevision": observed, "confirmProjectRemoval": false}),
    )
    .await;
    let classic_changed = read_response(&mut classic_responses, 101).await;
    assert_eq!(classic_changed["ok"], true, "{classic_changed}");
    assert_eq!(classic_changed["payload"]["revision"], observed + 1);

    wire_call(
        &mut actor,
        2,
        201,
        "runtimeSettings.update",
        json!({"expectedRevision": observed,
            "agentStatusHooks": {"codex": true, "claude": false}}),
    )
    .await;
    let stale_other_setting = read_response(&mut native_responses, 201).await;
    assert_eq!(stale_other_setting["ok"], false, "{stale_other_setting}");
    assert_eq!(
        stale_other_setting["errorCode"],
        "runtime_settings_revision_conflict"
    );
    assert_eq!(
        stale_other_setting["errorDetails"]["expectedRevision"],
        observed
    );
    assert_eq!(
        stale_other_setting["errorDetails"]["actualRevision"],
        observed + 1
    );
    let unchanged = actor.runtime_settings_snapshot().await.unwrap();
    assert_eq!(unchanged["revision"], observed + 1);
    assert_eq!(unchanged["confirmProjectRemoval"], false);
    assert_ne!(unchanged["agentStatusHooks"]["codex"], true);

    // A refreshed Native client changes only one Provider. Other settings
    // retain their values and Host advances its atomic revision once.
    wire_call(
        &mut actor,
        2,
        202,
        "runtimeSettings.update",
        json!({"expectedRevision": observed + 1,
            "agentStatusHooks": {"codex": true, "claude": false}}),
    )
    .await;
    let native_written = read_response(&mut native_responses, 202).await;
    assert_eq!(native_written["ok"], true, "{native_written}");
    assert_eq!(native_written["payload"]["revision"], observed + 2);
    assert_eq!(native_written["payload"]["agentStatusHooks"]["codex"], true);
    assert_eq!(
        native_written["payload"]["agentStatusHooks"]["claude"],
        false
    );

    // Classic's now-stale Hook toggle must not roll back the Native change.
    wire_call(
        &mut actor,
        1,
        102,
        "runtimeSettings.update",
        json!({"expectedRevision": observed + 1,
            "agentStatusHooks": {"codex": false, "claude": true}}),
    )
    .await;
    let stale_hooks = read_response(&mut classic_responses, 102).await;
    assert_eq!(stale_hooks["ok"], false, "{stale_hooks}");
    assert_eq!(
        stale_hooks["errorCode"],
        "runtime_settings_revision_conflict"
    );
    assert_eq!(stale_hooks["errorDetails"]["actualRevision"], observed + 2);
    let preserved = actor.runtime_settings_snapshot().await.unwrap();
    assert_eq!(preserved["revision"], observed + 2);
    assert_eq!(preserved["agentStatusHooks"]["codex"], true);
    assert_eq!(preserved["agentStatusHooks"]["claude"], false);
    assert_eq!(preserved["confirmProjectRemoval"], false);

    // Fresh Classic settings can update Claude without losing Codex.
    wire_call(
        &mut actor,
        1,
        103,
        "runtimeSettings.update",
        json!({"expectedRevision": observed + 2,
            "agentStatusHooks": {"codex": true, "claude": true}}),
    )
    .await;
    let merged = read_response(&mut classic_responses, 103).await;
    assert_eq!(merged["ok"], true, "{merged}");
    assert_eq!(merged["payload"]["revision"], observed + 3);
    assert_eq!(merged["payload"]["agentStatusHooks"]["codex"], true);
    assert_eq!(merged["payload"]["agentStatusHooks"]["claude"], true);
    assert_eq!(merged["payload"]["confirmProjectRemoval"], false);
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
    assert!(actor.runtime_store.confirm_project_removal().await.unwrap());
}
