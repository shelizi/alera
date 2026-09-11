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
use super::requests::require_string_key;
use super::ServerActor;

const MAX_ENCODED_CHUNK_BYTES: usize = MAX_PROMPT_IMAGE_CHUNK_BYTES.div_ceil(3) * 4;
type UploadGate = Mutex<()>;
type UploadGateRegistry = Mutex<HashMap<String, Weak<UploadGate>>>;

static UPLOAD_GATES: OnceLock<UploadGateRegistry> = OnceLock::new();

impl ServerActor {
    pub(super) fn start_mobile_prompt_image_upload(&self, payload: &Value) -> HostResult<Value> {
        handle_prompt_image_request(
            self.runtime_dir.clone(),
            "mobile.promptImage.start",
            payload,
        )
    }

    pub(super) fn append_mobile_prompt_image_chunk(&self, payload: &Value) -> HostResult<Value> {
        handle_prompt_image_request(
            self.runtime_dir.clone(),
            "mobile.promptImage.chunk",
            payload,
        )
    }

    pub(super) fn complete_mobile_prompt_image_upload(&self, payload: &Value) -> HostResult<Value> {
        handle_prompt_image_request(
            self.runtime_dir.clone(),
            "mobile.promptImage.complete",
            payload,
        )
    }

    pub(super) fn cancel_mobile_prompt_image_upload(&self, payload: &Value) -> HostResult<Value> {
        handle_prompt_image_request(
            self.runtime_dir.clone(),
            "mobile.promptImage.cancel",
            payload,
        )
    }
}

pub(super) fn handle_prompt_image_request(
    runtime_dir: PathBuf,
    request_type: &str,
    payload: &Value,
) -> HostResult<Value> {
    let store = PromptImageStore::in_runtime_dir(&runtime_dir);
    match request_type {
        "mobile.promptImage.start" => {
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
        "mobile.promptImage.chunk" => {
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
        "mobile.promptImage.complete" => {
            let upload_id = require_string_key(payload, "uploadId")?;
            let path = with_upload_gate(&upload_id, || {
                store.complete(&upload_id).map_err(prompt_image_error)
            })?;
            Ok(json!({"path": path}))
        }
        "mobile.promptImage.cancel" => {
            let upload_id = require_string_key(payload, "uploadId")?;
            with_upload_gate(&upload_id, || {
                store.cancel(&upload_id).map_err(prompt_image_error)
            })?;
            Ok(json!({}))
        }
        _ => Err(HostError::state("Unsupported prompt image operation.")),
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
