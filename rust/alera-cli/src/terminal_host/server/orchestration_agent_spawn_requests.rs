use alera_core::agent_descriptor::agent_descriptor;
use alera_core::git::GitBaseDrift;
use alera_core::runtime::{OrchestrationDispatchStatus, WorkspaceStatus};
use serde_json::{json, Value};

use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::orchestration::coordinator_loop::CoordinatorConfig;
use crate::terminal_host::orchestration::dispatch_preamble::build_dispatch_bootstrap;

use super::dispatch_context_install::{
    DispatchContextContinuation, DispatchInstallOrigin, PendingAgentSpawn,
};
use super::orchestration_dispatch_requests::DispatchPreparation;
use super::orchestration_validation::{optional_string, require_string, state_error};
use super::ServerActor;

impl ServerActor {
    pub(super) async fn fail_active_dispatch_for_closed_session(
        &mut self,
        session_id: &str,
        reason: &str,
    ) {
        let dispatch = match self
            .runtime_store
            .active_orchestration_dispatch_for_handle(session_id)
            .await
        {
            Ok(dispatch) => dispatch,
            Err(error) => {
                tracing::error!(
                    "failed to inspect active orchestration dispatch for exited terminal {session_id}: {error}"
                );
                return;
            }
        };
        let Some(dispatch) = dispatch else {
            let Some(metadata) = self.owned_spawn_metadata(session_id).await else {
                return;
            };
            if !metadata.pending_readiness {
                return;
            }
            match self
                .runtime_store
                .record_orchestration_task_startup_failure(&metadata.task_id, reason)
                .await
            {
                Ok(_) => {
                    self.mark_owned_spawn_failure(session_id, reason).await;
                }
                Err(error) => {
                    tracing::error!(
                        "failed to record orchestration startup exit for task {}: {error}",
                        metadata.task_id
                    );
                }
            }
            return;
        };
        let result = if dispatch.status == OrchestrationDispatchStatus::AwaitingAcceptance
            && self
                .is_owned_orchestration_spawn(session_id, &dispatch.task_id)
                .await
        {
            let failed = self
                .runtime_store
                .fail_orchestration_startup(&dispatch.id, reason)
                .await;
            if failed.is_ok() {
                // The tab has to say a failure is already on the task's budget.
                // A spawn timeout arriving afterwards finds the dispatch no
                // longer active and would otherwise charge the task a second
                // time for the same dead terminal.
                self.mark_owned_spawn_failure(session_id, reason).await;
            }
            failed
        } else {
            self.runtime_store
                .fail_orchestration_dispatch(&dispatch.id, reason)
                .await
        };
        if let Err(error) = result {
            tracing::error!(
                "failed to mark orchestration dispatch {} failed after terminal close: {error}",
                dispatch.id
            );
        }
    }

    /// Mints and starts a terminal tab with the default agent command.
    pub(super) async fn coordinator_create_worker_terminal(
        &mut self,
        config: &CoordinatorConfig,
        ready: &[alera_core::runtime::OrchestrationTask],
        drift: Option<&GitBaseDrift>,
    ) -> anyhow::Result<()> {
        let Some(workspace_id) = &config.workspace_id else {
            self.coordinator_log(
                "no idle worker terminals and no --workspace scope; cannot create workers",
            );
            return Ok(());
        };
        let Some(workspace) = self.runtime_store.find_workspace(workspace_id).await? else {
            self.coordinator_log(&format!(
                "workspace {workspace_id} not found; cannot create worker terminal"
            ));
            return Ok(());
        };
        if workspace.status != WorkspaceStatus::Active {
            self.coordinator_log(&format!(
                "workspace {workspace_id} is not active; cannot create worker terminal"
            ));
            return Ok(());
        }
        agent_descriptor(&config.agent_type)
            .ok_or_else(|| anyhow::anyhow!("unsupported agent type: {}", config.agent_type))?;
        // Every adapter is pre-dispatched. Waiting for the agent to announce
        // itself first is not an option any more, and never was a working one:
        // a worker that has been asked nothing never reports that it is idle,
        // so a bare terminal would sit there holding the task forever.
        for task in ready {
            let Some(spec) = self.coordinator_preflight_with_drift(task, drift) else {
                continue;
            };
            let preflight = (spec, drift.cloned());
            let profile = self.coordinator_profile_for_task(task).await;
            // The spawn resumes from the dispatch context install completion;
            // the "created worker terminal" log is emitted by that
            // continuation.
            self.coordinator_spawn_predispatched_worker(
                &config.run_id,
                workspace_id,
                &config.agent_type,
                &task.id,
                config.coordinator_handle.as_deref(),
                preflight,
                profile.as_deref(),
            )
            .await?;
            break;
        }
        Ok(())
    }

