use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;

use super::actor_test_harness::{local_client, test_actor};
use super::deferred_admission::{
    DeferredAdmission, DeferredRequestClass, DEFERRED_REQUEST_BACKPRESSURE_CODE,
};
use crate::terminal_host::client::ClientHandle;
use crate::terminal_host::host_error::HostError;

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
async fn bulk_beyond_the_dispatch_reserve_is_rejected() {
    let admission = Arc::new(DeferredAdmission::paused_with_limits(1, 3, 1));
    admission.add_test_permits(1);
    let (blocker_release_tx, blocker_release_rx) = tokio::sync::oneshot::channel::<()>();
    admission
        .schedule(
            DeferredRequestClass::Bulk,
            "test.blocker",
            Some(1),
            async move {
                let _ = blocker_release_rx.await;
            },
        )
        .unwrap();
    admission
        .schedule(DeferredRequestClass::Bulk, "test.queued", Some(1), async {})
        .unwrap();

    let error = admission
        .schedule(
            DeferredRequestClass::Bulk,
            "test.rejected",
            Some(1),
            async {},
        )
        .expect_err("a bulk request past the noncritical share must be rejected");
    let HostError::Conflict {
        code,
        message,
        details,
    } = &error
    else {
        panic!("expected a typed conflict, got {error:?}");
    };
    assert_eq!(code, DEFERRED_REQUEST_BACKPRESSURE_CODE);
    assert_eq!(message, "The runtime host is busy. Retry the request.");
    assert_eq!(details["requestType"], "test.rejected");
    assert_eq!(details["requestClass"], "bulk");
    assert_eq!(details["active"], 1);
    assert_eq!(details["pending"], 1);
    assert_eq!(details["capacity"], 3);
    assert_eq!(
        error.wire_response(7)["errorCode"],
        DEFERRED_REQUEST_BACKPRESSURE_CODE
    );

    let snapshot = admission.snapshot();
    assert_eq!(snapshot["active"], 1);
    assert_eq!(snapshot["pending"], 1);
    assert_eq!(snapshot["activeLimit"], 1);
    assert_eq!(snapshot["capacity"], 3);
    assert_eq!(snapshot["rejected"], 1);
    let rejected = request_type_entry(&snapshot, "test.rejected");
    assert_eq!(rejected["requestClass"], "bulk");
    assert_eq!(rejected["rejected"], 1);
    assert_eq!(rejected["queued"], 0);
    let queued = request_type_entry(&snapshot, "test.queued");
    assert_eq!(queued["queued"], 1);
    assert_eq!(queued["pending"], 1);
    let blocker = request_type_entry(&snapshot, "test.blocker");
    assert_eq!(blocker["active"], 1);
    assert_eq!(blocker["started"], 1);

    admission
        .schedule(
            DeferredRequestClass::DispatchCritical,
            "test.critical",
            None,
            async {},
        )
        .expect("the dispatch reserve still admits critical work");
    assert_eq!(admission.snapshot()["pending"], 2);
    blocker_release_tx.send(()).unwrap();
}

#[tokio::test]
async fn dispatch_critical_jobs_start_before_queued_bulk_work() {
    let admission = Arc::new(DeferredAdmission::paused_with_limits(2, 8, 1));
    admission.add_test_permits(1);
    let (blocker_release_tx, blocker_release_rx) = tokio::sync::oneshot::channel::<()>();
    admission
        .schedule(
            DeferredRequestClass::Bulk,
            "test.blocker",
            Some(1),
            async move {
                let _ = blocker_release_rx.await;
            },
        )
        .unwrap();
    let (bulk_started_tx, mut bulk_started_rx) = tokio::sync::mpsc::unbounded_channel();
    admission
        .schedule(
            DeferredRequestClass::Bulk,
            "test.bulk",
            Some(1),
            async move {
                let _ = bulk_started_tx.send(());
            },
        )
        .unwrap();
    let (critical_started_tx, mut critical_started_rx) = tokio::sync::mpsc::unbounded_channel();
    let (critical_release_tx, critical_release_rx) = tokio::sync::oneshot::channel::<()>();
    admission
        .schedule(
            DeferredRequestClass::DispatchCritical,
            "test.critical",
            None,
            async move {
                let _ = critical_started_tx.send(());
                let _ = critical_release_rx.await;
            },
        )
        .unwrap();

    tokio::time::sleep(Duration::from_millis(30)).await;

    admission.add_test_permits(1);
    tokio::time::timeout(Duration::from_secs(1), critical_started_rx.recv())
        .await
        .expect("the dispatch-critical job should start ahead of queued bulk work")
        .unwrap();
    assert!(
        bulk_started_rx.try_recv().is_err(),
        "queued bulk work must not start before the dispatch-critical job"
    );

    let snapshot = admission.snapshot();
    let critical = request_type_entry(&snapshot, "test.critical");
    assert!(critical["queueWaitMs"].as_u64().unwrap() >= 20);
    assert_eq!(critical["maxQueueWaitMs"], critical["queueWaitMs"]);

    critical_release_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(1), bulk_started_rx.recv())
        .await
        .expect("the queued bulk job should start once critical work finishes")
        .unwrap();
    blocker_release_tx.send(()).unwrap();
}

