use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use super::*;
use crate::terminal_host::client::ClientHandle;

use super::super::actor_test_harness::{mobile_client, test_actor};
use super::super::deferred_admission::DeferredAdmission;

#[test]
fn mutations_for_one_upload_are_serialized() {
    let ready = Arc::new(Barrier::new(3));
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let mut workers = Vec::new();

    for _ in 0..2 {
        let ready = Arc::clone(&ready);
        let active = Arc::clone(&active);
        let peak = Arc::clone(&peak);
        workers.push(thread::spawn(move || {
            ready.wait();
            with_upload_gate("same-upload", || {
                let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(current, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(25));
                active.fetch_sub(1, Ordering::SeqCst);
                Ok(())
            })
            .expect("gated mutation");
        }));
    }

    ready.wait();
    for worker in workers {
        worker.join().expect("worker");
    }
    assert_eq!(peak.load(Ordering::SeqCst), 1);
}

#[test]
fn different_uploads_can_mutate_concurrently() {
    let entered = Arc::new(Barrier::new(3));
    let mut workers = Vec::new();

    for upload_id in ["first-upload", "second-upload"] {
        let entered = Arc::clone(&entered);
        workers.push(thread::spawn(move || {
            with_upload_gate(upload_id, || {
                entered.wait();
                Ok(())
            })
            .expect("gated mutation");
        }));
    }

    entered.wait();
    for worker in workers {
        worker.join().expect("worker");
    }
}

#[test]
fn disconnected_upload_results_release_their_store_reservation() {
    let directory = tempfile::tempdir().expect("tempdir");
    let store = PromptFileStore::in_runtime_dir(directory.path());
    let reservation = store.start("orphaned.bin", 1).expect("start");
    let start_result = Ok(json!({"uploadId": reservation.upload_id}));

    cancel_orphaned_start(directory.path(), &start_result);

    assert_eq!(
        store.append_chunk(&reservation.upload_id, 0, b"x"),
        Err(super::super::prompt_file_store::PromptFileStoreError::Missing)
    );

    let reservation = store.start("completed.bin", 1).expect("start");
    store
        .append_chunk(&reservation.upload_id, 0, b"x")
        .expect("append");
    let complete_result = handle_prompt_file_request(
        directory.path().to_path_buf(),
        "mobile.promptFile.complete",
        &json!({"uploadId": reservation.upload_id}),
    )
    .expect("complete");
    let completed_path = complete_result["path"].as_str().unwrap().to_string();

    cancel_orphaned_start(directory.path(), &Ok(complete_result));

    assert!(!std::path::Path::new(&completed_path).exists());
    assert_eq!(
        store.append_chunk(&reservation.upload_id, 0, b"x"),
        Err(super::super::prompt_file_store::PromptFileStoreError::Missing)
    );
}

#[tokio::test]
async fn orphaned_start_cleanup_respects_deferred_io_budget() {
    let directory = tempfile::tempdir().expect("tempdir");
    let store = PromptFileStore::in_runtime_dir(directory.path());
    let mut actor = test_actor(&directory, HashMap::new(), HashMap::new()).await;
    actor.deferred_admission = Arc::new(DeferredAdmission::paused_with_limits(
        usize::MAX,
        usize::MAX,
        0,
    ));
    let orphaned = store.start("orphaned-budget.bin", 2).expect("start");

    actor.handle_mobile_prompt_file_finished(
        1,
        1,
        "mobile.promptFile.start",
        None,
        Ok(json!({"uploadId": orphaned.upload_id})),
    );
    assert_eq!(
        store
            .append_chunk(&orphaned.upload_id, 0, b"x")
            .expect("orphan cleanup should wait for shared I/O admission"),
        1
    );

    actor.deferred_admission.add_test_permits(1);
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if matches!(
                store.append_chunk(&orphaned.upload_id, 1, b"y"),
                Err(super::super::prompt_file_store::PromptFileStoreError::Missing)
            ) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("orphaned prompt file start should be cancelled after admission");
}

