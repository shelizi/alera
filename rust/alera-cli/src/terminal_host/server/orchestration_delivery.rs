use std::time::Duration;

use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::orchestration::agent_prompt_injection as prompt_injection;
use crate::terminal_host::orchestration::message_delivery::{
    skips_auto_enter, DEFERRED_ENTER_DELAY_MS,
};
use crate::terminal_host::orchestration::message_formatter::format_messages_for_injection;
use crate::terminal_host::protocol::event;
use crate::terminal_host::session::{PtyWriteCompletion, Session};

use super::{deferred_admission, ServerActor, ServerCommand, TERMINAL_INPUT_BACKPRESSURE_CODE};

impl ServerActor {
    /// Pushes unread messages to an idle agent and stamps `delivered_at` only
    /// after the deferred Enter succeeds. Active coordinators are excluded so
    /// auto-submit cannot steal their prompt.
    pub(super) async fn deliver_pending_messages(&mut self, handle: &str) {
        if self.is_active_coordinator_handle(handle) {
            return;
        }
        let running = self.sessions.get(handle).is_some_and(Session::running);
        if !running {
            return;
        }
        if self.orchestration_delivery_in_flight.contains(handle) {
            return;
        }
        self.orchestration_delivery_backpressured.remove(handle);
        let messages = match self
            .runtime_store
            .undelivered_unread_orchestration_messages(handle)
            .await
        {
            Ok(messages) if !messages.is_empty() => messages,
            _ => return,
        };
        let formatted = format_messages_for_injection(&messages);
        let Some(session) = self.sessions.get_mut(handle) else {
            return;
        };
        let session_instance_id = session.instance_id();
        let ids: Vec<String> = messages.iter().map(|message| message.id.clone()).collect();
        let paste = prompt_injection::build_agent_prompt_paste_bytes(&formatted);
        if let Err(error) = session.queue_write(
            PtyWriteCompletion::OrchestrationPaste {
                session_instance_id,
                message_ids: ids,
                force_submit: false,
            },
            &paste,
        ) {
            if matches!(
                &error,
                HostError::Conflict { code, .. }
                    if code == TERMINAL_INPUT_BACKPRESSURE_CODE
            ) {
                self.orchestration_delivery_backpressured
                    .insert(handle.to_string());
            } else {
                self.broadcast_terminal_error(handle, error.wire_message());
            }
            return;
        }
        self.orchestration_delivery_in_flight
            .insert(handle.to_string());
    }

    fn is_active_coordinator_handle(&self, handle: &str) -> bool {
        self.coordinators
            .values()
            .any(|coordinator| coordinator.config.coordinator_handle.as_deref() == Some(handle))
    }

    /// Send-time hook: deliver immediately only when the recipient's agent is
    /// already idle right now.
    pub(super) async fn deliver_pending_messages_if_idle(&mut self, handle: &str) {
        if self.agent_presence.is_injection_ready(handle) {
            self.deliver_pending_messages(handle).await;
        }
    }

    pub(super) async fn retry_backpressured_delivery_if_idle(&mut self, handle: &str) {
        if self.orchestration_delivery_backpressured.contains(handle)
            && self.agent_presence.is_injection_ready(handle)
        {
            self.deliver_pending_messages(handle).await;
        }
    }

    pub(super) async fn handle_orchestration_deferred_enter(
        &mut self,
        session_id: String,
        session_instance_id: u64,
        message_ids: Vec<String>,
        force_submit: bool,
    ) {
        let skip_auto_enter = message_ids.is_empty()
            && !force_submit
            && skips_auto_enter(self.agent_presence.agent_type(&session_id));
        let current_instance_id = self.sessions.get(&session_id).map(Session::instance_id);
        if current_instance_id != Some(session_instance_id) {
            self.orchestration_delivery_in_flight.remove(&session_id);
            return;
        }
        if !message_ids.is_empty() && self.is_active_coordinator_handle(&session_id) {
            self.orchestration_delivery_in_flight.remove(&session_id);
            return;
        }
        let Some(session) = self.sessions.get_mut(&session_id) else {
            return;
        };
        if !session.running() {
            // Terminal closed in the 500ms window: leave delivered_at NULL so
            // the batch is redelivered on the next idle transition.
            self.orchestration_delivery_in_flight.remove(&session_id);
            return;
        }
        if skip_auto_enter {
            return;
        }
        if let Err(error) = session.queue_write(
            PtyWriteCompletion::OrchestrationEnter {
                session_instance_id,
                message_ids: message_ids.clone(),
            },
            prompt_injection::AGENT_PROMPT_SUBMIT,
        ) {
            if matches!(
                &error,
                HostError::Conflict { code, .. }
                    if code == TERMINAL_INPUT_BACKPRESSURE_CODE
            ) {
                self.schedule_orchestration_enter(
                    session_id,
                    session_instance_id,
                    message_ids,
                    force_submit,
                );
                return;
            }
            self.orchestration_delivery_in_flight.remove(&session_id);
            self.broadcast_terminal_error(&session_id, error.wire_message());
        }
    }

    pub(super) fn schedule_orchestration_enter(
        &mut self,
        session_id: String,
        session_instance_id: u64,
        message_ids: Vec<String>,
        force_submit: bool,
    ) {
        let inbox = self.inbox.clone();
        let cleanup_session_id = session_id.clone();
        if let Err(error) = self.deferred_admission.schedule_delayed(
            Duration::from_millis(DEFERRED_ENTER_DELAY_MS),
            deferred_admission::DeferredRequestClass::Maintenance,
            "orchestration.deferredEnter",
            None,
            async move {
                let _ = inbox.send(ServerCommand::OrchestrationDeferredEnter {
                    session_id,
                    session_instance_id,
                    message_ids,
                    force_submit,
                });
            },
        ) {
            self.orchestration_delivery_in_flight
                .remove(&cleanup_session_id);
            self.broadcast_terminal_error(&cleanup_session_id, error.wire_message());
        }
    }

    pub(super) fn queue_orchestration_paste(
        &mut self,
        session_id: &str,
        prompt: &str,
        message_ids: Vec<String>,
        force_submit: bool,
    ) -> HostResult<()> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| HostError::state(format!("terminal {session_id} vanished")))?;
        let session_instance_id = session.instance_id();
        let paste = prompt_injection::build_agent_prompt_paste_bytes(prompt);
        session.queue_write(
            PtyWriteCompletion::OrchestrationPaste {
                session_instance_id,
                message_ids,
                force_submit,
            },
            &paste,
        )
    }

    pub(super) fn queue_orchestration_control(
        &mut self,
        session_id: &str,
        bytes: &[u8],
    ) -> HostResult<()> {
        let session = self
            .sessions
            .get_mut(session_id)
            .filter(|session| session.running())
            .ok_or_else(|| HostError::state(format!("terminal is not running: {session_id}")))?;
        let session_instance_id = session.instance_id();
        session.queue_write(
            PtyWriteCompletion::OrchestrationEnter {
                session_instance_id,
                message_ids: Vec::new(),
            },
            bytes,
        )
    }

    pub(super) fn broadcast_terminal_error(&self, session_id: &str, message: String) {
        if let Some(session) = self.sessions.get(session_id) {
            let clients: Vec<u64> = session.clients.iter().copied().collect();
            self.broadcast(&clients, event("error", session.error_payload(&message)));
        }
    }
}
