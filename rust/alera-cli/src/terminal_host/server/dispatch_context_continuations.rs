//! What a committed dispatch context install resumes: inject the preamble,
//! paste for a coordinator tick, or finish an `agentSpawn`'s worker. Each arm
//! re-validates the part of actor state it depends on - the parked origin,
//! the target session, the coordinator run - because all of it may have
//! changed while the write ran.

use serde_json::Value;

use crate::terminal_host::host_error::HostError;
use crate::terminal_host::protocol::{error_response, ok_response};
use crate::terminal_host::session::Session;

use super::dispatch_context_install::{
    DispatchContextContinuation, DispatchInstallOrigin, PendingDispatchContext,
};
use super::ServerActor;

impl ServerActor {
    /// Resumes the parked continuation of a committed install.
    pub(super) async fn run_dispatch_context_continuation(
        &mut self,
        dispatch_id: &str,
        pending: PendingDispatchContext,
    ) {
        match pending.continuation {
            DispatchContextContinuation::DispatchRequest {
                origin,
                to,
                inject,
                force_submit,
                preamble,
                response,
                consumed_tab,
            } => {
                let request_target = match &origin {
                    DispatchInstallOrigin::Request {
                        client_id,
                        request_id,
                    } => Some((*client_id, *request_id)),
                    DispatchInstallOrigin::Coordinator { .. }
                    | DispatchInstallOrigin::Internal => None,
                };
                if inject {
                    if request_target
                        .is_some_and(|(client_id, _)| !self.clients.contains_key(&client_id))
                    {
                        let _ = self
                            .runtime_store
                            .fail_orchestration_startup(dispatch_id, "requester disconnected")
                            .await;
                        self.remove_dispatch_context(&to);
                        return;
                    }
                    if !self.sessions.get(&to).is_some_and(Session::running) {
                        self.remove_dispatch_context(&to);
                        let _ = self
                            .runtime_store
                            .fail_orchestration_startup(dispatch_id, "terminal session vanished")
                            .await;
                        if let Some((client_id, request_id)) = request_target {
                            self.client_write(
                                client_id,
                                error_response(
                                    request_id,
                                    &HostError::state(format!("terminal {to} vanished")),
                                ),
                            );
                        }
                        return;
                    }
                    if let Err(error) =
                        self.queue_orchestration_paste(&to, &preamble, Vec::new(), force_submit)
                    {
                        self.remove_dispatch_context(&to);
                        let _ = self
                            .runtime_store
                            .fail_orchestration_startup(dispatch_id, "terminal input unavailable")
                            .await;
                        match request_target {
                            Some((client_id, request_id)) => {
                                self.client_write(client_id, error_response(request_id, &error));
                            }
                            None => {
                                if let DispatchInstallOrigin::Coordinator { .. } = origin {
                                    self.coordinator_log(&format!(
                                        "dispatch failed: {}",
                                        error.wire_message()
                                    ));
                                }
                            }
                        }
                        return;
                    }
                }
                if let Some(mut tab) = consumed_tab {
                    tab.payload["pendingOrchestration"] = Value::Null;
                    tab.updated_at = chrono::Utc::now();
                    let workspace_id = tab.workspace_id.clone();
                    // Dropping this leaves the tab claiming a dispatch it
                    // already consumed, so a restart replays the prompt.
                    if let Err(error) = self.runtime_store.upsert_workspace_tab(tab).await {
                        tracing::error!(
                            "failed to clear the pending orchestration payload: {error}"
                        );
                    }
                    self.broadcast_workspace_tabs_changed(Some(&workspace_id));
                }
                if let Some((client_id, request_id)) = request_target {
                    self.client_write(client_id, ok_response(request_id, response));
                }
            }
            DispatchContextContinuation::CoordinatorPaste {
                run_id,
                task_id,
                handle,
                preamble,
                force_submit,
            } => {
                if !self.coordinators.contains_key(&run_id) {
                    let _ = self
                        .runtime_store
                        .fail_orchestration_startup(dispatch_id, "coordinator run stopped")
                        .await;
                    self.remove_dispatch_context(&handle);
                    return;
                }
                if !self.sessions.get(&handle).is_some_and(Session::running) {
                    self.remove_dispatch_context(&handle);
                    let _ = self
                        .runtime_store
                        .fail_orchestration_startup(dispatch_id, "terminal not writable")
                        .await;
                    return;
                }
                if let Err(error) =
                    self.queue_orchestration_paste(&handle, &preamble, Vec::new(), force_submit)
                {
                    self.remove_dispatch_context(&handle);
                    let _ = self
                        .runtime_store
                        .fail_orchestration_startup(dispatch_id, "terminal input unavailable")
                        .await;
                    self.coordinator_log(&format!(
                        "dispatch of {task_id} failed: {}",
                        error.wire_message()
                    ));
                    return;
                }
                self.coordinator_log(&format!("dispatched {task_id} to {handle}"));
            }
            DispatchContextContinuation::AgentSpawn { origin, pending } => {
                self.finish_agent_spawn_install(origin, *pending).await;
            }
            DispatchContextContinuation::Detached => {}
        }
    }

    /// Reports an install failure to the continuation's origin after the
    /// dispatch startup was failed.
    pub(super) fn fail_dispatch_context_continuation(
        &mut self,
        continuation: DispatchContextContinuation,
        error: HostError,
    ) {
        match continuation {
            DispatchContextContinuation::DispatchRequest { origin, .. }
            | DispatchContextContinuation::AgentSpawn { origin, .. } => match origin {
                DispatchInstallOrigin::Request {
                    client_id,
                    request_id,
                } => self.client_write(client_id, error_response(request_id, &error)),
                DispatchInstallOrigin::Coordinator { .. } => {
                    self.coordinator_log(&format!("dispatch failed: {}", error.wire_message()))
                }
                DispatchInstallOrigin::Internal => {}
            },
            DispatchContextContinuation::CoordinatorPaste { task_id, .. } => self.coordinator_log(
                &format!("dispatch of {task_id} failed: {}", error.wire_message()),
            ),
            DispatchContextContinuation::Detached => {}
        }
    }

    /// A parked install lost its context slot: its dispatch was failed by
    /// whoever invalidated the handle, so only a parked request origin needs
    /// an answer.
    pub(super) fn answer_superseded_dispatch_install(
        &self,
        continuation: DispatchContextContinuation,
    ) {
        let (client_id, request_id) = match continuation {
            DispatchContextContinuation::DispatchRequest { origin, .. }
            | DispatchContextContinuation::AgentSpawn { origin, .. } => match origin {
                DispatchInstallOrigin::Request {
                    client_id,
                    request_id,
                } => (client_id, request_id),
                _ => return,
            },
            _ => return,
        };
        self.client_write(
            client_id,
            error_response(
                request_id,
                &HostError::state("dispatch context superseded"),
            ),
        );
    }
}
