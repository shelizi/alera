use alera_core::agent_descriptor::agent_descriptor;
use alera_core::runtime::{OrchestrationDispatchContext, OrchestrationDispatchStatus};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::orchestration::dispatch_preamble::{
    build_dispatch_bootstrap, build_dispatch_preamble, build_worker_contract,
    parse_allow_stale_base_from_spec, PreambleParams, WorkerKind,
};

use super::dispatch_context_install::{DispatchContextContinuation, DispatchInstallOrigin};
use super::orchestration_validation::{optional_string, require_string, state_error};
use super::ServerActor;

fn context_token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn validate_dispatch_context_token(
    dispatch: &OrchestrationDispatchContext,
    payload: &Value,
) -> HostResult<()> {
    if let Some(expected_hash) = dispatch.context_token_hash.as_deref() {
        let token = optional_string(payload, "contextToken")
            .ok_or_else(|| HostError::state("dispatch context token is required"))?;
        if context_token_hash(&token) != expected_hash {
            return Err(HostError::state(
                "dispatch context token is invalid or stale",
            ));
        }
    }
    Ok(())
}

/// The outcome of `prepare_orchestration_dispatch`: either a response the
/// caller may answer with immediately, or a committed dispatch whose context
/// install parks until `DispatchContextInstalled` lands.
pub(super) enum DispatchPreparation {
    DryRun(Value),
    Ready(PreparedDispatch),
}

/// A committed dispatch waiting on its context install. The continuation
/// turns this into the inject step plus the request's answer.
pub(super) struct PreparedDispatch {
    pub dispatch_id: String,
    pub to: String,
    pub context_token: String,
    pub inject: bool,
    pub force_submit: bool,
    pub preamble: String,
    pub response: Value,
}

impl ServerActor {
    // --- dispatch -------------------------------------------------------------

    /// Validates and commits the dispatch record, then parks the request while
    /// the context file installs on the shared deferred I/O budget. The
    /// `DispatchRequest` continuation injects the preamble and answers the
    /// request once `DispatchContextInstalled` lands.
    pub(super) async fn orchestration_dispatch_request(
        &mut self,
        client_id: u64,
        request_id: i64,
        payload: &Value,
    ) -> HostResult<Option<Value>> {
        let prepared = match self.prepare_orchestration_dispatch(payload).await? {
            DispatchPreparation::DryRun(response) => return Ok(Some(response)),
            DispatchPreparation::Ready(prepared) => prepared,
        };
        self.start_prepared_dispatch(
            prepared,
            DispatchInstallOrigin::Request {
                client_id,
                request_id,
            },
        )
        .await
    }

    /// Starts the context install for an already-committed dispatch and parks
    /// the origin until completion; a start failure fails the startup the same
    /// way the synchronous install path did.
    pub(super) async fn start_prepared_dispatch(
        &mut self,
        prepared: PreparedDispatch,
        origin: DispatchInstallOrigin,
    ) -> HostResult<Option<Value>> {
        let dispatch_id = prepared.dispatch_id.clone();
        let continuation = DispatchContextContinuation::DispatchRequest {
            origin,
            to: prepared.to.clone(),
            inject: prepared.inject,
            force_submit: prepared.force_submit,
            preamble: prepared.preamble,
            response: prepared.response,
            consumed_tab: None,
        };
        match self.start_dispatch_context_install(
            &prepared.to,
            &prepared.dispatch_id,
            &prepared.context_token,
            continuation,
        ) {
            Ok(()) => Ok(None),
            Err(error) => {
                let _ = self
                    .runtime_store
                    .fail_orchestration_startup(&dispatch_id, "could not install worker context")
                    .await;
                Err(error)
            }
        }
    }

