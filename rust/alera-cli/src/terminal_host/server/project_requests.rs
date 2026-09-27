use alera_core::git as core_git;
use alera_core::runtime::{Project, ProjectConfig, RuntimeStore};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::project_management::{effective_project_config, host_directory_roots, rename_project};
use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::protocol::event;

use super::request_payloads::{json_result, parse_payload, require_string_key};
use super::ServerActor;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectRenameRequest {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectConfigUpsertRequest {
    project_id: String,
    config: ProjectConfig,
    #[serde(default)]
    updated_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ProjectStoreChange {
    Projects,
    ProjectConfigs,
}

pub(super) struct ProjectStoreRequestOutcome {
    pub(super) value: Value,
    pub(super) change: Option<ProjectStoreChange>,
}

pub(super) struct ProjectStoreRequestHandler<'a> {
    runtime_store: &'a RuntimeStore,
}

impl<'a> ProjectStoreRequestHandler<'a> {
    pub(super) const fn new(runtime_store: &'a RuntimeStore) -> Self {
        Self { runtime_store }
    }

    pub(super) async fn execute(
        &self,
        request_type: &str,
        payload: &Value,
    ) -> HostResult<ProjectStoreRequestOutcome> {
        let (value, change) = match request_type {
            "project.list" => (json_result(self.runtime_store.list_projects().await)?, None),
            "project.upsert" => {
                let project: Project = parse_payload(payload)?;
                (
                    json_result(self.runtime_store.upsert_project(project).await)?,
                    Some(ProjectStoreChange::Projects),
                )
            }
            "projectConfig.find" => {
                let project_id = require_string_key(payload, "projectId")?;
                (
                    json_result(self.runtime_store.find_project_config(&project_id).await)?,
                    None,
                )
            }
            "projectConfig.list" => (
                json_result(self.runtime_store.list_project_configs().await)?,
                None,
            ),
            "projectConfig.upsert" => {
                let request: ProjectConfigUpsertRequest = parse_payload(payload)?;
                (
                    json_result(
                        self.runtime_store
                            .upsert_project_config(
                                &request.project_id,
                                request.config,
                                request.updated_at.unwrap_or_else(Utc::now),
                            )
                            .await,
                    )?,
                    Some(ProjectStoreChange::ProjectConfigs),
                )
            }
            "projectConfig.remove" => {
                let project_id = require_string_key(payload, "projectId")?;
                json_result(self.runtime_store.remove_project_config(&project_id).await)?;
                (json!({}), Some(ProjectStoreChange::ProjectConfigs))
            }
            _ => return Err(HostError::format("Unknown project store request.")),
        };
        Ok(ProjectStoreRequestOutcome { value, change })
    }
}

pub(super) struct ProjectRenameHandler<'a> {
    runtime_store: &'a RuntimeStore,
}

impl<'a> ProjectRenameHandler<'a> {
    pub(super) const fn new(runtime_store: &'a RuntimeStore) -> Self {
        Self { runtime_store }
    }

    pub(super) async fn execute(&self, payload: &Value) -> HostResult<Value> {
        let request: ProjectRenameRequest = parse(payload)?;
        let project = rename_project(self.runtime_store, &request.id, &request.name)
            .await
            .map_err(state_error)?;
        serde_json::to_value(project).map_err(state_error)
    }
}

impl ServerActor {
    pub(super) async fn project_rename_request(&mut self, payload: &Value) -> HostResult<Value> {
        let value = ProjectRenameHandler::new(&self.runtime_store)
            .execute(payload)
            .await?;
        self.broadcast_authenticated(event("projectsChanged", json!({})));
        Ok(value)
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

#[cfg(test)]
mod tests {
    use alera_core::runtime::{Project, ProjectConfig, ProjectKind, RuntimeStore};
    use chrono::Utc;
    use serde_json::json;

    use super::{ProjectRenameHandler, ProjectStoreChange, ProjectStoreRequestHandler};

    #[tokio::test]
    async fn project_store_requests_can_be_tested_without_server_actor() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        let now = Utc::now();
        let handler = ProjectStoreRequestHandler::new(&store);

        let upserted = handler
            .execute(
                "project.upsert",
                &json!({
                    "id": "p",
                    "name": "Project",
                    "repoPath": "/p",
                    "createdAt": now,
                    "updatedAt": now,
                    "kind": "folder",
                }),
            )
            .await
            .unwrap();
        assert_eq!(upserted.change, Some(ProjectStoreChange::Projects));
        assert_eq!(upserted.value["id"], "p");

        let listed = handler.execute("project.list", &json!({})).await.unwrap();
        assert!(listed.change.is_none());
        assert_eq!(listed.value.as_array().unwrap().len(), 1);

        let config = ProjectConfig {
            git_hosting_provider: Some("github".into()),
            ..ProjectConfig::default()
        };
        let config_upserted = handler
            .execute(
                "projectConfig.upsert",
                &json!({"projectId": "p", "config": config, "updatedAt": now}),
            )
            .await
            .unwrap();
        assert_eq!(
            config_upserted.change,
            Some(ProjectStoreChange::ProjectConfigs)
        );

        let found = handler
            .execute("projectConfig.find", &json!({"projectId": "p"}))
            .await
            .unwrap();
        assert_eq!(found.value["gitHostingProvider"], "github");

        let configs = handler
            .execute("projectConfig.list", &json!({}))
            .await
            .unwrap();
        assert_eq!(configs.value["p"]["gitHostingProvider"], "github");

        let removed = handler
            .execute("projectConfig.remove", &json!({"projectId": "p"}))
            .await
            .unwrap();
        assert_eq!(removed.change, Some(ProjectStoreChange::ProjectConfigs));
        assert_eq!(removed.value, json!({}));
    }

    #[tokio::test]
    async fn rename_can_be_tested_without_server_actor() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        let now = Utc::now();
        store
            .upsert_project(Project {
                id: "p".into(),
                name: "Before".into(),
                repo_path: "/p".into(),
                created_at: now,
                updated_at: now,
                kind: ProjectKind::Folder,
            })
            .await
            .unwrap();

        let renamed = ProjectRenameHandler::new(&store)
            .execute(&json!({"id": "p", "name": "After"}))
            .await
            .unwrap();

        assert_eq!(renamed["name"], "After");
        assert_eq!(
            store.find_project("p").await.unwrap().unwrap().name,
            "After"
        );
    }
}
