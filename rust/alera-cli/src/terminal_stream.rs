//! Native GPUI terminal stream over authenticated, long-lived CLI ↔ Host.
//!
//! The CLI sidecar owns the Host token. GPUI exchanges a narrowly allowlisted
//! NDJSON protocol on private stdin/stdout, never the Host token.

use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::runtime_host_client::RuntimeHostRpcClient;

const MAX_REQUEST_LINE: usize = 16 * 1024;
const MAX_PENDING: usize = 32;
const MAX_INPUT_BYTES: usize = 4 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamCommandKind {
    Observe,
    Recover,
    Write,
}

struct StreamCommand {
    request_id: u64,
    host_verb: &'static str,
    session_id: String,
    payload: Value,
    kind: StreamCommandKind,
}

struct PendingRequest {
    native_id: u64,
    session_id: String,
    kind: StreamCommandKind,
}

#[derive(Default)]
struct StreamSessions {
    pending_observe: HashSet<String>,
    active: HashSet<String>,
}

impl StreamSessions {
    fn pending(&mut self, id: &str) {
        self.pending_observe.insert(id.to_owned());
    }

    fn accept(&mut self, id: &str) {
        self.pending_observe.remove(id);
        self.active.insert(id.to_owned());
    }

    fn remove(&mut self, id: &str) {
        self.pending_observe.remove(id);
        self.active.remove(id);
    }

    fn forward(&self, message: &Value) -> bool {
        let event = message.get("event").and_then(Value::as_str);
        if !matches!(
            event,
            Some(
                "output"
                    | "outputResyncRequired"
                    | "exit"
                    | "error"
                    | "terminalDriverChanged"
                    | "terminalPulseChanged"
            )
        ) {
            return false;
        }
        let id = message
            .get("payload")
            .and_then(|payload| payload.get("sessionId"))
            .and_then(Value::as_str);
        id.is_some_and(|id| self.active.contains(id) || self.pending_observe.contains(id))
    }
}

/// The authenticated Host broadcasts richer Agent status events containing
/// prompts, tool input and assistant messages. The private GPUI sidecar must
/// never forward those strings, or any arbitrary Host event, across stdio.
/// A malformed/oversized batch is converted to a snapshot invalidation.
fn sanitize_agent_presence_event(message: &Value) -> Option<Value> {
    if message.get("event").and_then(Value::as_str) != Some("agentPresenceChanged") {
        return None;
    }
    let Some(changes) = message
        .pointer("/payload/changes")
        .and_then(Value::as_array)
    else {
        return Some(json!({"event": "agentPresenceChanged", "payload": {}}));
    };
    if changes.len() > 128 {
        return Some(json!({"event": "agentPresenceChanged", "payload": {}}));
    }
    let mut allowed = Vec::with_capacity(changes.len());
    for change in changes {
        // Real Host PTY shutdowns emit only sessionId + removed:true.
        // Without the full workspace/tab identity, never forward a deletion
        // that could target a recycled handle; request an authenticated
        // terminal list instead.
        if change.get("removed") == Some(&Value::Bool(true))
            && (change.get("workspaceId").and_then(Value::as_str).is_none()
                || change.get("tabId").and_then(Value::as_str).is_none())
        {
            return Some(json!({"event": "agentPresenceChanged", "payload": {}}));
        }
        let (Some(session_id), Some(workspace_id), Some(tab_id)) = (
            change
                .get("terminalSessionId")
                .and_then(Value::as_str)
                .filter(|id| valid_identity(id, 128)),
            change
                .get("workspaceId")
                .and_then(Value::as_str)
                .filter(|id| valid_identity(id, 256)),
            change
                .get("tabId")
                .and_then(Value::as_str)
                .filter(|id| valid_identity(id, 256)),
        ) else {
            continue;
        };
        if change.get("removed") == Some(&Value::Bool(true)) {
            allowed.push(json!({
                "terminalSessionId": session_id, "workspaceId": workspace_id,
                "tabId": tab_id, "removed": true,
            }));
            continue;
        }
        let Some(state) = change
            .get("state")
            .and_then(Value::as_str)
            .filter(|state| matches!(*state, "working" | "waiting" | "blocked" | "done"))
        else {
            continue;
        };
        let Some(agent_type) = change
            .get("agentType")
            .and_then(Value::as_str)
            .filter(|type_name| valid_identity(type_name, 64))
        else {
            continue;
        };
        let timestamps = ["stateStartedAt", "updatedAt"].map(|field| {
            change.get(field).and_then(Value::as_str).filter(|value| {
                value.len() <= 64 && chrono::DateTime::parse_from_rfc3339(value).is_ok()
            })
        });
        let [Some(started_at), Some(updated_at)] = timestamps else {
            continue;
        };
        allowed.push(json!({
            "terminalSessionId": session_id, "workspaceId": workspace_id,
            "tabId": tab_id, "agentType": agent_type, "state": state,
            "stateStartedAt": started_at, "updatedAt": updated_at,
            "interrupted": change.get("interrupted").and_then(Value::as_bool),
        }));
    }
    Some(json!({"event": "agentPresenceChanged", "payload": {"changes": allowed}}))
}

