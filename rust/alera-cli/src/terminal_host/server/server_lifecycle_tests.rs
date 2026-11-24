use super::*;

use crate::terminal_host::orchestration::agent_presence::AgentPresenceState;

#[tokio::test]
async fn last_app_client_disconnect_preserves_host_agent_presence() {
    let dir = tempfile::tempdir().unwrap();
    let (first_app_handle, _first_app_rx) = ClientHandle::test_channels();
    let (second_app_handle, _second_app_rx) = ClientHandle::test_channels();
    let (cli_handle, _cli_rx) = ClientHandle::test_channels();
    let mut actor = actor_test_harness::test_actor(
        &dir,
        HashMap::from([
            (1, ClientState::local(first_app_handle, true)),
            (2, ClientState::local(second_app_handle, true)),
            (3, ClientState::local(cli_handle, false)),
        ]),
        HashMap::new(),
    )
    .await;
    actor
        .agent_presence
        .update("term-1", "claude".to_string(), AgentPresenceState::Done);

    actor.dispose_client(3).await;
    assert!(actor.agent_presence.is_injection_ready("term-1"));
    actor.dispose_client(1).await;
    assert!(actor.agent_presence.is_injection_ready("term-1"));
    actor.dispose_client(2).await;

    assert!(actor.agent_presence.is_injection_ready("term-1"));
}

#[tokio::test]
async fn disconnect_checkpoints_only_attached_sessions_while_another_client_remains() {
    let dir = tempfile::tempdir().unwrap();
    let (departing, _departing_rx) = ClientHandle::test_terminal_channels();
    let (remaining, mut remaining_rx) = ClientHandle::test_terminal_channels();
    let mut attached = Session::driver_test_stub("attached", 120, 40);
    attached.attach(1);
    attached.attach(2);
    assert!(attached.register_view_lease(1, "departing-view"));
    assert!(attached.register_view_lease(2, "remaining-view"));
    attached.append_output(b"attached output");
    let attached_tick = attached.arm_checkpoint().unwrap();
    let mut sessions = HashMap::from([("attached".into(), attached)]);

    // Exercise both attached and unattended sessions, including a client
    // that left transient pause/cursor state without an attachment.
    for i in 0..63 {
        let id = format!("unrelated-{i}");
        let mut session = Session::driver_test_stub(&id, 120, 40);
        if i % 2 == 0 {
            session.attach(2);
        }
        session.mark_output_backpressured(1);
        session.set_delivered_output_cursor(1, 0);
        session.append_output(b"unrelated output");
        assert_eq!(session.arm_checkpoint(), Some(0));
        sessions.insert(id, session);
    }
    let mut actor = actor_test_harness::test_actor(
        &dir,
        HashMap::from([
            (1, ClientState::local(departing, false)),
            (2, ClientState::local(remaining, true)),
        ]),
        sessions,
    )
    .await;

    actor.dispose_client(1).await;
    assert!(!actor.clients.contains_key(&1));
    assert!(actor.clients.contains_key(&2));
    let attached = actor.sessions.get_mut("attached").unwrap();
    assert!(!attached.clients.contains(&1));
    assert!(!attached.has_view_lease(1, "departing-view"));
    assert!(attached.has_view_lease(2, "remaining-view"));
    assert_eq!(attached.output_batch_len(), 0);
    assert_eq!(attached.durable_output_batch_len(), 0);
    assert!(!attached.checkpoint_due(attached_tick));
    assert_eq!(attached.delivered_output_cursor(2), Some(15));
    let frame = remaining_rx.try_recv().unwrap();
    let ClientFrame::SequencedTerminal { frame, .. } = frame else {
        panic!("expected a sequenced terminal output frame");
    };
    let ClientFrame::SharedOutput(output) = *frame else {
        panic!("expected the final shared output for the attached session");
    };
    assert_eq!(output.data().as_ref(), b"attached output");
    assert!(remaining_rx.try_recv().is_err());
    let persisted = actor
        .history
        .read("attached", usize::MAX)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted.buffer, b"attached output");
    assert!(persisted.running);

    for i in 0..63 {
        let id = format!("unrelated-{i}");
        let session = actor.sessions.get_mut(&id).unwrap();
        assert_eq!(session.clients.contains(&2), i % 2 == 0);
        assert!(!session.output_resync_pending(1));
        assert_eq!(session.delivered_output_cursor(1), None);
        assert_eq!(session.output_batch_len(), 16);
        assert_eq!(session.durable_output_batch_len(), 16);
        assert_eq!(session.arm_checkpoint(), None);
        assert!(
            actor.history.read(&id, usize::MAX).await.unwrap().is_none(),
            "{id}"
        );
    }
    // The original scheduled checkpoint is still current and persists the
    // bytes normally; an unrelated disconnect did not supersede it.
    actor.handle_checkpoint_tick("unrelated-0".into(), 0).await;
    assert_eq!(actor.sessions["unrelated-0"].durable_output_batch_len(), 0);
    let persisted = actor
        .history
        .read("unrelated-0", usize::MAX)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted.buffer, b"unrelated output");
    assert!(actor.history.flush_pending(Duration::from_secs(5)).await);
}

#[tokio::test]
async fn last_authenticated_disconnect_keeps_full_history_durability_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, _rx) = ClientHandle::test_channels();
    let mut detached = Session::driver_test_stub("detached", 120, 40);
    detached.append_output(b"unattended output");
    let tick = detached.arm_checkpoint().unwrap();
    let (unauth_handle, _unauth_rx) = ClientHandle::test_channels();
    let mut unauthenticated = ClientState::local(unauth_handle, false);
    unauthenticated.authenticated = false;
    let mut actor = actor_test_harness::test_actor(
        &dir,
        HashMap::from([(1, ClientState::local(handle, true)), (2, unauthenticated)]),
        HashMap::from([("detached".into(), detached)]),
    )
    .await;
    actor.dispose_client(1).await;
    assert!(actor.clients.contains_key(&2));
    assert!(!actor.disposed);
    assert!(actor.sessions["detached"].running());
    assert_eq!(actor.sessions["detached"].durable_output_batch_len(), 0);
    assert!(!actor
        .sessions
        .get_mut("detached")
        .unwrap()
        .checkpoint_due(tick));
    let checkpoint = actor
        .history
        .read("detached", usize::MAX)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(checkpoint.buffer, b"unattended output");
    assert!(checkpoint.running);
    assert!(actor.history.flush_pending(Duration::from_secs(5)).await);
}

#[tokio::test]
async fn unauthenticated_disconnect_does_not_snapshot_background_sessions() {
    let dir = tempfile::tempdir().unwrap();
    let (handle, _rx) = ClientHandle::test_channels();
    let mut unauthenticated = ClientState::local(handle, false);
    unauthenticated.authenticated = false;
    let mut session = Session::driver_test_stub("background", 120, 40);
    session.append_output(b"background output");
    let tick = session.arm_checkpoint().unwrap();
    let mut actor = actor_test_harness::test_actor(
        &dir,
        HashMap::from([(1, unauthenticated)]),
        HashMap::from([("background".into(), session)]),
    )
    .await;
    actor.dispose_client(1).await;
    assert_eq!(actor.sessions["background"].durable_output_batch_len(), 17);
    assert!(actor
        .history
        .read("background", usize::MAX)
        .await
        .unwrap()
        .is_none());
    actor
        .handle_checkpoint_tick("background".into(), tick)
        .await;
    let checkpoint = actor
        .history
        .read("background", usize::MAX)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(checkpoint.buffer, b"background output");
    assert!(actor.history.flush_pending(Duration::from_secs(5)).await);
}
