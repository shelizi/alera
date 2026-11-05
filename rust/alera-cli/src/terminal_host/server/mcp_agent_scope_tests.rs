use std::collections::HashMap;

use serde_json::json;

use super::tests::{fixture, proof};
use crate::terminal_host::client::ClientHandle;
use crate::terminal_host::host_error::HostError;
use crate::terminal_host::orchestration::agent_presence::AgentPresenceState;
use crate::terminal_host::server::actor_test_harness::{local_client, test_actor};

#[tokio::test]
async fn cross_workspace_send_and_status_reject_spoofed_identity_without_side_effects() {
    let dir = tempfile::tempdir().unwrap();
    let (client, _) = ClientHandle::test_channels();
    let mut actor = test_actor(&dir, HashMap::from([(1, local_client(client))]), fixture()).await;
    actor
        .agent_presence
        .update("foreign", "codex".into(), AgentPresenceState::Working);
    actor
        .agent_presence
        .update("recipient", "codex".into(), AgentPresenceState::Working);
    let sender_token = actor.mcp_ui.mint_token("sender");
    let foreign_token = actor.mcp_ui.mint_token("foreign");
    actor.sessions.get_mut("recipient").unwrap().workspace_id = "other-workspace".into();
    let foreign_message = actor.handle_mcp_agent_request(1, "agentControl.send", &json!({
        "sessionId": "foreign", "workspaceId": "other-workspace", "capability": foreign_token,
        "toHandle": "recipient", "subject": "Private", "body": "Private message",
        "clientMutationId": "foreign-message",
    })).await.unwrap();
    let mut request = proof(&sender_token);
    request["toHandle"] = json!("recipient");
    request["subject"] = json!("Attack");
    request["body"] = json!("Must not be queued");
    request["clientMutationId"] = json!("denied-mutation");
    request["messageId"] = foreign_message["messageId"].clone();
    // Fake grant claims and App role fields are untrusted payload data.
    request["crossWorkspace"] = json!(true);
    request["clientKind"] = json!("app");
    request["grantId"] = json!("caller-invented");
    for (handle, workspace, token) in [
        ("sender", "workspace", sender_token.as_str()),
        ("sender", "other-workspace", sender_token.as_str()),
        ("foreign", "other-workspace", sender_token.as_str()),
        ("foreign", "other-workspace", "not-issued"),
    ] {
        request["sessionId"] = json!(handle);
        request["workspaceId"] = json!(workspace);
        request["capability"] = json!(token);
        for operation in ["agentControl.send", "agentControl.messageStatus"] {
            assert!(
                matches!(
                    actor.handle_mcp_agent_request(1, operation, &request).await,
                    Err(HostError::Unauthorized(_))
                ),
                "{operation} must reject forged identity"
            );
        }
    }
    assert_eq!(
        actor
            .runtime_store
            .all_orchestration_messages_for_handle("recipient", None, 100)
            .await
            .unwrap()
            .len(),
        1,
        "denied sends must not persist messages"
    );
    let mut status = json!({
        "sessionId": "foreign", "workspaceId": "other-workspace", "capability": foreign_token,
        "messageId": foreign_message["messageId"],
    });
    assert!(actor
        .handle_mcp_agent_request(1, "agentControl.messageStatus", &status)
        .await
        .is_ok());
    actor.mcp_ui.revoke("foreign");
    assert!(matches!(
        actor
            .handle_mcp_agent_request(1, "agentControl.messageStatus", &status)
            .await,
        Err(HostError::Unauthorized(_))
    ));
    // Even the original sender cannot read a receipt after changing workspace.
    actor.sessions.get_mut("foreign").unwrap().workspace_id = "workspace".into();
    status["workspaceId"] = json!("workspace");
    status["capability"] = json!(actor.mcp_ui.mint_token("foreign"));
    assert!(matches!(
        actor
            .handle_mcp_agent_request(1, "agentControl.messageStatus", &status)
            .await,
        Err(HostError::Unauthorized(_))
    ));
}
