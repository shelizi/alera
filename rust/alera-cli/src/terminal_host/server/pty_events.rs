use super::*;
use crate::terminal_host::session::PtyEvent;

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
        self.record_orchestration_output_activity(&session_id);
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

    fn record_orchestration_output_activity(&mut self, session_id: &str) {
        if self
            .orchestration_activity_last_recorded
            .get(session_id)
            .is_some_and(|last| last.elapsed() < ORCHESTRATION_ACTIVITY_WRITE_INTERVAL)
        {
            return;
        }
        self.orchestration_activity_last_recorded
            .insert(session_id.to_string(), Instant::now());

        let runtime_store = self.runtime_store.clone();
        let task_session_id = session_id.to_string();
        let log_session_id = task_session_id.clone();
        let task = async move {
            let dispatch = match runtime_store
                .active_orchestration_dispatch_for_handle(&task_session_id)
                .await
            {
                Ok(dispatch) => dispatch,
                Err(_) => return,
            };
            if let Some(dispatch) = dispatch {
                let _ = runtime_store
                    .record_orchestration_activity(&dispatch.id)
                    .await;
            }
        };
        if let Err(error) = self.deferred_admission.schedule(
            super::deferred_admission::DeferredRequestClass::Maintenance,
            "terminal.orchestrationActivity.persist",
            None,
            task,
        ) {
            tracing::debug!(
                session_id = %log_session_id,
                "terminal orchestration activity persistence deferred: {}",
                error.wire_message()
            );
        }
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
        let due = self
            .sessions
            .get_mut(&session_id)
            .is_some_and(|session| session.checkpoint_due(generation));
        if !due {
            return;
        }
        self.flush_durable_output_batch(&session_id);
        if let Some(session) = self.sessions.get_mut(&session_id) {
            let checkpoint = session.checkpoint(None);
            self.history
                .queue_checkpoint(checkpoint, self.config.scrollback_bytes as usize);
        }
    }

    pub(super) async fn immediate_checkpoint(&mut self, session_id: &str) {
        self.flush_durable_output_batch(session_id);
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.invalidate_checkpoint();
            let checkpoint = session.checkpoint(None);
            self.history
                .queue_checkpoint(checkpoint, self.config.scrollback_bytes as usize);
        }
    }

    fn persist_output_batch(&mut self, session_id: String, sequence: i64, data: Vec<u8>) {
        self.history.queue_output(
            session_id,
            sequence,
            data,
            self.config.scrollback_bytes as usize,
        );
    }
}
