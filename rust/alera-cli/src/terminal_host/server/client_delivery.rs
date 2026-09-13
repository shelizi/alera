use super::*;
use crate::terminal_host::orchestration::message_waiters::WaitKind;
use crate::terminal_host::protocol::PROTOCOL_VERSION;

use super::orchestration_message_requests::take_ask_receipt_context;
use super::requests::idempotency_receipts::settle_receipt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LocalClientRole {
    App,
    Cli,
}

impl ServerActor {
    pub(super) fn require_auth(&self, client_id: u64) -> HostResult<()> {
        match self.clients.get(&client_id) {
            Some(client) if client.authenticated => Ok(()),
            _ => Err(HostError::unauthorized(
                "Terminal host client is not authenticated.",
            )),
        }
    }

    pub(super) fn require_authenticated_local_request(
        &self,
        client_id: u64,
        request_type: &str,
    ) -> HostResult<()> {
        self.require_auth(client_id)?;
        self.require_request_allowed(client_id, request_type)
    }

    pub(super) fn require_session_id(&self, payload: &Value) -> HostResult<String> {
        let session_id = match payload.get("sessionId") {
            Some(Value::String(value)) => value.clone(),
            _ => return Err(HostError::format("Terminal session id is required.")),
        };
        Ok(session_id)
    }

    pub(super) fn require_session(&self, payload: &Value) -> HostResult<String> {
        let session_id = self.require_session_id(payload)?;
        if !self.sessions.contains_key(&session_id) {
            return Err(HostError::state(format!(
                "Terminal session is not attached: {session_id}"
            )));
        }
        Ok(session_id)
    }

    /// Authenticates a client and negotiates the wire format for it.
    pub(super) fn handle_hello(&mut self, client_id: u64, payload: &Value) -> HostResult<Value> {
        let version_ok = payload.get("protocolVersion") == Some(&json!(PROTOCOL_VERSION));
        let token_ok = payload.get("token").and_then(Value::as_str) == Some(self.token.as_str());
        if !version_ok || !token_ok {
            return Err(HostError::unauthorized(
                "Terminal host authentication failed.",
            ));
        }
        let binary_frames = payload
            .get("binaryFrames")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let local_role = requested_local_role(payload);
        if let Some(client) = self.clients.get_mut(&client_id) {
            client.authenticated = true;
            client.binary_frames = binary_frames;
            if client.kind == ClientKind::Local {
                client.local_role = local_role;
            }
        }
        self.cancel_shutdown_timer();
        if binary_frames {
            // Queued after this response on the same lane, so the writer emits
            // the response as a line and only then switches. A shared flag
            // could flip first and frame the response the client is still
            // reading as a line.
            self.upgrade_client_to_binary(client_id);
        }
        Ok(json!({
            "binaryFrames": binary_frames,
            "clientKind": match local_role {
                LocalClientRole::App => "app",
                LocalClientRole::Cli => "cli",
            }
        }))
    }

    /// Queues the in-band switch to binary frames for one client.
    pub(super) fn upgrade_client_to_binary(&self, client_id: u64) {
        if let Some(client) = self.clients.get(&client_id) {
            if client
                .handle
                .send_control(ClientFrame::UpgradeToBinary)
                .is_err()
            {
                self.disconnect_client_soon(client_id, DisconnectReason::TransportWriteFailed);
            }
        }
    }

    pub(super) fn client_write(&self, client_id: u64, message: Value) {
        self.try_client_write(client_id, message);
    }

    pub(super) fn try_client_write(&self, client_id: u64, message: Value) -> bool {
        if let Some(client) = self.clients.get(&client_id) {
            if client.handle.send_control(message.into()).is_err() {
                self.disconnect_client_soon(client_id, DisconnectReason::TransportWriteFailed);
                return false;
            }
            return true;
        }
        false
    }

    pub(super) fn restart_runtime_after_client_write(&self, client_id: u64) {
        if let Some(client) = self.clients.get(&client_id) {
            if client
                .handle
                .send_control(ClientFrame::RestartRuntimeAfterWrite {
                    inbox: self.inbox.clone(),
                })
                .is_err()
            {
                self.disconnect_client_soon(client_id, DisconnectReason::TransportWriteFailed);
            }
        }
    }

    pub(super) fn shutdown_runtime_after_client_write(&self, client_id: u64) {
        if let Some(client) = self.clients.get(&client_id) {
            if client
                .handle
                .send_control(ClientFrame::ShutdownRuntimeAfterWrite {
                    inbox: self.inbox.clone(),
                })
                .is_err()
            {
                self.disconnect_client_soon(client_id, DisconnectReason::TransportWriteFailed);
            }
        }
    }

    pub(super) fn broadcast(&self, client_ids: &[u64], message: Value) {
        for id in client_ids {
            if let Some(client) = self.clients.get(id) {
                if client.handle.send_control(message.clone().into()).is_err() {
                    self.disconnect_client_soon(*id, DisconnectReason::TransportWriteFailed);
                }
            }
        }
    }

    pub(super) fn broadcast_authenticated(&self, message: Value) {
        for (client_id, client) in &self.clients {
            if client.authenticated && client.handle.send_control(message.clone().into()).is_err() {
                self.disconnect_client_soon(*client_id, DisconnectReason::TransportWriteFailed);
            }
        }
    }

    pub(super) fn broadcast_authenticated_local(&self, message: Value) {
        for (client_id, client) in &self.clients {
            if client.authenticated
                && client.kind == ClientKind::Local
                && client.handle.send_control(message.clone().into()).is_err()
            {
                self.disconnect_client_soon(*client_id, DisconnectReason::TransportWriteFailed);
            }
        }
    }

