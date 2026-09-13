use alera_core::agent_descriptor::agent_descriptor;
use alera_core::git::GitBaseDrift;
use alera_core::runtime::{OrchestrationGateStatus, OrchestrationTask, OrchestrationTaskStatus};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::terminal_host::orchestration::agent_presence::AgentPresenceState;
use crate::terminal_host::orchestration::coordinator_loop::{
    CoordinatorConfig, COORDINATOR_DISPATCH_STALE_THRESHOLD,
};
use crate::terminal_host::orchestration::dispatch_preamble::{
    build_dispatch_preamble, parse_allow_stale_base_from_spec, GateResolution, PreambleParams,
    WorkerKind,
};
use crate::terminal_host::session::Session;

use super::deferred_admission::DeferredRequestClass;
use super::dispatch_context_install::DispatchContextContinuation;
use super::orchestration_owned_spawn::is_pending_orchestration_worker;
use super::{ServerActor, ServerCommand};

const PENDING_WORKER_TAB_GRACE_SECONDS: i64 = 120;

/// What a dispatch round settled on after the cheap on-actor checks. The
/// git drift probe parks between planning and execution.
enum CoordinatorDispatchPlan {
    /// No idle worker: pre-dispatch a task into a terminal the spawn creates.
    SpawnWorker { ready: Vec<OrchestrationTask> },
    /// Paste preambles into idle worker terminals.
    Dispatch {
        ready: Vec<OrchestrationTask>,
        idle_terminals: Vec<String>,
        slots: usize,
    },
}

impl ServerActor {
    pub(super) async fn coordinator_dispatch_ready_tasks(
        &mut self,
        config: &CoordinatorConfig,
    ) -> anyhow::Result<()> {
        if self.pending_drift_probes.contains(&config.run_id) {
            return Ok(());
        }
        let Some(plan) = self.coordinator_dispatch_plan(config).await? else {
            return Ok(());
        };
        match &config.workspace_id {
            // The git drift probe is workspace-scoped and shared by every task
            // in the round, so the whole dispatch stage parks behind one
            // off-actor probe instead of awaiting it on the mailbox.
            Some(workspace_id) => {
                self.schedule_coordinator_drift_probe(&config.run_id, workspace_id);
            }
            None => {
                self.coordinator_execute_dispatch_plan(config, plan, None)
                    .await?;
            }
        }
        Ok(())
    }

    /// Cheap on-actor portion of a dispatch round: policy, ready tasks,
    /// concurrency slots, and idle terminals. Re-run after the drift probe
    /// lands because the world may have changed while it was parked.
    async fn coordinator_dispatch_plan(
        &mut self,
        config: &CoordinatorConfig,
    ) -> anyhow::Result<Option<CoordinatorDispatchPlan>> {
        // A proposed but unresolved execution policy holds scheduling: the whole
        // point of the plan is that the user approves it before work starts. A
        // run with no policy is unaffected and schedules as it always did.
        if self
            .coordinator_policy_blocks_dispatch(&config.run_id)
            .await?
        {
            return Ok(None);
        }
        let ready = self
            .runtime_store
            .list_scoped_orchestration_tasks(
                Some(OrchestrationTaskStatus::Ready),
                Some(&config.run_id),
                config.workspace_id.as_deref(),
            )
            .await?;
        if ready.is_empty() {
            return Ok(None);
        }
        let occupied = self
            .runtime_store
            .list_scoped_orchestration_tasks(
                None,
                Some(&config.run_id),
                config.workspace_id.as_deref(),
            )
            .await?
            .into_iter()
            .filter(|task| {
                matches!(
                    task.status,
                    OrchestrationTaskStatus::Dispatched | OrchestrationTaskStatus::Stalled
                )
            })
            .count();
        let slots = config.max_concurrent.saturating_sub(occupied);
        if slots == 0 {
            return Ok(None);
        }

        let idle_terminals = self.coordinator_available_terminals(config).await?;
        if idle_terminals.is_empty() {
            // One worker terminal per tick: the app spawns it eagerly and the
            // agent's presence appears before the next dispatch attempt.
            if self.coordinator_pending_worker_tabs(config).await? > 0 {
                self.coordinator_log("waiting for a spawned worker terminal to report presence");
                return Ok(None);
            }
            return Ok(Some(CoordinatorDispatchPlan::SpawnWorker { ready }));
        }
        Ok(Some(CoordinatorDispatchPlan::Dispatch {
            ready,
            idle_terminals,
            slots,
        }))
    }

