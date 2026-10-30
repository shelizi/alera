use serde_json::json;

use crate::terminal_host::protocol::{error_response, event, ok_response};
use crate::terminal_host::session::workspace_shutdown::WorkspaceShutdown;

use super::runtime_mutations::{
    RuntimeMutationEffect, RuntimeMutationFinished, RuntimeMutationOutcome, RuntimeMutationRequest,
};
use super::terminal_startup_commands::terminal_session_id;
use super::ServerActor;

impl ServerActor {
    /// A workspace/project cascade must not silently terminate a running,
    /// previously transferred PTY: it has no per-view receipt like tab.remove.
    /// Serialized runtime mutations block new terminal attachments until the
    /// background SQLite operation completes, after this actor preflight.
    async fn ensure_no_live_moved_terminal_for_cleanup(
        &self,
        workspace_id: &str,
    ) -> crate::terminal_host::host_error::HostResult<()> {
        use crate::terminal_host::host_error::HostError;

        // Do not erase a persisted transfer receipt merely because its
        // original process is already missing (or the Host restarted).
        // Until there is a verified crash-resolution action, even a fully
        // detached/dead moved Tab must survive an implicit cascade.
        if self
            .runtime_store
            .list_workspace_tabs(workspace_id)
            .await
            .map_err(|error| HostError::state(error.to_string()))?
            .iter()
            .any(|tab| tab.kind == "terminal" && tab.payload["lastTerminalTabTransfer"].is_object())
        {
            return Err(HostError::state(
                "Workspace cleanup would erase a transferred Terminal receipt. Explicitly resolve and close the Terminal Tab before deleting its Workspace or Project.",
            ));
        }
        for session in self.sessions.values().filter(|session| session.running()) {
            let stored = self
                .runtime_store
                .find_workspace_tab(&session.tab_id)
                .await
                .map_err(|error| HostError::state(error.to_string()))?;
            if stored.is_some_and(|tab| {
                tab.kind == "terminal"
                    && tab.payload["lastTerminalTabTransfer"].is_object()
                    && (tab.workspace_id == workspace_id || session.workspace_id == workspace_id)
            }) {
                return Err(HostError::state(
                    "Workspace cleanup cannot silently terminate a transferred live PTY, even after every view detached. Close its Tab explicitly from the current owning view first.",
                ));
            }
        }
        Ok(())
    }