    pub(super) fn broadcast_authenticated_mobile(&self, message: Value) {
        for (client_id, client) in &self.clients {
            if client.authenticated
                && client.kind == ClientKind::Mobile
                && client.handle.send_control(message.clone().into()).is_err()
            {
                self.disconnect_client_soon(*client_id, DisconnectReason::TransportWriteFailed);
            }
        }
    }

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

fn requested_local_role(payload: &Value) -> LocalClientRole {
    match payload.get("clientKind").and_then(Value::as_str) {
        Some("app") => LocalClientRole::App,
        _ => LocalClientRole::Cli,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_roles_are_additive_and_absent_means_cli() {
        assert_eq!(requested_local_role(&json!({})), LocalClientRole::Cli);
        assert_eq!(
            requested_local_role(&json!({"clientKind": "cli"})),
            LocalClientRole::Cli
        );
        assert_eq!(
            requested_local_role(&json!({"clientKind": "app"})),
            LocalClientRole::App
        );
        assert_eq!(
            requested_local_role(&json!({"clientKind": "legacy"})),
            LocalClientRole::Cli
        );
    }

    #[tokio::test]
    async fn authentication_failures_are_unauthorized_with_legacy_messages() {
        let dir = tempfile::tempdir().unwrap();
        let mut actor = crate::terminal_host::server::actor_test_harness::test_actor(
            &dir,
            std::collections::HashMap::new(),
            std::collections::HashMap::new(),
        )
        .await;

        let error = actor.require_auth(1).unwrap_err();
        assert_eq!(
            error.outcome_class(),
            crate::terminal_host::host_error::OutcomeClass::Unauthorized
        );
        assert_eq!(
            error.wire_message(),
            "Terminal host client is not authenticated."
        );

        let error = actor
            .handle_hello(
                1,
                &json!({"protocolVersion": PROTOCOL_VERSION, "token": "wrong"}),
            )
            .unwrap_err();
        assert_eq!(
            error.outcome_class(),
            crate::terminal_host::host_error::OutcomeClass::Unauthorized
        );
        assert_eq!(error.wire_message(), "Terminal host authentication failed.");
    }

    #[tokio::test]
    async fn control_bursts_survive_a_saturated_terminal_queue() {
        let dir = tempfile::tempdir().unwrap();
        let store = TerminalHostHistoryStore::open(dir.path()).await.unwrap();
        let runtime_store = RuntimeStore::open(dir.path()).await.unwrap();
        let account_push = super::account_push_state::AccountPushState::new(
            dir.path().to_path_buf(),
            runtime_store.clone(),
        )
        .await
        .unwrap();
        let (inbox, mut inbox_rx) = mpsc::unbounded_channel();
        let (control_out, mut control_out_rx) = mpsc::unbounded_channel();
        let (terminal_out, _terminal_out_rx) =
            mpsc::channel::<ClientFrame>(CLIENT_TERMINAL_OUT_QUEUE_CAPACITY);
        for index in 0..CLIENT_TERMINAL_OUT_QUEUE_CAPACITY {
            terminal_out
                .try_send(json!({"terminal": index}).into())
                .unwrap();
        }
        let actor = ServerActor {
            runtime_dir: dir.path().to_path_buf(),
            control_file_path: dir.path().join("runtime-host.json"),
            token: "token".to_string(),
            config: TerminalHostConfig::default(),
            store,
            runtime_store,
            automation_wake: Arc::new(Notify::new()),
            automations_active: false,
            sessions: HashMap::new(),
            ssh_bootstrap_jobs: HashMap::new(),
            project_clone_jobs: HashMap::new(),
            agent_title_jobs: HashMap::new(),
            managed_workspace_jobs: 0,
            mutation_queue: Default::default(),
            agent_quota_cache: None,
            configuration_transfers: Default::default(),
            account_push,
            clients: HashMap::from([(
                1,
                ClientState {
                    handle: ClientHandle::new(control_out, terminal_out),
                    authenticated: true,
                    binary_frames: false,
                    kind: ClientKind::Local,
                    local_role: LocalClientRole::Cli,
                    mobile_device_id: None,
                    mobile_device_name: None,
                    cloud_device_id: None,
                    relay_client_id: None,
                },
            )]),
            mobile_prompt_file_uploads: HashMap::new(),
            mobile_prompt_image_uploads: HashMap::new(),
            agent_presence: AgentPresenceRegistry::default(),
            orchestration_waiters: MessageWaiterRegistry::default(),
            orchestration_delivery_in_flight: HashSet::new(),
            orchestration_delivery_backpressured: HashSet::new(),
            orchestration_activity_last_recorded: HashMap::new(),
            pending_dispatch_installs: HashMap::new(),
            pending_drift_probes: HashSet::new(),
            coordinators: HashMap::new(),
            resources: ResourceMonitorState::default(),
            terminal_pulses: Default::default(),
            codex: None,
            codex_starting: None,
            deferred_admission: Arc::new(
                super::super::deferred_admission::DeferredAdmission::default(),
            ),
            workspace_sidebar_snapshots: Default::default(),
            inbox,
            next_client_id: Arc::new(AtomicU64::new(2)),
            mobile_gateway: None,
            shutdown_gen: 0,
            disposed: false,
        };

        for index in 0..(CLIENT_TERMINAL_OUT_QUEUE_CAPACITY * 2) {
            actor.client_write(1, json!({"response": index}));
            actor.broadcast_authenticated(json!({"event": index}));
        }

        let mut received = Vec::new();
        while let Ok(message) = control_out_rx.try_recv() {
            received.push(message);
        }
        assert_eq!(received.len(), CLIENT_TERMINAL_OUT_QUEUE_CAPACITY * 4);
        assert!(inbox_rx.try_recv().is_err());
        assert!(actor.clients.contains_key(&1));
    }
}
