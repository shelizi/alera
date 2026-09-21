use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde_json::{json, Value};

use crate::terminal_host::host_error::{HostError, HostResult};

use super::prompt_image_store::{
    PromptImageStore, MAX_PROMPT_IMAGE_CHUNK_BYTES, MAX_PROMPT_IMAGE_STORE_BYTES,
};
use super::request_route_policy::MobilePromptImageOperation;
use super::requests::require_string_key;
use super::{ServerActor, ServerCommand};

const MAX_ENCODED_CHUNK_BYTES: usize = MAX_PROMPT_IMAGE_CHUNK_BYTES.div_ceil(3) * 4;
type UploadGate = Mutex<()>;
type UploadGateRegistry = Mutex<HashMap<String, Weak<UploadGate>>>;

static UPLOAD_GATES: OnceLock<UploadGateRegistry> = OnceLock::new();

impl ServerActor {
    pub(super) fn start_mobile_prompt_image_request(
        &self,
        client_id: u64,
        request_id: i64,
        operation: MobilePromptImageOperation,
        request_type: &str,
        payload: &Value,
    ) -> HostResult<()> {
        let runtime_dir = self.runtime_dir.clone();
        let upload_id = payload
            .get("uploadId")
            .and_then(Value::as_str)
            .map(str::to_string);
        let payload = payload.clone();
        let inbox = self.inbox.clone();
        self.deferred_admission.schedule(
            super::deferred_admission::DeferredRequestClass::Bulk,
            request_type,
            Some(client_id),
            async move {
                let result = tokio::task::spawn_blocking(move || {
                    handle_prompt_image_request(runtime_dir, operation, &payload)
                })
                .await
                .map_err(|error| {
                    HostError::state(format!("Prompt image operation failed: {error}"))
                })
                .and_then(|result| result);
                let _ = inbox.send(ServerCommand::MobilePromptImageFinished {
                    client_id,
                    request_id,
                    operation,
                    upload_id,
                    result,
                });
            },
        )
    }

    pub(super) fn handle_mobile_prompt_image_finished(
        &mut self,
        client_id: u64,
        request_id: i64,
        operation: MobilePromptImageOperation,
        requested_upload_id: Option<&str>,
        result: HostResult<Value>,
    ) {
        if matches!(
            operation,
            MobilePromptImageOperation::Complete | MobilePromptImageOperation::Cancel
        ) {
            if let Some(upload_id) = requested_upload_id {
                self.remove_mobile_prompt_image_upload(client_id, upload_id);
            }
        }
        if !self.clients.contains_key(&client_id) {
            self.cleanup_orphaned_prompt_image_start(operation, &result);
            return;
        }
        let response = match &result {
            Ok(value) => crate::terminal_host::protocol::ok_response(request_id, value.clone()),
            Err(error) => crate::terminal_host::protocol::error_response(request_id, error),
        };
        if !self.try_client_write(client_id, response) {
            self.cleanup_orphaned_prompt_image_start(operation, &result);
            return;
        }
        if result.is_err() || operation != MobilePromptImageOperation::Start {
            return;
        }
        if let Some(upload_id) = result
            .as_ref()
            .ok()
            .and_then(|value| value.get("uploadId"))
            .and_then(Value::as_str)
        {
            self.mobile_prompt_image_uploads
                .entry(client_id)
                .or_default()
                .insert(upload_id.to_string());
        }
    }

    pub(super) fn cancel_mobile_prompt_image_uploads(&mut self, client_id: u64) {
        let Some(upload_ids) = self.mobile_prompt_image_uploads.remove(&client_id) else {
            return;
        };
        self.schedule_prompt_image_cleanup(upload_ids.into_iter().collect());
    }

    fn cleanup_orphaned_prompt_image_start(
        &self,
        operation: MobilePromptImageOperation,
        result: &HostResult<Value>,
    ) {
        if operation != MobilePromptImageOperation::Start {
            return;
        }
        let Some(upload_id) = result
            .as_ref()
            .ok()
            .and_then(|value| value.get("uploadId"))
            .and_then(Value::as_str)
        else {
            return;
        };
        self.schedule_prompt_image_cleanup(vec![upload_id.to_string()]);
    }

    fn schedule_prompt_image_cleanup(&self, upload_ids: Vec<String>) {
        let runtime_dir = self.runtime_dir.clone();
        if let Err(error) = self.deferred_admission.schedule(
            super::deferred_admission::DeferredRequestClass::Maintenance,
            "mobile.promptImage.cleanup",
            None,
            async move {
                let _ = tokio::task::spawn_blocking(move || {
                    let store = PromptImageStore::in_runtime_dir(&runtime_dir);
                    for upload_id in upload_ids {
                        let _ = with_upload_gate(&upload_id, || {
                            store.cancel(&upload_id).map_err(prompt_image_error)
                        });
                    }
                })
                .await;
            },
        ) {
            tracing::warn!(
                "prompt image cleanup was not admitted: {}",
                error.wire_message()
            );
        }
    }

