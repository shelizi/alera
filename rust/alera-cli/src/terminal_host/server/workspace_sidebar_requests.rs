use alera_core::{
    git as core_git,
    runtime::{RuntimeStore, SharedWorkbenchPrefsWriter, SharedWorkbenchViewPrefs, WorkspaceTag},
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::protocol::{error_response, event, ok_response};

use super::{ServerActor, ServerCommand};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateViewPrefsRequest {
    prefs: SharedWorkbenchViewPrefs,
    expected_revision: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetWorkspaceTagsRequest {
    workspace_id: String,
    tag_ids: Vec<String>,
}

impl ServerActor {
    pub(super) fn agent_presence_items(&self) -> Value {
        let items = self.orchestration_terminals(&json!({}))["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|item| {
                item.get("agentType").is_some_and(Value::is_string)
                    && item.get("agentState").is_some_and(Value::is_string)
            })
            .cloned()
            .collect::<Vec<_>>();
        Value::Array(items)
    }

    pub(super) fn agent_presence_timestamp(&self, entry: &Value) -> chrono::DateTime<Utc> {
        entry
            .get("stateStartedAt")
            .and_then(Value::as_str)
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.with_timezone(&Utc))
            .unwrap_or_else(Utc::now)
    }

    pub(super) fn broadcast_agent_presence_changed(&self) {
        self.broadcast_authenticated(event("agentPresenceChanged", json!({})));
    }

    pub(super) fn broadcast_agent_presence_changes(&self, changes: Vec<Value>) {
        if changes.is_empty() {
            return;
        }
        self.broadcast_authenticated(event("agentPresenceChanged", json!({"changes": changes})));
    }

    pub(super) fn start_workspace_sidebar_snapshot(&self, client_id: u64, request_id: i64) {
        let runtime_store = self.runtime_store.clone();
        let inbox = self.inbox.clone();
        tokio::spawn(async move {
            let result = load_workspace_sidebar_snapshot(&runtime_store).await;
            let _ = inbox.send(ServerCommand::WorkspaceSidebarSnapshotFinished {
                client_id,
                request_id,
                result,
            });
        });
    }

    pub(super) fn finish_workspace_sidebar_snapshot(
        &self,
        client_id: u64,
        request_id: i64,
        result: HostResult<Value>,
    ) {
        if self.require_auth(client_id).is_err() {
            return;
        }
        match result {
            Ok(mut payload) => {
                payload["agentPresence"] = self.agent_presence_items();
                self.client_write(client_id, ok_response(request_id, payload));
            }
            Err(error) => {
                self.client_write(client_id, error_response(request_id, &error));
            }
        }
    }

    pub(super) async fn workspace_sidebar_snapshot(&self, client_id: u64) -> HostResult<Value> {
        self.require_auth(client_id)?;
        let mut payload = load_workspace_sidebar_snapshot(&self.runtime_store).await?;
        payload["agentPresence"] = self.agent_presence_items();
        Ok(payload)
    }

    pub(super) async fn workbench_view_prefs(&self, client_id: u64) -> HostResult<Value> {
        self.require_auth(client_id)?;
        serde_json::to_value(
            self.runtime_store
                .shared_workbench_view_prefs()
                .await
                .map_err(state_error)?,
        )
        .map_err(state_error)
    }

    pub(super) async fn workspace_activity(&self, client_id: u64) -> HostResult<Value> {
        self.require_auth(client_id)?;
        serde_json::to_value(
            self.runtime_store
                .list_workspace_activity()
                .await
                .map_err(state_error)?,
        )
        .map_err(state_error)
    }

    pub(super) async fn upsert_workspace_activity(
        &mut self,
        client_id: u64,
        payload: &Value,
    ) -> HostResult<Value> {
        self.require_auth(client_id)?;
        let entries = serde_json::from_value(payload.clone()).map_err(format_error)?;
        let value = self
            .runtime_store
            .record_workspace_activity_batch(entries)
            .await
            .map_err(state_error)?;
        self.broadcast_authenticated(event("workspaceActivityChanged", json!({})));
        serde_json::to_value(value).map_err(state_error)
    }

    pub(super) async fn remove_workspace_activity(
        &mut self,
        client_id: u64,
        payload: &Value,
    ) -> HostResult<Value> {
        self.require_auth(client_id)?;
        let workspace_id = string_field(payload, "workspaceId")?;
        self.runtime_store
            .remove_workspace_activity(workspace_id)
            .await
            .map_err(state_error)?;
        self.broadcast_authenticated(event("workspaceActivityChanged", json!({})));
        Ok(json!({}))
    }

    pub(super) async fn update_workbench_view_prefs(
        &mut self,
        client_id: u64,
        payload: &Value,
    ) -> HostResult<Value> {
        self.require_auth(client_id)?;
        let mut compatible = payload.clone();
        let current = self
            .runtime_store
            .shared_workbench_view_prefs()
            .await
            .map_err(state_error)?;
        let current_json = serde_json::to_value(current.prefs).map_err(state_error)?;
        if let Some(prefs) = compatible.get_mut("prefs").and_then(Value::as_object_mut) {
            for key in [
                "sectionSort",
                "collapsedSectionIds",
                "othersSectionCollapsed",
            ] {
                if !prefs.contains_key(key) {
                    prefs.insert(key.to_string(), current_json[key].clone());
                }
            }
        }
        let request: UpdateViewPrefsRequest =
            serde_json::from_value(compatible).map_err(format_error)?;
        let writer = if self.is_mobile_client(client_id) {
            SharedWorkbenchPrefsWriter::Mobile
        } else {
            SharedWorkbenchPrefsWriter::Desktop
        };
        let value = self
            .runtime_store
            .update_shared_workbench_view_prefs(request.prefs, request.expected_revision, writer)
            .await
            .map_err(state_error)?;
        self.broadcast_authenticated(event("workbenchViewPrefsChanged", json!({})));
        serde_json::to_value(value).map_err(state_error)
    }

    pub(super) async fn rename_workspace_request(
        &mut self,
        client_id: u64,
        payload: &Value,
    ) -> HostResult<Value> {
        self.require_auth(client_id)?;
        let workspace_id = string_field(payload, "workspaceId")?;
        let name = string_field(payload, "name")?;
        let workspace = self
            .runtime_store
            .rename_workspace(workspace_id, name)
            .await
            .map_err(state_error)?;
        let project_id = workspace.project_id.clone();
        self.broadcast_workspaces_changed(Some(&project_id));
        serde_json::to_value(workspace).map_err(state_error)
    }

    pub(super) async fn workspace_repository_web_url(
        &self,
        client_id: u64,
        payload: &Value,
    ) -> HostResult<Value> {
        self.require_auth(client_id)?;
        let workspace_id = string_field(payload, "workspaceId")?;
        let workspace = self
            .runtime_store
            .find_workspace(workspace_id)
            .await
            .map_err(state_error)?
            .ok_or_else(|| HostError::state(format!("Workspace not found: {workspace_id}")))?;
        let remote_url = core_git::repository_remote_url(&workspace.path)
            .map_err(|error| HostError::state(error.to_string()))?;
        Ok(json!({"remoteUrl": remote_url}))
    }

    pub(super) async fn create_workspace_tag(
        &mut self,
        client_id: u64,
        payload: &Value,
    ) -> HostResult<Value> {
        self.require_auth(client_id)?;
        let name = string_field(payload, "name")?.trim();
        if name.is_empty() {
            return Err(HostError::format("Tag name cannot be empty."));
        }
        let color = payload
            .get("color")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        let now = Utc::now();
        let tag = self
            .runtime_store
            .upsert_tag(WorkspaceTag {
                id: Uuid::new_v4().to_string(),
                name: name.to_string(),
                color,
                created_at: now,
                updated_at: now,
            })
            .await
            .map_err(state_error)?;
        self.broadcast_authenticated(event("workspaceTagsChanged", json!({})));
        serde_json::to_value(tag).map_err(state_error)
    }

    pub(super) async fn set_tags_for_workspace(
        &mut self,
        client_id: u64,
        payload: &Value,
    ) -> HostResult<Value> {
        self.require_auth(client_id)?;
        let request: SetWorkspaceTagsRequest =
            serde_json::from_value(payload.clone()).map_err(format_error)?;
        let workspace = self
            .runtime_store
            .set_workspace_tags(&request.workspace_id, &request.tag_ids)
            .await
            .map_err(state_error)?;
        let project_id = workspace.project_id.clone();
        self.broadcast_workspaces_changed(Some(&project_id));
        serde_json::to_value(workspace).map_err(state_error)
    }
}

