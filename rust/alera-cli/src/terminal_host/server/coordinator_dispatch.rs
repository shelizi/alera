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
        &self,
        config: &CoordinatorConfig,
    ) -> anyhow::Result<Vec<String>> {
        // Filter in-memory before cloning handles or touching SQLite.
        let mut candidates: Vec<String> = self
            .sessions
            .iter()
            .filter(|(handle, session)| {
                session.running()
                    && config
                        .workspace_id
                        .as_ref()
                        .is_none_or(|scope| &session.workspace_id == scope)
                    && config.coordinator_handle.as_deref() != Some(handle.as_str())
                    && self.agent_presence.is_injection_ready(handle)
            })
            .map(|(handle, _)| handle.clone())
            .collect();
        let busy = self
            .runtime_store
            .active_orchestration_dispatch_handles(&candidates)
            .await?;
        candidates.retain(|handle| !busy.contains(handle));
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
        let mut unready_running = Vec::new();
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
            // Ready workers are not pending, regardless of their dispatch.
            // Only unresolved running workers need the batched busy check.
            if self.agent_presence.get(&handle).is_some_and(|presence| {
                presence.state.accepts_injection() || presence.state == AgentPresenceState::Done
            }) {
                continue;
            }
            unready_running.push(handle);
        }
        let busy = self
            .runtime_store
            .active_orchestration_dispatch_handles(&unready_running)
            .await?;
        pending += unready_running
            .iter()
            .filter(|handle| !busy.contains(*handle))
            .count();
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
        self.orchestration_output_activity.remove(handle);
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

#[cfg(test)]
mod selection_tests {
    use super::*;
    use crate::terminal_host::server::actor_test_harness::test_actor;
    use crate::terminal_host::server::orchestration_contract_tests::ready_task;
    use alera_core::runtime::WorkspaceTabRecord;
    use chrono::{Duration, Utc};
    use serde_json::json;
    use std::collections::{HashMap, HashSet};

    fn config() -> CoordinatorConfig {
        CoordinatorConfig {
            run_id: "run".into(),
            coordinator_handle: Some("coord".into()),
            poll_interval_ms: 2000,
            max_concurrent: 4,
            workspace_id: Some("w1".into()),
            agent_type: "codex".into(),
        }
    }

    fn worker(
        actor: &mut ServerActor,
        handle: &str,
        workspace: &str,
        state: Option<AgentPresenceState>,
        running: bool,
    ) {
        let mut session = Session::driver_test_stub(handle, 80, 24);
        session.workspace_id = workspace.into();
        if !running {
            session.handle_exit(0);
        }
        actor.sessions.insert(handle.into(), session);
        if let Some(state) = state {
            actor.agent_presence.update(handle, "codex".into(), state);
        }
    }

