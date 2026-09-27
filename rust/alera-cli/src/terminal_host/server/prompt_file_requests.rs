use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde_json::{json, Value};

use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::protocol::{error_response, ok_response};

use super::prompt_file_store::{
    PromptFileStore, MAX_PROMPT_FILE_BYTES, MAX_PROMPT_FILE_CHUNK_BYTES,
    MAX_PROMPT_FILE_STORE_BYTES,
};
use super::request_route_policy::MobilePromptFileOperation;
use super::requests::require_string_key;
use super::{ServerActor, ServerCommand};

const MAX_ENCODED_CHUNK_BYTES: usize = MAX_PROMPT_FILE_CHUNK_BYTES.div_ceil(3) * 4;
type UploadGate = Mutex<()>;
type UploadGateRegistry = Mutex<HashMap<String, Weak<UploadGate>>>;

static UPLOAD_GATES: OnceLock<UploadGateRegistry> = OnceLock::new();

impl ServerActor {
    pub(super) fn start_mobile_prompt_file_request(
        &self,
        client_id: u64,
        request_id: i64,
        operation: MobilePromptFileOperation,
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
                    handle_prompt_file_request(runtime_dir, operation, &payload)
                })
                .await
                .map_err(|error| HostError::state(format!("Prompt file operation failed: {error}")))
                .and_then(|result| result);
                let _ = inbox.send(ServerCommand::MobilePromptFileFinished {
                    client_id,
                    request_id,
                    operation,
                    upload_id,
                    result,
                });
            },
        )
    }

    pub(super) fn handle_mobile_prompt_file_finished(
        &mut self,
        client_id: u64,
        request_id: i64,
        operation: MobilePromptFileOperation,
        requested_upload_id: Option<&str>,
        result: HostResult<Value>,
    ) {
        if matches!(
            operation,
            MobilePromptFileOperation::Complete | MobilePromptFileOperation::Cancel
        ) {
            if let Some(upload_id) = requested_upload_id {
                self.remove_mobile_prompt_file_upload(client_id, upload_id);
            }
        }
        if !self.clients.contains_key(&client_id) {
            self.cleanup_orphaned_prompt_file_start(&result);
            return;
        }
        let response = match &result {
            Ok(value) => ok_response(request_id, value.clone()),
            Err(error) => error_response(request_id, error),
        };
        if !self.try_client_write(client_id, response) {
            self.cleanup_orphaned_prompt_file_start(&result);
            return;
        }
        if result.is_err() {
            return;
        }
        if operation == MobilePromptFileOperation::Start {
            if let Some(upload_id) = result
                .as_ref()
                .ok()
                .and_then(|value| value.get("uploadId"))
                .and_then(Value::as_str)
            {
                self.mobile_prompt_file_uploads
                    .entry(client_id)
                    .or_default()
                    .insert(upload_id.to_string());
            }
        }
    }

    pub(super) fn cancel_mobile_prompt_file_uploads(&mut self, client_id: u64) {
        let Some(upload_ids) = self.mobile_prompt_file_uploads.remove(&client_id) else {
            return;
        };
        self.schedule_prompt_file_cleanup(upload_ids.into_iter().collect());
    }

    fn cleanup_orphaned_prompt_file_start(&self, result: &HostResult<Value>) {
        let Some(upload_id) = result
            .as_ref()
            .ok()
            .and_then(|value| value.get("uploadId"))
            .and_then(Value::as_str)
        else {
            return;
        };
        self.schedule_prompt_file_cleanup(vec![upload_id.to_string()]);
    }

    fn schedule_prompt_file_cleanup(&self, upload_ids: Vec<String>) {
        let runtime_dir = self.runtime_dir.clone();
        if let Err(error) = self.deferred_admission.schedule(
            super::deferred_admission::DeferredRequestClass::Maintenance,
            "mobile.promptFile.cleanup",
            None,
            async move {
                let _ = tokio::task::spawn_blocking(move || {
                    for upload_id in upload_ids {
                        cancel_upload(&runtime_dir, &upload_id);
                    }
                })
                .await;
            },
        ) {
            tracing::warn!(
                "prompt file cleanup was not admitted: {}",
                error.wire_message()
            );
        }
    }

    fn remove_mobile_prompt_file_upload(&mut self, client_id: u64, upload_id: &str) {
        let Some(upload_ids) = self.mobile_prompt_file_uploads.get_mut(&client_id) else {
            return;
        };
        upload_ids.remove(upload_id);
        if upload_ids.is_empty() {
            self.mobile_prompt_file_uploads.remove(&client_id);
        }
    }

    pub(super) fn execute_mobile_prompt_file_operation(
        &self,
        operation: MobilePromptFileOperation,
        payload: &Value,
    ) -> HostResult<Value> {
        handle_prompt_file_request(self.runtime_dir.clone(), operation, payload)
    }
}

