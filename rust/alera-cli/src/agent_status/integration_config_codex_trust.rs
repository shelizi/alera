use std::collections::BTreeMap;

use serde_json::json;
use sha2::{Digest, Sha256};

pub(super) fn codex_trusted_hash(event_label: &str, command: &str) -> String {
    let identity = BTreeMap::from([
        ("event_name", json!(event_label)),
        (
            "hooks",
            json!([{ "async": false, "command": command, "timeout": 600, "type": "command" }]),
        ),
    ]);
    let serialized = serde_json::to_string(&identity).expect("serializable trust identity");
    format!(
        "sha256:{}",
        hex::encode(Sha256::digest(serialized.as_bytes()))
    )
}
