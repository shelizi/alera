use super::terminal_startup_commands::auto_closes_on_success;
use super::*;
use crate::terminal_host::session::PtyEvent;

struct OutputPersistenceState {
    key: (std::path::PathBuf, String),
    pending: std::sync::atomic::AtomicUsize,
    notify: tokio::sync::Notify,
}

struct OutputPersistenceCompletion(std::sync::Arc<OutputPersistenceState>);

impl Drop for OutputPersistenceCompletion {
    fn drop(&mut self) {
        complete_output_persistence(&self.0);
    }
}

impl Drop for OutputPersistenceState {
    fn drop(&mut self) {
        let Some(states) = OUTPUT_PERSISTENCE_STATES.get() else {
            return;
        };
        let mut states = states.lock().unwrap_or_else(|error| error.into_inner());
        if states
            .get(&self.key)
            .is_some_and(|state| state.upgrade().is_none())
        {
            states.remove(&self.key);
        }
    }
}

static OUTPUT_PERSISTENCE_STATES: std::sync::OnceLock<
    std::sync::Mutex<
        std::collections::HashMap<
            (std::path::PathBuf, String),
            std::sync::Weak<OutputPersistenceState>,
        >,
    >,
> = std::sync::OnceLock::new();

fn output_persistence_state(
    runtime_dir: &std::path::Path,
    session_id: &str,
) -> std::sync::Arc<OutputPersistenceState> {
    let states = OUTPUT_PERSISTENCE_STATES.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    let mut states = states.lock().unwrap_or_else(|error| error.into_inner());
    states.retain(|_, state| state.strong_count() > 0);
    let key = (runtime_dir.to_path_buf(), session_id.to_string());
    if let Some(state) = states.get(&key).and_then(std::sync::Weak::upgrade) {
        return state;
    }
    let state = std::sync::Arc::new(OutputPersistenceState {
        key: key.clone(),
        pending: std::sync::atomic::AtomicUsize::new(0),
        notify: tokio::sync::Notify::new(),
    });
    states.insert(key, std::sync::Arc::downgrade(&state));
    state
}

fn complete_output_persistence(state: &OutputPersistenceState) {
    state
        .pending
        .fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    state.notify.notify_waiters();
    // `notified()` registers its waiter when it is polled, so keep a permit
    // for the case where completion lands between the barrier's check and await.
    state.notify.notify_one();
}

impl ServerActor {
    pub(super) async fn handle_pty_event(&mut self, session_id: String, pty_event: PtyEvent) {
        match pty_event {
            PtyEvent::Output(data) => self.handle_pty_output(session_id, data).await,
            #[cfg(windows)]
            PtyEvent::ChildExited => {
                if let Some(session) = self.sessions.get_mut(&session_id) {
                    session.close_pty_after_child_exit();
                }
            }
            PtyEvent::Error(message) => {
                self.flush_all_output(&session_id);
                if let Some(session) = self.sessions.get_mut(&session_id) {
                    let payload = session.error_payload(&message);
                    let clients: Vec<u64> = session.clients.iter().copied().collect();
                    self.broadcast(&clients, event("error", payload));
                }
            }
            PtyEvent::InputWritten { completion, error } => {
                self.handle_pty_input_written(session_id, completion, error)
                    .await;
            }
            PtyEvent::Exit(code) => self.handle_session_exit(session_id, code).await,
        }
    }

    async fn handle_pty_output(&mut self, session_id: String, data: Vec<u8>) {
        #[cfg(windows)]
        let cursor_response_error = self.sessions.get_mut(&session_id).and_then(|session| {
            if !session.take_initial_conpty_cursor_query(&data) {
                return None;
            }
            let session_instance_id = session.instance_id();
            session
                .queue_write(
                    PtyWriteCompletion::ConPtyStartupCursorResponse {
                        session_instance_id,
                    },
                    b"\x1b[1;1R",
                )
                .err()
                .map(|error| error.wire_message())
        });
        #[cfg(windows)]
        if let Some(message) = cursor_response_error {
            self.broadcast_terminal_error(&session_id, message);
        }

        let state = self.sessions.get_mut(&session_id).map(|session| {
            let (output_generation, durable_generation, title_change) =
                session.append_output(&data);
            let title_event = title_change.map(|title| {
                event(
                    "terminalTitleChanged",
                    json!({
                        "sessionId": session.id,
                        "workspaceId": session.workspace_id,
                        "tabId": session.tab_id,
                        "title": title,
                    }),
                )
            });
            (
                output_generation,
                session.output_batch_len(),
                durable_generation,
                session.durable_output_batch_len(),
                session.arm_checkpoint(),
                title_event,
            )
        });
        let Some((
            output_generation,
            output_len,
            durable_generation,
            durable_len,
            checkpoint,
            title_event,
        )) = state
        else {
            return;
        };
        if let Some(title_event) = title_event {
            self.broadcast_authenticated_mobile(title_event);
        }
        self.record_orchestration_output_activity(&session_id).await;
        if let Some(generation) = output_generation {
            self.spawn_output_batch_timer(session_id.clone(), generation);
        }
        if let Some(generation) = durable_generation {
            self.spawn_durable_output_batch_timer(session_id.clone(), generation);
        }
        if output_len >= OUTPUT_BATCH_MAX_BYTES {
            self.flush_output_batch(&session_id);
        }
        if durable_len >= OUTPUT_BATCH_MAX_BYTES {
            self.flush_durable_output_batch(&session_id);
        }
        if let Some(generation) = checkpoint {
            self.spawn_checkpoint_timer(session_id, generation);
        }
    }