    pub(super) async fn orchestration_agent_spawn_request(
        &mut self,
        client_id: u64,
        request_id: i64,
        payload: &Value,
    ) -> HostResult<Option<Value>> {
        self.orchestration_agent_spawn_with_preflight(
            DispatchInstallOrigin::Request {
                client_id,
                request_id,
            },
            payload,
            None,
        )
        .await
    }

    async fn orchestration_agent_spawn_with_preflight(
        &mut self,
        origin: DispatchInstallOrigin,
        payload: &Value,
        preflight: Option<(String, Option<GitBaseDrift>)>,
    ) -> HostResult<Option<Value>> {
        let workspace_id = require_string(payload, "workspace")?;
        let task_id = require_string(payload, "task")?;
        let from = require_string(payload, "from")?;
        // A profile is the single source of truth for the adapter and the
        // launch command, so it replaces --agent/--command rather than layering
        // on top of them.
        let resolved = self.resolve_spawn_profile(payload).await?;
        let agent_type = resolved.agent_type.clone();
        let descriptor = agent_descriptor(&agent_type)
            .ok_or_else(|| HostError::format(format!("unsupported agent type: {agent_type}")))?;
        let task = self
            .runtime_store
            .orchestration_task_by_id(&task_id)
            .await
            .map_err(state_error)?
            .ok_or_else(|| HostError::state(format!("orchestration task not found: {task_id}")))?;
        if task.workspace_id != workspace_id {
            return Err(HostError::state(format!(
                "task belongs to workspace {}, not {workspace_id}",
                task.workspace_id
            )));
        }
        if task.coordinator_handle != from {
            return Err(HostError::state(format!(
                "coordinator ownership conflict: task is owned by {}",
                task.coordinator_handle
            )));
        }
        if let Some(terminal) = optional_string(payload, "terminal") {
            let prepared = match self
                .prepare_orchestration_dispatch(&json!({
                    "task": task_id,
                    "to": terminal,
                    "from": from,
                    "inject": true,
                    "forceSubmit": descriptor.force_submit,
                    "completionPolicy": "return-immediately",
                    "terminalPolicy": "keep-open",
                    "agentProfile": resolved.profile_name,
                    "agentQuotaGroup": resolved.quota_group,
                }))
                .await?
            {
                DispatchPreparation::Ready(prepared) => prepared,
                DispatchPreparation::DryRun(_) => {
                    return Err(HostError::state("unexpected dispatch dry run"))
                }
            };
            // The dispatch's own continuation injects the preamble and answers
            // this request once the context file exists.
            return self.start_prepared_dispatch(prepared, origin).await;
        }
        let workspace = self
            .runtime_store
            .find_workspace(&workspace_id)
            .await
            .map_err(state_error)?
            .ok_or_else(|| HostError::state(format!("workspace not found: {workspace_id}")))?;
        if workspace.status != WorkspaceStatus::Active {
            return Err(HostError::state(format!(
                "workspace is not active: {workspace_id}"
            )));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let keep_on_failure = payload
            .get("keepOnFailure")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let bootstrap = build_dispatch_bootstrap();
        let prepared = match self
            .prepare_orchestration_dispatch(&json!({
                "task": task_id,
                "to": id,
                "from": from,
                "inject": false,
                "completionPolicy": "return-immediately",
                "terminalPolicy": "keep-open",
                "agentProfile": resolved.profile_name,
                "agentQuotaGroup": resolved.quota_group,
            }))
            .await?
        {
            DispatchPreparation::Ready(prepared) => prepared,
            DispatchPreparation::DryRun(_) => {
                return Err(HostError::state("unexpected dispatch dry run"))
            }
        };
        let dispatch_id = prepared.dispatch_id.clone();
        let pending = PendingAgentSpawn {
            handle: id.clone(),
            resolved,
            descriptor,
            preflight,
            bootstrap,
            keep_on_failure,
            task,
            workspace_id,
            from,
            title: optional_string(payload, "title"),
            dispatch_response: prepared.response,
        };
        // The worker PTY must not exist before the context file it may read is
        // on disk, so the spawn resumes from `DispatchContextInstalled`.
        if let Err(error) = self.start_dispatch_context_install(
            &id,
            &prepared.dispatch_id,
            &prepared.context_token,
            DispatchContextContinuation::AgentSpawn {
                origin,
                pending: Box::new(pending),
            },
        ) {
            let _ = self
                .runtime_store
                .fail_orchestration_startup(&dispatch_id, "could not install worker context")
                .await;
            return Err(error);
        }
        Ok(None)
    }

    pub(super) async fn orchestration_agent_spawn_timeout(
        &mut self,
        payload: &Value,
    ) -> HostResult<Value> {
        let handle = require_string(payload, "terminal")?;
        if let Some(dispatch) = self
            .runtime_store
            .active_orchestration_dispatch_for_handle(&handle)
            .await
            .map_err(state_error)?
        {
            if dispatch.status == OrchestrationDispatchStatus::AwaitingAcceptance {
                let failed = self
                    .runtime_store
                    .fail_orchestration_startup(&dispatch.id, "acceptance timeout")
                    .await
                    .map_err(state_error)?;
                self.remove_dispatch_context(&handle);
                let terminal_removed = self.cleanup_failed_owned_spawn(&handle).await;
                return Ok(json!({
                    "outcome": "startup_failed",
                    "dispatch": failed,
                    "terminalRemoved": terminal_removed,
                }));
            }
        }
        let tab = self
            .runtime_store
            .find_workspace_tab(&handle)
            .await
            .map_err(state_error)?;
        let task_id = tab
            .as_ref()
            .and_then(|tab| tab.payload.get("pendingOrchestration"))
            .or_else(|| {
                tab.as_ref()
                    .and_then(|tab| tab.payload.get("orchestrationSpawn"))
            })
            .and_then(|pending| pending.get("task"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| HostError::state("no pending spawn or acceptance for terminal"))?;
        let failure_recorded = tab
            .as_ref()
            .and_then(|tab| {
                tab.payload
                    .pointer("/orchestrationSpawn/startupFailureRecorded")
            })
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let task = if failure_recorded {
            self.runtime_store
                .orchestration_task_by_id(&task_id)
                .await
                .map_err(state_error)?
                .ok_or_else(|| {
                    HostError::state(format!("orchestration task not found: {task_id}"))
                })?
        } else {
            let task = self
                .runtime_store
                .record_orchestration_task_startup_failure(&task_id, "agent readiness timeout")
                .await
                .map_err(state_error)?;
            self.mark_owned_spawn_failure(&handle, "agent readiness timeout")
                .await;
            task
        };
        let terminal_removed = self.cleanup_failed_owned_spawn(&handle).await;
        Ok(json!({
            "outcome": "startup_failed",
            "task": task,
            "terminalRemoved": terminal_removed,
        }))
    }

    pub(super) async fn cleanup_failed_owned_spawn(&mut self, handle: &str) -> bool {
        let Ok(Some(tab)) = self.runtime_store.find_workspace_tab(handle).await else {
            return false;
        };
        let spawn = tab.payload.get("orchestrationSpawn");
        let owned = spawn
            .and_then(|value| value.get("owned"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let keep = spawn
            .and_then(|value| value.get("keepOnFailure"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !owned {
            return false;
        }
        if keep && self.make_failed_owned_spawn_inert(handle).await {
            return false;
        }
        if self
            .runtime_store
            .remove_workspace_tab(&tab.id)
            .await
            .is_err()
        {
            return false;
        }
        self.terminate_sessions_for_tab(&tab.id).await;
        self.broadcast_workspace_tabs_changed(Some(&tab.workspace_id));
        true
    }

    pub(super) async fn coordinator_spawn_predispatched_worker(
        &mut self,
        run_id: &str,
        workspace_id: &str,
        agent_type: &str,
        task_id: &str,
        coordinator_handle: Option<&str>,
        preflight: (String, Option<GitBaseDrift>),
        profile: Option<&str>,
    ) -> anyhow::Result<()> {
        // A profile supersedes the run-level agent type, so send only one of
        // them: the host rejects both together. The spawn itself resumes from
        // the dispatch context install completion.
        self.orchestration_agent_spawn_with_preflight(
            DispatchInstallOrigin::Coordinator {
                run_id: run_id.to_string(),
            },
            &json!({
                "workspace": workspace_id,
                "agent": profile.is_none().then_some(agent_type),
                "profile": profile,
                "task": task_id,
                "from": coordinator_handle,
            }),
            Some(preflight),
        )
        .await
        .map_err(|error| anyhow::anyhow!(error.wire_message()))?;
        Ok(())
    }
}