    async fn dispatch_status(actor: &ServerActor, handle: &str, status: &str) {
        let task = ready_task(actor, "coord").await;
        let dispatch = actor
            .runtime_store
            .create_orchestration_dispatch(&task, handle)
            .await
            .unwrap();
        sqlx::query("UPDATE orchestrationDispatchContexts SET status = ? WHERE id = ?")
            .bind(status)
            .bind(&dispatch.id)
            .execute(actor.runtime_store.pool())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn available_workers_match_legacy_checks_across_batch_boundaries() {
        let dir = tempfile::tempdir().unwrap();
        let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
        let mut expected: HashSet<String> = (0..260).map(|i| format!("worker-{i}")).collect();
        for handle in &expected {
            worker(
                &mut actor,
                handle,
                "w1",
                Some(AgentPresenceState::Done),
                true,
            );
        }
        for (i, status) in [
            "pending",
            "dispatched",
            "awaiting_acceptance",
            "stalled",
            "completed",
            "failed",
            "cancelled",
            "superseded",
            "startup_failed",
            "circuit_broken",
        ]
        .into_iter()
        .enumerate()
        {
            let handle = format!("worker-{}", i * 28);
            dispatch_status(&actor, &handle, status).await;
            if i < 4 {
                expected.remove(&handle);
            }
        }
        for (handle, workspace, state, running) in [
            ("coord", "w1", Some(AgentPresenceState::Done), true),
            (
                "other-workspace",
                "w2",
                Some(AgentPresenceState::Done),
                true,
            ),
            ("working", "w1", Some(AgentPresenceState::Working), true),
            ("waiting", "w1", Some(AgentPresenceState::Waiting), true),
            ("blocked", "w1", Some(AgentPresenceState::Blocked), true),
            ("exited", "w1", Some(AgentPresenceState::Done), false),
            ("unknown", "w1", None, true),
        ] {
            worker(&mut actor, handle, workspace, state, running);
        }

        let actual = actor
            .coordinator_available_terminals(&config())
            .await
            .unwrap();
        assert_eq!(actual.len(), expected.len());
        assert_eq!(actual.into_iter().collect::<HashSet<_>>(), expected);
        // A global run may select idle terminals outside the original scope,
        // but never the coordinator itself, or a busy/non-ready session.
        let mut global = config();
        global.workspace_id = None;
        expected.insert("other-workspace".into());
        let actual = actor
            .coordinator_available_terminals(&global)
            .await
            .unwrap();
        assert_eq!(actual.into_iter().collect::<HashSet<_>>(), expected);

        // The availability snapshot does not grant dispatch authority:
        // the transaction still rejects a conflicting assignment.
        let task = ready_task(&actor, "coord").await;
        assert!(actor
            .runtime_store
            .create_orchestration_dispatch(&task, "worker-0")
            .await
            .is_err());
    }

    async fn tab(
        actor: &ServerActor,
        id: &str,
        handle: Option<&str>,
        workspace: &str,
        age_secs: i64,
        kind: &str,
        command: &str,
    ) {
        let created_at = Utc::now() - Duration::seconds(age_secs);
        let mut payload = json!({"spawnOnCreate": true, "initialCommand": command});
        if let Some(handle) = handle {
            payload["terminalSessionId"] = json!(handle);
        }
        actor
            .runtime_store
            .upsert_workspace_tab(WorkspaceTabRecord {
                id: id.into(),
                workspace_id: workspace.into(),
                kind: kind.into(),
                title: "worker".into(),
                created_at,
                updated_at: created_at,
                payload,
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn pending_worker_scan_excludes_ready_busy_old_and_out_of_scope_tabs() {
        let dir = tempfile::tempdir().unwrap();
        let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
        let command = agent_descriptor("codex").unwrap().default_command;
        for i in 0..140 {
            let handle = format!("pending-{i}");
            worker(
                &mut actor,
                &handle,
                "w1",
                Some(AgentPresenceState::Working),
                true,
            );
            tab(
                &actor,
                &format!("tab-{i}"),
                Some(&handle),
                "w1",
                0,
                "terminal",
                command,
            )
            .await;
        }
        for (handle, status) in [
            ("pending-0", "pending"),
            ("pending-127", "dispatched"),
            ("pending-128", "awaiting_acceptance"),
            ("pending-139", "stalled"),
        ] {
            dispatch_status(&actor, handle, status).await;
        }
        worker(
            &mut actor,
            "done",
            "w1",
            Some(AgentPresenceState::Done),
            true,
        );
        worker(
            &mut actor,
            "exited",
            "w1",
            Some(AgentPresenceState::Done),
            false,
        );
        worker(&mut actor, "fallback", "w1", None, true);
        tab(
            &actor,
            "ready-tab",
            Some("done"),
            "w1",
            0,
            "terminal",
            command,
        )
        .await;
        tab(
            &actor,
            "dead-tab",
            Some("exited"),
            "w1",
            0,
            "terminal",
            command,
        )
        .await;
        tab(
            &actor,
            "not-started-tab",
            Some("not-started"),
            "w1",
            0,
            "terminal",
            command,
        )
        .await;
        tab(&actor, "fallback", None, "w1", 0, "terminal", command).await;
        tab(
            &actor,
            "old-tab",
            Some("old"),
            "w1",
            600,
            "terminal",
            command,
        )
        .await;
        tab(
            &actor,
            "foreign-tab",
            Some("foreign"),
            "w2",
            0,
            "terminal",
            command,
        )
        .await;
        tab(&actor, "file-tab", Some("file"), "w1", 0, "editor", command).await;
        tab(
            &actor,
            "different-agent-tab",
            Some("different-agent"),
            "w1",
            0,
            "terminal",
            "unrelated command",
        )
        .await;
        // Two tabs may refer to the same handle; preserve the old per-tab
        // count rather than treating duplicate handles as one pending tab.
        tab(
            &actor,
            "duplicate-tab",
            Some("pending-1"),
            "w1",
            0,
            "terminal",
            command,
        )
        .await;
        assert_eq!(
            actor
                .coordinator_pending_worker_tabs(&config())
                .await
                .unwrap(),
            140
        );

        // Completing an active dispatch makes its still-unready tab pending.
        sqlx::query("UPDATE orchestrationDispatchContexts SET status = 'completed' WHERE assignee_handle = 'pending-127'")
            .execute(actor.runtime_store.pool()).await.unwrap();
        assert_eq!(
            actor
                .coordinator_pending_worker_tabs(&config())
                .await
                .unwrap(),
            141
        );
        let mut global = config();
        global.workspace_id = None;
        assert_eq!(
            actor
                .coordinator_pending_worker_tabs(&global)
                .await
                .unwrap(),
            0
        );
    }
}
