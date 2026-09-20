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

use super::request_payloads::{json_result, parse_payload, require_string_key};
use super::workspace_activity_requests::WorkspaceActivityRequestHandler;
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum WorkspaceTagChange {
    None,
    Tags,
    Workspaces(Option<String>),
    TagsAndWorkspaces,
}

#[derive(Debug)]
pub(super) struct WorkspaceTagRequestOutcome {
    pub(super) value: Value,
    pub(super) change: WorkspaceTagChange,
}

pub(super) struct WorkspaceTagRequestHandler<'a> {
    runtime_store: &'a RuntimeStore,
}

impl<'a> WorkspaceTagRequestHandler<'a> {
    pub(super) const fn new(runtime_store: &'a RuntimeStore) -> Self {
        Self { runtime_store }
    }

    pub(super) async fn execute(
        &self,
        request_type: &str,
        payload: &Value,
    ) -> HostResult<WorkspaceTagRequestOutcome> {
        let (value, change) = match request_type {
            "workspaceTag.list" => (
                json_result(self.runtime_store.list_tags().await)?,
                WorkspaceTagChange::None,
            ),
            "workspaceTag.create" => {
                let name = string_field(payload, "name")?.trim();
                if name.is_empty() {
                    return Err(HostError::format("Tag name cannot be empty."));
                }
                if let Some(existing) = self
                    .runtime_store
                    .list_tags()
                    .await
                    .map_err(state_error)?
                    .into_iter()
                    .find(|tag| tag.name.eq_ignore_ascii_case(name))
                {
                    return Err(HostError::conflict(
                        "workspace_tag_name_conflict",
                        format!("A tag named '{name}' already exists."),
                        json!({
                            "name": name,
                            "existingTagId": existing.id,
                        }),
                    ));
                }
                let color = payload
                    .get("color")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                let now = Utc::now();
                (
                    json_result(
                        self.runtime_store
                            .upsert_tag(WorkspaceTag {
                                id: Uuid::new_v4().to_string(),
                                name: name.to_string(),
                                color,
                                created_at: now,
                                updated_at: now,
                            })
                            .await,
                    )?,
                    WorkspaceTagChange::Tags,
                )
            }
            "workspaceTag.setForWorkspace" => {
                let request: SetWorkspaceTagsRequest =
                    serde_json::from_value(payload.clone()).map_err(format_error)?;
                let workspace = self
                    .runtime_store
                    .set_workspace_tags(&request.workspace_id, &request.tag_ids)
                    .await
                    .map_err(state_error)?;
                let project_id = workspace.project_id.clone();
                (
                    serde_json::to_value(workspace).map_err(state_error)?,
                    WorkspaceTagChange::Workspaces(Some(project_id)),
                )
            }
            "workspaceTag.upsert" => {
                let tag: WorkspaceTag = parse_payload(payload)?;
                (
                    json_result(self.runtime_store.upsert_tag(tag).await)?,
                    WorkspaceTagChange::TagsAndWorkspaces,
                )
            }
            "workspaceTag.remove" => {
                let id = require_string_key(payload, "id")?;
                json_result(self.runtime_store.remove_tag(&id).await)?;
                (json!({}), WorkspaceTagChange::TagsAndWorkspaces)
            }
            "workspaceTag.assign" => {
                let workspace_id = require_string_key(payload, "workspaceId")?;
                let tag_id = require_string_key(payload, "tagId")?;
                json_result(self.runtime_store.assign_tag(&workspace_id, &tag_id).await)?;
                (json!({}), WorkspaceTagChange::Workspaces(None))
            }
            "workspaceTag.unassign" => {
                let workspace_id = require_string_key(payload, "workspaceId")?;
                let tag_id = require_string_key(payload, "tagId")?;
                json_result(
                    self.runtime_store
                        .unassign_tag(&workspace_id, &tag_id)
                        .await,
                )?;
                (json!({}), WorkspaceTagChange::Workspaces(None))
            }
            _ => return Err(HostError::format("Unknown workspace tag request.")),
        };
        Ok(WorkspaceTagRequestOutcome { value, change })
    }
}

#[derive(Default)]
pub(super) struct WorkspaceSidebarSnapshotState {
    in_flight: bool,
    waiters: Vec<(u64, i64)>,
}

