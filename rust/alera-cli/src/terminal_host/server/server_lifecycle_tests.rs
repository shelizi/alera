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
