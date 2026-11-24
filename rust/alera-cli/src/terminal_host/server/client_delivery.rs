use super::*;
use crate::terminal_host::protocol::PROTOCOL_VERSION;

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

    fn broadcast_to<'a>(
        &self,
        recipients: impl Iterator<Item = (u64, &'a ClientState)>,
        message: Value,
    ) {
        let mut recipients = recipients.peekable();
        let Some((first_id, first)) = recipients.next() else {
            return;
        };
        if recipients.peek().is_none() {
            // The common one-app case needs neither a deep clone nor shared state.
            if first.handle.send_control(message.into()).is_err() {
                self.disconnect_client_soon(first_id, DisconnectReason::TransportWriteFailed);
            }
            return;
        }
        let frame = ClientFrame::shared_json(message);
        for (id, client) in std::iter::once((first_id, first)).chain(recipients) {
            if client.handle.send_control(frame.clone()).is_err() {
                self.disconnect_client_soon(id, DisconnectReason::TransportWriteFailed);
            }
        }
    }

    pub(super) fn broadcast(&self, client_ids: &[u64], message: Value) {
        self.broadcast_to(
            client_ids
                .iter()
                .filter_map(|id| self.clients.get(id).map(|client| (*id, client))),
            message,
        );
    }

    pub(super) fn broadcast_authenticated(&self, message: Value) {
        self.broadcast_to(
            self.clients
                .iter()
                .filter(|(_, client)| client.authenticated)
                .map(|(id, client)| (*id, client)),
            message,
        );
    }

    pub(super) fn broadcast_authenticated_local(&self, message: Value) {
        self.broadcast_to(
            self.clients
                .iter()
                .filter(|(_, client)| client.authenticated && client.kind == ClientKind::Local)
                .map(|(id, client)| (*id, client)),
            message,
        );
    }

    pub(super) fn broadcast_authenticated_mobile(&self, message: Value) {
        self.broadcast_to(
            self.clients
                .iter()
                .filter(|(_, client)| client.authenticated && client.kind == ClientKind::Mobile)
                .map(|(id, client)| (*id, client)),
            message,
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
    async fn broadcast_shares_json_only_among_eligible_recipients() {
        use crate::terminal_host::server::actor_test_harness::{
            local_client, mobile_client, test_actor,
        };
        let dir = tempfile::tempdir().unwrap();
        let (a, mut ar) = ClientHandle::test_channels();
        let (b, mut br) = ClientHandle::test_channels();
        let (c, mut cr) = ClientHandle::test_channels();
        let (d, mut dr) = ClientHandle::test_channels();
        let mut anonymous = local_client(d);
        anonymous.authenticated = false;
        let actor = test_actor(
            &dir,
            HashMap::from([
                (1, local_client(a)),
                (2, local_client(b)),
                (3, mobile_client(c, "phone")),
                (4, anonymous),
            ]),
            HashMap::new(),
        )
        .await;

        let message = json!({"event": "changed", "payload": {"text": "測試".repeat(1024)}});
        actor.broadcast_authenticated_local(message.clone());
        let unwrap = |frame: ClientFrame| match frame {
            ClientFrame::OrderedControl { frame, .. } => match *frame {
                ClientFrame::SharedJson(shared) => shared,
                _ => panic!("multi-recipient broadcast must share JSON"),
            },
            _ => panic!("missing control ordering envelope"),
        };
        let first = unwrap(ar.try_recv().unwrap());
        let second = unwrap(br.try_recv().unwrap());
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.value(), &message);
        assert!(cr.try_recv().is_err());
        assert!(dr.try_recv().is_err());

        actor.broadcast_authenticated_mobile(message.clone());
        let only = cr.try_recv().unwrap();
        assert!(matches!(only, ClientFrame::OrderedControl { frame, .. }
            if matches!(*frame, ClientFrame::Json(ref value) if value == &message)));
        assert!(ar.try_recv().is_err());
        assert!(br.try_recv().is_err());
        assert!(dr.try_recv().is_err());
    }

    #[tokio::test]
    async fn control_bursts_survive_a_saturated_terminal_queue() {
        let dir = tempfile::tempdir().unwrap();
        let history = TerminalHostHistoryRepository::open(dir.path())
            .await
            .unwrap();
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
            history,
            runtime_store,
            automation_wake: Arc::new(Notify::new()),
            automations_active: false,
            sessions: HashMap::new(),
            ssh_bootstrap_jobs: HashMap::new(),
            project_clone_jobs: HashMap::new(),
            agent_title_jobs: HashMap::new(),
            agent_report_jobs: HashMap::new(),
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
            mcp_ui: Default::default(),
            orchestration_waiters: MessageWaiterRegistry::default(),
            orchestration_delivery_in_flight: HashSet::new(),
            orchestration_delivery_backpressured: HashSet::new(),
            orchestration_output_activity: Default::default(),
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
            history_search_state: Default::default(),
            inbox,
            next_client_id: Arc::new(AtomicU64::new(2)),
            mobile_gateway: None,
            shutdown_gen: 0,
            shutdown_timer: Default::default(),
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
