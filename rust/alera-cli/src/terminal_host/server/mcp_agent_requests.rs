//! Agent-to-Agent collaboration through the existing durable Orchestration v2
//! inbox. This is a distinct, capability-checked surface: a local CLI cannot
//! claim another Terminal's identity merely by setting ALERA_* environment
//! variables. Cross-workspace writes and raw PTY input remain unavailable.

use serde_json::{json, Value};

use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::orchestration::group_resolution::is_group_address;

use super::client_delivery::LocalClientRole;
use super::requests::idempotency_receipts::{
    optional_client_mutation_id, payload_digest, prepare_receipt, remove_receipt, settle_receipt,
    ReceiptPrepareOutcome,
};
use super::{ClientKind, ServerActor};

const SEND_OPERATION: &str = "agentControl.send";
const MAX_SUBJECT_BYTES: usize = 128;
const MAX_BODY_BYTES: usize = 8_192;

impl ServerActor {
    pub(super) async fn handle_mcp_agent_request(
        &mut self,
        client_id: u64,
        request_type: &str,
        payload: &Value,
    ) -> HostResult<Value> {
        // The capability must be issued to a live PTY by this Host; it is
        // never stored in a tab, exposed to an App, or inferred from env IDs.
        let local_cli = self.clients.get(&client_id).is_some_and(|client| {
            client.authenticated
                && client.kind == ClientKind::Local
                && client.local_role == LocalClientRole::Cli
        });
        if !local_cli {
            return Err(HostError::unauthorized(
                "MCP Agent control requires an authenticated local Agent CLI.",
            ));
        }
        let session = self.require_mcp_ui_session(payload)?;
        let from = session.id.clone();
        let workspace = session.workspace_id.clone();

        match request_type {
            "agentControl.scope" => Ok(json!({
                "sessionId": from, "workspaceId": workspace, "authorized": true,
                "crossWorkspace": false, "directInput": false,
            })),
            "agentControl.send" => {
                let to = agent_short(payload, "toHandle", 128)?;
                if is_group_address(&to) || to == from {
                    return Err(HostError::format(
                        "MCP Agent messages must target one other live Agent.",
                    ));
                }
                let recipient = self.sessions.get(&to).filter(|target| {
                    target.running()
                        && target.workspace_id == workspace
                        && self.agent_presence.get(&to).is_some()
                });
                if recipient.is_none() {
                    return Err(HostError::unauthorized(
                        "Recipient Agent is stopped, missing, or in another workspace.",
                    ));
                }
                let subject = agent_short(payload, "subject", MAX_SUBJECT_BYTES)?;
                let body = agent_short(payload, "body", MAX_BODY_BYTES)?;
                if subject.chars().any(char::is_control)
                    || body
                        .chars()
                        .any(|c| c.is_control() && c != '\n' && c != '\t')
                {
                    return Err(HostError::format(
                        "Agent message contains invalid control characters.",
                    ));
                }
                let mutation_id = optional_client_mutation_id(payload)?.ok_or_else(|| {
                    HostError::format("clientMutationId is required for Agent messages.")
                })?;
                // Never persist capability tokens or arbitrary MCP JSON into
                // the idempotency table. Only these explicitly allowed fields
                // contribute to the digest and the Orchestration message.
                let command = json!({
                    "from": from, "to": to, "subject": subject, "body": body,
                    "type": "status", "priority": "normal", "workspaceId": workspace,
                });
                let digest = payload_digest(&command)
                    .map_err(|_| HostError::state("Cannot validate Agent message."))?;
                let scope = format!("mcp-agent:{from}");
                match prepare_receipt(
                    &self.runtime_store,
                    SEND_OPERATION,
                    &scope,
                    &mutation_id,
                    &digest,
                    None,
                )
                .await
                .map_err(|_| HostError::state("Cannot prepare Agent message receipt."))?
                {
                    ReceiptPrepareOutcome::Replay(value) => return Ok(value),
                    ReceiptPrepareOutcome::Conflict => {
                        return Err(HostError::state(
                            "clientMutationId was already used with a different Agent message.",
                        ));
                    }
                    ReceiptPrepareOutcome::InProgress => {
                        return Err(HostError::state(
                            "Agent message is already being submitted.",
                        ));
                    }
                    ReceiptPrepareOutcome::Created => {}
                }
                let delivered = self.orchestration_send(client_id, &command).await;
                match delivered {
                    Ok(envelope) => {
                        let message = envelope["messages"]
                            .as_array()
                            .and_then(|items| items.first())
                            .filter(|item| {
                                item["from_handle"] == from
                                    && item["to_handle"] == to
                                    && item["workspace_id"] == workspace
                            })
                            .ok_or_else(|| HostError::state("Invalid Orchestration receipt."))?;
                        let receipt = json!({
                            "kind": "agentMessage", "messageId": message["id"],
                            "fromHandle": from, "toHandle": to,
                            "workspaceId": workspace,
                            "deliveryStatus": if message["delivered_at"].is_null() {
                                "queued"
                            } else {
                                "delivered"
                            },
                            "read": message["read"], "completionConfirmed": false,
                        });
                        settle_receipt(
                            &self.runtime_store,
                            SEND_OPERATION,
                            &scope,
                            &mutation_id,
                            &digest,
                            &receipt,
                        )
                        .await
                        .map_err(|_| HostError::state("Could not settle Agent message receipt."))?;
                        Ok(receipt)
                    }
                    Err(error) => {
                        let _ = remove_receipt(
                            &self.runtime_store,
                            SEND_OPERATION,
                            &scope,
                            &mutation_id,
                            &digest,
                        )
                        .await;
                        Err(error)
                    }
                }
            }
            "agentControl.messageStatus" => {
                let id = agent_short(payload, "messageId", 128)?;
                let message = self
                    .runtime_store
                    .orchestration_message_by_id(&id)
                    .await
                    .map_err(|_| HostError::state("Cannot load Agent message status."))?
                    .filter(|message| {
                        message.from_handle == from
                            && message.workspace_id.as_deref() == Some(workspace.as_str())
                    })
                    .ok_or_else(|| {
                        HostError::unauthorized(
                            "Agent message is missing or belongs to another caller.",
                        )
                    })?;
                Ok(json!({
                    "kind": "agentMessageStatus", "messageId": message.id,
                    "toHandle": message.to_handle,
                    "workspaceId": workspace,
                    "deliveryStatus": if message.delivered_at.is_some() {
                        "delivered"
                    } else {
                        "queued"
                    },
                    "read": message.read, "state": message.state,
                    // A delivery timestamp and a read flag are NOT proof
                    // that the recipient finished the requested work.
                    "completionConfirmed": false,
                }))
            }
            _ => Err(HostError::state("Unknown MCP Agent operation.")),
        }
    }
}

