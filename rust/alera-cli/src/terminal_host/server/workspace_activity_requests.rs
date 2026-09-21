use alera_core::runtime::RuntimeStore;
use serde_json::{json, Value};

use crate::terminal_host::host_error::{HostError, HostResult};

pub(super) struct WorkspaceActivityRequestHandler<'a> {
    runtime_store: &'a RuntimeStore,
}

impl<'a> WorkspaceActivityRequestHandler<'a> {
    pub(super) const fn new(runtime_store: &'a RuntimeStore) -> Self {
        Self { runtime_store }
    }

    pub(super) async fn list(&self) -> HostResult<Value> {
        serde_json::to_value(
            self.runtime_store
                .list_workspace_activity()
                .await
                .map_err(state_error)?,
        )
        .map_err(state_error)
    }

    pub(super) async fn upsert_all(&self, payload: &Value) -> HostResult<Value> {
        let entries = serde_json::from_value(payload.clone()).map_err(format_error)?;
        let value = self
            .runtime_store
            .record_workspace_activity_batch(entries)
            .await
            .map_err(state_error)?;
        serde_json::to_value(value).map_err(state_error)
    }

    pub(super) async fn remove(&self, payload: &Value) -> HostResult<Value> {
        let workspace_id = required(payload, "workspaceId")?;
        self.runtime_store
            .remove_workspace_activity(workspace_id)
            .await
            .map_err(state_error)?;
        Ok(json!({}))
    }
}

fn required<'a>(payload: &'a Value, key: &str) -> HostResult<&'a str> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| HostError::format(format!("{key} is required.")))
}

fn format_error(error: impl std::fmt::Display) -> HostError {
    HostError::format(error.to_string())
}

fn state_error(error: impl std::fmt::Display) -> HostError {
    HostError::state(error.to_string())
}

#[cfg(test)]
mod tests {
    use alera_core::runtime::RuntimeStore;
    use chrono::{Duration, SecondsFormat, Utc};
    use serde_json::json;

    use super::WorkspaceActivityRequestHandler;

    #[tokio::test]
    async fn activity_crud_can_be_tested_without_server_actor() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        let handler = WorkspaceActivityRequestHandler::new(&store);
        let newer = Utc::now();
        let older = newer - Duration::minutes(1);

        handler
            .upsert_all(&json!({"workspace": older}))
            .await
            .unwrap();
        handler
            .upsert_all(&json!({"workspace": newer}))
            .await
            .unwrap();
        let listed = handler.list().await.unwrap();
        assert_eq!(
            listed["workspace"].as_str().unwrap(),
            newer.to_rfc3339_opts(SecondsFormat::Millis, true)
        );

        handler
            .remove(&json!({"workspaceId": "workspace"}))
            .await
            .unwrap();
        assert_eq!(handler.list().await.unwrap(), json!({}));
    }
}
