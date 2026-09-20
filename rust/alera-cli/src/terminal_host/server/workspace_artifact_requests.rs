use alera_core::runtime::{LinkedReview, RuntimeStore, WorkbenchLayoutRecord};
use serde_json::{json, Value};

use crate::terminal_host::host_error::{HostError, HostResult};

use super::request_payloads::{json_result, parse_payload, require_string_key};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum WorkspaceArtifactChange {
    LinkedReviews,
    WorkbenchLayouts,
}

pub(super) struct WorkspaceArtifactRequestOutcome {
    pub(super) value: Value,
    pub(super) change: Option<WorkspaceArtifactChange>,
}

pub(super) struct WorkspaceArtifactRequestHandler<'a> {
    runtime_store: &'a RuntimeStore,
}

impl<'a> WorkspaceArtifactRequestHandler<'a> {
    pub(super) const fn new(runtime_store: &'a RuntimeStore) -> Self {
        Self { runtime_store }
    }

    pub(super) async fn execute(
        &self,
        request_type: &str,
        payload: &Value,
    ) -> HostResult<WorkspaceArtifactRequestOutcome> {
        let (value, change) = match request_type {
            "linkedReview.find" => {
                let workspace_id = require_string_key(payload, "workspaceId")?;
                (
                    json_result(self.runtime_store.find_linked_review(&workspace_id).await)?,
                    None,
                )
            }
            "linkedReview.upsert" => {
                let review: LinkedReview = parse_payload(payload)?;
                (
                    json_result(self.runtime_store.upsert_linked_review(review).await)?,
                    Some(WorkspaceArtifactChange::LinkedReviews),
                )
            }
            "linkedReview.remove" => {
                let workspace_id = require_string_key(payload, "workspaceId")?;
                json_result(self.runtime_store.remove_linked_review(&workspace_id).await)?;
                (json!({}), Some(WorkspaceArtifactChange::LinkedReviews))
            }
            "layout.find" => {
                let workspace_id = require_string_key(payload, "workspaceId")?;
                (
                    json_result(
                        self.runtime_store
                            .find_workbench_layout(&workspace_id)
                            .await,
                    )?,
                    None,
                )
            }
            "layout.upsert" => {
                let layout: WorkbenchLayoutRecord = parse_payload(payload)?;
                (
                    json_result(self.runtime_store.upsert_workbench_layout(layout).await)?,
                    Some(WorkspaceArtifactChange::WorkbenchLayouts),
                )
            }
            "layout.remove" => {
                let workspace_id = require_string_key(payload, "workspaceId")?;
                json_result(
                    self.runtime_store
                        .remove_workbench_layout(&workspace_id)
                        .await,
                )?;
                (json!({}), Some(WorkspaceArtifactChange::WorkbenchLayouts))
            }
            _ => return Err(HostError::format("Unknown workspace artifact request.")),
        };
        Ok(WorkspaceArtifactRequestOutcome { value, change })
    }
}

#[cfg(test)]
mod tests {
    use alera_core::runtime::{Project, ProjectKind, Workspace};
    use chrono::Utc;

    use super::*;

    #[tokio::test]
    async fn workspace_artifacts_can_be_tested_without_server_actor() {
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
        let workspace: Workspace = serde_json::from_value(json!({
            "id": "w", "instanceId": "instance", "hostId": "local", "projectId": "p", "name": "Workspace", "path": "/p",
            "createdAt": now, "updatedAt": now, "kind": "main", "status": "active", "reusesExistingBranch": false,
        }))
        .unwrap();
        store.upsert_workspace(workspace).await.unwrap();

        let handler = WorkspaceArtifactRequestHandler::new(&store);
        let review = handler
            .execute(
                "linkedReview.upsert",
                &json!({
                    "workspaceId": "w",
                    "dismissed": false,
                    "provider": "github",
                    "number": 42,
                    "url": "https://example.test/review/42",
                    "linkedAt": now,
                }),
            )
            .await
            .unwrap();
        assert_eq!(review.change, Some(WorkspaceArtifactChange::LinkedReviews));
        let found_review = handler
            .execute("linkedReview.find", &json!({"workspaceId": "w"}))
            .await
            .unwrap();
        assert_eq!(found_review.value["number"], 42);

        let layout = handler
            .execute(
                "layout.upsert",
                &json!({"workspaceId": "w", "data": {"marker": "saved"}}),
            )
            .await
            .unwrap();
        assert_eq!(
            layout.change,
            Some(WorkspaceArtifactChange::WorkbenchLayouts)
        );
        let found_layout = handler
            .execute("layout.find", &json!({"workspaceId": "w"}))
            .await
            .unwrap();
        assert_eq!(found_layout.value["data"]["marker"], "saved");

        let removed_review = handler
            .execute("linkedReview.remove", &json!({"workspaceId": "w"}))
            .await
            .unwrap();
        assert_eq!(removed_review.value, json!({}));
        let removed_layout = handler
            .execute("layout.remove", &json!({"workspaceId": "w"}))
            .await
            .unwrap();
        assert_eq!(removed_layout.value, json!({}));
    }
}
