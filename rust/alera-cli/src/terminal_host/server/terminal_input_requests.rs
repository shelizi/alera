use serde_json::Value;
use std::time::Duration;

use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::protocol::decode_bytes;
use crate::terminal_host::session::PtyWriteCompletion;

use super::ClientKind;
use super::ServerActor;

impl ServerActor {
    /// Global lifecycle actions are local-only, Host-authoritative, and
    /// scoped to the exact running Agent. For transferred PTYs require the
    /// original attached view's transfer/lease protocol instead.
    pub(super) fn verify_global_agent_control_target(
        &self,
        session_id: &str,
        payload: &Value,
    ) -> HostResult<&str> {
        let expected_workspace = payload
            .get("expectedWorkspaceId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty());
        let expected_tab = payload
            .get("expectedTabId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty());
        let session = self.sessions.get(session_id).ok_or_else(|| {
            HostError::state("Global Agent is no longer in the Host inventory. Refresh first.")
        })?;
        if !session.running()
            || expected_workspace != Some(session.workspace_id.as_str())
            || expected_tab != Some(session.tab_id.as_str())
        {
            return Err(HostError::state(
                "Global Agent control rejected: Session, Workspace or Tab changed.",
            ));
        }
        if !session.moved_workspace_fence.is_empty() {
            return Err(HostError::state(
                "Transferred Agent requires its original view and transfer lease to control.",
            ));
        }
        let provider = self.agent_presence.agent_type(session_id).ok_or_else(|| {
            HostError::state(
                "Agent Hook identity is not verified. Open original Terminal to control.",
            )
        })?;
        if alera_core::agent_descriptor::agent_descriptor(provider).is_none() {
            return Err(HostError::state(
                "Unsupported Agent interrupt/terminate adapter.",
            ));
        }
        Ok(provider)
    }

    /// Enqueue the provider-declared interrupt bytes into a verified, live
    /// Agent PTY. Never terminate the session or synthesize an Enter/paste.
    pub(super) fn queue_verified_agent_interrupt(
        &mut self,
        client_id: u64,
        request_id: i64,
        payload: &Value,
    ) -> HostResult<bool> {
        let session_id = self.require_session(payload)?;
        let provider = self.verify_global_agent_control_target(&session_id, payload)?;
        let descriptor = alera_core::agent_descriptor::agent_descriptor(provider)
            .ok_or_else(|| HostError::state("Unsupported Agent interrupt adapter."))?;
        let interrupt_bytes = descriptor.interrupt_bytes;
        let session = self
            .sessions
            .get_mut(&session_id)
            .ok_or_else(|| HostError::state("Agent Session was closed before interrupt."))?;
        session.queue_write(
            PtyWriteCompletion::ClientRequest {
                client_id,
                request_id,
            },
            interrupt_bytes,
        )?;
        Ok(true)
    }

    /// Session-specific Native input must stay bound to the same live Host
    /// view after a worktree transfer. No SQLite round trip per keystroke.
    pub(super) fn queue_observed_terminal_input(
        &mut self,
        client_id: u64,
        request_id: i64,
        payload: &Value,
    ) -> HostResult<bool> {
        let session_id = self.require_session(payload)?;
        let workspace_id = payload.get("workspaceId").and_then(Value::as_str);
        let tab_id = payload.get("tabId").and_then(Value::as_str);
        let lease = payload.get("viewLeaseId").and_then(Value::as_str);
        let authorized = self.sessions.get(&session_id).is_some_and(|session| {
            session.running()
                && workspace_id == Some(session.workspace_id.as_str())
                && tab_id == Some(session.tab_id.as_str())
                && lease.is_some_and(|lease| session.has_view_lease(client_id, lease))
        });
        if !authorized {
            return Err(HostError::state(
                "Terminal input requires the observed live Workspace, Tab and view lease.",
            ));
        }
        self.queue_terminal_input(client_id, request_id, payload)
    }

    pub(super) async fn finish_terminal_client_write(
        &mut self,
        session_id: &str,
        client_id: u64,
        request_id: i64,
        error: Option<String>,
    ) {
        if let Some(message) = error {
            self.client_write(
                client_id,
                crate::terminal_host::protocol::error_response(
                    request_id,
                    &HostError::state(message),
                ),
            );
            return;
        }
        self.client_write(
            client_id,
            crate::terminal_host::protocol::ok_response(request_id, serde_json::json!({})),
        );
        self.retry_backpressured_delivery_if_idle(session_id).await;
    }

    /// Dedicated, fail-closed cross-workspace Agent RPC. Old Hosts reject this
    /// request type instead of downgrading to an unsafe handle-only write.
    pub(super) fn queue_verified_agent_input(
        &mut self,
        client_id: u64,
        request_id: i64,
        payload: &Value,
    ) -> HostResult<bool> {
        let valid_owner = ["expectedWorkspaceId", "expectedTabId"]
            .into_iter()
            .all(|field| {
                payload
                    .get(field)
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.is_empty())
            });
        if !valid_owner {
            return Err(HostError::state(
                "Guarded terminal write requires workspace and tab.",
            ));
        }
        self.queue_terminal_input(client_id, request_id, payload)
    }

    pub(super) fn queue_terminal_input(
        &mut self,
        client_id: u64,
        request_id: i64,
        payload: &Value,
    ) -> HostResult<bool> {
        let session_id = self.require_session(payload)?;
        // Global Agent control has no foreground terminal lease. Require the
        // complete owner identity at the Host write boundary, not just a UI
        // preflight: a moved tab/reused handle must never receive the prompt.
        // Legacy terminal clients remain unchanged when neither field exists.
        let expected_workspace = payload.get("expectedWorkspaceId").filter(|v| !v.is_null());
        let expected_tab = payload.get("expectedTabId").filter(|v| !v.is_null());
        if expected_workspace.is_some() || expected_tab.is_some() {
            let workspace = expected_workspace
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| {
                    HostError::state("Guarded terminal write requires workspace and tab.")
                })?;
            let tab = expected_tab
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| {
                    HostError::state("Guarded terminal write requires workspace and tab.")
                })?;
            let identity_matches = self.sessions.get(&session_id).is_some_and(|session| {
                session.running() && session.workspace_id == workspace && session.tab_id == tab
            });
            if !identity_matches {
                return Err(HostError::state(
                    "Guarded terminal write rejected: Session, Workspace or Tab changed.",
                ));
            }
        }
        let mut bytes = decode_bytes(payload.get("dataBase64"))?;
        let deferred_enter = payload
            .get("deferredEnter")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let enter_only = bytes.is_empty() && deferred_enter;
        if bytes.is_empty() && !deferred_enter {
            return Ok(false);
        }
        if enter_only {
            bytes.extend_from_slice(
                crate::terminal_host::orchestration::agent_prompt_injection::AGENT_PROMPT_SUBMIT,
            );
        } else if payload
            .get("bracketedPaste")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            bytes = crate::terminal_host::orchestration::agent_prompt_injection::build_agent_prompt_paste_bytes(
                &String::from_utf8_lossy(&bytes),
            );
        }
        let session = self
            .sessions
            .get_mut(&session_id)
            .ok_or_else(|| HostError::state("Terminal session is not attached."))?;
        let completion = PtyWriteCompletion::ClientRequest {
            client_id,
            request_id,
        };
        if deferred_enter && !enter_only {
            session.queue_write_deferred(
                completion,
                &bytes,
                Duration::from_millis(
                    crate::terminal_host::orchestration::message_delivery::DEFERRED_ENTER_DELAY_MS,
                ),
                crate::terminal_host::orchestration::agent_prompt_injection::AGENT_PROMPT_SUBMIT,
            )?;
        } else {
            session.queue_write(completion, &bytes)?;
        }
        // Typing from the phone claims the driver seat (most recent actor
        // wins) without changing the current viewport.
        let is_mobile = self
            .clients
            .get(&client_id)
            .is_some_and(|client| client.kind == ClientKind::Mobile);
        if is_mobile {
            self.claim_mobile_driver(client_id, &session_id, None);
        }
        Ok(true)
    }
}
