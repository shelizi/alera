//! Authenticated settings command admission and scalar result ACK.
use std::collections::HashMap;

use serde_json::json;
use tokio::sync::mpsc;

use crate::terminal_host::client::ClientHandle;
use crate::terminal_host::session::Session;

use super::actor_test_harness::test_actor;
use super::ClientState;

#[tokio::test]
async fn settings_result_requires_genuine_app_ack_and_known_scalar_type() {
    let dir = tempfile::tempdir().unwrap();
    let (app_events, mut app_output) = mpsc::unbounded_channel();
    let (app_pty, _) = mpsc::channel(4);
    let (cli_events, _cli_output) = mpsc::unbounded_channel();
    let (cli_pty, _) = mpsc::channel(4);
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
        HashMap::from([("agent".into(), Session::driver_test_stub("agent", 80, 24))]),
    )
    .await;
    actor
        .handle_mcp_ui_request(
            1,
            "uiControl.register",
            &json!({
                "windowId":"main",
            }),
        )
        .await
        .unwrap();
    let token = actor.mcp_ui.mint_token("agent");
    let request = json!({
        "sessionId":"agent", "workspaceId":"workspace",
        "capability":token, "action":"update_setting",
        "arguments":{"path":"terminal.fontSize","value":16,"expectedValue":13},
    });
    let accepted = actor
        .handle_mcp_ui_request(2, "uiControl.request", &request)
        .await
        .unwrap();
    assert_eq!(accepted["status"], "accepted");
    assert_eq!(accepted["applied"], false);
    assert!(app_output.try_recv().is_ok());
    let id = accepted["requestId"].as_str().unwrap();
    let receipt = json!({
        "sessionId":"agent", "workspaceId":"workspace",
        "capability":token, "requestId":id,
    });
    assert_eq!(
        actor
            .handle_mcp_ui_request(2, "uiControl.status", &receipt,)
            .await
            .unwrap()["settingValue"],
        serde_json::Value::Null
    );
    assert!(
        actor
            .handle_mcp_ui_request(
                2,
                "uiControl.complete",
                &json!({
                    "requestId":id,"applied":true,"settingValue":16,
                }),
            )
            .await
            .is_err(),
        "a CLI cannot claim Flutter settings were applied"
    );
    assert!(
        actor
            .handle_mcp_ui_request(
                1,
                "uiControl.complete",
                &json!({
                    "requestId":id,"applied":true,
                    "settingValue":{"privateSecret":"should never be exposed"},
                }),
            )
            .await
            .is_err(),
        "only known scalar setting values may traverse Host"
    );
    assert!(
        actor
            .handle_mcp_ui_request(
                1,
                "uiControl.complete",
                &json!({
                    "requestId":id,"applied":true,
                }),
            )
            .await
            .is_err(),
        "an ACK without actual readback is not successful"
    );
    actor
        .handle_mcp_ui_request(
            1,
            "uiControl.complete",
            &json!({
                "requestId":id,"applied":true,"settingValue":16,
            }),
        )
        .await
        .unwrap();
    let status = actor
        .handle_mcp_ui_request(2, "uiControl.status", &receipt)
        .await
        .unwrap();
    assert_eq!(status["status"], "applied");
    assert_eq!(status["settingPath"], "terminal.fontSize");
    assert_eq!(status["settingValue"], 16);
    assert!(
        actor
            .handle_mcp_ui_request(
                1,
                "uiControl.complete",
                &json!({
                    "requestId":id,"applied":true,"settingValue":17,
                }),
            )
            .await
            .is_err(),
        "repeat ACK must not overwrite completed receipts"
    );
}
