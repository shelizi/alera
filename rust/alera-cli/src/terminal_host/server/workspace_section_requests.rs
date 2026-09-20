use serde_json::{json, Value};

use alera_core::runtime::RuntimeStore;

use super::ServerActor;
use crate::terminal_host::{
    host_error::{HostError, HostResult},
    protocol::event,
};

pub(super) struct WorkspaceSectionRequestHandler {
    runtime_store: RuntimeStore,
}

pub(super) struct WorkspaceSectionRequestOutcome {
    pub(super) value: Value,
    pub(super) changed: bool,
}

impl WorkspaceSectionRequestHandler {
    pub(super) const fn new(runtime_store: RuntimeStore) -> Self {
        Self { runtime_store }
    }

    pub(super) async fn execute(
        &self,
        verb: &str,
        payload: &Value,
    ) -> HostResult<WorkspaceSectionRequestOutcome> {
        let (value, changed) = match verb {
            "workspaceSection.list" => (
                serde_json::to_value(
                    self.runtime_store
                        .list_workspace_sections()
                        .await
                        .map_err(state_error)?,
                )
                .map_err(state_error)?,
                false,
            ),
            "workspaceSection.create" => (
                serde_json::to_value(
                    self.runtime_store
                        .create_workspace_section(
                            required(payload, "name")?,
                            required(payload, "workspaceId")?,
                        )
                        .await
                        .map_err(state_error)?,
                )
                .map_err(state_error)?,
                true,
            ),
            "workspaceSection.setForWorkspace" => {
                let section = match payload.get("sectionId") {
                    Some(Value::Null) => None,
                    Some(Value::String(id)) if !id.is_empty() => Some(id.as_str()),
                    _ => return Err(HostError::format("sectionId must be a section id or null.")),
                };
                self.runtime_store
                    .set_workspace_section(required(payload, "workspaceId")?, section)
                    .await
                    .map_err(state_error)?;
                (json!({}), true)
            }
            "workspaceSection.remove" => {
                self.runtime_store
                    .remove_workspace_section(required(payload, "id")?)
                    .await
                    .map_err(state_error)?;
                (json!({}), true)
            }
            _ => return Err(HostError::format("Unknown section request.")),
        };
        Ok(WorkspaceSectionRequestOutcome { value, changed })
    }
}

impl ServerActor {
    pub(super) async fn workspace_section_request(
        &mut self,
        client_id: u64,
        verb: &str,
        payload: &Value,
    ) -> HostResult<Value> {
        self.require_auth(client_id)?;
        let outcome = WorkspaceSectionRequestHandler::new(self.runtime_store.clone())
            .execute(verb, payload)
            .await?;
        if outcome.changed {
            self.broadcast_workspaces_changed(None);
            self.broadcast_authenticated(event("workspaceSectionsChanged", json!({})));
        }
        Ok(outcome.value)
    }
}

fn required<'a>(payload: &'a Value, key: &str) -> HostResult<&'a str> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| HostError::format(format!("{key} is required.")))
}

fn state_error(error: impl std::fmt::Display) -> HostError {
    HostError::state(error.to_string())
}