    fn remove_mobile_prompt_image_upload(&mut self, client_id: u64, upload_id: &str) {
        let Some(upload_ids) = self.mobile_prompt_image_uploads.get_mut(&client_id) else {
            return;
        };
        upload_ids.remove(upload_id);
        if upload_ids.is_empty() {
            self.mobile_prompt_image_uploads.remove(&client_id);
        }
    }

    pub(super) fn execute_mobile_prompt_image_operation(
        &self,
        operation: MobilePromptImageOperation,
        payload: &Value,
    ) -> HostResult<Value> {
        handle_prompt_image_request(self.runtime_dir.clone(), operation, payload)
    }
}

pub(super) fn handle_prompt_image_request(
    runtime_dir: PathBuf,
    operation: MobilePromptImageOperation,
    payload: &Value,
) -> HostResult<Value> {
    let store = PromptImageStore::in_runtime_dir(&runtime_dir);
    match operation {
        MobilePromptImageOperation::Start => {
            let format = require_string_key(payload, "format")?;
            let declared_bytes = payload
                .get("sizeBytes")
                .and_then(Value::as_u64)
                .or_else(|| payload.get("length").and_then(Value::as_u64))
                .ok_or_else(|| HostError::state("Prompt image sizeBytes must be an integer."))?;
            let reservation = store
                .start(&format, declared_bytes)
                .map_err(prompt_image_error)?;
            Ok(json!({
                "uploadId": reservation.upload_id,
                "chunkBytes": reservation.chunk_bytes,
                "maxFileBytes": super::prompt_image_store::MAX_PROMPT_IMAGE_BYTES,
                "maxStoreBytes": MAX_PROMPT_IMAGE_STORE_BYTES,
            }))
        }
        MobilePromptImageOperation::Chunk => {
            let upload_id = require_string_key(payload, "uploadId")?;
            let offset = payload
                .get("offset")
                .and_then(Value::as_u64)
                .ok_or_else(|| HostError::state("Prompt image offset must be an integer."))?;
            let encoded = require_string_key(payload, "dataBase64")?;
            if encoded.len() > MAX_ENCODED_CHUNK_BYTES {
                return Err(HostError::state(format!(
                    "Prompt image chunk exceeds the {MAX_PROMPT_IMAGE_CHUNK_BYTES}-byte decoded limit."
                )));
            }
            let bytes = STANDARD
                .decode(encoded.as_bytes())
                .map_err(|_| HostError::state("Prompt image chunk is not valid base64."))?;
            let next_offset = with_upload_gate(&upload_id, || {
                store
                    .append_chunk(&upload_id, offset, &bytes)
                    .map_err(prompt_image_error)
            })?;
            Ok(json!({"nextOffset": next_offset}))
        }
        MobilePromptImageOperation::Complete => {
            let upload_id = require_string_key(payload, "uploadId")?;
            let path = with_upload_gate(&upload_id, || {
                store.complete(&upload_id).map_err(prompt_image_error)
            })?;
            Ok(json!({"path": path}))
        }
        MobilePromptImageOperation::Cancel => {
            let upload_id = require_string_key(payload, "uploadId")?;
            with_upload_gate(&upload_id, || {
                store.cancel(&upload_id).map_err(prompt_image_error)
            })?;
            Ok(json!({}))
        }
    }
}

fn with_upload_gate<T>(
    upload_id: &str,
    operation: impl FnOnce() -> HostResult<T>,
) -> HostResult<T> {
    let gate = {
        let mut gates = upload_gates()
            .lock()
            .map_err(|error| HostError::state(format!("Prompt image gate failed: {error}")))?;
        gates.retain(|_, weak| weak.strong_count() > 0);
        gates
            .entry(upload_id.to_string())
            .or_insert_with(Weak::new)
            .upgrade()
            .unwrap_or_else(|| {
                let gate = Arc::new(Mutex::new(()));
                gates.insert(upload_id.to_string(), Arc::downgrade(&gate));
                gate
            })
    };
    let _guard = gate
        .lock()
        .map_err(|error| HostError::state(format!("Prompt image gate failed: {error}")))?;
    operation()
}

fn upload_gates() -> &'static UploadGateRegistry {
    UPLOAD_GATES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn prompt_image_error(error: super::prompt_image_store::PromptImageStoreError) -> HostError {
    HostError::state(error.to_string())
}
