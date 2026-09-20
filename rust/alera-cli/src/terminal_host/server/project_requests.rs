use alera_core::git as core_git;
use alera_core::runtime::{ProjectConfig, RuntimeStore};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::project_management::{
    commit_project_registration, effective_project_config, host_directory_roots,
    prepare_project_registration, register_project, rename_project, PreparedProjectRegistration,
};
use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::protocol::{error_response, event, ok_response};

use super::{ServerActor, ServerCommand};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectRegisterRequest {
    path: String,
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectRenameRequest {
    id: String,
    name: String,
}

impl ServerActor {
    pub(super) fn start_project_registration(
        &self,
        client_id: u64,
        request_id: i64,
        payload: &Value,
    ) -> HostResult<()> {
        let request: ProjectRegisterRequest = parse(payload)?;
        let path = request.path;
        let name = request.name;
        let inbox = self.inbox.clone();
        self.deferred_admission.schedule(
            super::deferred_admission::DeferredRequestClass::Bulk,
            "project.register",
            Some(client_id),
            async move {
                let result = tokio::task::spawn_blocking(move || {
                    prepare_project_registration(&path, name.as_deref()).map_err(state_error)
                })
                .await
                .unwrap_or_else(|error| {
                    Err(HostError::state(format!(
                        "Project registration preparation failed: {error}"
                    )))
                });
                let _ = inbox.send(ServerCommand::ProjectRegistrationPrepared {
                    client_id,
                    request_id,
                    result,
                });
            },
        )
    }

    pub(super) async fn finish_project_registration(
        &mut self,
        client_id: u64,
        request_id: i64,
        result: HostResult<PreparedProjectRegistration>,
    ) {
        if self.require_auth(client_id).is_err() {
            return;
        }
        let result = match result {
            Ok(prepared) => commit_project_registration(&self.runtime_store, prepared)
                .await
                .map_err(state_error)
                .and_then(|registration| serde_json::to_value(registration).map_err(state_error)),
            Err(error) => Err(error),
        };
        match result {
            Ok(value) => {
                self.broadcast_project_state_changed();
                self.client_write(client_id, ok_response(request_id, value));
            }
            Err(error) => self.client_write(client_id, error_response(request_id, &error)),
        }
    }

    pub(super) async fn project_register_request(&mut self, payload: &Value) -> HostResult<Value> {
        let request: ProjectRegisterRequest = parse(payload)?;
        let result = register_project(&self.runtime_store, &request.path, request.name.as_deref())
            .await
            .map_err(state_error)?;
        self.broadcast_project_state_changed();
        serde_json::to_value(result).map_err(state_error)
    }

    pub(super) async fn project_rename_request(&mut self, payload: &Value) -> HostResult<Value> {
        let request: ProjectRenameRequest = parse(payload)?;
        let project = rename_project(&self.runtime_store, &request.id, &request.name)
            .await
            .map_err(state_error)?;
        self.broadcast_authenticated(event("projectsChanged", json!({})));
        serde_json::to_value(project).map_err(state_error)
    }

    pub(super) async fn project_remove_preview_request(
        &self,
        payload: &Value,
    ) -> HostResult<Value> {
        let id = string_key(payload, "id")?;
        let workspaces = self
            .runtime_store
            .list_workspaces(&id)
            .await
            .map_err(state_error)?;
        let mut tab_count = 0usize;
        let mut session_count = 0usize;
        for workspace in &workspaces {
            tab_count += self
                .runtime_store
                .list_workspace_tabs(&workspace.id)
                .await
                .map_err(state_error)?
                .len();
            session_count += self
                .sessions
                .values()
                .filter(|session| session.workspace_id() == workspace.id)
                .count();
        }
        let has_config_override = self
            .runtime_store
            .find_project_config(&id)
            .await
            .map_err(state_error)?
            .is_some();
        Ok(json!({
            "projectId": id,
            "workspaceCount": workspaces.len(),
            "tabCount": tab_count,
            "activeSessionCount": session_count,
            "hasConfigOverride": has_config_override,
        }))
    }

    pub(super) fn host_directory_roots_request(&self) -> HostResult<Value> {
        Ok(json!({ "roots": host_directory_roots() }))
    }

    pub(super) fn broadcast_project_state_changed(&self) {
        self.broadcast_authenticated(event("projectsChanged", json!({})));
        self.broadcast_workspaces_changed(None);
        self.broadcast_workspace_tabs_changed(None);
    }
}

pub(super) async fn load_effective_project_config(
    runtime_store: RuntimeStore,
    project_id: String,
) -> HostResult<Value> {
    let result = effective_project_config(&runtime_store, &project_id)
        .await
        .map_err(state_error)?;
    serde_json::to_value(result).map_err(state_error)
}

pub(super) async fn load_project_branches(
    runtime_store: RuntimeStore,
    project_id: String,
) -> HostResult<Value> {
    let project = runtime_store
        .find_project(&project_id)
        .await
        .map_err(state_error)?
        .ok_or_else(|| HostError::state(format!("Project not found: {project_id}")))?;
    tokio::task::spawn_blocking(move || {
        let branches = core_git::list_branches(&project.repo_path).map_err(state_error)?;
        let local_branches = branches
            .iter()
            .filter_map(
                |branch| match core_git::branch_exists(&project.repo_path, branch) {
                    Ok(true) => Some(Ok(branch.clone())),
                    Ok(false) => None,
                    Err(error) => Some(Err(state_error(error))),
                },
            )
            .collect::<HostResult<Vec<String>>>()?;
        Ok(json!({
            "projectId": project.id,
            "branches": branches,
            "localBranches": local_branches,
        }))
    })
    .await
    .unwrap_or_else(|error| {
        Err(HostError::state(format!(
            "Deferred request failed: {error}"
        )))
    })
}

fn parse<T: for<'de> Deserialize<'de>>(payload: &Value) -> HostResult<T> {
    serde_json::from_value(payload.clone()).map_err(state_error)
}

fn string_key(payload: &Value, key: &str) -> HostResult<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| HostError::state(format!("Missing or invalid {key}.")))
}

fn state_error(error: impl std::fmt::Display) -> HostError {
    HostError::state(error.to_string())
}

#[allow(dead_code)]
fn _assert_project_config_is_serializable(config: ProjectConfig) -> Value {
    serde_json::to_value(config).unwrap_or(Value::Null)
}