    pub(super) async fn prepare_runtime_mutation(
        &mut self,
        client_id: u64,
        request: &RuntimeMutationRequest,
    ) -> crate::terminal_host::host_error::HostResult<WorkspaceShutdown> {
        use crate::terminal_host::host_error::HostError;

        if let RuntimeMutationRequest::RemoveManagedWorkspace { request } = request {
            return self.prepare_managed_workspace_removal(request).await;
        }
        if let RuntimeMutationRequest::RemoveTab { tab_id, .. } = request {
            if let Some(tab) = self
                .runtime_store
                .find_workspace_tab(tab_id)
                .await
                .map_err(|error| HostError::state(error.to_string()))?
            {
                if tab.kind == "terminal" && tab.payload["lastTerminalTabTransfer"].is_object() {
                    let session_id = terminal_session_id(&tab);
                    let Some(session) = self.sessions.get(&session_id).filter(|session| {
                        session.running()
                            && session.tab_id == *tab_id
                            && session.workspace_id == tab.workspace_id
                    }) else {
                        // Do not erase crash evidence by deleting metadata
                        // while the original PTY's identity is unknown.
                        return Err(HostError::state(
                            "Transferred Terminal's original live PTY owner cannot be confirmed. Resolve recovery before deleting its Tab.",
                        ));
                    };
                    let receipt = &tab.payload["lastTerminalTabTransfer"];
                    let status = self
                        .terminal_tab_transfer_status(
                            client_id,
                            &json!({
                                "tabId": tab_id,
                                "sessionId": session_id,
                                "sourceWorkspaceId": receipt["from"],
                                "targetWorkspaceId": receipt["to"],
                                "clientMutationId": receipt["id"],
                            }),
                        )
                        .await?;
                    if status["applied"] != true {
                        return Err(HostError::state(
                            "Transferred Terminal PTY instance is not verified. Resolve its original transfer before deleting the Tab.",
                        ));
                    }
                    if !session.clients.contains(&client_id) {
                        return Err(HostError::state(
                            "The requesting client is not attached to this transferred Terminal. Reattach from its current workspace before closing it.",
                        ));
                    }
                    if session
                        .clients
                        .iter()
                        .any(|attached| *attached != client_id)
                    {
                        return Err(HostError::state(
                            "Transferred Terminal has another attached viewer. All other desktop/mobile views must detach before removing the Tab.",
                        ));
                    }
                }
            }
            return Ok(WorkspaceShutdown::default());
        }
        // Other serialized removals can delete the same terminal implicitly.
        // Detaching all viewers is not consent to kill an existing live PTY.
        let affected_workspace_ids = match request {
            RuntimeMutationRequest::RemoveWorkspace { workspace_id, .. }
            | RuntimeMutationRequest::RemoveWorkspaceTabs { workspace_id }
            | RuntimeMutationRequest::SleepWorkspace { workspace_id } => {
                vec![workspace_id.clone()]
            }
            RuntimeMutationRequest::RemoveProject { project_id }
            | RuntimeMutationRequest::RemoveProjectWorkspaces { project_id } => self
                .runtime_store
                .list_workspaces(project_id)
                .await
                .map_err(|error| HostError::state(error.to_string()))?
                .into_iter()
                .map(|workspace| workspace.id)
                .collect(),
            _ => Vec::new(),
        };
        for workspace_id in &affected_workspace_ids {
            self.ensure_no_live_moved_terminal_for_cleanup(workspace_id)
                .await?;
        }
        // Check when the queued operation starts, not when it was enqueued:
        // an earlier removal may have just failed and retained a shutdown.
        for workspace_id in self.mutation_queue.pending_workspace_shutdowns.keys() {
            let removes_owner = match request {
                RuntimeMutationRequest::RemoveWorkspace {
                    workspace_id: target,
                    ..
                } => target == workspace_id,
                RuntimeMutationRequest::RemoveProject { project_id }
                | RuntimeMutationRequest::RemoveProjectWorkspaces { project_id } => {
                    let workspace = self
                        .runtime_store
                        .find_workspace(workspace_id)
                        .await
                        .map_err(|error| HostError::state(error.to_string()))?;
                    workspace.is_none_or(|workspace| workspace.project_id == *project_id)
                }
                _ => false,
            };
            if removes_owner {
                return Err(HostError::state("Workspace has unfinished process shutdown. Retry confirmed workspace cleanup before removing its records or project."));
            }
        }
        Ok(WorkspaceShutdown::default())
    }

    pub(super) async fn prepare_managed_workspace_removal(
        &mut self,
        request: &crate::managed_workspace::ManagedWorkspaceRemoveRequest,
    ) -> crate::terminal_host::host_error::HostResult<WorkspaceShutdown> {
        use crate::terminal_host::host_error::HostError;

        if !request.close_sessions
            && request.active_workspace_id.as_deref() == Some(request.id.as_str())
        {
            return Err(HostError::state("Workspace is active in the workbench"));
        }
        // close_sessions=true terminates before Git or SQLite cleanup, so
        // the actor must reject even fully detached moved sessions.
        self.ensure_no_live_moved_terminal_for_cleanup(&request.id)
            .await?;
        if request.close_sessions {
            let mut shutdown = WorkspaceShutdown::capture(
                self.sessions
                    .values()
                    .filter(|session| session.workspace_id == request.id),
            )
            .await?;
            if let Some(pending) = self
                .mutation_queue
                .pending_workspace_shutdowns
                .remove(&request.id)
            {
                shutdown.merge(pending);
            }
            self.terminate_terminal_sessions_for_workspace(&request.id)
                .await;
            return Ok(shutdown);
        } else if self
            .mutation_queue
            .pending_workspace_shutdowns
            .contains_key(&request.id)
        {
            return Err(HostError::state(
                "Workspace has unfinished process shutdown. Confirm session cleanup to retry.",
            ));
        } else if self
            .sessions
            .values()
            .any(|session| session.workspace_id == request.id && session.running())
        {
            return Err(HostError::state(
                "Workspace has a live terminal session or process",
            ));
        }
        Ok(WorkspaceShutdown::default())
    }

