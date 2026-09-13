# Terminal Host Observability Outcome Taxonomy

This document defines the envelope-level outcome class for terminal-host responses. The class is derived locally from `HostError` or the result passed to the response constructor and is intended for machine-readable tracing, not for wire clients.

## Compatibility boundary

The wire schema is unchanged. A success response remains `{id, ok: true, payload}` and an error response remains `{id, ok: false, error}`. Only `HostError::Conflict` continues to add the existing `errorCode` and `errorDetails` fields.

No `outcomeClass` field is added to any response. The classification is emitted through the `terminal host response built` tracing event with the fields `id`, `outcome_class`, and `error_code`. `error_code` is an empty string when the wire response has no `errorCode`.

## Classes

The canonical machine values are returned by `OutcomeClass::as_str()`.

| Class | Meaning | Current envelope representation |
| --- | --- | --- |
| `ok` | The host result is successful. | `ok: true` with `payload`; no `errorCode`. |
| `state_error` | An untyped host state failure, including legacy format failures. | `ok: false` with `error`; no `errorCode`. |
| `typed_conflict` | A typed conflict whose code is not a known backpressure code. | `ok: false` with `error`, `errorCode`, and `errorDetails`. |
| `backpressure` | Admission or terminal-input capacity rejected the request. | `ok: false` with `error`, `errorCode`, and `errorDetails`. |
| `unauthorized` | An authentication or access-policy failure represented by `HostError::Unauthorized`. | `ok: false` with the legacy untyped `error`; no new wire field. |
| `timeout` | A host operation timeout represented by `HostError::Timeout`. | `ok: false` with the legacy untyped `error`; no new wire field. |

`Unauthorized` and `Timeout` are dedicated `HostError` variants. Their constructors retain the producer's message verbatim, while `outcome_class()` maps the variants directly to the corresponding observability class. A `HostError::State` with the same text remains `state_error`, so ordinary state errors cannot be reclassified by message content.

`BACKPRESSURE_ERROR_CODES` currently contains `deferred_request_backpressure` and `terminal_input_backpressure`.

## Classification rules

Classification is evaluated in this order:

1. `OutcomeClass::from_result` returns `ok` for `Ok(_)`; this is how the success class is represented because no `HostError` value can represent success.
2. A `HostError::Conflict` whose code is in `BACKPRESSURE_ERROR_CODES` returns `backpressure`.
3. Any other `HostError::Conflict` returns `typed_conflict`, including unknown or newly introduced conflict codes. An unknown code must never be treated as backpressure.
4. `HostError::Unauthorized(_)` returns `unauthorized`.
5. `HostError::Timeout(_)` returns `timeout`.
6. All `HostError::State` values and every `HostError::Format` value return `state_error`.

`HostError::Format` is deliberately grouped into `state_error`. Its wire text still keeps the existing `FormatException: ` prefix, but the envelope has no separate format code and older clients already consume it as an untyped error. This preserves one stable class for all untyped error envelopes while retaining the exact wire message.

The current unauthorized message sources are `rust/alera-cli/src/terminal_host/server/client_delivery.rs:15`, `rust/alera-cli/src/terminal_host/server/client_delivery.rs:52`, `rust/alera-cli/src/terminal_host/server/automation_request_authorization.rs:14-19,26`, `rust/alera-cli/src/terminal_host/server/agent_profile_launch_requests.rs:55,238,245`, and `rust/alera-cli/src/terminal_host/server/requests.rs:208`.

The current timeout message sources are `rust/alera-cli/src/terminal_host/server/ai_assist_command_execution.rs:76`, `rust/alera-cli/src/terminal_host/server/codex_app_server.rs:169`, `rust/alera-cli/src/terminal_host/server/codex_server_startup.rs:39`, `rust/alera-cli/src/terminal_host/server/codex_dictation.rs:31,46,65`, and `rust/alera-cli/src/terminal_host/server/terminal_pulse_configuration.rs:326`.

The relay-only error `Relay authorization renewal requires an encrypted relay connection` is intentionally not classified as `unauthorized`: it describes an invalid connection mode rather than a rejected caller identity or access policy.

## Known production points

| Class | Producer or mapping point |
| --- | --- |
| `ok` | `rust/alera-cli/src/terminal_host/protocol.rs:337` (`ok_response`). |
| `state_error` | `rust/alera-cli/src/terminal_host/server/request_payloads.rs:18` maps ordinary operation failures to `HostError::State`; `rust/alera-cli/src/terminal_host/server/request_payloads.rs:9,20` are the parse and serialization `Format` sources that collapse to this class. |
| `typed_conflict` | Representative non-backpressure conflicts: `rust/alera-cli/src/terminal_host/server/requests/runtime_settings.rs:114`, `rust/alera-cli/src/terminal_host/server/project_clone_requests.rs:57`, `rust/alera-cli/src/terminal_host/server/workspace_sidebar_requests.rs:275`, and `rust/alera-cli/src/terminal_host/server/declared_catalog_requests.rs:266`. |
| `backpressure` | `rust/alera-cli/src/terminal_host/server/deferred_admission.rs:18,213`, `rust/alera-cli/src/terminal_host/server/deferred_admission/delayed.rs:76`, and `rust/alera-cli/src/terminal_host/server.rs:247` with `rust/alera-cli/src/terminal_host/session/input_queue.rs:68-70`. |
| `unauthorized` | `HostError::Unauthorized` constructions from the authentication and access-policy sources listed above. |
| `timeout` | `HostError::Timeout` constructions from the timeout sources listed above. |

The `request_payloads.rs` mapping is important to the taxonomy: ordinary operation errors are `State`, while JSON parsing and serialization failures are `Format`; both produce the untyped `state_error` class under the rules above.

## Response construction and tracing

`ok_response` and `error_response` in `rust/alera-cli/src/terminal_host/protocol.rs:337-356` are the response envelope construction points. `ok_response` records `outcome_class = "ok"` and an empty `error_code`. `error_response` calls `HostError::outcome_class()` and `HostError::error_code()` before returning the unchanged `wire_response` value.

The tracing event does not include payloads, error messages, details, credentials, or terminal content. For conflicts, `error_code` is the same code already present in the wire `errorCode` field. For untyped errors, `error_code` is empty.

## Envelope mapping examples

| Result | Class | Wire fields |
| --- | --- | --- |
| `Ok(payload)` | `ok` | `id`, `ok: true`, `payload` |
| `Err(HostError::State(_))` | `state_error` | `id`, `ok: false`, `error` |
| `Err(HostError::Unauthorized(_))` | `unauthorized` | `id`, `ok: false`, `error` |
| `Err(HostError::Timeout(_))` | `timeout` | `id`, `ok: false`, `error` |
| `Err(HostError::Format(_))` | `state_error` | `id`, `ok: false`, `error` with the existing `FormatException: ` text |
| `Err(HostError::Conflict { code, details, .. })` | `backpressure` or `typed_conflict` according to `code` | `id`, `ok: false`, `error`, `errorCode`, `errorDetails` |

The timeout returned by an orchestration wait can also be a successful domain result: `rust/alera-cli/src/terminal_host/server/orchestration_wait_requests.rs:138,368` places `outcome: "timeout"` inside a payload and then uses `ok_response`. That response is envelope class `ok`; the nested payload outcome is a separate domain result and is not rewritten by this taxonomy.

## Non-goals

This change does not add a wire field, bump a protocol version, change `errorCode` or `errorDetails` behavior, or alter client-side error handling. A future stable error code can be added without changing the envelope contract.
