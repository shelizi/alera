//! Host admission guarantees for selecting a live Agent's existing UI Tab.
use std::collections::HashMap;

use serde_json::json;
use tokio::sync::mpsc;

use crate::terminal_host::client::ClientHandle;
use crate::terminal_host::orchestration::agent_presence::AgentPresenceState;
use crate::terminal_host::session::Session;

use super::actor_test_harness::test_actor;
use super::ClientState;

#[tokio::test]
async fn focus_only_resolves_live_local_agent_and_never_accepts_client_tab_id() {
    let dir = tempfile::tempdir().unwrap();
    let (app_events, mut app_output) = mpsc::unbounded_channel();
    let (app_pty, _) = mpsc::channel(4);
    let (cli_events, _cli_output) = mpsc::unbounded_channel();
    let (cli_pty, _) = mpsc::channel(4);
    let mut foreign = Session::driver_test_stub("foreign", 80, 24);
    foreign.workspace_id = "other-workspace".to_string();
    let mut actor = test_actor(
        &dir,
        HashMap::from([
            (
                1,
                ClientState::local(ClientHandle::new(app_events, app_pty), true),
            ),
            (
                2,
                ClientState::local(ClientHandle::new(cli_events, cli_pty), false),
            ),
        ]),
        HashMap::from([
            (
                "sender".to_string(),
                Session::driver_test_stub("sender", 80, 24),
            ),
            (
                "target".to_string(),
                Session::driver_test_stub("target", 80, 24),
            ),
            ("foreign".to_string(), foreign),
        ]),
    )
    .await;
    actor
        .agent_presence
        .update("target", "claude".into(), AgentPresenceState::Working);
    actor
        .agent_presence
        .update("foreign", "codex".into(), AgentPresenceState::Working);
    let token = actor.mcp_ui.mint_token("sender");
    actor
        .handle_mcp_ui_request(1, "uiControl.register", &json!({"windowId":"main"}))
        .await
        .unwrap();
    let mut request = json!({
        "sessionId":"sender","workspaceId":"workspace",
        "capability":token,"action":"focus_agent",
        "arguments":{"handle":"target"},
    });
    let accepted = actor
        .handle_mcp_ui_request(2, "uiControl.request", &request)
        .await
        .unwrap();
    assert_eq!(accepted["status"], "accepted");
    assert_eq!(accepted["applied"], false);
    assert!(app_output.try_recv().is_ok());
    request["arguments"]["handle"] = json!("foreign");
    assert!(actor
        .handle_mcp_ui_request(2, "uiControl.request", &request)
        .await
        .is_err());
    request["arguments"]["handle"] = json!("missing");
    assert!(actor
        .handle_mcp_ui_request(2, "uiControl.request", &request)
        .await
        .is_err());
    request["arguments"]["handle"] = json!("target");
    request["arguments"]["tabId"] = json!("foreign-tab");
    assert!(actor
        .handle_mcp_ui_request(2, "uiControl.request", &request)
        .await
        .is_err());
    request["arguments"] = json!({"handle":"target"});
    actor.sessions.remove("target");
    assert!(actor
        .handle_mcp_ui_request(2, "uiControl.request", &request)
        .await
        .is_err());
}
