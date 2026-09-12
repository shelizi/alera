//! Two-phase dispatch context install.
//!
//! The context file authenticates a dispatched worker, so it has to exist
//! before the worker's preamble can use it, but writing it is filesystem I/O
//! that must not stall the actor mailbox. The actor therefore reserves a
//! generation on the per-handle gate, commits the dispatch record, and parks
//! the request; the write runs on the shared deferred I/O budget and the
//! `DispatchContextInstalled` completion resumes the parked continuation only
//! after re-validating that the generation and the dispatch owner are still
//! live.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use alera_core::runtime::{
    OrchestrationDispatchStatus, OrchestrationTask, WorkspaceTabRecord,
};
use serde_json::{json, Value};

use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::orchestration::agent_registry::AgentAdapter;
use crate::terminal_host::orchestration::dispatch_preamble::BaseDrift;

use super::orchestration_profile_spawn::ResolvedSpawnProfile;
use super::{ServerActor, ServerCommand};

type DispatchContextGate = Mutex<u64>;
type DispatchContextGateRegistry = Mutex<HashMap<String, Weak<DispatchContextGate>>>;

static DISPATCH_CONTEXT_GATES: OnceLock<DispatchContextGateRegistry> = OnceLock::new();

fn dispatch_context_gate(path: &Path) -> HostResult<Arc<DispatchContextGate>> {
    let key = path.to_string_lossy().into_owned();
    let mut gates = DISPATCH_CONTEXT_GATES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|error| HostError::state(format!("dispatch context registry failed: {error}")))?;
    gates.retain(|_, gate| gate.strong_count() > 0);
    if let Some(gate) = gates.get(&key).and_then(Weak::upgrade) {
        return Ok(gate);
    }
    let gate = Arc::new(Mutex::new(0));
    gates.insert(key, Arc::downgrade(&gate));
    Ok(gate)
}

/// Who initiated a dispatch context install; decides where the outcome goes.
pub(super) enum DispatchInstallOrigin {
    /// A parked client request answered through the request/response channel.
    Request { client_id: u64, request_id: i64 },
    /// A coordinator tick; outcomes are reported on the coordinator log.
    Coordinator { run_id: String },
    /// An internal caller such as pending-dispatch replay; failures only mark
    /// the dispatch so the trigger retries.
    Internal,
}

/// What the actor resumes once the context file lands. Every continuation
/// carries the state it needs because the install happens off the actor.
pub(super) enum DispatchContextContinuation {
    /// `orchestration.dispatch` (also inject-mode `agentSpawn`): inject the
    /// preamble when asked, then answer the parked request.
    DispatchRequest {
        origin: DispatchInstallOrigin,
        to: String,
        inject: bool,
        force_submit: bool,
        preamble: String,
        response: Value,
        /// Pending-dispatch replay: clear the tab marker once the inject
        /// committed, so a restart does not replay the prompt twice.
        consumed_tab: Option<WorkspaceTabRecord>,
    },
    /// Coordinator tick: paste the prepared preamble into the assignee.
    CoordinatorPaste {
        run_id: String,
        task_id: String,
        handle: String,
        preamble: String,
        force_submit: bool,
    },
    /// `agentSpawn`'s own-terminal path: finish creating the worker, then
    /// answer the parked request (or report on the coordinator log).
    AgentSpawn {
        origin: DispatchInstallOrigin,
        pending: Box<PendingAgentSpawn>,
    },
    /// Install the file without a parked request (test and maintenance use).
    #[allow(dead_code)]
    Detached,
}

/// State an `agentSpawn` carries across the context install: everything the
/// spawn step needs once the context file exists.
pub(super) struct PendingAgentSpawn {
    pub handle: String,
    pub resolved: ResolvedSpawnProfile,
    pub adapter: &'static AgentAdapter,
    pub preflight: Option<(String, Option<BaseDrift>)>,
    pub bootstrap: String,
    pub keep_on_failure: bool,
    pub task: OrchestrationTask,
    pub workspace_id: String,
    pub from: String,
    pub title: Option<String>,
    pub dispatch_response: Value,
}