    pub(super) async fn handle_runtime_mutation_finished(
        &mut self,
        finished: RuntimeMutationFinished,
    ) {
        let RuntimeMutationFinished {
            client_id,
            request_id,
            outcome,
        } = finished;
        let RuntimeMutationOutcome {
            result,
            ended_pointer_tab_ids,
            mut closed_session_tab_ids,
            committed_tab_ids,
            effect_on_error,
            stopped_workspace_tab_ids,
            pending_workspace_shutdown,
        } = outcome;
        if let Some(pending) = pending_workspace_shutdown {
            let (workspace_id, shutdown) = *pending;
            // Sessions are already gone. Keep their process ownership for the
            // next attempt instead of letting an empty recapture permit deletion.
            self.mutation_queue
                .pending_workspace_shutdowns
                .insert(workspace_id, shutdown);
        }
        // Teardown is irreversible even if a later Git operation fails. Retire
        // only those tabs whose resources were closed, and notify every client
        // so scrollback and transcript watches are released.
        let mut stopped_tab_cleanup_error = None;
        for tab_id in &stopped_workspace_tab_ids {
            if let Err(error) = self.runtime_store.remove_workspace_tab(tab_id).await {
                stopped_tab_cleanup_error =
                    Some(crate::terminal_host::host_error::HostError::state(format!(
                        "Failed to retire stopped workspace tab: {error}"
                    )));
            }
        }
        if !stopped_workspace_tab_ids.is_empty() {
            self.broadcast_workspace_tabs_changed(None);
        }
        let _ = ended_pointer_tab_ids;
        closed_session_tab_ids.extend(committed_tab_ids);
        closed_session_tab_ids.sort_unstable();
        closed_session_tab_ids.dedup();
        let _ = closed_session_tab_ids;
        match result {
            Ok(completion) => {
                let _ = completion.closed_tab_ids;
                self.apply_runtime_mutation_effect(completion.effect).await;
                if let Some(error) = stopped_tab_cleanup_error {
                    self.client_write(client_id, error_response(request_id, &error));
                } else {
                    self.client_write(client_id, ok_response(request_id, completion.response));
                }
            }
            Err(error) => {
                if let Some(effect) = effect_on_error {
                    self.apply_runtime_mutation_effect(effect).await;
                }
                self.client_write(client_id, error_response(request_id, &error));
            }
        }
        self.complete_runtime_mutation();
        self.schedule_shutdown_if_idle();
    }