    /// All validation and store mutations for `orchestration.dispatch`; the
    /// context install and the post-install inject step are left to the
    /// continuation the caller attaches.
    pub(super) async fn prepare_orchestration_dispatch(
        &mut self,
        payload: &Value,
    ) -> HostResult<DispatchPreparation> {
        let task_id = require_string(payload, "task")?;
        let to = require_string(payload, "to")?;
        let from = require_string(payload, "from")?;
        let inject = payload
            .get("inject")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let dry_run = payload
            .get("dryRun")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let return_preamble = payload
            .get("returnPreamble")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let assume_agent = optional_string(payload, "assumeAgent");
        let assumed_descriptor = assume_agent
            .as_deref()
            .map(|agent| {
                agent_descriptor(agent)
                    .ok_or_else(|| HostError::format(format!("unsupported agent type: {agent}")))
            })
            .transpose()?;
        let allow_self_dispatch = payload
            .get("allowSelfDispatch")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if from == to && !allow_self_dispatch {
            return Err(HostError::state(format!(
                "self_dispatch_requires_opt_in: from and to both resolve to {to}; pass --allow-self-dispatch for a deliberate protocol test"
            )));
        }

        let task = self
            .runtime_store
            .orchestration_task_by_id(&task_id)
            .await
            .map_err(state_error)?
            .ok_or_else(|| HostError::state(format!("orchestration task not found: {task_id}")))?;
        let (_allow_stale, stripped_spec) = parse_allow_stale_base_from_spec(&task.spec);
        let (profile_name, effective_spec) = self
            .compose_orchestration_prompt(&task, &stripped_spec, payload)
            .await?;
        if from != task.coordinator_handle {
            return Err(HostError::state(format!(
                "coordinator ownership conflict: task is owned by {}, not {from}",
                task.coordinator_handle
            )));
        }
        let gate_resolution = self.latest_resolved_gate(&task_id).await?;

        if dry_run {
            // Builds the preamble without mutating any state; the dispatch id
            // is a placeholder that a real dispatch will replace.
            let preamble = build_dispatch_preamble(&PreambleParams {
                task_id: &task_id,
                dispatch_id: "ctx_dryrun",
                task_spec: &effective_spec,
                coordinator_handle: &from,
                base_drift: None,
                gate_resolution: gate_resolution.as_ref(),
                worker_kind: WorkerKind::BareShell,
            });
            return Ok(DispatchPreparation::DryRun(
                json!({ "dryRun": true, "preamble": preamble }),
            ));
        }

        if inject {
            // Refuse to dump a preamble into a bare shell: injection requires
            // a recognized agent to be running in the target terminal.
            // Stable `stale_terminal_handle:` prefix is matched by recovery docs
            // and agents re-listing terminals after PTY death.
            let Some(session) = self.sessions.get(&to) else {
                return Err(HostError::state(format!(
                    "stale_terminal_handle: terminal {to} not found; remint the PTY \
                     (reopen the tab) or dispatch to a live handle from terminal-list"
                )));
            };
            if !session.running() {
                return Err(HostError::state(format!(
                    "stale_terminal_handle: terminal {to} is not running; remint the PTY \
                     (reopen the tab) or dispatch to a live handle from terminal-list"
                )));
            }
            if session.workspace_id != task.workspace_id {
                return Err(HostError::state(format!(
                    "terminal {to} belongs to workspace {}, not task workspace {}",
                    session.workspace_id, task.workspace_id
                )));
            }
            if self.agent_presence.get(&to).is_none() && assumed_descriptor.is_none() {
                return Err(HostError::state(format!(
                    "no agent detected in terminal {to}; use --assume-agent <agent> for an audited injection override or dispatch without --inject and submit the returned bootstrap manually"
                )));
            }
            if !self.agent_presence.is_injection_ready(&to) && assumed_descriptor.is_none() {
                return Err(HostError::state(format!(
                    "agent in terminal {to} is not idle; cannot inject"
                )));
            }
        }

        let completion_policy = optional_string(payload, "completionPolicy")
            .unwrap_or_else(|| "return-immediately".to_string());
        if completion_policy != "return-immediately" {
            return Err(HostError::format(format!(
                "unsupported completion policy: {completion_policy}; only return-immediately is implemented"
            )));
        }
        let context_token = uuid::Uuid::new_v4().simple().to_string();
        let context_hash = context_token_hash(&context_token);
        let dispatch = self
            .runtime_store
            .create_scoped_orchestration_dispatch(
                &task_id,
                &to,
                task.run_id.as_deref(),
                &task.workspace_id,
                &task.coordinator_handle,
                Some(&context_hash),
                &completion_policy,
                optional_string(payload, "terminalPolicy")
                    .as_deref()
                    .unwrap_or("keep-open"),
            )
            .await
            .map_err(state_error)?;
        // Recorded even when null so `task-show` always reports how the worker
        // was launched, and so fallback selection can read the attempt history.
        self.runtime_store
            .set_orchestration_dispatch_profile(
                &dispatch.id,
                profile_name.as_deref(),
                optional_string(payload, "agentQuotaGroup").as_deref(),
            )
            .await
            .map_err(state_error)?;
        if let Some(descriptor) = assumed_descriptor {
            self.runtime_store
                .insert_orchestration_audit_event(
                    Some(&from),
                    "dispatch.inject.assume_agent",
                    &dispatch.id,
                    &format!(
                        "agent readiness bypassed with explicit adapter {}",
                        descriptor.id
                    ),
                )
                .await
                .map_err(state_error)?;
        }
        self.orchestration_activity_last_recorded.remove(&to);
        let preamble = build_dispatch_preamble(&PreambleParams {
            task_id: &task_id,
            dispatch_id: &dispatch.id,
            task_spec: &effective_spec,
            coordinator_handle: &from,
            base_drift: None,
            gate_resolution: gate_resolution.as_ref(),
            worker_kind: WorkerKind::PromptReturningAgent,
        });
        let force_submit = payload
            .get("forceSubmit")
            .and_then(Value::as_bool)
            .unwrap_or_else(|| {
                assumed_descriptor
                    .map(|descriptor| descriptor.force_submit)
                    .unwrap_or(false)
            });
        let dispatch_id = dispatch.id.clone();

        let mut response = json!({
            "dispatch": dispatch,
            "taskId": task_id,
            "runId": task.run_id,
            "workspaceId": task.workspace_id,
            "coordinatorHandle": task.coordinator_handle,
            "assigneeHandle": to,
            "dispatchingTerminal": from,
            "startupState": if inject { "dispatch_submitted_unconfirmed" } else { "awaiting_manual_delivery" },
            "dispatchPreambleVersion": 2,
            "contextToken": context_token,
            "contextPath": self.dispatch_context_path(&to),
            "assumedAgent": assume_agent,
        });
        let bootstrap = build_dispatch_bootstrap();
        if return_preamble {
            response["preamble"] = Value::String(preamble.clone());
        } else if !inject {
            response["preamble"] = Value::String(bootstrap.clone());
        }
        response["bootstrap"] = Value::String(bootstrap);
        Ok(DispatchPreparation::Ready(PreparedDispatch {
            dispatch_id,
            to,
            context_token,
            inject,
            force_submit,
            preamble,
            response,
        }))
    }