/// A parked install, keyed by dispatch id. The reserved `generation` plus the
/// gate's current value decide whether a completion may still commit.
pub(super) struct PendingDispatchContext {
    pub handle: String,
    pub generation: u64,
    pub gate: Arc<DispatchContextGate>,
    pub continuation: DispatchContextContinuation,
}

/// Writes the context file while holding the gate so installs, removals and
/// generation bumps stay totally ordered per handle. The gate guard is only
/// ever taken on a blocking thread, never inside the actor's async work.
fn write_dispatch_context_file(
    gate: &DispatchContextGate,
    generation: u64,
    path: &Path,
    bytes: &[u8],
) -> HostResult<()> {
    let _current = gate
        .lock()
        .map_err(|error| HostError::state(format!("dispatch context gate failed: {error}")))?;
    if *_current != generation {
        return Err(HostError::state("dispatch context install superseded"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| HostError::state("invalid dispatch context path"))?;
    std::fs::create_dir_all(parent).map_err(|error| HostError::state(error.to_string()))?;
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(path)
            .map_err(|error| HostError::state(error.to_string()))?;
        file.write_all(bytes)
            .map_err(|error| HostError::state(error.to_string()))?;
    }
    #[cfg(not(unix))]
    std::fs::write(path, bytes).map_err(|error| HostError::state(error.to_string()))?;
    Ok(())
}

impl ServerActor {
    pub(super) fn dispatch_context_path(&self, handle: &str) -> PathBuf {
        let safe_handle: String = handle
            .chars()
            .map(|value| {
                if value.is_ascii_alphanumeric() || value == '-' || value == '_' {
                    value
                } else {
                    '_'
                }
            })
            .collect();
        self.runtime_dir
            .join("orchestration-contexts")
            .join(format!("{safe_handle}.json"))
    }

    /// Reserves the context-file generation for `handle` and queues the write
    /// on the shared deferred I/O budget. The `DispatchContextInstalled`
    /// completion commits or unwinds the parked `continuation`.
    pub(super) fn start_dispatch_context_install(
        &mut self,
        handle: &str,
        dispatch_id: &str,
        token: &str,
        continuation: DispatchContextContinuation,
    ) -> HostResult<()> {
        let path = self.dispatch_context_path(handle);
        let gate = dispatch_context_gate(&path)?;
        let generation = {
            let mut generation = gate
                .lock()
                .map_err(|error| {
                    HostError::state(format!("dispatch context gate failed: {error}"))
                })?;
            *generation = generation.wrapping_add(1);
            *generation
        };
        let bytes = serde_json::to_vec(&json!({ "dispatchId": dispatch_id, "token": token }))
            .map_err(|error| HostError::state(error.to_string()))?;
        self.pending_dispatch_installs.insert(
            dispatch_id.to_string(),
            PendingDispatchContext {
                handle: handle.to_string(),
                generation,
                gate: gate.clone(),
                continuation,
            },
        );
        let inbox = self.inbox.clone();
        let dispatch_id_owned = dispatch_id.to_string();
        if let Err(error) = self.deferred_admission.schedule(
            super::deferred_admission::DeferredRequestClass::DispatchCritical,
            "orchestration.dispatchContext.install",
            None,
            async move {
                let result = tokio::task::spawn_blocking(move || {
                    write_dispatch_context_file(&gate, generation, &path, &bytes)
                })
                .await
                .unwrap_or_else(|error| {
                    Err(HostError::state(format!(
                        "dispatch context install failed: {error}"
                    )))
                });
                let _ = inbox.send(ServerCommand::DispatchContextInstalled {
                    dispatch_id: dispatch_id_owned,
                    generation,
                    result,
                });
            },
        ) {
            self.pending_dispatch_installs.remove(dispatch_id);
            return Err(error);
        }
        Ok(())
    }

    /// Bumps the generation so an in-flight install can never commit, then
    /// removes the file on the shared deferred I/O budget.
    pub(super) fn remove_dispatch_context(&mut self, handle: &str) {
        let path = self.dispatch_context_path(handle);
        let Ok(gate) = dispatch_context_gate(&path) else {
            return;
        };
        let cleanup_generation = {
            let Ok(mut generation) = gate.lock() else {
                return;
            };
            *generation = generation.wrapping_add(1);
            *generation
        };
        // An install for the handle can no longer commit once its generation
        // is gone; answer its parked origin before dropping it.
        let superseded: Vec<String> = self
            .pending_dispatch_installs
            .iter()
            .filter(|(_, pending)| pending.handle == handle)
            .map(|(dispatch_id, _)| dispatch_id.clone())
            .collect();
        for dispatch_id in superseded {
            if let Some(pending) = self.pending_dispatch_installs.remove(&dispatch_id) {
                self.answer_superseded_dispatch_install(pending.continuation);
            }
        }

        if let Err(error) = self.deferred_admission.schedule(
            super::deferred_admission::DeferredRequestClass::Maintenance,
            "orchestration.dispatchContext.cleanup",
            None,
            async move {
                let _ = tokio::task::spawn_blocking(move || {
                    let Ok(generation) = gate.lock() else {
                        return;
                    };
                    if *generation != cleanup_generation {
                        return;
                    }
                    let _ = std::fs::remove_file(path);
                })
                .await;
            },
        ) {
            tracing::warn!(
                "dispatch context cleanup was not admitted: {}",
                error.wire_message()
            );
        }
    }

    /// Resumes a parked install: re-validates the reserved generation and the
    /// dispatch owner before the continuation may commit.
    pub(super) async fn finish_dispatch_context_install(
        &mut self,
        dispatch_id: String,
        generation: u64,
        result: HostResult<()>,
    ) {
        let Some(pending) = self.pending_dispatch_installs.remove(&dispatch_id) else {
            return;
        };
        let still_current = pending
            .gate
            .lock()
            .map(|current| *current == generation)
            .unwrap_or(false);
        if pending.generation != generation || !still_current {
            return;
        }
        if let Err(error) = result {
            let _ = self
                .runtime_store
                .fail_orchestration_startup(&dispatch_id, "could not install worker context")
                .await;
            self.fail_dispatch_context_continuation(pending.continuation, error);
            return;
        }
        let live = matches!(
            self.runtime_store
                .orchestration_dispatch_by_id(&dispatch_id)
                .await,
            Ok(Some(dispatch)) if dispatch.status == OrchestrationDispatchStatus::AwaitingAcceptance
        );
        if !live {
            // The owner was invalidated while the write ran; the context file
            // belongs to nobody now.
            self.remove_dispatch_context(&pending.handle);
            return;
        }
        self.run_dispatch_context_continuation(&dispatch_id, pending)
            .await;
    }

    /// Startup recovery: dispatches still `awaiting_acceptance` can never
    /// accept after a restart - sessions and in-flight installs are
    /// process-local, and the DB only stores the context token hash, so the
    /// file cannot be rebuilt. Fails the startup so the task returns to the
    /// retry policy, then removes the context through the generation gate.
    /// Runs once before the command loop, so it cannot double-fire.
    pub(super) async fn recover_orphaned_dispatch_startups(&mut self) {
        let orphaned = match self.runtime_store.fail_orphaned_startup_dispatches().await {
            Ok(orphaned) => orphaned,
            Err(error) => {
                tracing::error!("failed to sweep orphaned dispatch startups: {error}");
                return;
            }
        };
        for dispatch in &orphaned {
            if let Some(handle) = dispatch.assignee_handle.as_deref() {
                self.remove_dispatch_context(handle);
            }
        }
    }
}