async fn load_workspace_sidebar_snapshot(runtime_store: &RuntimeStore) -> HostResult<Value> {
    let projects = runtime_store.list_projects().await.map_err(state_error)?;
    let workspaces = runtime_store
        .list_all_workspaces()
        .await
        .map_err(state_error)?;
    let tags = runtime_store.list_tags().await.map_err(state_error)?;
    let sections = runtime_store
        .list_workspace_sections()
        .await
        .map_err(state_error)?;
    let activity = runtime_store
        .list_workspace_activity()
        .await
        .map_err(state_error)?;
    let view_prefs = runtime_store
        .shared_workbench_view_prefs()
        .await
        .map_err(state_error)?;
    let runtime_settings = runtime_store
        .runtime_settings()
        .await
        .map_err(state_error)?;
    let terminal_tab_count_by_workspace_id = runtime_store
        .terminal_tab_counts_by_workspace()
        .await
        .map_err(state_error)?;
    Ok(json!({
        "projects": projects,
        "workspaces": workspaces,
        "tags": tags,
        "sections": sections,
        "activity": activity,
        "viewPrefs": view_prefs,
        "runtimeSettings": runtime_settings,
        "terminalTabCountByWorkspaceId": terminal_tab_count_by_workspace_id,
    }))
}

fn string_field<'a>(payload: &'a Value, key: &str) -> HostResult<&'a str> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| HostError::format(format!("{key} is required.")))
}

fn state_error(error: impl std::fmt::Display) -> HostError {
    HostError::state(error.to_string())
}

fn format_error(error: impl std::fmt::Display) -> HostError {
    HostError::format(error.to_string())
}
