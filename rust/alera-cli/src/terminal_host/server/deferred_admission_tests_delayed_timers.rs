use std::sync::Arc;
use std::time::Duration;

use super::deferred_admission::{
    DeferredAdmission, DeferredRequestClass, DEFERRED_REQUEST_BACKPRESSURE_CODE,
};
use super::deferred_admission_tests::request_type_entry;
use crate::terminal_host::host_error::HostError;

#[tokio::test]
async fn delayed_timers_do_not_consume_active_slots_or_metrics() {
    let admission = Arc::new(DeferredAdmission::paused_with_limits(1, 16, 0));
    for index in 0..8 {
        admission
            .schedule_delayed(
                Duration::from_secs(60),
                DeferredRequestClass::Maintenance,
                format!("test.timer.{index}"),
                None,
                async {},
            )
            .unwrap();
    }

    let snapshot = admission.snapshot();
    assert_eq!(snapshot["active"], 0);
    assert_eq!(snapshot["pending"], 0);
    assert!(snapshot["requestTypes"].as_array().unwrap().is_empty());

    admission.add_test_permits(1);
    let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
    let (finished_tx, finished_rx) = tokio::sync::oneshot::channel::<()>();
    admission
        .schedule(DeferredRequestClass::Bulk, "test.real", None, async move {
            let _ = release_rx.await;
            let _ = finished_tx.send(());
        })
        .unwrap();

    let snapshot = admission.snapshot();
    assert_eq!(snapshot["active"], 1);
    assert_eq!(snapshot["pending"], 0);
    assert!(snapshot["requestTypes"]
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| !entry["requestType"]
            .as_str()
            .unwrap()
            .starts_with("test.timer.")));
    assert_eq!(request_type_entry(&snapshot, "test.real")["active"], 1);

    release_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(1), finished_rx)
        .await
        .expect("the real admitted job should finish")
        .unwrap();
}

#[tokio::test]
async fn delayed_timer_delivers_when_admission_is_full() {
    let admission = Arc::new(DeferredAdmission::paused_with_limits(1, 3, 0));
    admission.add_test_permits(1);
    let (blocker_release_tx, blocker_release_rx) = tokio::sync::oneshot::channel::<()>();
    let (blocker_finished_tx, blocker_finished_rx) = tokio::sync::oneshot::channel::<()>();
    admission
        .schedule(
            DeferredRequestClass::Bulk,
            "test.blocker",
            None,
            async move {
                let _ = blocker_release_rx.await;
                let _ = blocker_finished_tx.send(());
            },
        )
        .unwrap();

    let (timer_tx, timer_rx) = tokio::sync::oneshot::channel::<()>();
    admission
        .schedule_delayed(
            Duration::from_millis(1),
            DeferredRequestClass::Maintenance,
            "test.timer",
            None,
            async move {
                let _ = timer_tx.send(());
            },
        )
        .unwrap();
    let (queued_tx, queued_rx) = tokio::sync::oneshot::channel::<()>();
    admission
        .schedule(
            DeferredRequestClass::Bulk,
            "test.queued",
            None,
            async move {
                let _ = queued_tx.send(());
            },
        )
        .unwrap();

    let snapshot = admission.snapshot();
    assert_eq!(snapshot["active"], 1);
    assert_eq!(snapshot["pending"], 1);
    assert!(snapshot["requestTypes"]
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry["requestType"] != "test.timer"));

    tokio::time::timeout(Duration::from_secs(1), timer_rx)
        .await
        .expect("a due timer must deliver even while admission is full")
        .unwrap();

    blocker_release_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(1), blocker_finished_rx)
        .await
        .expect("the blocker should finish")
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), queued_rx)
        .await
        .expect("queued admitted work should eventually run")
        .unwrap();
}

#[tokio::test]
async fn delayed_timer_rejection_is_synchronous() {
    let admission = Arc::new(DeferredAdmission::paused_with_limits(1, 1, 0));
    admission
        .schedule_delayed(
            Duration::from_secs(60),
            DeferredRequestClass::Maintenance,
            "test.timer.first",
            Some(7),
            async {},
        )
        .unwrap();

    let error = admission
        .schedule_delayed(
            Duration::from_secs(60),
            DeferredRequestClass::Maintenance,
            "test.timer.rejected",
            Some(7),
            async {},
        )
        .expect_err("the bounded timer lane must reject synchronously");
    let HostError::Conflict { code, details, .. } = error else {
        panic!("expected a typed conflict");
    };
    assert_eq!(code, DEFERRED_REQUEST_BACKPRESSURE_CODE);
    assert_eq!(details["requestType"], "test.timer.rejected");
    assert_eq!(details["active"], 0);
    assert_eq!(details["pending"], 0);
    assert_eq!(details["capacity"], 1);

    let snapshot = admission.snapshot();
    assert_eq!(snapshot["active"], 0);
    assert_eq!(snapshot["pending"], 0);
    assert!(snapshot["requestTypes"].as_array().unwrap().is_empty());
}
