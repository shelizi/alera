use super::terminal_startup_commands::auto_closes_on_success;
use super::*;

impl ServerActor {
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
        if let Some(mut session) = self.sessions.remove(session_id) {
            session
                .terminate(true, &self.history, self.config.scrollback_bytes as usize)
                .await;
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
}