    pub(super) async fn orchestration_dispatch_show(
        &mut self,
        payload: &Value,
    ) -> HostResult<Value> {
        let task_id = require_string(payload, "task")?;
        let active = self
            .runtime_store
            .active_orchestration_dispatch_for_task(&task_id)
            .await
            .map_err(state_error)?;
        let history = self
            .runtime_store
            .list_orchestration_dispatches_for_task(&task_id)
            .await
            .map_err(state_error)?;
        Ok(json!({ "active": active, "history": history }))
    }

    pub(super) async fn active_worker_dispatch(
        &self,
        payload: &Value,
    ) -> HostResult<OrchestrationDispatchContext> {
        let terminal = optional_string(payload, "terminal").ok_or_else(|| {
            HostError::format("terminal is required; run inside an Alera terminal.")
        })?;
        let dispatch = self
            .runtime_store
            .active_orchestration_dispatch_for_handle(&terminal)
            .await
            .map_err(state_error)?
            .ok_or_else(|| {
                HostError::state(format!("no active dispatch for terminal {terminal}"))
            })?;
        validate_dispatch_context_token(&dispatch, payload)?;
        Ok(dispatch)
    }

    pub(super) async fn completion_worker_dispatch(
        &self,
        payload: &Value,
    ) -> HostResult<(OrchestrationDispatchContext, bool)> {
        let terminal = optional_string(payload, "terminal").ok_or_else(|| {
            HostError::format("terminal is required; run inside an Alera terminal.")
        })?;
        let active = self
            .runtime_store
            .active_orchestration_dispatch_for_handle(&terminal)
            .await
            .map_err(state_error)?;
        let (dispatch, replay) = match active {
            Some(dispatch) => (dispatch, false),
            None => {
                let latest = self
                    .runtime_store
                    .latest_orchestration_dispatch_for_handle(&terminal)
                    .await
                    .map_err(state_error)?
                    .filter(|dispatch| dispatch.status == OrchestrationDispatchStatus::Completed)
                    .ok_or_else(|| {
                        HostError::state(format!("no active dispatch for terminal {terminal}"))
                    })?;
                (latest, true)
            }
        };
        validate_dispatch_context_token(&dispatch, payload)?;
        Ok((dispatch, replay))
    }