#[tokio::test]
async fn backpressured_bulk_requests_do_not_stall_control_requests() {
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

    actor
        .handle_line(
            1,
            json!({
                "id": 62,
                "type": "hostDirectory.list",
                "payload": {"path": dir.path().to_string_lossy()},
            })
            .to_string(),
        )
        .await;
    let rejected = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("a saturated deferred budget must answer with backpressure")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(rejected["id"], 62);
    assert_eq!(rejected["ok"], false);
    assert_eq!(rejected["errorCode"], DEFERRED_REQUEST_BACKPRESSURE_CODE);
    assert_eq!(
        rejected["errorDetails"]["requestType"],
        "hostDirectory.list"
    );
    assert_eq!(rejected["errorDetails"]["requestClass"], "bulk");
    assert_eq!(rejected["errorDetails"]["active"], 1);
    assert_eq!(rejected["errorDetails"]["pending"], 1);
    assert_eq!(rejected["errorDetails"]["capacity"], 2);

    actor
        .handle_line(
            1,
            json!({"id": 63, "type": "status.get", "payload": {}}).to_string(),
        )
        .await;
    let status = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("status.get must answer while deferred work is backpressured")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(status["id"], 63);
    assert_eq!(status["ok"], true);
    let admission = &status["payload"]["deferredAdmission"];
    assert_eq!(admission["active"], 1);
    assert_eq!(admission["pending"], 1);
    assert_eq!(admission["rejected"], 1);
    let fill = request_type_entry(admission, "test.fill");
    assert_eq!(fill["active"], 1);
    assert_eq!(fill["pending"], 1);
    let rejected_type = request_type_entry(admission, "hostDirectory.list");
    assert_eq!(rejected_type["rejected"], 1);

    actor
        .handle_line(
            1,
            json!({"id": 64, "type": "host.shutdown", "payload": {"force": true}}).to_string(),
        )
        .await;
    let shutdown = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("host.shutdown must answer while deferred work is backpressured")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(shutdown["id"], 64);
    assert_eq!(shutdown["ok"], true);
    assert_eq!(shutdown["payload"]["stopped"], true);
    assert_eq!(shutdown["payload"]["forced"], true);

    blocker_release_tx.send(()).unwrap();
}

