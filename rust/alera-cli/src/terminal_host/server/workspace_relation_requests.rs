use alera_core::runtime::RuntimeStore;
use serde_json::{json, Value};

use crate::terminal_host::host_error::{HostError, HostResult};

use super::request_payloads::{json_result, require_string_key};

pub(super) struct WorkspaceRelationRequestOutcome {
    pub(super) value: Value,
    pub(super) changed: bool,
}

pub(super) struct WorkspaceRelationRequestHandler<'a> {
    runtime_store: &'a RuntimeStore,
}

impl<'a> WorkspaceRelationRequestHandler<'a> {
    pub(super) const fn new(runtime_store: &'a RuntimeStore) -> Self {
        Self { runtime_store }
    }

    pub(super) async fn execute(
        &self,
        request_type: &str,
        payload: &Value,
    ) -> HostResult<WorkspaceRelationRequestOutcome> {
        let (value, changed) = match request_type {
            "workspaceRelation.list" => (
                json_result(self.runtime_store.list_relations().await)?,
                false,
            ),
            "workspaceRelation.link" => {
                let parent_id = require_string_key(payload, "parentWorkspaceId")?;
                let child_id = require_string_key(payload, "childWorkspaceId")?;
                (
                    json_result(
                        self.runtime_store
                            .link_workspaces(&parent_id, &child_id)
                            .await,
                    )?,
                    true,
                )
            }
            "workspaceRelation.unlink" => {
                let parent_id = require_string_key(payload, "parentWorkspaceId")?;
                let child_id = require_string_key(payload, "childWorkspaceId")?;
                json_result(
                    self.runtime_store
                        .unlink_workspaces(&parent_id, &child_id)
                        .await,
                )?;
                (json!({}), true)
            }
            _ => return Err(HostError::format("Unknown workspace relation request.")),
        };
        Ok(WorkspaceRelationRequestOutcome { value, changed })
    }
}

#[cfg(test)]
mod tests {
    use alera_core::runtime::{Project, ProjectKind, Workspace};
    use chrono::Utc;

    use super::*;

    #[tokio::test]
    async fn workspace_relations_can_be_tested_without_server_actor() {
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
        for (id, instance_id, name) in [
            ("parent", "instance-parent", "Parent"),
            ("child", "instance-child", "Child"),
        ] {
            let workspace: Workspace = serde_json::from_value(json!({
                "id": id,
                "instanceId": instance_id,
                "hostId": "local",
                "projectId": "p",
                "name": name,
                "path": format!("/p/{id}"),
                "createdAt": now,
                "updatedAt": now,
                "kind": "linked",
                "status": "active",
                "reusesExistingBranch": false,
            }))
            .unwrap();
            store.upsert_workspace(workspace).await.unwrap();
        }

        let handler = WorkspaceRelationRequestHandler::new(&store);
        let linked = handler
            .execute(
                "workspaceRelation.link",
                &json!({
                    "parentWorkspaceId": "parent",
                    "childWorkspaceId": "child",
                }),
            )
            .await
            .unwrap();
        assert!(linked.changed);
        assert_eq!(linked.value["parentWorkspaceId"], "parent");
        assert_eq!(linked.value["childWorkspaceId"], "child");

        let listed = handler
            .execute("workspaceRelation.list", &json!({}))
            .await
            .unwrap();
        assert!(!listed.changed);
        assert_eq!(listed.value.as_array().unwrap().len(), 1);

        let unlinked = handler
            .execute(
                "workspaceRelation.unlink",
                &json!({
                    "parentWorkspaceId": "parent",
                    "childWorkspaceId": "child",
                }),
            )
            .await
            .unwrap();
        assert!(unlinked.changed);
        assert_eq!(unlinked.value, json!({}));
        assert!(store.list_relations().await.unwrap().is_empty());
    }
}