    async fn apply_runtime_mutation_effect(&mut self, effect: RuntimeMutationEffect) {
        match effect {
            RuntimeMutationEffect::ProjectRemoved {
                project_id,
                workspace_ids,
            } => {
                if let Some(server) = self.codex.as_ref() {
                    server.clear_thread_hydrations().await;
                }
                self.terminate_terminal_sessions_for_workspaces(&workspace_ids)
                    .await;
                self.broadcast_authenticated(event("projectsChanged", json!({})));
                self.broadcast_workspaces_changed(Some(&project_id));
                self.broadcast_workspace_tabs_changed(None);
                self.broadcast_authenticated(event("projectConfigsChanged", json!({})));
            }
            RuntimeMutationEffect::WorkspaceRemoved { workspace_id } => {
                if let Some(server) = self.codex.as_ref() {
                    server.clear_thread_hydrations().await;
                }
                self.terminate_terminal_sessions_for_workspace(&workspace_id)
                    .await;
                self.broadcast_workspaces_changed(None);
                self.broadcast_workspace_tabs_changed(Some(&workspace_id));
            }
            RuntimeMutationEffect::ProjectWorkspacesRemoved {
                project_id,
                workspace_ids,
            } => {
                if let Some(server) = self.codex.as_ref() {
                    server.clear_thread_hydrations().await;
                }
                self.terminate_terminal_sessions_for_workspaces(&workspace_ids)
                    .await;
                self.broadcast_workspaces_changed(Some(&project_id));
                self.broadcast_workspace_tabs_changed(None);
            }
            RuntimeMutationEffect::ManagedWorkspaceRemoved {
                project_id,
                workspace_id,
            } => {
                if let Some(server) = self.codex.as_ref() {
                    server.clear_thread_hydrations().await;
                }
                self.terminate_terminal_sessions_for_workspace(&workspace_id)
                    .await;
                self.broadcast_workspaces_changed(Some(&project_id));
                self.broadcast_workspace_tabs_changed(Some(&workspace_id));
            }
            RuntimeMutationEffect::WorkspaceBranchSwitched {
                project_id,
                workspace_id,
            } => {
                // Branch switching mutates the checkout in place. Keep terminal
                // and agent sessions alive; only invalidate workspace metadata
                // and tab consumers that derive source-control state from it.
                self.broadcast_workspaces_changed(Some(&project_id));
                self.broadcast_workspace_tabs_changed(Some(&workspace_id));
            }
            RuntimeMutationEffect::TabRemoved {
                tab_id,
                workspace_id,
            } => {
                // A stale, refused tab.remove must not cancel a moved tab's
                // agent-title job. Cancel only after the serialized removal
                // has actually committed.
                self.cancel_agent_title_job(&tab_id);
                if let Some(server) = self.codex.as_ref() {
                    server.forget_thread_hydration(&tab_id).await;
                }
                self.terminate_terminal_sessions_for_tab(&tab_id).await;
                self.broadcast_workspace_tabs_changed(workspace_id.as_deref());
            }
            RuntimeMutationEffect::WorkspaceTabsRemoved { workspace_id } => {
                if let Some(server) = self.codex.as_ref() {
                    server.clear_thread_hydrations().await;
                }
                self.terminate_terminal_sessions_for_workspace(&workspace_id)
                    .await;
                self.broadcast_workspace_tabs_changed(Some(&workspace_id));
                self.broadcast_authenticated(event("workbenchLayoutsChanged", json!({})));
            }
            RuntimeMutationEffect::WorkspaceSlept { workspace_id } => {
                if let Some(server) = self.codex.as_ref() {
                    server.clear_thread_hydrations().await;
                }
                self.terminate_terminal_sessions_for_workspace(&workspace_id)
                    .await;
                self.broadcast_workspace_tabs_changed(Some(&workspace_id));
                self.broadcast_authenticated(event(
                    "workbenchLayoutsChanged",
                    json!({"workspaceId": workspace_id}),
                ));
                self.broadcast_authenticated(event(
                    "workspaceActivityChanged",
                    json!({"workspaceId": workspace_id}),
                ));
            }
        }
    }

    pub(super) async fn terminate_sessions_for_tab(&mut self, tab_id: &str) {
        self.cancel_agent_title_job(tab_id);
        self.terminate_terminal_sessions_for_tab(tab_id).await;
    }

    async fn terminate_terminal_sessions_for_tab(&mut self, tab_id: &str) {
        let session_ids = self
            .sessions
            .iter()
            .filter(|(_, session)| session.tab_id == tab_id)
            .map(|(session_id, _)| session_id.clone())
            .collect();
        self.terminate_sessions(session_ids).await;
    }

    async fn terminate_terminal_sessions_for_workspace(&mut self, workspace_id: &str) {
        let session_ids = self
            .sessions
            .iter()
            .filter(|(_, session)| session.workspace_id == workspace_id)
            .map(|(session_id, _)| session_id.clone())
            .collect();
        self.terminate_sessions(session_ids).await;
    }

    async fn terminate_terminal_sessions_for_workspaces(&mut self, workspace_ids: &[String]) {
        let session_ids = self
            .sessions
            .iter()
            .filter(|(_, session)| {
                workspace_ids
                    .iter()
                    .any(|workspace_id| workspace_id == &session.workspace_id)
            })
            .map(|(session_id, _)| session_id.clone())
            .collect();
        self.terminate_sessions(session_ids).await;
    }

    async fn terminate_sessions(&mut self, session_ids: Vec<String>) {
        if session_ids.is_empty() {
            return;
        }
        for session_id in session_ids {
            self.disarm_terminal_pulse(&session_id);
            self.queue_terminal_exit_push(&session_id, None).await;
            self.cleanup_orchestration_for_closed_session(
                &session_id,
                "terminal was explicitly terminated",
            )
            .await;
            self.flush_all_output(&session_id);
            if let Some(mut session) = self.sessions.remove(&session_id) {
                session
                    .terminate(true, &self.history, self.config.scrollback_bytes as usize)
                    .await;
            }
        }
        self.schedule_shutdown_if_idle();
    }
}
