use alera_core::runtime::OrchestrationTaskStatus;
use serde_json::{json, Value};

use crate::terminal_host::host_error::{HostError, HostResult};

use super::orchestration_validation::{optional_string, require_string, state_error};
use super::ServerActor;

impl ServerActor {
    pub(super) async fn orchestration_run_list(&mut self, payload: &Value) -> HostResult<Value> {
        let runs = self
            .runtime_store
            .list_orchestration_coordinator_runs(optional_string(payload, "workspace").as_deref())
            .await
            .map_err(state_error)?;
        Ok(
            json!({ "kind": "runs", "items": runs, "runs": runs, "filters": { "workspace": optional_string(payload, "workspace") } }),
        )
    }

    pub(super) async fn orchestration_run_show(&mut self, payload: &Value) -> HostResult<Value> {
        let id = require_string(payload, "id")?;
        let run = self
            .runtime_store
            .orchestration_coordinator_run_by_id(&id)
            .await
            .map_err(state_error)?
            .ok_or_else(|| HostError::state(format!("coordinator run not found: {id}")))?;
        let tasks = self
            .runtime_store
            .list_scoped_orchestration_tasks(None, Some(&id), Some(&run.workspace_id))
            .await
            .map_err(state_error)?;
        Ok(json!({ "run": run, "tasks": tasks }))
    }

    pub(super) async fn orchestration_status(&mut self, payload: &Value) -> HostResult<Value> {
        let id = require_string(payload, "id")?;
        let run = self
            .runtime_store
            .orchestration_coordinator_run_by_id(&id)
            .await
            .map_err(state_error)?
            .ok_or_else(|| HostError::state(format!("coordinator run not found: {id}")))?;
        let tasks = self
            .runtime_store
            .list_scoped_orchestration_tasks(None, Some(&id), Some(&run.workspace_id))
            .await
            .map_err(state_error)?;
        let mut run_handles = tasks
            .iter()
            .filter_map(|task| task.assignee_handle.as_deref())
            .collect::<std::collections::HashSet<_>>();
        if let Some(coordinator_handle) = run.coordinator_handle.as_deref() {
            run_handles.insert(coordinator_handle);
        }
        let terminals = self.orchestration_terminals(&json!({}))["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|terminal| {
                terminal.get("workspaceId").and_then(Value::as_str)
                    == Some(run.workspace_id.as_str())
                    && terminal
                        .get("handle")
                        .and_then(Value::as_str)
                        .is_some_and(|handle| run_handles.contains(handle))
            })
            .cloned()
            .collect::<Vec<_>>();
        let active = tasks
            .iter()
            .filter(|task| {
                matches!(
                    task.status,
                    OrchestrationTaskStatus::Dispatched | OrchestrationTaskStatus::Stalled
                )
            })
            .count();
        Ok(json!({
            "run": run,
            "tasks": tasks,
            "terminals": terminals,
            "activeTaskCount": active,
            "lastActivityAt": run.last_activity_at,
        }))
    }
    // --- reset --------------------------------------------------------------

    pub(super) async fn orchestration_reset(&mut self, payload: &Value) -> HostResult<Value> {
        let tasks = payload
            .get("tasks")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let messages = payload
            .get("messages")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        // No explicit scope means reset everything, mirroring Orca's --all
        // default.
        let all =
            (!tasks && !messages) || payload.get("all").and_then(Value::as_bool).unwrap_or(false);
        if all || tasks {
            for (_, handle) in self.coordinators.drain() {
                handle.stop();
            }
            self.runtime_store
                .reset_orchestration_tasks()
                .await
                .map_err(state_error)?;
        }
        if all || messages {
            self.runtime_store
                .reset_orchestration_messages()
                .await
                .map_err(state_error)?;
        }
        Ok(json!({
            "tasks": all || tasks,
            "messages": all || messages,
        }))
    }
}