#[tokio::test]
async fn disconnected_upload_cleanup_respects_deferred_io_budget() {
    let directory = tempfile::tempdir().expect("tempdir");
    let store = PromptFileStore::in_runtime_dir(directory.path());
    let (handle, _receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &directory,
        HashMap::from([(1, mobile_client(handle, "phone"))]),
        HashMap::new(),
    )
    .await;
    actor.deferred_admission = Arc::new(DeferredAdmission::paused_with_limits(
        usize::MAX,
        usize::MAX,
        0,
    ));

    let disconnected = store.start("budgeted-disconnect.bin", 2).expect("start");
    actor.handle_mobile_prompt_file_finished(
        1,
        1,
        "mobile.promptFile.start",
        None,
        Ok(json!({"uploadId": disconnected.upload_id})),
    );
    actor.dispose_client(1).await;

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(
        store
            .append_chunk(&disconnected.upload_id, 0, b"x")
            .expect("cleanup must wait for shared I/O admission"),
        1
    );

    actor.deferred_admission.add_test_permits(1);
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if matches!(
                store.append_chunk(&disconnected.upload_id, 1, b"y"),
                Err(super::super::prompt_file_store::PromptFileStoreError::Missing)
            ) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("disconnected upload should be cancelled after budget capacity is released");
}

#[tokio::test]
async fn acknowledged_uploads_are_tracked_until_cancelled_or_disconnected() {
    let directory = tempfile::tempdir().expect("tempdir");
    let store = PromptFileStore::in_runtime_dir(directory.path());
    let (handle, _receiver) = ClientHandle::test_channels();
    let mut actor = test_actor(
        &directory,
        HashMap::from([(1, mobile_client(handle, "phone"))]),
        HashMap::new(),
    )
    .await;

    let cancelled = store.start("cancelled.bin", 1).expect("start");
    actor.handle_mobile_prompt_file_finished(
        1,
        1,
        "mobile.promptFile.start",
        None,
        Ok(json!({"uploadId": cancelled.upload_id})),
    );
    assert!(actor.mobile_prompt_file_uploads[&1].contains(&cancelled.upload_id));
    actor.handle_mobile_prompt_file_finished(
        1,
        2,
        "mobile.promptFile.cancel",
        Some(&cancelled.upload_id),
        Ok(json!({})),
    );
    assert!(!actor.mobile_prompt_file_uploads.contains_key(&1));

    let incomplete = store.start("incomplete.bin", 2).expect("start");
    store
        .append_chunk(&incomplete.upload_id, 0, b"x")
        .expect("append");
    actor.handle_mobile_prompt_file_finished(
        1,
        3,
        "mobile.promptFile.start",
        None,
        Ok(json!({"uploadId": incomplete.upload_id})),
    );
    let complete_result = handle_prompt_file_request(
        directory.path().to_path_buf(),
        "mobile.promptFile.complete",
        &json!({"uploadId": incomplete.upload_id}),
    );
    assert!(complete_result.is_err());
    actor.handle_mobile_prompt_file_finished(
        1,
        4,
        "mobile.promptFile.complete",
        Some(&incomplete.upload_id),
        complete_result,
    );
    assert!(!actor.mobile_prompt_file_uploads.contains_key(&1));
    assert_eq!(
        store.append_chunk(&incomplete.upload_id, 1, b"x"),
        Err(super::super::prompt_file_store::PromptFileStoreError::Missing)
    );

    let disconnected = store.start("disconnected.bin", 1).expect("start");
    actor.handle_mobile_prompt_file_finished(
        1,
        5,
        "mobile.promptFile.start",
        None,
        Ok(json!({"uploadId": disconnected.upload_id})),
    );
    actor.dispose_client(1).await;

    for _ in 0..50 {
        if matches!(
            store.append_chunk(&disconnected.upload_id, 0, b"x"),
            Err(super::super::prompt_file_store::PromptFileStoreError::Missing)
        ) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("disconnected upload reservation was not cancelled");
}
