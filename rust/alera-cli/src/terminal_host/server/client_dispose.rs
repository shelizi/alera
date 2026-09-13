use super::*;
use crate::terminal_host::orchestration::message_waiters::WaitKind;

use super::orchestration_message_requests::take_ask_receipt_context;
use super::requests::idempotency_receipts::settle_receipt;

impl ServerActor {
    pub(super) fn disconnect_client_soon(&self, client_id: u64, reason: DisconnectReason) {
        let _ = self.inbox.send(ServerCommand::ClientDisconnected {
            id: client_id,
            reason,
        });
    }

    pub(super) async fn dispose_client(&mut self, client_id: u64) {
        let reason = default_disconnect_reason(self, client_id);
        self.dispose_client_with_reason(client_id, reason).await;
    }

    pub(super) async fn dispose_client_with_reason(
        &mut self,
        client_id: u64,
        reason: DisconnectReason,
    ) {
        let Some((authenticated, mobile_disconnected)) =
            self.clients.get(&client_id).map(|client| {
                (
                    client.authenticated,
                    client.authenticated && matches!(client.kind, ClientKind::Mobile),
                )
            })
        else {
            return;
        };
        let removed_waiters = self.orchestration_waiters.remove_client(client_id);
        let waiters_removed = removed_waiters.len();
        for waiter in removed_waiters {
            let WaitKind::Ask { thread_id, .. } = waiter.kind else {
                continue;
            };
            let Some(context) = take_ask_receipt_context(&thread_id) else {
                continue;
            };
            // A disconnect is terminal but is not a timeout. Keep the explicit
            // `timedOut` field for schema parity, without timeout-only timings.
            let payload = json!({
                "answered": false,
                "disconnected": true,
                "timedOut": false,
                "outcome": "disconnected",
            });
            if let Err(error) = settle_receipt(
                &self.runtime_store,
                "orchestration.ask",
                &context.caller_scope,
                &context.client_mutation_id,
                &context.payload_digest,
                &payload,
            )
            .await
            {
                tracing::error!(
                    client_mutation_id = %context.client_mutation_id,
                    "failed to settle orchestration.ask disconnect receipt: {error}"
                );
            }
        }
        self.cancel_queued_runtime_mutations(client_id);
        self.release_mobile_driver_for_client(client_id);
        self.deferred_admission.disconnect_client(client_id);
        let admission_still_occupied = admission_occupied(&self.deferred_admission.snapshot());
        self.cancel_mobile_prompt_file_uploads(client_id);
        self.cancel_mobile_prompt_image_uploads(client_id);
        let session_ids: Vec<String> = self.sessions.keys().cloned().collect();
        let mut sessions_detached = 0;
        for session_id in session_ids {
            self.flush_all_output(&session_id);
            if let Some(session) = self.sessions.get_mut(&session_id) {
                if session.clients.contains(&client_id) {
                    sessions_detached += 1;
                }
                session.detach(client_id);
            }
            self.immediate_checkpoint(&session_id).await;
        }
        self.clients.remove(&client_id);
        self.configuration_transfers.disconnect(client_id);
        if mobile_disconnected {
            self.broadcast_authenticated(event("mobileDevicesChanged", json!({})));
        }
        self.schedule_shutdown_if_idle();
        let receipts_left_pending =
            super::requests::idempotency_receipts::pending_count(&self.runtime_store)
                .await
                .ok();
        log_cleanup_completion(
            reason,
            client_id,
            authenticated,
            waiters_removed,
            sessions_detached,
            admission_still_occupied,
            receipts_left_pending,
        );
    }

    pub(super) async fn dispose_mobile_clients(&mut self) {
        let client_ids = self
            .clients
            .iter()
            .filter_map(|(id, client)| (client.kind == ClientKind::Mobile).then_some(*id))
            .collect::<Vec<_>>();
        for client_id in client_ids {
            self.dispose_client_with_reason(client_id, DisconnectReason::Replaced)
                .await;
        }
    }

    pub(super) async fn dispose_mobile_clients_for_device(&mut self, device_id: &str) {
        let client_ids = self
            .clients
            .iter()
            .filter_map(|(id, client)| {
                (client.kind == ClientKind::Mobile
                    && client.mobile_device_id.as_deref() == Some(device_id))
                .then_some(*id)
            })
            .collect::<Vec<_>>();
        for client_id in client_ids {
            self.dispose_client_with_reason(client_id, DisconnectReason::DeviceRevoked)
                .await;
        }
    }
}

fn default_disconnect_reason(actor: &ServerActor, client_id: u64) -> DisconnectReason {
    if actor.disposed {
        return DisconnectReason::HostShutdown;
    }
    match actor.clients.get(&client_id) {
        Some(client) if client.relay_client_id.is_some() => DisconnectReason::Replaced,
        Some(client) if !client.authenticated => DisconnectReason::Unauthenticated,
        Some(_) => DisconnectReason::ProtocolViolation,
        None => DisconnectReason::PeerClosed,
    }
}

fn admission_occupied(snapshot: &Value) -> u64 {
    snapshot["active"]
        .as_u64()
        .unwrap_or_default()
        .saturating_add(snapshot["pending"].as_u64().unwrap_or_default())
}

fn log_cleanup_completion(
    reason: DisconnectReason,
    client_id: u64,
    authenticated: bool,
    waiters_removed: usize,
    sessions_detached: usize,
    admission_still_occupied: u64,
    receipts_left_pending: Option<i64>,
) {
    if let Some(receipts_left_pending) = receipts_left_pending {
        tracing::info!(
            event = "cleanup-completion",
            reason = reason.as_str(),
            client_id,
            authenticated,
            waiters_removed,
            sessions_detached,
            admission_still_occupied,
            receipts_left_pending,
            "client disconnect cleanup completed"
        );
    } else {
        tracing::info!(
            event = "cleanup-completion",
            reason = reason.as_str(),
            client_id,
            authenticated,
            waiters_removed,
            sessions_detached,
            admission_still_occupied,
            "client disconnect cleanup completed"
        );
    }
}
