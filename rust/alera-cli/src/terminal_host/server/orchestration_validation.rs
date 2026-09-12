use alera_core::runtime::{
    OrchestrationMessagePriority, OrchestrationMessageType, ORCHESTRATION_SUBJECT_MAX_BYTES,
};
use serde_json::Value;

use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::protocol::ORCHESTRATION_MAX_WAIT_TIMEOUT_MS as MAX_WAIT_TIMEOUT_MS;

const DEFAULT_WAIT_TIMEOUT_MS: u64 = 120_000;

pub(super) fn optional_string(payload: &Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(super) fn require_string(payload: &Value, key: &str) -> HostResult<String> {
    optional_string(payload, key).ok_or_else(|| HostError::format(format!("{key} is required.")))
}

pub(super) fn parse_message_type(payload: &Value) -> HostResult<OrchestrationMessageType> {
    match payload.get("type").and_then(Value::as_str) {
        None => Ok(OrchestrationMessageType::Status),
        Some(raw) => OrchestrationMessageType::parse(raw)
            .ok_or_else(|| HostError::format(format!("unknown message type: {raw}"))),
    }
}

pub(super) fn parse_priority(payload: &Value) -> HostResult<OrchestrationMessagePriority> {
    match payload.get("priority").and_then(Value::as_str) {
        None => Ok(OrchestrationMessagePriority::Normal),
        Some(raw) => OrchestrationMessagePriority::parse(raw)
            .ok_or_else(|| HostError::format(format!("unknown message priority: {raw}"))),
    }
}

pub(super) fn parse_type_filter(payload: &Value) -> HostResult<Vec<OrchestrationMessageType>> {
    let Some(types) = payload.get("types") else {
        return Ok(Vec::new());
    };
    let Some(items) = types.as_array() else {
        return Err(HostError::format("types must be an array of strings."));
    };
    items
        .iter()
        .map(|item| {
            item.as_str()
                .and_then(OrchestrationMessageType::parse)
                .ok_or_else(|| HostError::format(format!("unknown message type: {item}")))
        })
        .collect()
}

pub(super) fn wait_timeout_ms(payload: &Value) -> u64 {
    payload
        .get("timeoutMs")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_WAIT_TIMEOUT_MS)
        .min(MAX_WAIT_TIMEOUT_MS)
}

pub(super) fn state_wait_timeout_ms(payload: &Value) -> u64 {
    payload
        .get("timeoutMs")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_WAIT_TIMEOUT_MS)
        .min(MAX_WAIT_TIMEOUT_MS)
}

pub(super) fn state_error(error: impl std::fmt::Display) -> HostError {
    HostError::state(error.to_string())
}

pub(super) fn prefixed_subject(prefix: &str, subject: &str) -> String {
    let mut end = subject
        .len()
        .min(ORCHESTRATION_SUBJECT_MAX_BYTES.saturating_sub(prefix.len()));
    while !subject.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    format!("{prefix}{}", &subject[..end])
}

pub(super) fn validate_result_schema(
    result: &serde_json::Map<String, Value>,
    schema_raw: Option<&str>,
) -> HostResult<()> {
    let Some(schema_raw) = schema_raw else {
        return Ok(());
    };
    let schema: Value = serde_json::from_str(schema_raw)
        .map_err(|error| HostError::state(format!("stored result schema is invalid: {error}")))?;
    let validator = jsonschema::validator_for(&schema)
        .map_err(|error| HostError::state(format!("stored result schema is invalid: {error}")))?;
    validator
        .validate(&Value::Object(result.clone()))
        .map_err(|error| HostError::format(format!("result does not match schema: {error}")))
}

pub(super) fn validate_result_schema_definition(schema_raw: &str) -> HostResult<()> {
    let schema: Value = serde_json::from_str(schema_raw)
        .map_err(|error| HostError::format(format!("result schema is invalid JSON: {error}")))?;
    jsonschema::validator_for(&schema)
        .map(|_| ())
        .map_err(|error| HostError::format(format!("result schema is invalid: {error}")))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn wait_timeouts_apply_the_shared_upper_bound() {
        let payload = serde_json::json!({"timeoutMs": 900_000});
        assert_eq!(state_wait_timeout_ms(&payload), MAX_WAIT_TIMEOUT_MS);
        assert_eq!(wait_timeout_ms(&payload), MAX_WAIT_TIMEOUT_MS);
        assert_eq!(
            state_wait_timeout_ms(&serde_json::json!({"timeoutMs": u64::MAX})),
            MAX_WAIT_TIMEOUT_MS
        );
        assert_eq!(
            state_wait_timeout_ms(&serde_json::json!({"timeoutMs": 300_000})),
            300_000
        );
        assert_eq!(
            state_wait_timeout_ms(&serde_json::json!({})),
            DEFAULT_WAIT_TIMEOUT_MS
        );
    }

    #[test]
    fn prefixed_subject_stays_within_the_storage_limit_on_utf8_boundaries() {
        let subject = "á".repeat(ORCHESTRATION_SUBJECT_MAX_BYTES);
        let prefixed = prefixed_subject("Re: ", &subject);
        assert!(prefixed.starts_with("Re: "));
        assert!(prefixed.len() <= ORCHESTRATION_SUBJECT_MAX_BYTES);
        assert!(prefixed.is_char_boundary(prefixed.len()));
    }

    #[test]
    fn result_schema_distinguishes_integer_from_fractional_number() {
        let schema = r#"{"properties":{"count":{"type":"integer"}}}"#;
        for value in [json!(1), json!(1.0)] {
            let result = json!({"count": value}).as_object().unwrap().clone();
            assert!(validate_result_schema(&result, Some(schema)).is_ok());
        }
        let fractional = json!({"count": 1.5}).as_object().unwrap().clone();
        assert!(validate_result_schema(&fractional, Some(schema)).is_err());
    }

    #[test]
    fn result_schema_enforces_nested_enum_items_and_additional_properties() {
        let schema = r#"{
            "type":"object",
            "required":["status","details"],
            "additionalProperties":false,
            "properties":{
                "status":{"enum":["ok"]},
                "details":{
                    "type":"object",
                    "required":["checks"],
                    "properties":{"checks":{"type":"array","items":{"type":"boolean"}}}
                }
            }
        }"#;
        let valid = json!({"status":"ok","details":{"checks":[true, false]}})
            .as_object()
            .unwrap()
            .clone();
        assert!(validate_result_schema(&valid, Some(schema)).is_ok());
        for invalid in [
            json!({"status":"bad","details":{"checks":[true]}}),
            json!({"status":"ok","details":{"checks":["yes"]}}),
            json!({"status":"ok","details":{"checks":[]},"extra":true}),
        ] {
            assert!(validate_result_schema(invalid.as_object().unwrap(), Some(schema)).is_err());
        }
    }
}
