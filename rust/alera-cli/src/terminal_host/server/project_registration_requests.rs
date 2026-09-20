use alera_core::runtime::RuntimeStore;
use serde::Deserialize;
use serde_json::Value;

use crate::project_management::{
    commit_project_registration, prepare_project_registration, register_project,
    PreparedProjectRegistration,
};
use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::protocol::{error_response, ok_response};

use super::deferred_request_scheduler::DeferredRequestScheduler;
use super::request_payloads::parse_payload;
use super::{ServerActor, ServerCommand};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectRegisterRequest {
    path: String,
    name: Option<String>,
}

#[derive(Clone)]
pub(super) struct ProjectRegistrationRequestHandler {
    runtime_store: RuntimeStore,
    scheduler: DeferredRequestScheduler,
}

impl ProjectRegistrationRequestHandler {
    pub(super) fn new(runtime_store: RuntimeStore, scheduler: DeferredRequestScheduler) -> Self {
        Self {
            runtime_store,
            scheduler,
        }
    }

    pub(super) fn start(
        &self,
        client_id: u64,
        request_id: i64,
        request_type: &str,
        payload: &Value,
    ) -> HostResult<()> {
        let request: ProjectRegisterRequest = parse_payload(payload)?;
        let path = request.path;
        let name = request.name;
        self.scheduler.schedule_typed(
            client_id,
            request_id,
            request_type,
            async move {
                tokio::task::spawn_blocking(move || {
                    prepare_project_registration(&path, name.as_deref()).map_err(state_error)
                })
                .await
                .unwrap_or_else(|error| {
                    Err(HostError::state(format!(
                        "Project registration preparation failed: {error}"
                    )))
                })
            },
            |client_id, request_id, result| ServerCommand::ProjectRegistrationPrepared {
                client_id,
                request_id,
                result,
            },
        )
    }

    pub(super) async fn execute_inline(&self, payload: &Value) -> HostResult<Value> {
        let request: ProjectRegisterRequest = parse_payload(payload)?;
        let registration =
            register_project(&self.runtime_store, &request.path, request.name.as_deref())
                .await
                .map_err(state_error)?;
        serde_json::to_value(registration).map_err(state_error)
    }

    pub(super) async fn commit_prepared(
        &self,
        result: HostResult<PreparedProjectRegistration>,
    ) -> HostResult<Value> {
        let prepared = result?;
        let registration = commit_project_registration(&self.runtime_store, prepared)
            .await
            .map_err(state_error)?;
        serde_json::to_value(registration).map_err(state_error)
    }
}

impl ServerActor {
    pub(super) async fn finish_project_registration(
        &mut self,
        client_id: u64,
        request_id: i64,
        result: HostResult<PreparedProjectRegistration>,
    ) {
        if self.require_auth(client_id).is_err() {
            return;
        }
        let result = ProjectRegistrationRequestHandler::new(
            self.runtime_store.clone(),
            self.deferred_request_scheduler(),
        )
        .commit_prepared(result)
        .await;
        match result {
            Ok(value) => {
                self.broadcast_project_state_changed();
                self.client_write(client_id, ok_response(request_id, value));
            }
            Err(error) => self.client_write(client_id, error_response(request_id, &error)),
        }
    }
}

fn state_error(error: impl std::fmt::Display) -> HostError {
    HostError::state(error.to_string())
}