    pub(super) async fn orchestration_dispatch_accept(
        &mut self,
        payload: &Value,
    ) -> HostResult<Value> {
        let terminal = optional_string(payload, "terminal")
            .ok_or_else(|| HostError::format("terminal is required."))?;
        let dispatch = self.active_worker_dispatch(payload).await?;
        let accepted = self
            .runtime_store
            .accept_orchestration_dispatch(
                &dispatch.id,
                &terminal,
                &optional_string(payload, "contextToken")
                    .map(|token| context_token_hash(&token))
                    .unwrap_or_default(),
            )
            .await
            .map_err(state_error)?;
        self.consume_owned_spawn_metadata(&terminal).await;
        Ok(json!({ "outcome": "accepted", "dispatch": accepted }))
    }

    pub(super) async fn orchestration_dispatch_interrupt(
        &mut self,
        payload: &Value,
    ) -> HostResult<Value> {
        let id = require_string(payload, "id")?;
        let reason = require_string(payload, "reason")?;
        let dispatch = self
            .runtime_store
            .orchestration_dispatch_by_id(&id)
            .await
            .map_err(state_error)?
            .ok_or_else(|| HostError::state(format!("dispatch not found: {id}")))?;
        if !matches!(
            dispatch.status,
            OrchestrationDispatchStatus::Dispatched | OrchestrationDispatchStatus::Stalled
        ) {
            return Err(HostError::state("dispatch is not interruptible"));
        }
        let actor = optional_string(payload, "actor");
        let force = payload
            .get("force")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !force && actor.as_deref() != Some(dispatch.coordinator_handle.as_str()) {
            return Err(HostError::state(format!(
                "only coordinator {} can interrupt dispatch {id}; use --force for audited recovery",
                dispatch.coordinator_handle
            )));
        }
        let handle = dispatch
            .assignee_handle
            .clone()
            .ok_or_else(|| HostError::state("dispatch has no assignee"))?;
        let agent_type = self.agent_presence.agent_type(&handle).unwrap_or("codex");
        let descriptor = agent_descriptor(agent_type)
            .ok_or_else(|| HostError::state(format!("no interrupt adapter for {agent_type}")))?;
        self.queue_orchestration_control(&handle, descriptor.interrupt_bytes)?;
        self.runtime_store
            .insert_orchestration_audit_event(
                actor.as_deref(),
                if force {
                    "dispatch.interrupt.force"
                } else {
                    "dispatch.interrupt"
                },
                &id,
                &reason,
            )
            .await
            .map_err(state_error)?;
        Ok(json!({ "dispatchId": id, "interrupted": true, "reason": reason }))
    }

    pub(super) fn orchestration_worker_help(&self) -> Value {
        json!({
            "dispatchPreambleVersion": 2,
            "workerInstructions": build_worker_contract("<coordinator-handle>", WorkerKind::PromptReturningAgent),
            "commands": [
                "alera orchestration dispatch-accept",
                "alera orchestration --json context",
                "alera orchestration heartbeat --phase <phase>",
                "alera orchestration escalate --subject <subject> --body <details>",
                "alera orchestration complete --summary <summary> --completion-kind success --artifacts '[]' --validation '[]'"
            ]
        })
    }
}