fn valid_identity(value: &str, limit: usize) -> bool {
    !value.is_empty()
        && value.len() <= limit
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn field<'a>(payload: &'a Value, key: &str, limit: usize) -> Result<&'a str> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| valid_identity(value, limit))
        .ok_or_else(|| anyhow!("invalid scoped Terminal stream {key}"))
}

fn parse_command(value: &Value) -> Result<StreamCommand> {
    let request_id = value
        .get("id")
        .and_then(Value::as_u64)
        .filter(|id| *id > 0)
        .ok_or_else(|| anyhow!("Terminal stream request requires positive id"))?;
    let name = value
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Terminal stream request type is missing"))?;
    let payload = value
        .get("payload")
        .filter(|payload| payload.is_object())
        .ok_or_else(|| anyhow!("Terminal stream request payload must be an object"))?;
    let session_id = field(payload, "sessionId", 128)?.to_owned();
    let (kind, host_verb) = match name {
        "observe" => {
            field(payload, "workspaceId", 256)?;
            field(payload, "tabId", 256)?;
            field(payload, "viewLeaseId", 128)?;
            if payload
                .get("resumeCursor")
                .is_some_and(|value| !value.is_u64() && !value.is_null())
            {
                return Err(anyhow!("invalid resume cursor"));
            }
            (StreamCommandKind::Observe, "terminal.observeExisting")
        }
        "recover" => {
            field(payload, "workspaceId", 256)?;
            field(payload, "tabId", 256)?;
            field(payload, "viewLeaseId", 128)?;
            (StreamCommandKind::Recover, "terminal.recoverExistingWorker")
        }
        "write" => {
            field(payload, "workspaceId", 256)?;
            field(payload, "tabId", 256)?;
            field(payload, "viewLeaseId", 128)?;
            let encoded = payload
                .get("dataBase64")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("terminal input bytes missing"))?;
            let bytes = STANDARD
                .decode(encoded)
                .context("terminal input is not valid base64")?;
            if bytes.is_empty() || bytes.len() > MAX_INPUT_BYTES {
                return Err(anyhow!("terminal input must be between 1 and 4096 bytes"));
            }
            (StreamCommandKind::Write, "terminal.writeObserved")
        }
        _ => return Err(anyhow!("terminal stream command not allowed")),
    };
    // Project only the narrowly authorized fields into the Host request.
    // Do not forward arbitrary Native JSON flags such as deferredEnter,
    // bracketedPaste, launch, or administrative mutation options.
    let sanitized_payload = match kind {
        StreamCommandKind::Observe => json!({
            "sessionId": session_id,
            "workspaceId": field(payload, "workspaceId", 256)?,
            "tabId": field(payload, "tabId", 256)?,
            "viewLeaseId": field(payload, "viewLeaseId", 128)?,
            "resumeCursor": payload.get("resumeCursor").cloned().unwrap_or(Value::Null),
        }),
        StreamCommandKind::Recover => json!({
            "sessionId": session_id,
            "workspaceId": field(payload, "workspaceId", 256)?,
            "tabId": field(payload, "tabId", 256)?,
        }),
        StreamCommandKind::Write => json!({
            "sessionId": session_id,
            "workspaceId": field(payload, "workspaceId", 256)?,
            "tabId": field(payload, "tabId", 256)?,
            "viewLeaseId": field(payload, "viewLeaseId", 128)?,
            "dataBase64": payload.get("dataBase64"),
            "deferredEnter": false,
            "bracketedPaste": false,
        }),
    };
    Ok(StreamCommand {
        request_id,
        host_verb,
        session_id,
        payload: sanitized_payload,
        kind,
    })
}

