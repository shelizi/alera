use alera_core::runtime::RuntimeStore;
use serde_json::Value;

use crate::project_management::list_host_directory;
use crate::terminal_host::host_error::{HostError, HostResult};

use super::deferred_request_scheduler::DeferredRequestScheduler;
use super::project_requests::{load_effective_project_config, load_project_branches};
use super::request_route_policy::DeferredReadRoute;
use super::requests::require_string_key;
use super::workspace_sidebar_requests::load_workspace_repository_web_url;

pub(super) struct DeferredReadRequestHandler {
    runtime_store: RuntimeStore,
    scheduler: DeferredRequestScheduler,
}

impl DeferredReadRequestHandler {
    pub(super) fn new(runtime_store: RuntimeStore, scheduler: DeferredRequestScheduler) -> Self {
        Self {
            runtime_store,
            scheduler,
        }
    }

    pub(super) fn start(
        &self,
        route: DeferredReadRoute,
        client_id: u64,
        request_id: i64,
        request_type: &str,
        payload: &Value,
    ) -> HostResult<()> {
        match route {
            DeferredReadRoute::ProjectConfigEffective => {
                let project_id = require_string_key(payload, "projectId")?.to_string();
                self.scheduler.schedule(
                    client_id,
                    request_id,
                    request_type,
                    load_effective_project_config(self.runtime_store.clone(), project_id),
                )
            }
            DeferredReadRoute::ProjectBranchesList => {
                let project_id = require_string_key(payload, "projectId")?.to_string();
                self.scheduler.schedule(
                    client_id,
                    request_id,
                    request_type,
                    load_project_branches(self.runtime_store.clone(), project_id),
                )
            }
            DeferredReadRoute::WorkspaceRepositoryWebUrl => {
                let workspace_id = require_string_key(payload, "workspaceId")?.to_string();
                self.scheduler.schedule(
                    client_id,
                    request_id,
                    request_type,
                    load_workspace_repository_web_url(self.runtime_store.clone(), workspace_id),
                )
            }
            DeferredReadRoute::HostDirectoryList => {
                let path = require_string_key(payload, "path")?.to_string();
                self.scheduler
                    .schedule_blocking(client_id, request_id, request_type, move || {
                        let entries = list_host_directory(&path)
                            .map_err(|error| HostError::state(error.to_string()))?;
                        serde_json::to_value(entries)
                            .map_err(|error| HostError::state(error.to_string()))
                    })
            }
        }
    }
}
