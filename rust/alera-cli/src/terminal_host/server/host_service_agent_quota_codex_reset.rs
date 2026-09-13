use serde_json::Value;

use crate::agent_quota::consume_codex_reset_credit;
use crate::terminal_host::host_error::{HostError, HostResult};

use super::deferred_admission::DeferredRequestClass;
use super::requests::idempotency_receipts::{
    optional_client_mutation_id, payload_digest, prepare_receipt, remove_receipt, settle_receipt,
    ReceiptPrepareOutcome,
};
use super::{ServerActor, ServerCommand};

impl ServerActor {
    pub(super) async fn start_agent_quota_codex_reset_request(
        &mut self,
        client_id: u64,
        request_id: i64,
        payload: &Value,
    ) -> HostResult<()> {
        let environment_signature = self
            .agent_quota_cache
            .as_ref()
            .map(|(_, signature, _)| *signature)
            .unwrap_or(0);
        let client_mutation_id = optional_client_mutation_id(payload)?;
        let (receipt_context, replay_result) = if let Some(client_mutation_id) = client_mutation_id
        {
            let caller_scope = self.agent_profile_launch_caller_scope(client_id)?;
            let digest =
                payload_digest(payload).map_err(|error| HostError::state(error.to_string()))?;
            let outcome = prepare_receipt(
                &self.runtime_store,
                "agentQuota.consumeCodexResetCredit",
                &caller_scope,
                &client_mutation_id,
                &digest,
                None,
            )
            .await
            .map_err(|error| HostError::state(error.to_string()))?;
            match outcome {
                ReceiptPrepareOutcome::Created => {
                    (Some((caller_scope, client_mutation_id, digest)), None)
                }
                ReceiptPrepareOutcome::Replay(result) => (None, Some(result)),
                ReceiptPrepareOutcome::Conflict => {
                    return Err(HostError::state(
                        "clientMutationId was already used with a different agentQuota.consumeCodexResetCredit payload.",
                    ));
                }
                ReceiptPrepareOutcome::InProgress => {
                    return Err(HostError::state("clientMutationId is already in progress."));
                }
            }
        } else {
            (None, None)
        };
        let store = self.runtime_store.clone();
        let payload = payload.clone();
        let inbox = self.inbox.clone();
        let scheduled_receipt_context = receipt_context.clone();
        let scheduled = self.deferred_admission.schedule(
            DeferredRequestClass::Bulk,
            "agentQuota.consumeCodexResetCredit",
            Some(client_id),
            async move {
                let result = if let Some(result) = replay_result {
                    Ok(result)
                } else {
                    match consume_codex_reset_credit(&store, payload).await {
                        Ok(result) => {
                            if let Some((caller_scope, client_mutation_id, digest)) =
                                scheduled_receipt_context.as_ref()
                            {
                                match settle_receipt(
                                    &store,
                                    "agentQuota.consumeCodexResetCredit",
                                    caller_scope,
                                    client_mutation_id,
                                    digest,
                                    &result,
                                )
                                .await
                                {
                                    Ok(()) => Ok(result),
                                    Err(error) => Err(HostError::state(error.to_string())),
                                }
                            } else {
                                Ok(result)
                            }
                        }
                        Err(error) => {
                            if let Some((caller_scope, client_mutation_id, digest)) =
                                scheduled_receipt_context.as_ref()
                            {
                                if let Err(cleanup_error) = remove_receipt(
                                    &store,
                                    "agentQuota.consumeCodexResetCredit",
                                    caller_scope,
                                    client_mutation_id,
                                    digest,
                                )
                                .await
                                {
                                    tracing::error!(
                                        client_mutation_id = %client_mutation_id,
                                        "failed to roll back Codex reset receipt: {cleanup_error}"
                                    );
                                }
                            }
                            Err(HostError::state(error.to_string()))
                        }
                    }
                };
                let _ = inbox.send(ServerCommand::AgentQuotaCodexResetFinished {
                    client_id,
                    request_id,
                    environment_signature,
                    result,
                });
            },
        );
        if let Err(error) = scheduled {
            if let Some((caller_scope, client_mutation_id, digest)) = receipt_context {
                if let Err(cleanup_error) = remove_receipt(
                    &self.runtime_store,
                    "agentQuota.consumeCodexResetCredit",
                    &caller_scope,
                    &client_mutation_id,
                    &digest,
                )
                .await
                {
                    tracing::error!(
                        client_mutation_id = %client_mutation_id,
                        "failed to roll back rejected Codex reset receipt: {cleanup_error}"
                    );
                }
            }
            return Err(error);
        }
        Ok(())
    }
}
