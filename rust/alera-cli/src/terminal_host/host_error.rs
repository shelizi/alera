use std::fmt;

use serde_json::{json, Value};

/// Stable classification for a terminal-host response outcome.
///
/// The class is an observability value only. It is not serialized into the
/// wire envelope, so adding or refining a class does not change protocol
/// compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutcomeClass {
    Ok,
    StateError,
    TypedConflict,
    Backpressure,
    Unauthorized,
    Timeout,
}

impl OutcomeClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            OutcomeClass::Ok => "ok",
            OutcomeClass::StateError => "state_error",
            OutcomeClass::TypedConflict => "typed_conflict",
            OutcomeClass::Backpressure => "backpressure",
            OutcomeClass::Unauthorized => "unauthorized",
            OutcomeClass::Timeout => "timeout",
        }
    }
}

impl fmt::Display for OutcomeClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error codes whose conflicts represent admission backpressure rather than
/// a domain or optimistic-concurrency conflict.
pub const BACKPRESSURE_ERROR_CODES: &[&str] = &[
    "deferred_request_backpressure",
    "terminal_input_backpressure",
];

/// Errors surfaced to clients over the wire. The untyped variants render their
/// messages as-is because some client recovery paths pattern-match exact text,
/// and [`HostError::Format`] renders like Dart's `FormatException`.
#[derive(Debug, Clone)]
pub enum HostError {
    /// The message is sent verbatim. Some of these strings are pattern-matched
    /// by the app client, so keep them exact.
    State(String),
    /// An authentication failure rendered as the legacy untyped error.
    Unauthorized(String),
    /// A timeout rendered as the legacy untyped error.
    Timeout(String),
    /// Equivalent to Dart's `FormatException`; rendered as `FormatException: <msg>`.
    Format(String),
    /// A typed, recoverable conflict. The message remains available for older
    /// clients while newer clients inspect the additive code and details.
    Conflict {
        code: String,
        message: String,
        details: Value,
    },
}

impl HostError {
    pub fn state(message: impl Into<String>) -> Self {
        HostError::State(message.into())
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        HostError::Unauthorized(message.into())
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        HostError::Timeout(message.into())
    }

    pub fn format(message: impl Into<String>) -> Self {
        HostError::Format(message.into())
    }

    pub fn conflict(code: impl Into<String>, message: impl Into<String>, details: Value) -> Self {
        HostError::Conflict {
            code: code.into(),
            message: message.into(),
            details,
        }
    }

    /// Classify this error without changing its legacy wire representation.
    pub fn outcome_class(&self) -> OutcomeClass {
        match self {
            HostError::Unauthorized(_) => OutcomeClass::Unauthorized,
            HostError::Timeout(_) => OutcomeClass::Timeout,
            HostError::State(_) | HostError::Format(_) => OutcomeClass::StateError,
            HostError::Conflict { code, .. }
                if BACKPRESSURE_ERROR_CODES.contains(&code.as_str()) =>
            {
                OutcomeClass::Backpressure
            }
            HostError::Conflict { .. } => OutcomeClass::TypedConflict,
        }
    }

    /// Return the additive error code carried by a typed conflict, if any.
    pub fn error_code(&self) -> Option<&str> {
        match self {
            HostError::Conflict { code, .. } => Some(code),
            HostError::State(_)
            | HostError::Unauthorized(_)
            | HostError::Timeout(_)
            | HostError::Format(_) => None,
        }
    }

    /// The string placed in the `error` field of an error response.
    pub fn wire_message(&self) -> String {
        match self {
            HostError::State(message)
            | HostError::Unauthorized(message)
            | HostError::Timeout(message) => message.clone(),
            HostError::Format(message) => format!("FormatException: {message}"),
            HostError::Conflict { message, .. } => message.clone(),
        }
    }

    pub fn wire_response(&self, id: i64) -> Value {
        let mut response = json!({ "id": id, "ok": false, "error": self.wire_message() });
        if let HostError::Conflict { code, details, .. } = self {
            response["errorCode"] = json!(code);
            response["errorDetails"] = details.clone();
        }
        response
    }
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.wire_message())
    }
}