    async fn record_orchestration_output_activity(&mut self, session_id: &str) {
        if self
            .orchestration_activity_last_recorded
            .get(session_id)
            .is_some_and(|last| last.elapsed() < ORCHESTRATION_ACTIVITY_WRITE_INTERVAL)
        {
            return;
        }
        let dispatch = match self
            .runtime_store
            .active_orchestration_dispatch_for_handle(session_id)
            .await
        {
            Ok(dispatch) => dispatch,
            Err(_) => return,
        };
        self.orchestration_activity_last_recorded
            .insert(session_id.to_string(), Instant::now());
        let Some(dispatch) = dispatch else {
            return;
        };
        let _ = self
            .runtime_store
            .record_orchestration_activity(&dispatch.id)
            .await;
    }

    async fn handle_pty_input_written(
        &mut self,
        session_id: String,
        completion: PtyWriteCompletion,
        error: Option<String>,
    ) {
        match completion {
            PtyWriteCompletion::ClientRequest {
                client_id,
                request_id,
            } => {
                self.finish_terminal_client_write(&session_id, client_id, request_id, error)
                    .await
            }
            PtyWriteCompletion::OrchestrationPaste {
                session_instance_id,
                message_ids,
                force_submit,
            } => {
                if let Some(message) = error {
                    self.orchestration_delivery_in_flight.remove(&session_id);
                    self.broadcast_terminal_error(&session_id, message);
                    return;
                }
                if self.sessions.get(&session_id).map(Session::instance_id)
                    != Some(session_instance_id)
                {
                    self.orchestration_delivery_in_flight.remove(&session_id);
                    return;
                }
                if !force_submit && skips_auto_enter(self.agent_presence.agent_type(&session_id)) {
                    if !message_ids.is_empty() {
                        let delivered = self
                            .runtime_store
                            .mark_orchestration_messages_delivered(&message_ids)
                            .await
                            .is_ok();
                        self.orchestration_delivery_in_flight.remove(&session_id);
                        if delivered && self.agent_presence.is_injection_ready(&session_id) {
                            self.deliver_pending_messages(&session_id).await;
                        }
                    }
                    return;
                }
                self.schedule_orchestration_enter(
                    session_id,
                    session_instance_id,
                    message_ids,
                    force_submit,
                );
            }
            PtyWriteCompletion::OrchestrationEnter {
                session_instance_id,
                message_ids,
            } => {
                let current = self.sessions.get(&session_id).map(Session::instance_id);
                if error.is_some() || current != Some(session_instance_id) {
                    self.orchestration_delivery_in_flight.remove(&session_id);
                    if let Some(message) = error {
                        self.broadcast_terminal_error(&session_id, message);
                    }
                    return;
                }
                let message_backed = !message_ids.is_empty();
                let delivered = !message_backed
                    || self
                        .runtime_store
                        .mark_orchestration_messages_delivered(&message_ids)
                        .await
                        .is_ok();
                if message_backed {
                    self.orchestration_delivery_in_flight.remove(&session_id);
                }
                if delivered
                    && self.agent_presence.is_injection_ready(&session_id)
                    && (message_backed
                        || self
                            .orchestration_delivery_backpressured
                            .contains(&session_id))
                {
                    self.deliver_pending_messages(&session_id).await;
                }
            }
            PtyWriteCompletion::StartupPlain {
                session_instance_id,
            }
            | PtyWriteCompletion::StartupSubmit {
                session_instance_id,
            }
            | PtyWriteCompletion::TerminalPulse {
                session_instance_id,
                ..
            } => {
                // Internal writes only report errors; they never trigger startup submission.
                let current = self.sessions.get(&session_id).map(Session::instance_id);
                if let Some(message) = error.filter(|_| current == Some(session_instance_id)) {
                    self.broadcast_terminal_error(&session_id, message);
                }
            }
            #[cfg(windows)]
            PtyWriteCompletion::ConPtyStartupCursorResponse {
                session_instance_id,
            } => {
                let current = self.sessions.get(&session_id).map(Session::instance_id);
                if let Some(message) = error.filter(|_| current == Some(session_instance_id)) {
                    self.broadcast_terminal_error(&session_id, message);
                }
            }
            PtyWriteCompletion::StartupPaste {
                session_instance_id,
            } => {
                let current = self.sessions.get(&session_id).map(Session::instance_id);
                if current != Some(session_instance_id) {
                    return;
                }
                if let Some(message) = error {
                    self.broadcast_terminal_error(&session_id, message);
                    return;
                }
                if let Err(error) =
                    self.schedule_terminal_startup_submit(session_id.clone(), session_instance_id)
                {
                    self.broadcast_terminal_error(&session_id, error.wire_message());
                }
            }
        }
    }

