use alera_core::runtime::OrchestrationGateStatus;
use serde_json::{json, Value};

use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::orchestration::dispatch_preamble::GateResolution;

use super::orchestration_validation::{optional_string, require_string, state_error};
use super::ServerActor;

impl ServerActor {
    pub(super) async fn latest_resolved_gate(
        &self,
        task_id: &str,
    ) -> HostResult<Option<GateResolution>> {
        let gates = self
            .runtime_store
            .list_orchestration_gates(Some(task_id), Some(OrchestrationGateStatus::Resolved))
            .await
            .map_err(state_error)?;
        Ok(gates.into_iter().last().map(|gate| GateResolution {
            question: gate.question,
            resolution: gate.resolution.unwrap_or_default(),
        }))
    }

    // --- gates --------------------------------------------------------------

    pub(super) async fn orchestration_gate_create(&mut self, payload: &Value) -> HostResult<Value> {
        let task_id = require_string(payload, "task")?;
        let question = require_string(payload, "question")?;
        let options: Vec<String> = payload
            .get("options")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let gate = self
            .runtime_store
            .create_orchestration_gate(&task_id, &question, &options)
            .await
            .map_err(state_error)?;
        self.queue_gate_push(&task_id, &question).await;
        Ok(json!(gate))
    }

    pub(super) async fn orchestration_gate_resolve(
        &mut self,
        payload: &Value,
    ) -> HostResult<Value> {
        let gate_id = require_string(payload, "id")?;
        let resolution = require_string(payload, "resolution")?;
        let gate = self
            .runtime_store
            .resolve_orchestration_gate(&gate_id, &resolution)
            .await
            .map_err(state_error)?;
        Ok(json!(gate))
    }

    pub(super) async fn orchestration_gate_list(&mut self, payload: &Value) -> HostResult<Value> {
        let task_id = optional_string(payload, "task");
        let status = match optional_string(payload, "status") {
            None => None,
            Some(raw) => Some(
                OrchestrationGateStatus::parse(&raw)
                    .ok_or_else(|| HostError::format(format!("unknown gate status: {raw}")))?,
            ),
        };
        let gates = self
            .runtime_store
            .list_orchestration_gates(task_id.as_deref(), status)
            .await
            .map_err(state_error)?;
        Ok(
            json!({ "kind": "gates", "items": gates, "gates": gates, "filters": { "task": task_id, "status": status.map(|value| value.as_str()) } }),
        )
    }
}
