use alera_core::runtime::{RuntimeStore, Workspace};
use serde_json::Value;

use crate::terminal_host::host_error::{HostError, HostResult};

use super::request_payloads::{json_result, parse_payload, require_string_key};

pub(super) struct WorkspaceRequestHandler<'a> {
    runtime_store: &'a RuntimeStore,
}

pub(super) struct WorkspaceRequestOutcome {
    pub(super) value: Value,
    pub(super) changed_project_id: Option<String>,
}

impl<'a> WorkspaceRequestHandler<'a> {
    pub(super) const fn new(runtime_store: &'a RuntimeStore) -> Self {
        Self { runtime_store }
    }

    pub(super) async fn execute(
        &self,
        request_type: &str,
        payload: &Value,
    ) -> HostResult<WorkspaceRequestOutcome> {
        let (value, changed_project_id) = match request_type {
            "workspace.list" => {
                let project_id = require_string_key(payload, "projectId")?;
                (
                    json_result(self.runtime_store.list_workspaces(&project_id).await)?,
                    None,
                )
            }
            "workspace.listAll" => (
                json_result(self.runtime_store.list_all_workspaces().await)?,
                None,
            ),
            "workspace.find" => {
                let id = require_string_key(payload, "id")?;
                (
                    json_result(self.runtime_store.find_workspace(&id).await)?,
                    None,
                )
            }
            "workspace.upsert" => {
                let workspace: Workspace = parse_payload(payload)?;
                let project_id = workspace.project_id.clone();
                (
                    json_result(self.runtime_store.upsert_workspace(workspace).await)?,
                    Some(project_id),
                )
            }
            "workspace.rename" => {
                let workspace_id = require_string_key(payload, "workspaceId")?;
                let name = require_string_key(payload, "name")?;
                let workspace = self
                    .runtime_store
                    .rename_workspace(&workspace_id, &name)
                    .await
                    .map_err(|error| HostError::state(error.to_string()))?;
                let project_id = workspace.project_id.clone();
                (
                    serde_json::to_value(workspace)
                        .map_err(|error| HostError::state(error.to_string()))?,
                    Some(project_id),
                )
            }
            "workspace.setPinned" => {
                let id = require_string_key(payload, "id")?;
                let is_pinned = payload
                    .get("isPinned")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| HostError::format("isPinned is required."))?;
                let workspace = self
                    .runtime_store
                    .set_workspace_pinned(&id, is_pinned)
                    .await
                    .map_err(|error| HostError::state(error.to_string()))?;
                let project_id = workspace.project_id.clone();
                (
                    serde_json::to_value(workspace)
                        .map_err(|error| HostError::state(error.to_string()))?,
                    Some(project_id),
                )
            }
            "workspaceCascade.preview" => {
                let workspace_ids = string_array(payload.get("workspaceIds"));
                let tag_ids = string_array(payload.get("tagIds"));
                let include_descendants = payload
                    .get("includeDescendants")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let include_tags = payload
                    .get("includeTags")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                (
                    json_result(
                        self.runtime_store
                            .cascade_preview(
                                &workspace_ids,
                                &tag_ids,
                                include_descendants,
                                include_tags,
                            )
                            .await,
                    )?,
                    None,
                )
            }
            _ => return Err(HostError::format("Unknown workspace request.")),
        };
        Ok(WorkspaceRequestOutcome {
            value,
            changed_project_id,
        })
    }
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use alera_core::runtime::{Project, ProjectKind};
    use chrono::Utc;
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn workspace_store_requests_can_be_tested_without_server_actor() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        let now = Utc::now();
        store
            .upsert_project(Project {
                id: "p".into(),
                name: "Project".into(),
                repo_path: "/p".into(),
                created_at: now,
                updated_at: now,
                kind: ProjectKind::Folder,
            })
            .await
            .unwrap();

        let handler = WorkspaceRequestHandler::new(&store);
        let upserted = handler
            .execute(
                "workspace.upsert",
                &json!({
                    "id": "w",
                    "instanceId": "instance",
                    "hostId": "local",
                    "projectId": "p",
                    "name": "Workspace",
                    "path": "/p",
                    "createdAt": now,
                    "updatedAt": now,
                    "kind": "main",
                    "status": "active",
                    "reusesExistingBranch": false,
                }),
            )
            .await
            .unwrap();
        assert_eq!(upserted.changed_project_id.as_deref(), Some("p"));
        assert_eq!(upserted.value["id"], "w");

        let listed = handler
            .execute("workspace.list", &json!({"projectId": "p"}))
            .await
            .unwrap();
        assert!(listed.changed_project_id.is_none());
        assert_eq!(listed.value.as_array().unwrap().len(), 1);

        let found = handler
            .execute("workspace.find", &json!({"id": "w"}))
            .await
            .unwrap();
        assert_eq!(found.value["name"], "Workspace");

        let renamed = handler
            .execute(
                "workspace.rename",
                &json!({"workspaceId": "w", "name": "Renamed"}),
            )
            .await
            .unwrap();
        assert_eq!(renamed.changed_project_id.as_deref(), Some("p"));
        assert_eq!(renamed.value["name"], "Renamed");

        let pinned = handler
            .execute("workspace.setPinned", &json!({"id": "w", "isPinned": true}))
            .await
            .unwrap();
        assert_eq!(pinned.changed_project_id.as_deref(), Some("p"));
        assert_eq!(pinned.value["isPinned"], true);

        let all = handler
            .execute("workspace.listAll", &json!({}))
            .await
            .unwrap();
        assert_eq!(all.value.as_array().unwrap().len(), 1);

        let cascade = handler
            .execute(
                "workspaceCascade.preview",
                &json!({
                    "workspaceIds": ["w"],
                    "tagIds": [],
                    "includeDescendants": false,
                    "includeTags": false,
                }),
            )
            .await
            .unwrap();
        assert!(cascade.changed_project_id.is_none());
        assert_eq!(cascade.value["workspaceIds"], json!(["w"]));
    }
}