    pub(super) async fn handle_session_exit(&mut self, session_id: String, exit_code: i32) {
        self.disarm_terminal_pulse(&session_id);
        self.queue_terminal_exit_push(&session_id, Some(exit_code))
            .await;
        let reason = format!("terminal exited with code {exit_code}");
        let keep_failed_setup =
            exit_code != 0 && self.should_keep_failed_setup_terminal(&session_id).await;
        let keep_failed_spawn = self.should_keep_failed_owned_spawn(&session_id).await;
        self.cleanup_orchestration_for_closed_session(&session_id, &reason)
            .await;
        let keep_failed_spawn =
            keep_failed_spawn && self.make_failed_owned_spawn_inert(&session_id).await;
        let keep_terminal = keep_failed_setup || keep_failed_spawn;
        self.flush_all_output(&session_id);
        let broadcast = self.sessions.get_mut(&session_id).and_then(|session| {
            let payload = session.handle_exit(exit_code)?;
            let clients: Vec<u64> = session.clients.iter().copied().collect();
            Some((event("exit", payload), clients))
        });
        if let Some((frame, clients)) = broadcast {
            self.broadcast(&clients, frame);
            if keep_terminal {
                self.immediate_checkpoint(&session_id).await;
            } else {
                match self.remove_terminal_session_tab(&session_id).await {
                    Ok(true) => {}
                    Ok(false) => self.immediate_checkpoint(&session_id).await,
                    Err(error) => {
                        tracing::error!(
                            "failed to remove tab for exited terminal {session_id}: {}",
                            error.wire_message()
                        );
                        self.immediate_checkpoint(&session_id).await;
                    }
                }
            }
        }
        self.schedule_shutdown_if_idle();
    }

    async fn should_keep_failed_setup_terminal(&self, session_id: &str) -> bool {
        let Some(tab_id) = self
            .sessions
            .get(session_id)
            .map(|session| session.tab_id.as_str())
        else {
            return false;
        };
        match self.runtime_store.find_workspace_tab(tab_id).await {
            Ok(Some(tab)) => auto_closes_on_success(&tab),
            Ok(None) => false,
            Err(error) => {
                tracing::error!("failed to inspect setup terminal {tab_id} after exit: {error}");
                true
            }
        }
    }

    pub(super) async fn remove_terminal_session_tab(
        &mut self,
        session_id: &str,
    ) -> HostResult<bool> {
        self.disarm_terminal_pulse(session_id);
        let metadata = self
            .sessions
            .get(session_id)
            .map(|session| (session.workspace_id.clone(), session.tab_id.clone()));
        let Some((workspace_id, tab_id)) = metadata else {
            return Ok(false);
        };
        let tab_exists = self
            .runtime_store
            .find_workspace_tab(&tab_id)
            .await
            .map_err(|error| HostError::state(error.to_string()))?
            .is_some();
        if !tab_exists {
            return Ok(false);
        }
        self.runtime_store
            .remove_workspace_tab(&tab_id)
            .await
            .map_err(|error| HostError::state(error.to_string()))?;
        if let Err(error) = self
            .runtime_store
            .record_workspace_activity(&workspace_id, chrono::Utc::now())
            .await
        {
            tracing::error!("failed to record activity for workspace {workspace_id}: {error}");
        }
        self.flush_all_output(session_id);
        self.await_output_writes(session_id).await;
        if let Some(mut session) = self.sessions.remove(session_id) {
            session.terminate(true, &self.store).await;
        }
        self.broadcast_workspace_tabs_changed(Some(&workspace_id));
        self.broadcast_authenticated(event("workspaceActivityChanged", json!({})));
        Ok(true)
    }

