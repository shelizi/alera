use super::disconnect_reason::DisconnectReason;
use super::{control_file, ServerActor};
use std::time::Duration;

const HISTORY_SHUTDOWN_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

impl ServerActor {
    pub(super) async fn dispose(&mut self) {
        if self.disposed {
            return;
        }
        self.disposed = true;
        for tab_id in self.agent_title_jobs.keys().cloned().collect::<Vec<_>>() {
            self.cancel_agent_title_job(&tab_id);
        }
        self.cancel_shutdown_timer();
        self.codex = None;
        self.stop_remote_relay().await;
        if let Some(handle) = self.mobile_gateway.take() {
            handle.abort();
        }
        let client_ids = self.clients.keys().copied().collect::<Vec<_>>();
        for client_id in client_ids {
            self.dispose_client_with_reason(client_id, DisconnectReason::HostShutdown)
                .await;
        }
        let session_ids: Vec<String> = self.sessions.keys().cloned().collect();
        for session_id in session_ids {
            self.terminal_pulses.disarm(&session_id);
            self.cleanup_orchestration_for_closed_session(&session_id, "terminal host shut down")
                .await;
            self.flush_all_output(&session_id);
            if let Some(mut session) = self.sessions.remove(&session_id) {
                session
                    .terminate(false, &self.history, self.config.scrollback_bytes as usize)
                    .await;
            }
        }
        // Normal request/liveness handling never waits on SQLite. Once the
        // actor is already disposed, give the write-behind repository a short
        // chance to make the last in-memory checkpoint durable before the
        // process exits. A sick database must not hold shutdown indefinitely.
        if !self
            .history
            .flush_pending(HISTORY_SHUTDOWN_DRAIN_TIMEOUT)
            .await
        {
            tracing::warn!(
                "terminal history still has in-memory writes after shutdown drain timeout"
            );
        }
        control_file::delete_control_file(&self.control_file_path);
    }
}