#[tokio::test]
async fn disconnect_drops_queued_owner_jobs_while_active_jobs_finish() {
    let admission = Arc::new(DeferredAdmission::paused_with_limits(1, 8, 1));
    admission.add_test_permits(1);
    let (active_release_tx, active_release_rx) = tokio::sync::oneshot::channel::<()>();
    let (active_finished_tx, active_finished_rx) = tokio::sync::oneshot::channel::<()>();
    admission
        .schedule(
            DeferredRequestClass::Bulk,
            "test.active",
            Some(7),
            async move {
                let _ = active_release_rx.await;
                let _ = active_finished_tx.send(());
            },
        )
        .unwrap();
    let (dropped_tx, mut dropped_rx) = tokio::sync::oneshot::channel::<()>();
    admission
        .schedule(
            DeferredRequestClass::Bulk,
            "test.queued",
            Some(7),
            async move {
                let _dropped_tx = dropped_tx;
                std::future::pending::<()>().await;
            },
        )
        .unwrap();
    let (other_finished_tx, other_finished_rx) = tokio::sync::oneshot::channel::<()>();
    admission
        .schedule(
            DeferredRequestClass::Bulk,
            "test.other",
            Some(9),
            async move {
                let _ = other_finished_tx.send(());
            },
        )
        .unwrap();

    admission.disconnect_client(7);

    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(1), &mut dropped_rx).await,
        Ok(Err(_))
    ));
    let snapshot = admission.snapshot();
    assert_eq!(snapshot["pending"], 1);
    assert_eq!(snapshot["disconnected"], 2);
    let queued = request_type_entry(&snapshot, "test.queued");
    assert_eq!(queued["queued"], 1);
    assert_eq!(queued["disconnected"], 1);
    let active = request_type_entry(&snapshot, "test.active");
    assert_eq!(active["active"], 1);
    assert_eq!(active["disconnected"], 1);

    active_release_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(1), active_finished_rx)
        .await
        .expect("the active job should still finish after its owner disconnected")
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), other_finished_rx)
        .await
        .expect("another client's queued job should run once the slot frees")
        .unwrap();
}

#[tokio::test]
async fn newly_routed_requests_are_rejected_under_saturation_while_status_answers() {
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

    // Fill admission: 1 active slot, 1 pending slot (total capacity = 2).
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

    // 1. Newly routed agentQuota.snapshot with forceRefresh: true must be rejected
    actor
        .handle_line(
            1,
            json!({
                "id": 70,
                "type": "agentQuota.snapshot",
                "payload": {"forceRefresh": true},
            })
            .to_string(),
        )
        .await;

    let quota_rejected = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("saturated admission must answer agentQuota.snapshot with backpressure")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(quota_rejected["id"], 70);
    assert_eq!(quota_rejected["ok"], false);
    assert_eq!(
        quota_rejected["errorCode"],
        DEFERRED_REQUEST_BACKPRESSURE_CODE
    );
    assert_eq!(
        quota_rejected["errorDetails"]["requestType"],
        "agentQuota.snapshot"
    );
    assert_eq!(quota_rejected["errorDetails"]["requestClass"], "bulk");

    // 2. Newly routed workspace.storageImpact must be rejected and not leak managed_workspace_jobs
    actor
        .handle_line(
            1,
            json!({
                "id": 71,
                "type": "workspace.storageImpact",
                "payload": {"id": "ws-test"},
            })
            .to_string(),
        )
        .await;

    let storage_rejected = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("saturated admission must answer workspace.storageImpact with backpressure")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(storage_rejected["id"], 71);
    assert_eq!(storage_rejected["ok"], false);
    assert_eq!(
        storage_rejected["errorCode"],
        DEFERRED_REQUEST_BACKPRESSURE_CODE
    );
    assert_eq!(
        storage_rejected["errorDetails"]["requestType"],
        "workspace.storageImpact"
    );
    assert_eq!(actor.managed_workspace_jobs, 0);

    // 3. status.get still answers promptly while saturated
    actor
        .handle_line(
            1,
            json!({"id": 72, "type": "status.get", "payload": {}}).to_string(),
        )
        .await;

    let status = tokio::time::timeout(Duration::from_secs(1), responses.recv())
        .await
        .expect("status.get must answer while admission is saturated")
        .unwrap()
        .as_json()
        .unwrap();
    assert_eq!(status["id"], 72);
    assert_eq!(status["ok"], true);

    let admission_snapshot = &status["payload"]["deferredAdmission"];
    assert_eq!(admission_snapshot["active"], 1);
    assert_eq!(admission_snapshot["pending"], 1);
    assert_eq!(admission_snapshot["rejected"], 2);

    let quota_entry = request_type_entry(admission_snapshot, "agentQuota.snapshot");
    assert_eq!(quota_entry["rejected"], 1);
    assert_eq!(quota_entry["pending"], 0);
    assert_eq!(quota_entry["queued"], 0);

    let storage_entry = request_type_entry(admission_snapshot, "workspace.storageImpact");
    assert_eq!(storage_entry["rejected"], 1);
    assert_eq!(storage_entry["pending"], 0);
    assert_eq!(storage_entry["queued"], 0);

    blocker_release_tx.send(()).unwrap();
}