    pub(super) async fn cleanup_orchestration_for_closed_session(
        &mut self,
        session_id: &str,
        reason: &str,
    ) {
        if self.agent_presence.remove(session_id) {
            self.broadcast_agent_presence_changes(vec![json!({
                "terminalSessionId": session_id,
                "removed": true,
            })]);
        }
        self.forget_push_session(session_id);
        self.orchestration_activity_last_recorded.remove(session_id);
        self.orchestration_delivery_in_flight.remove(session_id);
        self.orchestration_delivery_backpressured.remove(session_id);
        self.fail_active_dispatch_for_closed_session(session_id, reason)
            .await;
    }

    pub(super) fn handle_output_batch_tick(&mut self, session_id: String, generation: u64) {
        if self
            .sessions
            .get(&session_id)
            .is_some_and(|session| session.output_batch_due(generation))
        {
            self.flush_output_batch(&session_id);
        }
    }

    pub(super) fn handle_durable_output_batch_tick(&mut self, session_id: String, generation: u64) {
        if self
            .sessions
            .get(&session_id)
            .is_some_and(|session| session.durable_output_batch_due(generation))
        {
            self.flush_durable_output_batch(&session_id);
        }
    }

    fn flush_durable_output_batch(&mut self, session_id: &str) {
        let batch = self
            .sessions
            .get_mut(session_id)
            .and_then(Session::flush_durable_output_batch);
        if let Some(batch) = batch {
            self.persist_output_batch(session_id.to_string(), batch.sequence, batch.data);
        }
    }

    pub(super) fn flush_all_output(&mut self, session_id: &str) {
        self.flush_output_batch(session_id);
        self.flush_durable_output_batch(session_id);
    }

    pub(super) async fn handle_checkpoint_tick(&mut self, session_id: String, generation: u64) {
        let store = self.store.clone();
        let due = self
            .sessions
            .get_mut(&session_id)
            .is_some_and(|session| session.checkpoint_due(generation));
        if due {
            self.flush_durable_output_batch(&session_id);
            self.await_output_writes(&session_id).await;
            if let Some(session) = self.sessions.get_mut(&session_id) {
                let _ = session.write_checkpoint(&store, None).await;
            }
            let _ = store
                .trim_session(&session_id, self.config.scrollback_bytes as usize)
                .await;
        }
    }

    pub(super) async fn immediate_checkpoint(&mut self, session_id: &str) {
        let store = self.store.clone();
        self.flush_durable_output_batch(session_id);
        self.await_output_writes(session_id).await;
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.invalidate_checkpoint();
            let _ = session.write_checkpoint(&store, None).await;
            let _ = store
                .trim_session(session_id, self.config.scrollback_bytes as usize)
                .await;
        }
    }

    fn persist_output_batch(&mut self, session_id: String, sequence: i64, data: Vec<u8>) {
        let store = self.store.clone();
        let state = output_persistence_state(&self.runtime_dir, &session_id);
        state
            .pending
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        let task_session_id = session_id.clone();
        let completion = OutputPersistenceCompletion(std::sync::Arc::clone(&state));
        let task = async move {
            let _completion = completion;
            let _ = store.append_output(&task_session_id, sequence, &data).await;
        };
        if let Err(error) = self.deferred_admission.schedule(
            super::deferred_admission::DeferredRequestClass::Maintenance,
            "terminal.output.persist",
            None,
            task,
        ) {
            tracing::warn!(
                "terminal output persistence was not admitted for {session_id}: {}",
                error.wire_message()
            );
        }
    }

    pub(super) async fn await_output_writes(&mut self, session_id: &str) {
        let state = output_persistence_state(&self.runtime_dir, session_id);
        if tokio::time::timeout(OUTPUT_PERSISTENCE_BARRIER_TIMEOUT, async {
            loop {
                let notified = state.notify.notified();
                if state.pending.load(std::sync::atomic::Ordering::Acquire) == 0 {
                    break;
                }
                notified.await;
            }
        })
        .await
        .is_err()
        {
            tracing::warn!(
                "terminal output persistence barrier timed out for session {session_id}; continuing without blocking the host actor"
            );
        }
    }
}