impl std::error::Error for HostError {}

pub type HostResult<T> = Result<T, HostError>;

impl OutcomeClass {
    /// Classify a host result, including the success class that has no
    /// `HostError` variant.
    #[cfg(test)]
    pub fn from_result<T>(result: &HostResult<T>) -> Self {
        match result {
            Ok(_) => OutcomeClass::Ok,
            Err(error) => error.outcome_class(),
        }
    }
}

#[cfg(test)]
mod outcome_taxonomy_tests {
    use super::*;

    #[test]
    fn outcome_class_names_are_stable() {
        assert_eq!(OutcomeClass::Ok.as_str(), "ok");
        assert_eq!(OutcomeClass::StateError.as_str(), "state_error");
        assert_eq!(OutcomeClass::TypedConflict.as_str(), "typed_conflict");
        assert_eq!(OutcomeClass::Backpressure.as_str(), "backpressure");
        assert_eq!(OutcomeClass::Unauthorized.as_str(), "unauthorized");
        assert_eq!(OutcomeClass::Timeout.as_str(), "timeout");
    }

    #[test]
    fn state_and_format_errors_are_state_errors() {
        assert_eq!(
            HostError::state("workspace is unavailable").outcome_class(),
            OutcomeClass::StateError
        );
        assert_eq!(
            HostError::format("payload is invalid").outcome_class(),
            OutcomeClass::StateError
        );
    }

    #[test]
    fn typed_conflicts_are_not_backpressure_without_a_known_code() {
        let error = HostError::conflict(
            "workspace_revision_conflict",
            "refresh and retry",
            json!({"expected": 1}),
        );

        assert_eq!(error.outcome_class(), OutcomeClass::TypedConflict);
        assert_eq!(error.error_code(), Some("workspace_revision_conflict"));
    }

    #[test]
    fn both_known_backpressure_codes_are_backpressure() {
        for code in BACKPRESSURE_ERROR_CODES {
            let error = HostError::conflict(*code, "busy", Value::Null);
            assert_eq!(error.outcome_class(), OutcomeClass::Backpressure);
        }
    }

    #[test]
    fn dedicated_variants_preserve_legacy_wire_shape_and_message() {
        let cases = [
            (
                HostError::unauthorized("Terminal host client is not authenticated."),
                OutcomeClass::Unauthorized,
                "Terminal host client is not authenticated.",
            ),
            (
                HostError::timeout("AI Assist timed out after 30s."),
                OutcomeClass::Timeout,
                "AI Assist timed out after 30s.",
            ),
        ];
        for (error, expected_class, expected_message) in cases {
            assert_eq!(error.outcome_class(), expected_class);
            assert_eq!(error.wire_message(), expected_message);
            assert_eq!(error.error_code(), None);
            assert_eq!(error.to_string(), expected_message);

            let response = error.wire_response(7);
            assert_eq!(response["id"], 7);
            assert_eq!(response["ok"], false);
            assert_eq!(response["error"], expected_message);
            let object = response.as_object().unwrap();
            assert!(!object.contains_key("errorCode"));
            assert!(!object.contains_key("errorDetails"));
        }
    }

    #[test]
    fn state_messages_resembling_dedicated_variants_remain_state_errors() {
        for message in [
            "Terminal host client is not authenticated.",
            "Mobile clients cannot call terminal host request: workspace.list",
            "AI Assist timed out after 30s.",
            "Codex app-server request timed out: thread/start",
            "Codex app-server startup timed out.",
        ] {
            assert_eq!(
                HostError::state(message).outcome_class(),
                OutcomeClass::StateError
            );
        }
    }

    #[test]
    fn result_classification_exposes_ok() {
        let result: HostResult<Value> = Ok(Value::Null);
        assert_eq!(OutcomeClass::from_result(&result), OutcomeClass::Ok);

        let result: HostResult<Value> = Err(HostError::state("failed"));
        assert_eq!(OutcomeClass::from_result(&result), OutcomeClass::StateError);
    }
}