pub(crate) async fn run(client: RuntimeHostRpcClient) -> i32 {
    match run_stream(client).await {
        Ok(()) => 0,
        Err(error) => {
            // Never echo Host control-file contents, request data, or PTY
            // frames to stderr. Only the error class crosses diagnostics.
            eprintln!(
                "Native terminal event stream stopped: {}",
                error.root_cause()
            );
            1
        }
    }
}

async fn emit_line(stdout: &mut tokio::io::Stdout, value: &Value) -> Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    stdout.write_all(&bytes).await?;
    stdout.flush().await?;
    Ok(())
}

async fn run_stream(client: RuntimeHostRpcClient) -> Result<()> {
    let (mut reader, mut host_writer, mut host_request_id) = client.into_terminal_stream_parts();
    let mut native_input = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();
    let mut pending: HashMap<i64, PendingRequest> = HashMap::new();
    let mut sessions = StreamSessions::default();
    loop {
        tokio::select! {
            input = native_input.next_line() => {
                let Some(line) = input? else { return Ok(()); };
                if line.len() > MAX_REQUEST_LINE {
                    emit_line(&mut stdout, &json!({"id": 0, "ok": false, "error": "terminal stream request is too large"})).await?;
                    continue;
                }
                let parsed = serde_json::from_str::<Value>(&line)
                    .context("invalid Native terminal JSON request");
                let command = parsed.as_ref().ok().and_then(|value| parse_command(value).ok());
                let Some(command) = command else {
                    let native_id = parsed.as_ref().ok()
                        .and_then(|value| value.get("id")).and_then(Value::as_u64).unwrap_or(0);
                    emit_line(&mut stdout, &json!({"id": native_id, "ok": false, "error": "invalid or unauthorized terminal stream command"})).await?;
                    continue;
                };
                if pending.len() >= MAX_PENDING {
                    emit_line(&mut stdout, &json!({"id": command.request_id, "ok": false, "error": "terminal stream request queue full"})).await?;
                    continue;
                }
                if command.kind == StreamCommandKind::Write
                    && !sessions.active.contains(&command.session_id)
                {
                    emit_line(&mut stdout, &json!({"id": command.request_id, "ok": false, "error": "terminal session not observed"})).await?;
                    continue;
                }
                if command.kind == StreamCommandKind::Observe {
                    sessions.pending(&command.session_id);
                }
                let frame = json!({
                    "id": host_request_id,
                    "type": command.host_verb,
                    "payload": command.payload,
                });
                let mut bytes = serde_json::to_vec(&frame)?;
                bytes.push(b'\n');
                host_writer.write_all(&bytes).await?;
                host_writer.flush().await?;
                pending.insert(host_request_id, PendingRequest {
                    native_id: command.request_id,
                    session_id: command.session_id,
                    kind: command.kind,
                });
                host_request_id = host_request_id.checked_add(1)
                    .ok_or_else(|| anyhow!("terminal stream request id overflow"))?;
            }
            frame = reader.next_line() => {
                let Some(line) = frame? else {
                    emit_line(&mut stdout, &json!({"event": "disconnected", "payload": {}})).await?;
                    return Err(anyhow!("Host connection closed"));
                };
                let frame = serde_json::from_str::<Value>(&line)
                    .context("Host sent invalid terminal control frame")?;
                if let Some(host_id) = frame.get("id").and_then(Value::as_i64) {
                    if let Some(request) = pending.remove(&host_id) {
                        let ok = frame.get("ok").and_then(Value::as_bool) == Some(true);
                        if request.kind == StreamCommandKind::Observe {
                            if ok {
                                sessions.accept(&request.session_id);
                            } else {
                                sessions.remove(&request.session_id);
                            }
                        }
                        emit_line(&mut stdout, &json!({
                            "id": request.native_id,
                            "ok": ok,
                            "payload": frame.get("payload").cloned().unwrap_or(Value::Null),
                            "error": frame.get("error").cloned().unwrap_or(Value::Null),
                        })).await?;
                    }
                } else if let Some(redacted) = sanitize_agent_presence_event(&frame) {
                    emit_line(&mut stdout, &redacted).await?;
                } else if sessions.forward(&frame) {
                    emit_line(&mut stdout, &frame).await?;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_bounded_session_scoped_terminal_ipc_commands_are_permitted() {
        let valid = json!({
            "id": 1, "type": "observe",
            "payload": {
                "sessionId": "pty-1", "workspaceId": "ws-1",
                "tabId": "tab-1", "viewLeaseId": "gpui-window-1",
            }
        });
        let observe = parse_command(&valid).expect("scoped observe");
        assert_eq!(observe.host_verb, "terminal.observeExisting");
        assert_eq!(observe.session_id, "pty-1");
        assert_eq!(observe.request_id, 1);
        let recover = parse_command(&json!({
            "id": 2, "type": "recover",
            "payload": {
                "sessionId": "pty-2", "workspaceId": "ws-1", "tabId": "tab-2",
                "viewLeaseId": "gpui-window-1", "launch": {"shell": "malicious"},
                "workingDirectory": "C:/untrusted", "force": true
            }
        }))
        .expect("bounded recovery-only request");
        assert_eq!(recover.kind, StreamCommandKind::Recover);
        assert_eq!(recover.host_verb, "terminal.recoverExistingWorker");
        assert_eq!(
            recover.payload,
            json!({
                "sessionId": "pty-2", "workspaceId": "ws-1", "tabId": "tab-2"
            })
        );
        let write = parse_command(&json!({
            "id": 3, "type": "write",
            "payload": {
                "sessionId": "pty-1", "workspaceId": "ws-1",
                "tabId": "tab-1", "viewLeaseId": "gpui-window-1",
                "dataBase64": "YQ==", "deferredEnter": true,
                "bracketedPaste": true, "launch": {"shell":"evil"}
            }
        }))
        .expect("scoped user input");
        assert_eq!(write.host_verb, "terminal.writeObserved");
        assert_eq!(write.payload["dataBase64"], "YQ==");
        assert_eq!(write.payload["viewLeaseId"], "gpui-window-1");
        assert_eq!(write.payload["deferredEnter"], false);
        assert_eq!(write.payload["bracketedPaste"], false);
        assert!(write.payload.get("launch").is_none());
        for invalid in [
            json!({"id": 0, "type": "observe", "payload": valid["payload"]}),
            json!({"id": 2, "type": "observe", "payload": {"sessionId": "pty-1"}}),
            json!({"id": 2, "type": "observe", "payload": {"sessionId": "pty\nevil", "workspaceId": "ws", "tabId": "tab", "viewLeaseId": "lease"}}),
            json!({"id": 2, "type": "terminate", "payload": {"sessionId": "pty-1"}}),
            json!({"id": 2, "type": "host.shutdown", "payload": {}}),
            json!({"id": 2, "type": "create", "payload": {}}),
            json!({"id": 2, "type": "resize", "payload": {"sessionId": "pty-1", "cols": 80, "rows": 24}}),
            json!({"id": 2, "type": "pause", "payload": {"sessionId": "pty-1", "paused": false}}),
            json!({"id": 2, "type": "write", "payload": {"sessionId": "pty-1", "dataBase64": "invalid!!!"}}),
            json!({"id": 2, "type": "write", "payload": {"sessionId": "pty-1", "dataBase64": base64::engine::general_purpose::STANDARD.encode(vec![0_u8; 4097])}}),
        ] {
            assert!(parse_command(&invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn output_forwarding_is_scoped_to_subscribed_or_pending_sessions() {
        let mut sessions = StreamSessions::default();
        let output_a =
            json!({"event": "output", "payload": {"sessionId": "pty-a", "dataBase64": "YQ==" }});
        let output_b =
            json!({"event": "output", "payload": {"sessionId": "pty-b", "dataBase64": "Yg==" }});
        assert!(!sessions.forward(&output_a));
        sessions.pending("pty-a");
        assert!(sessions.forward(&output_a));
        assert!(!sessions.forward(&output_b));
        sessions.accept("pty-a");
        assert!(sessions.forward(&output_a));
        sessions.remove("pty-a");
        assert!(!sessions.forward(&output_a));
        assert!(
            !sessions.forward(&json!({"event": "mcpUiCommand", "payload": {"sessionId": "pty-a"}}))
        );
    }

    #[test]
    fn agent_presence_push_is_bounded_redacted_and_retains_exact_host_identity() {
        let host_event = json!({
            "event": "agentPresenceChanged",
            "payload": {"changes": [
                {
                    "terminalSessionId": "pty-1", "workspaceId": "ws-1",
                    "tabId": "tab-1", "agentType": "codex",
                    "state": "waiting", "stateStartedAt": "2026-11-11T07:00:00+00:00",
                    "updatedAt": "2026-11-11T07:00:01+00:00",
                    "interrupted": false, "prompt": "SECRET PROMPT",
                    "toolInput": "API_KEY=SECRET", "lastAssistantMessage": "PRIVATE",
                    "contextOverview": {"goal":"PRIVATE TASK", "progress":"SECRET REPORT"}
                },
                {"terminalSessionId": "pty-2", "workspaceId": "ws-2",
                 "tabId": "tab-2", "removed": true}
            ]}
        });
        let forwarded = sanitize_agent_presence_event(&host_event).unwrap();
        let changes = forwarded["payload"]["changes"].as_array().unwrap();
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0]["state"], "waiting");
        assert_eq!(changes[0]["workspaceId"], "ws-1");
        assert_eq!(changes[0]["tabId"], "tab-1");
        assert_eq!(changes[0]["updatedAt"], "2026-11-11T07:00:01+00:00");
        for field in ["prompt", "toolName", "toolInput", "lastAssistantMessage", "contextOverview"] {
            assert!(changes[0].get(field).is_none(), "sensitive {field} leaked");
        }
        assert_eq!(changes[1]["removed"], true);
        assert!(sanitize_agent_presence_event(
            &json!({"event": "mcpUiCommand", "payload": host_event["payload"]})
        )
        .is_none());
        assert!(sanitize_agent_presence_event(&json!({
            "event": "agentPresenceChanged", "payload": {"changes": [
                {"terminalSessionId": "bad\nhandle", "state": "working"},
                {"terminalSessionId": "pty-2", "workspaceId": "ws", "tabId": "tab",
                 "state": "hacked", "agentType": "codex"}
            ]}
        }))
        .unwrap()["payload"]["changes"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(
            sanitize_agent_presence_event(&json!({
                "event": "agentPresenceChanged", "payload": {}
            }))
            .is_some(),
            "legacy Host invalidation must still trigger snapshot"
        );
        assert_eq!(sanitize_agent_presence_event(&json!({
            "event": "agentPresenceChanged", "payload": {"changes": [
                {"terminalSessionId": "pty-1", "removed": true}
            ]}
        })).unwrap(), json!({"event":"agentPresenceChanged","payload":{}}),
          "Host closure without workspace/tab identity must invalidate, never delete recycled Tab");
    }
}
