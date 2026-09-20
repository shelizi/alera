use alera_core::runtime::RuntimeStore;
use serde_json::{json, Value};

use super::requests::require_string_key;
use crate::terminal_host::host_error::{HostError, HostResult};

pub(super) struct RuntimeMetadataRequestHandler<'a> {
    runtime_store: &'a RuntimeStore,
}

impl<'a> RuntimeMetadataRequestHandler<'a> {
    pub(super) const fn new(runtime_store: &'a RuntimeStore) -> Self {
        Self { runtime_store }
    }

    pub(super) async fn get(&self, payload: &Value) -> HostResult<Value> {
        let key = require_string_key(payload, "key")?;
        serde_json::to_value(
            self.runtime_store
                .get_metadata(&key)
                .await
                .map_err(state_error)?,
        )
        .map_err(state_error)
    }

    pub(super) async fn set(&self, payload: &Value) -> HostResult<Value> {
        let key = require_string_key(payload, "key")?;
        let value = require_string_key(payload, "value")?;
        self.runtime_store
            .set_metadata(&key, &value)
            .await
            .map_err(state_error)?;
        Ok(json!({}))
    }
}

fn state_error(error: impl std::fmt::Display) -> HostError {
    HostError::state(error.to_string())
}

#[cfg(test)]
mod tests {
    use alera_core::runtime::RuntimeStore;
    use serde_json::json;

    use super::RuntimeMetadataRequestHandler;

    #[tokio::test]
    async fn metadata_round_trip_can_be_tested_without_server_actor() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        let handler = RuntimeMetadataRequestHandler::new(&store);

        handler
            .set(&json!({"key": "sample", "value": "value"}))
            .await
            .unwrap();
        let value = handler.get(&json!({"key": "sample"})).await.unwrap();

        assert_eq!(value, json!("value"));
    }
}