fn agent_short(payload: &Value, key: &str, max_bytes: usize) -> HostResult<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty() && value.len() <= max_bytes)
        .map(str::to_owned)
        .ok_or_else(|| HostError::format(format!("{key} is invalid or too long.")))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::{json, Value};

    use crate::terminal_host::client::ClientHandle;
    use crate::terminal_host::orchestration::agent_presence::AgentPresenceState;
    use crate::terminal_host::server::actor_test_harness::{
        local_client, mobile_client, test_actor,
    };
    use crate::terminal_host::session::Session;

    fn fixture() -> HashMap<String, Session> {
        let mut foreign = Session::driver_test_stub("foreign", 80, 24);
        foreign.workspace_id = "other-workspace".into();
        HashMap::from([
            (
                "sender".to_string(),
                Session::driver_test_stub("sender", 80, 24),
            ),
            (
                "recipient".to_string(),
                Session::driver_test_stub("recipient", 80, 24),
            ),
            ("foreign".to_string(), foreign),
        ])
    }

    fn proof(token: &str) -> Value {
        json!({"sessionId":"sender", "workspaceId":"workspace", "capability":token})
    }

    #[tokio::test]
    async fn scope_requires_issued_capability_workspace_and_local_cli_role() {
        let dir = tempfile::tempdir().unwrap();
        let (local, _) = ClientHandle::test_channels();
        let (mobile, _) = ClientHandle::test_channels();
        let mut actor = test_actor(
            &dir,
            HashMap::from([
                (1, local_client(local)),
                (2, mobile_client(mobile, "phone")),
            ]),
            fixture(),
        )
        .await;
        let token = actor.mcp_ui.mint_token("sender");
        let valid = proof(&token);
        let scope = actor
            .handle_mcp_agent_request(1, "agentControl.scope", &valid)
            .await
            .unwrap();
        assert_eq!(scope["authorized"], true);
        assert_eq!(scope["crossWorkspace"], false);
        assert_eq!(scope["directInput"], false);
        assert!(actor
            .handle_mcp_agent_request(2, "agentControl.scope", &valid)
            .await
            .is_err());
        let mut forged = valid.clone();
        forged["capability"] = json!("not-issued-by-host");
        assert!(actor
            .handle_mcp_agent_request(1, "agentControl.scope", &forged)
            .await
            .is_err());
        forged = valid.clone();
        forged["workspaceId"] = json!("other-workspace");
        assert!(actor
            .handle_mcp_agent_request(1, "agentControl.scope", &forged)
            .await
            .is_err());
        actor.mcp_ui.revoke("sender");
        assert!(actor
            .handle_mcp_agent_request(1, "agentControl.scope", &valid)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn message_is_queued_for_busy_agent_idempotent_and_scoped() {
        let dir = tempfile::tempdir().unwrap();
        let (local, _) = ClientHandle::test_channels();
        let mut actor =
            test_actor(&dir, HashMap::from([(1, local_client(local))]), fixture()).await;
        // The target is busy, so the existing Orchestration flow must leave
        // the persistent message undelivered and never write to the test PTY.
        actor
            .agent_presence
            .update("recipient", "codex".into(), AgentPresenceState::Working);
        actor
            .agent_presence
            .update("foreign", "claude".into(), AgentPresenceState::Working);
        let token = actor.mcp_ui.mint_token("sender");
        let mut request = proof(&token);
        request["toHandle"] = json!("recipient");
        request["subject"] = json!("Review diff");
        request["body"] = json!("Please review the existing diff");
        request["clientMutationId"] = json!("send-001");
        let first = actor
            .handle_mcp_agent_request(1, "agentControl.send", &request)
            .await
            .unwrap();
        assert_eq!(first["kind"], "agentMessage");
        assert_eq!(first["deliveryStatus"], "queued");
        assert_eq!(first["completionConfirmed"], false);
        let message_id = first["messageId"].as_str().unwrap().to_owned();
        let replay = actor
            .handle_mcp_agent_request(1, "agentControl.send", &request)
            .await
            .unwrap();
        assert_eq!(replay, first);
        assert_eq!(
            actor
                .runtime_store
                .all_orchestration_messages_for_handle("recipient", None, 100,)
                .await
                .unwrap()
                .len(),
            1
        );

        let status = json!({
            "sessionId":"sender", "workspaceId":"workspace",
            "capability":token, "messageId":message_id,
        });
        let queued = actor
            .handle_mcp_agent_request(1, "agentControl.messageStatus", &status)
            .await
            .unwrap();
        assert_eq!(queued["deliveryStatus"], "queued");
        assert_eq!(queued["completionConfirmed"], false);
        assert!(queued.get("body").is_none());
        actor
            .runtime_store
            .mark_orchestration_messages_delivered(&[message_id.clone()])
            .await
            .unwrap();
        let delivered = actor
            .handle_mcp_agent_request(1, "agentControl.messageStatus", &status)
            .await
            .unwrap();
        assert_eq!(delivered["deliveryStatus"], "delivered");
        assert_eq!(delivered["completionConfirmed"], false);

        request["body"] = json!("Changed with duplicate id");
        assert!(actor
            .handle_mcp_agent_request(1, "agentControl.send", &request)
            .await
            .is_err());
        request["clientMutationId"] = json!("send-002");
        request["toHandle"] = json!("foreign");
        assert!(actor
            .handle_mcp_agent_request(1, "agentControl.send", &request)
            .await
            .is_err());
        request["toHandle"] = json!("missing");
        assert!(actor
            .handle_mcp_agent_request(1, "agentControl.send", &request)
            .await
            .is_err());
        request["toHandle"] = json!("sender");
        assert!(actor
            .handle_mcp_agent_request(1, "agentControl.send", &request)
            .await
            .is_err());
        request["toHandle"] = json!("recipient");
        request["body"] = json!("unsafe\u{001b}[31m");
        assert!(actor
            .handle_mcp_agent_request(1, "agentControl.send", &request)
            .await
            .is_err());

        let recipient_proof = json!({
            "sessionId":"recipient", "workspaceId":"workspace",
            "capability":actor.mcp_ui.mint_token("recipient"),
            "messageId":message_id,
        });
        assert!(
            actor
                .handle_mcp_agent_request(1, "agentControl.messageStatus", &recipient_proof,)
                .await
                .is_err(),
            "recipient may not read sender's private receipt"
        );
        actor.sessions.remove("sender");
        assert!(
            actor
                .handle_mcp_agent_request(1, "agentControl.messageStatus", &status,)
                .await
                .is_err(),
            "stopped caller cannot inspect old receipts"
        );
    }

    #[tokio::test]
    async fn host_replacement_and_pty_respawn_never_accept_old_agent_capability() {
        let dir = tempfile::tempdir().unwrap();
        let (first_client, _) = ClientHandle::test_channels();
        let mut before = test_actor(
            &dir,
            HashMap::from([(1, local_client(first_client))]),
            fixture(),
        )
        .await;
        let first_token = before.mcp_ui.mint_token("sender");
        let old_proof = proof(&first_token);
        assert!(before
            .handle_mcp_agent_request(1, "agentControl.scope", &old_proof)
            .await
            .is_ok());
        // Replace the whole ServerActor while keeping the same RuntimeStore
        // directory and session handle: in-memory capability is NOT persisted.
        drop(before);
        let (next_client, _) = ClientHandle::test_channels();
        let mut replacement = test_actor(
            &dir,
            HashMap::from([(1, local_client(next_client))]),
            fixture(),
        )
        .await;
        assert!(
            replacement
                .handle_mcp_agent_request(1, "agentControl.scope", &old_proof)
                .await
                .is_err(),
            "Host restart must reject the previous token"
        );
        let new_token = replacement.mcp_ui.mint_token("sender");
        assert_ne!(
            new_token, first_token,
            "PTY respawn must rotate the capability"
        );
        let new_proof = proof(&new_token);
        assert!(replacement
            .handle_mcp_agent_request(1, "agentControl.scope", &new_proof)
            .await
            .is_ok());
        assert!(
            replacement
                .handle_mcp_agent_request(1, "agentControl.scope", &old_proof)
                .await
                .is_err(),
            "Old token must remain unusable after fresh PTY minted one"
        );
        replacement.mcp_ui.revoke("sender");
        assert!(
            replacement
                .handle_mcp_agent_request(1, "agentControl.scope", &new_proof)
                .await
                .is_err(),
            "PTY termination revokes the currently valid token"
        );
    }
}
