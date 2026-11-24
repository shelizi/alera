use std::collections::HashMap;

use serde_json::Value;

use super::super::actor_test_harness::test_actor;
use crate::terminal_host::control_file;
use crate::terminal_host::protocol::TerminalHostConfig;

#[tokio::test]
async fn promotion_keeps_actor_state_and_cannot_be_downgraded() {
    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    control_file::write_control_file(&actor.control_file_path, 45678, "token", false).unwrap();
    actor.managed_workspace_jobs = 2;
    actor.shutdown_gen = 7;

    let response = actor.promote_persistent().unwrap();

    assert_eq!(response["persistent"], true);
    assert!(actor.config.persistent);
    assert_eq!(actor.managed_workspace_jobs, 2);
    assert!(actor.shutdown_gen > 7);
    let control: Value =
        serde_json::from_str(&std::fs::read_to_string(&actor.control_file_path).unwrap()).unwrap();
    assert_eq!(control["persistent"], true);

    actor.apply_config(TerminalHostConfig::default()).await;
    assert!(actor.config.persistent);
    assert_eq!(actor.managed_workspace_jobs, 2);
}

async fn wait_for_cancelled(handles: &[tokio::task::AbortHandle]) {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while handles.iter().any(|handle| !handle.is_finished()) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("cancelled idle tasks must finish before their original deadlines");
}

#[tokio::test]
async fn repeatedly_rearming_idle_shutdown_releases_every_superseded_task() {
    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    let (inbox, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;
    actor.config.persistent = false;
    actor.config.empty_shutdown_delay_seconds = 3600;

    actor.schedule_shutdown_if_idle();
    tokio::task::yield_now().await;
    let mut superseded = Vec::new();
    for _ in 0..256 {
        superseded.push(actor.shutdown_timer.task.as_ref().unwrap().abort_handle());
        actor.schedule_shutdown_if_idle();
    }
    let current = actor.shutdown_timer.task.as_ref().unwrap().abort_handle();
    wait_for_cancelled(&superseded).await;
    assert!(!current.is_finished());
    assert!(receiver.try_recv().is_err());

    // An event that was already queued before cancellation cannot disarm the
    // current timer, even though the superseded task is now gone.
    actor.handle_shutdown_tick(actor.shutdown_gen - 1).await;
    assert!(actor.shutdown_timer.task.is_some());
    assert!(!actor.disposed);

    actor.cancel_shutdown_timer();
    wait_for_cancelled(&[current]).await;
    assert!(actor.shutdown_timer.task.is_none());
    assert!(receiver.try_recv().is_err());
}

#[tokio::test]
async fn idle_shutdown_emits_only_the_current_generation_and_disarms_on_delivery() {
    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    let (inbox, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;
    actor.config.persistent = false;
    actor.config.empty_shutdown_delay_seconds = 3600;
    actor.schedule_shutdown_if_idle();
    let old = actor.shutdown_timer.task.as_ref().unwrap().abort_handle();

    actor.config.empty_shutdown_delay_seconds = 0;
    actor.schedule_shutdown_if_idle();
    let expected = actor.shutdown_gen;
    let command = tokio::time::timeout(std::time::Duration::from_secs(2), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    let super::ServerCommand::ShutdownTick { generation } = command else {
        panic!("expected the current shutdown tick");
    };
    assert_eq!(generation, expected);
    wait_for_cancelled(&[old]).await;
    actor.handle_shutdown_tick(generation).await;
    assert!(actor.disposed);
    assert!(actor.shutdown_timer.task.is_none());
    assert!(receiver.try_recv().is_err());
}

#[tokio::test]
async fn authenticated_client_or_actor_drop_cancels_idle_shutdown_task() {
    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    let (inbox, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;
    actor.config.persistent = false;
    actor.config.empty_shutdown_delay_seconds = 3600;
    actor.schedule_shutdown_if_idle();
    let old = actor.shutdown_timer.task.as_ref().unwrap().abort_handle();

    let (handle, _client_rx) = crate::terminal_host::client::ClientHandle::test_channels();
    actor
        .clients
        .insert(1, super::super::actor_test_harness::local_client(handle));
    actor.schedule_shutdown_if_idle();
    wait_for_cancelled(&[old]).await;
    assert!(actor.shutdown_timer.task.is_none());
    assert!(!actor.disposed);
    assert!(receiver.try_recv().is_err());

    actor.clients.clear();
    actor.schedule_shutdown_if_idle();
    let current = actor.shutdown_timer.task.as_ref().unwrap().abort_handle();
    drop(actor);
    wait_for_cancelled(&[current]).await;
    assert!(matches!(
        receiver.try_recv(),
        Err(tokio::sync::mpsc::error::TryRecvError::Disconnected)
    ));
}