#[cfg(test)]
fn cancel_orphaned_start(runtime_dir: &std::path::Path, result: &HostResult<Value>) {
    let Some(upload_id) = result
        .as_ref()
        .ok()
        .and_then(|value| value.get("uploadId"))
        .and_then(Value::as_str)
    else {
        return;
    };
    cancel_upload(runtime_dir, upload_id);
}

fn cancel_upload(runtime_dir: &std::path::Path, upload_id: &str) {
    let store = PromptFileStore::in_runtime_dir(runtime_dir);
    if let Err(error) = with_upload_gate(upload_id, || {
        store.cancel(upload_id).map_err(prompt_file_error)
    }) {
        if !error.wire_message().contains("reservation is missing") {
            tracing::warn!(target: "prompt_file_store", "Could not cancel disconnected prompt file upload: {error}");
        }
    }
}

fn handle_prompt_file_request(
    runtime_dir: PathBuf,
    operation: MobilePromptFileOperation,
    payload: &Value,
) -> HostResult<Value> {
    let store = PromptFileStore::in_runtime_dir(&runtime_dir);
    match operation {
        MobilePromptFileOperation::Start => {
            let display_name = require_string_key(payload, "name")?;
            let declared_bytes = payload
                .get("sizeBytes")
                .and_then(Value::as_u64)
                .ok_or_else(|| HostError::state("Prompt file sizeBytes must be an integer."))?;
            let reservation = store
                .start(&display_name, declared_bytes)
                .map_err(prompt_file_error)?;
            Ok(json!({
                "uploadId": reservation.upload_id,
                "chunkBytes": reservation.chunk_bytes,
                "maxFileBytes": MAX_PROMPT_FILE_BYTES,
                "maxStoreBytes": MAX_PROMPT_FILE_STORE_BYTES,
            }))
        }
        MobilePromptFileOperation::Chunk => {
            let upload_id = require_string_key(payload, "uploadId")?;
            let offset = payload
                .get("offset")
                .and_then(Value::as_u64)
                .ok_or_else(|| HostError::state("Prompt file offset must be an integer."))?;
            let encoded = require_string_key(payload, "dataBase64")?;
            if encoded.len() > MAX_ENCODED_CHUNK_BYTES {
                return Err(HostError::state(
                    "Prompt file chunk exceeds the decoded limit.",
                ));
            }
            let bytes = STANDARD
                .decode(encoded.as_bytes())
                .map_err(|_| HostError::state("Prompt file chunk is not valid base64."))?;
            let next_offset = with_upload_gate(&upload_id, || {
                store
                    .append_chunk(&upload_id, offset, &bytes)
                    .map_err(prompt_file_error)
            })?;
            Ok(json!({"nextOffset": next_offset}))
        }
        MobilePromptFileOperation::Complete => {
            let upload_id = require_string_key(payload, "uploadId")?;
            let path = with_upload_gate(&upload_id, || {
                store.complete(&upload_id).map_err(prompt_file_error)
            })?;
            Ok(json!({"path": path, "uploadId": upload_id}))
        }
        MobilePromptFileOperation::Cancel => {
            let upload_id = require_string_key(payload, "uploadId")?;
            with_upload_gate(&upload_id, || {
                store.cancel(&upload_id).map_err(prompt_file_error)
            })?;
            Ok(json!({}))
        }
    }
}

fn prompt_file_error(error: super::prompt_file_store::PromptFileStoreError) -> HostError {
    HostError::state(error.to_string())
}

fn with_upload_gate<T>(
    upload_id: &str,
    operation: impl FnOnce() -> HostResult<T>,
) -> HostResult<T> {
    let gate = {
        let mut gates = upload_gates()
            .lock()
            .map_err(|error| HostError::state(format!("Prompt file gate failed: {error}")))?;
        gates.retain(|_, gate| gate.strong_count() > 0);
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
    let guard = gate
        .lock()
        .map_err(|error| HostError::state(format!("Prompt file gate failed: {error}")))?;
    let result = operation();
    drop(guard);
    remove_idle_upload_gate(upload_id, &gate);
    result
}

fn upload_gates() -> &'static UploadGateRegistry {
    UPLOAD_GATES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn remove_idle_upload_gate(upload_id: &str, gate: &Arc<UploadGate>) {
    let Ok(mut gates) = upload_gates().lock() else {
        return;
    };
    if Arc::strong_count(gate) == 1
        && gates
            .get(upload_id)
            .is_some_and(|registered| Weak::ptr_eq(registered, &Arc::downgrade(gate)))
    {
        gates.remove(upload_id);
    }
}

#[cfg(test)]
#[path = "prompt_file_requests_tests.rs"]
mod tests;