    fn schedule_coordinator_drift_probe(&mut self, run_id: &str, workspace_id: &str) {
        if !self.pending_drift_probes.insert(run_id.to_string()) {
            return;
        }
        let store = self.runtime_store.clone();
        let inbox = self.inbox.clone();
        let workspace_id = workspace_id.to_string();
        let probe_run_id = run_id.to_string();
        let probe = async move {
            let workspace = store.find_workspace(&workspace_id).await.ok().flatten();
            let drift = match workspace {
                Some(workspace) => {
                    let path = workspace.path.clone();
                    // git2 walks are blocking; run off the actor thread.
                    tokio::task::spawn_blocking(move || {
                        alera_core::git::probe_base_drift(&path).ok().flatten()
                    })
                    .await
                    .ok()
                    .flatten()
                }
                None => None,
            };
            let _ = inbox.send(ServerCommand::CoordinatorDriftProbed {
                run_id: probe_run_id,
                drift,
            });
        };
        if let Err(error) = self.deferred_admission.schedule(
            DeferredRequestClass::DispatchCritical,
            "coordinator.drift_probe",
            None,
            probe,
        ) {
            self.pending_drift_probes.remove(run_id);
            self.coordinator_log(&format!(
                "drift probe was not admitted: {}",
                error.wire_message()
            ));
        }
    }

    /// A drift probe finished off the actor; resume the parked dispatch round
    /// only if the run is still live and no runtime mutation is interleaving.
    pub(super) async fn handle_coordinator_drift_probed(
        &mut self,
        run_id: String,
        drift: Option<GitBaseDrift>,
    ) {
        self.pending_drift_probes.remove(&run_id);
        if self.mutation_queue.has_runtime_mutations() {
            return;
        }
        let Some(config) = self
            .coordinators
            .get(&run_id)
            .map(|handle| handle.config.clone())
        else {
            return;
        };
        match self.coordinator_dispatch_plan(&config).await {
            Ok(Some(plan)) => {
                if let Err(error) = self
                    .coordinator_execute_dispatch_plan(&config, plan, drift)
                    .await
                {
                    self.coordinator_log(&format!("dispatch failed: {error}"));
                }
            }
            Ok(None) => {}
            Err(error) => self.coordinator_log(&format!("dispatch failed: {error}")),
        }
    }

    async fn coordinator_execute_dispatch_plan(
        &mut self,
        config: &CoordinatorConfig,
        plan: CoordinatorDispatchPlan,
        drift: Option<GitBaseDrift>,
    ) -> anyhow::Result<()> {
        match plan {
            CoordinatorDispatchPlan::SpawnWorker { ready } => {
                self.coordinator_create_worker_terminal(config, &ready, drift.as_ref())
                    .await
            }
            CoordinatorDispatchPlan::Dispatch {
                ready,
                mut idle_terminals,
                mut slots,
            } => {
                for task in ready {
                    if slots == 0 || idle_terminals.is_empty() {
                        break;
                    }
                    let handle = idle_terminals[0].clone();
                    match self
                        .coordinator_dispatch_task(config, &task.id, &handle, drift.clone())
                        .await
                    {
                        Ok(true) => {
                            idle_terminals.remove(0);
                            slots -= 1;
                        }
                        Ok(false) => {}
                        Err(error) => {
                            idle_terminals.remove(0);
                            self.coordinator_log(&format!(
                                "dispatch of {} failed: {error}",
                                task.id
                            ));
                        }
                    }
                }
                Ok(())
            }
        }
    }

    /// Idle worker candidates: running sessions in the scoped workspace with
    /// injection-ready agent presence, no active dispatch, and not the
    /// coordinator's own terminal.
    async fn coordinator_available_terminals(
        &mut self,
        config: &CoordinatorConfig,
    ) -> anyhow::Result<Vec<String>> {
        let mut candidates = Vec::new();
        let session_ids: Vec<(String, String, bool)> = self
            .sessions
            .iter()
            .map(|(session_id, session)| {
                (
                    session_id.clone(),
                    session.workspace_id.clone(),
                    session.running(),
                )
            })
            .collect();
        for (session_id, workspace_id, running) in session_ids {
            if !running {
                continue;
            }
            if let Some(scope) = &config.workspace_id {
                if &workspace_id != scope {
                    continue;
                }
            }
            if config.coordinator_handle.as_deref() == Some(session_id.as_str()) {
                continue;
            }
            if !self.agent_presence.is_injection_ready(&session_id) {
                continue;
            }
            let busy = self
                .runtime_store
                .active_orchestration_dispatch_for_handle(&session_id)
                .await?
                .is_some();
            if !busy {
                candidates.push(session_id);
            }
        }
        Ok(candidates)
    }

    async fn coordinator_pending_worker_tabs(
        &self,
        config: &CoordinatorConfig,
    ) -> anyhow::Result<usize> {
        let Some(workspace_id) = &config.workspace_id else {
            return Ok(0);
        };
        let tabs = self.runtime_store.list_workspace_tabs(workspace_id).await?;
        let now = chrono::Utc::now();
        let mut pending = 0;
        for tab in tabs.into_iter().filter(|tab| tab.kind == "terminal") {
            let Some((handle, created_at)) = (|| {
                if !is_pending_orchestration_worker(&tab.payload, &config.agent_type) {
                    return None;
                }
                let fallback_id = tab.id;
                let created_at = tab.created_at;
                let payload = tab.payload.as_object()?;
                payload
                    .get("terminalSessionId")
                    .and_then(Value::as_str)
                    .filter(|handle| !handle.is_empty())
                    .map(str::to_string)
                    .or(Some(fallback_id))
                    .map(|handle| (handle, created_at))
            })() else {
                continue;
            };
            if now.signed_duration_since(created_at)
                > chrono::Duration::seconds(PENDING_WORKER_TAB_GRACE_SECONDS)
            {
                continue;
            }
            if !self
                .sessions
                .get(&handle)
                .is_some_and(|session| session.running())
            {
                pending += 1;
                continue;
            }
            let active_dispatch = self
                .runtime_store
                .active_orchestration_dispatch_for_handle(&handle)
                .await?
                .is_some();
            if active_dispatch {
                continue;
            }
            let Some(presence) = self.agent_presence.get(&handle) else {
                pending += 1;
                continue;
            };
            if presence.state.accepts_injection() {
                continue;
            }
            if presence.state == AgentPresenceState::Done {
                continue;
            }
            pending += 1;
        }
        Ok(pending)
    }