impl WorkspaceSidebarSnapshotState {
    fn register(&mut self, client_id: u64, request_id: i64) -> bool {
        self.waiters.push((client_id, request_id));
        if self.in_flight {
            return false;
        }
        self.in_flight = true;
        true
    }

    fn take_waiters(&mut self) -> Vec<(u64, i64)> {
        self.in_flight = false;
        std::mem::take(&mut self.waiters)
    }
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

    pub(super) fn start_workspace_sidebar_snapshot(
        &mut self,
        client_id: u64,
        request_id: i64,
        request_type: &str,
    ) {
        if !self
            .workspace_sidebar_snapshots
            .register(client_id, request_id)
        {
            return;
        }
        let runtime_store = self.runtime_store.clone();
        let inbox = self.inbox.clone();
        if let Err(error) = self.deferred_admission.schedule(
            super::deferred_admission::DeferredRequestClass::Bulk,
            request_type,
            None,
            async move {
                let result = load_workspace_sidebar_snapshot(&runtime_store).await;
                let _ = inbox.send(ServerCommand::WorkspaceSidebarSnapshotFinished { result });
            },
        ) {
            self.finish_workspace_sidebar_snapshot(Err(error));
        }
    }

    pub(super) fn finish_workspace_sidebar_snapshot(&mut self, result: HostResult<Value>) {
        let waiters = self.workspace_sidebar_snapshots.take_waiters();
        match result {
            Ok(mut payload) => {
                payload["agentPresence"] = self.agent_presence_items();
                for (client_id, request_id) in waiters {
                    if self.require_auth(client_id).is_ok() {
                        self.client_write(client_id, ok_response(request_id, payload.clone()));
                    }
                }
            }
            Err(error) => {
                for (client_id, request_id) in waiters {
                    if self.require_auth(client_id).is_ok() {
                        self.client_write(client_id, error_response(request_id, &error));
                    }
                }
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
        WorkspaceActivityRequestHandler::new(&self.runtime_store)
            .list()
            .await
    }

    pub(super) async fn upsert_workspace_activity(
        &mut self,
        client_id: u64,
        payload: &Value,
    ) -> HostResult<Value> {
        self.require_auth(client_id)?;
        let value = WorkspaceActivityRequestHandler::new(&self.runtime_store)
            .upsert_all(payload)
            .await?;
        self.broadcast_authenticated(event("workspaceActivityChanged", json!({})));
        Ok(value)
    }

    pub(super) async fn remove_workspace_activity(
        &mut self,
        client_id: u64,
        payload: &Value,
    ) -> HostResult<Value> {
        self.require_auth(client_id)?;
        let value = WorkspaceActivityRequestHandler::new(&self.runtime_store)
            .remove(payload)
            .await?;
        self.broadcast_authenticated(event("workspaceActivityChanged", json!({})));
        Ok(value)
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

    pub(super) async fn workspace_tag_request(
        &mut self,
        client_id: u64,
        request_type: &str,
        payload: &Value,
    ) -> HostResult<Value> {
        self.require_auth(client_id)?;
        let outcome = WorkspaceTagRequestHandler::new(&self.runtime_store)
            .execute(request_type, payload)
            .await?;
        match outcome.change {
            WorkspaceTagChange::None => {}
            WorkspaceTagChange::Tags => {
                self.broadcast_authenticated(event("workspaceTagsChanged", json!({})));
            }
            WorkspaceTagChange::Workspaces(project_id) => {
                self.broadcast_workspaces_changed(project_id.as_deref());
            }
            WorkspaceTagChange::TagsAndWorkspaces => {
                self.broadcast_authenticated(event("workspaceTagsChanged", json!({})));
                self.broadcast_workspaces_changed(None);
            }
        }
        Ok(outcome.value)
    }
}

pub(super) async fn load_workspace_repository_web_url(
    runtime_store: RuntimeStore,
    workspace_id: String,
) -> HostResult<Value> {
    let workspace = runtime_store
        .find_workspace(&workspace_id)
        .await
        .map_err(state_error)?
        .ok_or_else(|| HostError::state(format!("Workspace not found: {workspace_id}")))?;
    tokio::task::spawn_blocking(move || {
        let remote_url = core_git::repository_remote_url(&workspace.path)
            .map_err(|error| HostError::state(error.to_string()))?;
        Ok(json!({"remoteUrl": remote_url}))
    })
    .await
    .unwrap_or_else(|error| {
        Err(HostError::state(format!(
            "Deferred request failed: {error}"
        )))
    })
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