    /// Dispatch with the stale-base pre-flight: a drift beyond the threshold
    /// silently skips (task stays ready, retried next tick) so circuit budget
    /// is not burned on a recoverable fetch-and-retry situation.
    async fn coordinator_dispatch_task(
        &mut self,
        config: &CoordinatorConfig,
        task_id: &str,
        handle: &str,
        drift: Option<GitBaseDrift>,
    ) -> anyhow::Result<bool> {
        let Some(task) = self.runtime_store.orchestration_task_by_id(task_id).await? else {
            return Ok(false);
        };
        let Some(stripped_spec) = self.coordinator_preflight_with_drift(&task, drift.as_ref())
        else {
            return Ok(false);
        };
        let (profile_name, profile_quota_group, effective_spec) = self
            .coordinator_profile_prompt_for_task(&task, &stripped_spec)
            .await?;
        let gates = self
            .runtime_store
            .list_orchestration_gates(Some(task_id), Some(OrchestrationGateStatus::Resolved))
            .await?;
        let gate_resolution = gates.into_iter().last().map(|gate| GateResolution {
            question: gate.question,
            resolution: gate.resolution.unwrap_or_default(),
        });

        let context_token = uuid::Uuid::new_v4().simple().to_string();
        let context_hash = hex::encode(Sha256::digest(context_token.as_bytes()));
        let dispatch = self
            .runtime_store
            .create_scoped_orchestration_dispatch(
                task_id,
                handle,
                Some(&config.run_id),
                config.workspace_id.as_deref().unwrap_or("global"),
                config
                    .coordinator_handle
                    .as_deref()
                    .unwrap_or("coordinator"),
                Some(&context_hash),
                "return-immediately",
                "keep-open",
            )
            .await?;
        self.runtime_store
            .set_orchestration_dispatch_profile(
                &dispatch.id,
                profile_name.as_deref(),
                profile_quota_group.as_deref(),
            )
            .await?;
        self.orchestration_activity_last_recorded.remove(handle);
        let preamble = build_dispatch_preamble(&PreambleParams {
            task_id,
            dispatch_id: &dispatch.id,
            task_spec: &effective_spec,
            coordinator_handle: config
                .coordinator_handle
                .as_deref()
                .unwrap_or("coordinator"),
            base_drift: drift.as_ref(),
            gate_resolution: gate_resolution.as_ref(),
            worker_kind: WorkerKind::PromptReturningAgent,
        });
        if !self.sessions.get(handle).is_some_and(Session::running) {
            self.runtime_store
                .fail_orchestration_startup(&dispatch.id, "terminal not writable")
                .await?;
            return Ok(false);
        }
        let force_submit =
            agent_descriptor(&config.agent_type).is_some_and(|descriptor| descriptor.force_submit);
        // The context write runs off the actor; the CoordinatorPaste
        // continuation re-validates the run and session before injecting.
        if let Err(error) = self.start_dispatch_context_install(
            handle,
            &dispatch.id,
            &context_token,
            DispatchContextContinuation::CoordinatorPaste {
                run_id: config.run_id.clone(),
                task_id: task_id.to_string(),
                handle: handle.to_string(),
                preamble,
                force_submit,
            },
        ) {
            self.runtime_store
                .fail_orchestration_startup(&dispatch.id, "could not install worker context")
                .await?;
            return Err(anyhow::anyhow!(error.to_string()));
        }
        Ok(true)
    }

    /// The stale-base check against an already-resolved drift. The probe
    /// itself runs off the actor via `schedule_coordinator_drift_probe`.
    pub(super) fn coordinator_preflight_with_drift(
        &self,
        task: &OrchestrationTask,
        drift: Option<&GitBaseDrift>,
    ) -> Option<String> {
        let (allow_stale, stripped_spec) = parse_allow_stale_base_from_spec(&task.spec);
        if let Some(drift) = drift {
            if drift.behind > COORDINATOR_DISPATCH_STALE_THRESHOLD && !allow_stale {
                self.coordinator_log(&format!(
                    "worktree is {} commits behind {}; skipping dispatch of {} this tick",
                    drift.behind, drift.base, task.id
                ));
                return None;
            }
        }
        Some(stripped_spec)
    }
}
