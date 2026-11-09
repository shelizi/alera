//! Native-side private stdio adapter for the authenticated Alera CLI stream.
//! The Host controls PTY lifetime and token authentication; this code never
//! reads Host control files, receives credentials, or starts a Host.

use crate::runtime_bridge::{terminal_cli_arguments, RuntimeCliBridge};
use crate::terminal_output_budget::TerminalParseBudget;
use alera_core::child_process::windowless_command;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::OsString;
use std::io::{self, BufRead as _, BufReader, Write as _};
use std::path::Path;
use std::process::{Child, Stdio};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

pub const STREAM_EVENT_BUDGET_PER_TICK: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamSessionIdentity {
    pub session_id: String,
    pub workspace_id: String,
    pub tab_id: String,
    lease_id: String,
}

impl StreamSessionIdentity {
    fn same_host_tab(&self, session_id: &str, workspace_id: &str, tab_id: &str) -> bool {
        self.session_id == session_id && self.workspace_id == workspace_id && self.tab_id == tab_id
    }
}

#[derive(Debug)]
pub enum StreamViewUpdate {
    Snapshot {
        identity: StreamSessionIdentity,
        bytes: Vec<u8>,
        cursor: u64,
    },
    Output {
        identity: StreamSessionIdentity,
        bytes: Vec<u8>,
    },
    Failed(String),
    Disconnected,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    Response {
        request_id: u64,
        ok: bool,
        payload: Value,
        error: Option<String>,
    },
    Output {
        session_id: String,
        data: Vec<u8>,
    },
    ResyncRequired {
        session_id: String,
    },
    Error {
        session_id: String,
        reason: String,
    },
    Exited {
        session_id: String,
        exit_code: i64,
    },
    Disconnected,
}

/// Session subscription registry independent of the GPUI presentation.
#[derive(Default)]
struct StreamSubscriptions {
    pending: HashMap<u64, StreamSessionIdentity>,
    active: HashMap<String, StreamSessionIdentity>,
    rejected: HashSet<String>,
    buffered: HashMap<String, Vec<Vec<u8>>>,
}

impl StreamSubscriptions {
    fn already_observing(&self, session_id: &str, workspace_id: &str, tab_id: &str) -> bool {
        self.active
            .get(session_id)
            .is_some_and(|it| it.same_host_tab(session_id, workspace_id, tab_id))
            || self
                .pending
                .values()
                .any(|it| it.same_host_tab(session_id, workspace_id, tab_id))
    }

    fn active(&self, session_id: &str, workspace_id: &str, tab_id: &str) -> bool {
        self.active
            .get(session_id)
            .is_some_and(|it| it.same_host_tab(session_id, workspace_id, tab_id))
    }

    fn process(&mut self, event: StreamEvent) -> Vec<StreamViewUpdate> {
        match event {
            StreamEvent::Response {
                request_id,
                ok,
                payload,
                error,
            } => {
                let Some(identity) = self.pending.remove(&request_id) else {
                    return if ok {
                        Vec::new()
                    } else {
                        vec![StreamViewUpdate::Failed(error.unwrap_or_else(|| {
                            "Terminal stream request rejected".into()
                        }))]
                    };
                };
                let session_id = identity.session_id.clone();
                let cursor = payload.get("outputCursor").and_then(Value::as_u64);
                let bytes = payload
                    .get("snapshotText")
                    .and_then(Value::as_str)
                    .map(|text| text.as_bytes().to_vec())
                    .or_else(|| {
                        payload
                            .get("snapshotBase64")
                            .and_then(Value::as_str)
                            .and_then(|encoded| STANDARD.decode(encoded).ok())
                    });
                if !ok
                    || payload.get("created") == Some(&Value::Bool(true))
                    || payload.get("running") != Some(&Value::Bool(true))
                    || cursor.is_none()
                    || bytes.is_none()
                {
                    self.active.remove(&session_id);
                    self.buffered.remove(&session_id);
                    self.rejected.insert(session_id);
                    return vec![StreamViewUpdate::Failed(error.unwrap_or_else(|| {
                        "Host cannot observe this existing Terminal".into()
                    }))];
                }
                let (cursor, bytes) = (cursor.unwrap(), bytes.unwrap());
                self.active.insert(session_id.clone(), identity.clone());
                self.rejected.remove(&session_id);
                let mut updates = vec![StreamViewUpdate::Snapshot {
                    identity: identity.clone(),
                    bytes,
                    cursor,
                }];
                for bytes in self.buffered.remove(&session_id).unwrap_or_default() {
                    updates.push(StreamViewUpdate::Output {
                        identity: identity.clone(),
                        bytes,
                    });
                }
                updates
            }
            StreamEvent::Output { session_id, data } => {
                if let Some(identity) = self.active.get(&session_id).cloned() {
                    vec![StreamViewUpdate::Output {
                        identity,
                        bytes: data,
                    }]
                } else if self.pending.values().any(|it| it.session_id == session_id) {
                    let queue = self.buffered.entry(session_id.clone()).or_default();
                    let total: usize = queue.iter().map(Vec::len).sum();
                    if total.saturating_add(data.len()) <= 256 * 1024 {
                        queue.push(data);
                        Vec::new()
                    } else {
                        self.pending.retain(|_, it| it.session_id != session_id);
                        self.buffered.remove(&session_id);
                        self.rejected.insert(session_id);
                        vec![StreamViewUpdate::Failed(
                            "Host output exceeded Native attach backlog".into(),
                        )]
                    }
                } else {
                    Vec::new()
                }
            }
            StreamEvent::ResyncRequired { session_id } => {
                self.active.remove(&session_id);
                self.rejected.insert(session_id);
                vec![StreamViewUpdate::Failed(
                    "Host output resync required".into(),
                )]
            }
            StreamEvent::Error { session_id, reason } => {
                self.active.remove(&session_id);
                self.rejected.insert(session_id);
                vec![StreamViewUpdate::Failed(reason)]
            }
            StreamEvent::Exited { session_id, .. } => {
                self.active.remove(&session_id);
                self.rejected.insert(session_id);
                vec![StreamViewUpdate::Failed("Terminal PTY exited".into())]
            }
            StreamEvent::Disconnected => vec![StreamViewUpdate::Disconnected],
        }
    }
}

fn valid_session_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, b'-' | b'_' | b'.' | b':'))
}

pub fn parse_stream_event(value: &Value) -> Option<StreamEvent> {
    if let Some(id) = value.get("id").and_then(Value::as_u64) {
        if id == 0 {
            return None;
        }
        return Some(StreamEvent::Response {
            request_id: id,
            ok: value.get("ok").and_then(Value::as_bool) == Some(true),
            payload: value.get("payload").cloned().unwrap_or(Value::Null),
            error: value
                .get("error")
                .and_then(Value::as_str)
                .map(str::to_owned),
        });
    }
    if value.get("event").and_then(Value::as_str) == Some("disconnected") {
        return Some(StreamEvent::Disconnected);
    }
    let payload = value.get("payload")?;
    let id = payload.get("sessionId").and_then(Value::as_str)?;
    if !valid_session_id(id) {
        return None;
    }
    match value.get("event").and_then(Value::as_str)? {
        "output" => {
            let encoded = payload.get("dataBase64").and_then(Value::as_str);
            // Host output events remain bounded. A corrupt or oversized
            // frame must not be silently lost: force a full retained-scrollback
            // recovery instead of skipping bytes and corrupting VT state.
            let Some(data) = encoded
                .filter(|encoded| encoded.len() <= 1_400_000)
                .and_then(|encoded| STANDARD.decode(encoded).ok())
            else {
                return Some(StreamEvent::Error {
                    session_id: id.to_owned(),
                    reason: "Invalid Host output; Terminal stream resync required".into(),
                });
            };
            Some(StreamEvent::Output {
                session_id: id.to_owned(),
                data,
            })
        }
        "outputResyncRequired" => Some(StreamEvent::ResyncRequired {
            session_id: id.to_owned(),
        }),
        "error" => Some(StreamEvent::Error {
            session_id: id.to_owned(),
            reason: payload
                .get("error")
                .and_then(Value::as_str)
                .map(|message| message.chars().take(200).collect())
                .unwrap_or_else(|| "Host Terminal session error".into()),
        }),
        "exit" => Some(StreamEvent::Exited {
            session_id: id.to_owned(),
            exit_code: payload
                .get("exitCode")
                .and_then(Value::as_i64)
                .unwrap_or(-1),
        }),
        _ => None,
    }
}

/// Pure request construction preserves the Host owner triple and view lease.
pub fn observed_write_request(
    request_id: u64,
    identity: &StreamSessionIdentity,
    bytes: &[u8],
) -> Value {
    json!({
        "id": request_id, "type": "write",
        "payload": {
            "sessionId": identity.session_id,
            "workspaceId": identity.workspace_id,
            "tabId": identity.tab_id,
            "viewLeaseId": identity.lease_id,
            "dataBase64": STANDARD.encode(bytes),
        },
    })
}

pub fn native_stream_cli_args(runtime_dir: &Path) -> Vec<OsString> {
    terminal_cli_arguments(runtime_dir, &[OsString::from("stream")])
}

pub fn observe_request(
    request_id: u64,
    session_id: &str,
    workspace_id: &str,
    tab_id: &str,
    lease_id: &str,
) -> Value {
    json!({
        "id": request_id,
        "type": "observe",
        "payload": {
            "sessionId": session_id,
            "workspaceId": workspace_id,
            "tabId": tab_id,
            "viewLeaseId": lease_id,
        }
    })
}

/// One CLI child per Native app, rather than one CLI process per key press.
/// Closing this child only releases its Host view subscription, never a PTY.
pub struct NativeTerminalStreamBridge {
    process: Child,
    requests: SyncSender<Value>,
    events: Receiver<StreamEvent>,
    next_id: u64,
    lease_prefix: String,
    subscriptions: StreamSubscriptions,
    ready_updates: VecDeque<StreamViewUpdate>,
    ui_parse_budget: TerminalParseBudget,
}

impl NativeTerminalStreamBridge {
    pub fn start(cli: &RuntimeCliBridge) -> io::Result<Self> {
        let (executable, runtime_dir) = cli.agent_cli_paths();
        let mut process = windowless_command(executable);
        process
            .args(native_stream_cli_args(runtime_dir))
            .env_remove("ALERA_WORKSPACE_ID")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = process.spawn()?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("stream stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("stream stdout"))?;
        let (request_tx, request_rx) = mpsc::sync_channel::<Value>(64);
        let (event_tx, event_rx) = mpsc::sync_channel::<StreamEvent>(128);
        let reader_tx = event_tx.clone();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else {
                    break;
                };
                let Ok(value) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                if let Some(event) = parse_stream_event(&value) {
                    if reader_tx.send(event).is_err() {
                        break;
                    }
                }
            }
            let _ = reader_tx.send(StreamEvent::Disconnected);
        });
        thread::spawn(move || {
            for request in request_rx {
                let Ok(mut frame) = serde_json::to_vec(&request) else {
                    break;
                };
                frame.push(b'\n');
                if stdin.write_all(&frame).is_err() || stdin.flush().is_err() {
                    break;
                }
            }
            // Dropping stdin makes the CLI stream close its authenticated
            // connection without terminating any Host-owned process.
            drop(stdin);
            drop(event_tx);
        });
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        Ok(Self {
            process: child,
            requests: request_tx,
            events: event_rx,
            next_id: 0,
            lease_prefix: format!("gpui-{}-{nonce}", std::process::id()),
            subscriptions: StreamSubscriptions::default(),
            ready_updates: VecDeque::new(),
            ui_parse_budget: TerminalParseBudget::default(),
        })
    }

    fn next_request_id(&mut self) -> u64 {
        self.next_id = self.next_id.wrapping_add(1).max(1);
        self.next_id
    }

    pub fn observe(
        &mut self,
        session_id: &str,
        workspace_id: &str,
        tab_id: &str,
    ) -> Option<(u64, String)> {
        if !valid_session_id(session_id)
            || !valid_session_id(workspace_id)
            || !valid_session_id(tab_id)
        {
            return None;
        }
        let id = self.next_request_id();
        let lease = format!("{}-{id}", self.lease_prefix);
        self.requests
            .try_send(observe_request(
                id,
                session_id,
                workspace_id,
                tab_id,
                &lease,
            ))
            .ok()?;
        Some((id, lease))
    }

    pub fn ensure_observing(&mut self, session_id: &str, workspace_id: &str, tab_id: &str) {
        if self
            .subscriptions
            .already_observing(session_id, workspace_id, tab_id)
            || self.subscriptions.rejected.contains(session_id)
        {
            return;
        }
        let Some((id, lease_id)) = self.observe(session_id, workspace_id, tab_id) else {
            return;
        };
        self.subscriptions.pending.insert(
            id,
            StreamSessionIdentity {
                session_id: session_id.to_owned(),
                workspace_id: workspace_id.to_owned(),
                tab_id: tab_id.to_owned(),
                lease_id,
            },
        );
    }

    pub fn is_observing(&self, session_id: &str, workspace_id: &str, tab_id: &str) -> bool {
        self.subscriptions.active(session_id, workspace_id, tab_id)
    }

    pub fn write_observed(
        &mut self,
        session_id: &str,
        workspace_id: &str,
        tab_id: &str,
        bytes: &[u8],
    ) -> bool {
        let Some(identity) = self.subscriptions.active.get(session_id).cloned() else {
            return false;
        };
        if !identity.same_host_tab(session_id, workspace_id, tab_id)
            || bytes.is_empty()
            || bytes.len() > 4 * 1024
        {
            return false;
        }
        let id = self.next_request_id();
        self.requests
            .try_send(observed_write_request(id, &identity, bytes))
            .is_ok()
    }

    pub fn poll_updates(&mut self) -> Vec<StreamViewUpdate> {
        for _ in 0..STREAM_EVENT_BUDGET_PER_TICK {
            if self.ready_updates.len() >= 16 {
                break;
            }
            let Some(event) = self.try_next_event() else {
                break;
            };
            self.ready_updates.extend(self.subscriptions.process(event));
        }
        let mut remaining = self.ui_parse_budget.max_read_bytes() as usize;
        let mut result = Vec::new();
        while let Some(event) = self.ready_updates.pop_front() {
            match event {
                StreamViewUpdate::Snapshot {
                    identity,
                    mut bytes,
                    cursor,
                } => {
                    if remaining == 0 {
                        self.ready_updates.push_front(StreamViewUpdate::Snapshot {
                            identity,
                            bytes,
                            cursor,
                        });
                        break;
                    }
                    if bytes.len() > remaining {
                        let full_len = bytes.len();
                        let tail = bytes.split_off(remaining);
                        let first_cursor = cursor.saturating_sub((full_len - remaining) as u64);
                        self.ready_updates.push_front(StreamViewUpdate::Output {
                            identity: identity.clone(),
                            bytes: tail,
                        });
                        remaining = 0;
                        result.push(StreamViewUpdate::Snapshot {
                            identity,
                            bytes,
                            cursor: first_cursor,
                        });
                    } else {
                        remaining -= bytes.len();
                        result.push(StreamViewUpdate::Snapshot {
                            identity,
                            bytes,
                            cursor,
                        });
                    }
                }
                StreamViewUpdate::Output {
                    identity,
                    mut bytes,
                } => {
                    if remaining == 0 {
                        self.ready_updates
                            .push_front(StreamViewUpdate::Output { identity, bytes });
                        break;
                    }
                    if bytes.len() > remaining {
                        let tail = bytes.split_off(remaining);
                        self.ready_updates.push_front(StreamViewUpdate::Output {
                            identity: identity.clone(),
                            bytes: tail,
                        });
                        remaining = 0;
                    } else {
                        remaining -= bytes.len();
                    }
                    result.push(StreamViewUpdate::Output { identity, bytes });
                }
                other => result.push(other),
            }
        }
        result
    }

    /// Feed Native VT parse time back into the Classic-derived adaptive 4–64
    /// KiB frame budget; do not parse a huge Host frame synchronously in GPUI.
    pub fn note_ui_parse(&mut self, bytes: usize, elapsed: Duration) {
        self.ui_parse_budget.observe(Instant::now(), bytes, elapsed);
    }

    pub fn try_next_event(&mut self) -> Option<StreamEvent> {
        match self.events.try_recv() {
            Ok(event) => Some(event),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(StreamEvent::Disconnected),
        }
    }
}

impl Drop for NativeTerminalStreamBridge {
    fn drop(&mut self) {
        // Only the private CLI IPC process is stopped; Runtime Host owns PTYs.
        let _ = self.process.kill();
        let _ = self.process.try_wait();
    }
}

#[cfg(test)]
mod subscription_tests {
    use super::*;

    #[test]
    fn snapshot_is_applied_before_output_arriving_during_observation() {
        let mut sessions = StreamSubscriptions::default();
        sessions.pending.insert(
            7,
            StreamSessionIdentity {
                session_id: "pty-1".into(),
                workspace_id: "ws-1".into(),
                tab_id: "tab-1".into(),
                lease_id: "lease-1".into(),
            },
        );
        assert!(sessions
            .process(StreamEvent::Output {
                session_id: "pty-1".into(),
                data: b"new".to_vec(),
            })
            .is_empty());
        let updates = sessions.process(StreamEvent::Response {
            request_id: 7,
            ok: true,
            payload: json!({
                "sessionId": "pty-1", "created": false, "running": true,
                "snapshotBase64": "b2xk", "outputCursor": 3
            }),
            error: None,
        });
        assert_eq!(updates.len(), 2);
        assert!(
            matches!(&updates[0], StreamViewUpdate::Snapshot { bytes, cursor: 3, .. } if bytes==b"old")
        );
        assert!(matches!(&updates[1], StreamViewUpdate::Output { bytes, .. } if bytes==b"new"));
        assert!(sessions.active("pty-1", "ws-1", "tab-1"));
        assert!(!sessions.active("pty-1", "wrong-worktree", "tab-1"));
    }

    #[test]
    fn rejected_subscription_does_not_grant_write_access() {
        let mut sessions = StreamSubscriptions::default();
        sessions.pending.insert(
            9,
            StreamSessionIdentity {
                session_id: "pty-1".into(),
                workspace_id: "ws-1".into(),
                tab_id: "tab-1".into(),
                lease_id: "lease-1".into(),
            },
        );
        let updates = sessions.process(StreamEvent::Response {
            request_id: 9,
            ok: false,
            payload: Value::Null,
            error: Some("stale worktree".into()),
        });
        assert!(
            matches!(&updates[0], StreamViewUpdate::Failed(reason) if reason=="stale worktree")
        );
        assert!(!sessions.active("pty-1", "ws-1", "tab-1"));
        assert!(sessions.rejected.contains("pty-1"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn native_observed_write_carries_original_view_lease_and_never_argv_text() {
        let identity = StreamSessionIdentity {
            session_id: "pty-1".into(),
            workspace_id: "workspace-one".into(),
            tab_id: "tab-1".into(),
            lease_id: "native-view-lease-1".into(),
        };
        let input = "echo 貓\r";
        let packet = observed_write_request(18, &identity, input.as_bytes());
        assert_eq!(packet["type"], "write");
        assert_eq!(packet["id"], 18);
        assert_eq!(packet["payload"]["workspaceId"], "workspace-one");
        assert_eq!(packet["payload"]["tabId"], "tab-1");
        assert_eq!(packet["payload"]["viewLeaseId"], "native-view-lease-1");
        assert_eq!(
            STANDARD
                .decode(packet["payload"]["dataBase64"].as_str().unwrap())
                .unwrap(),
            input.as_bytes(),
        );
        assert!(!native_stream_cli_args(std::path::Path::new("C:/runtime"))
            .iter()
            .any(|arg| arg == input));
        assert!(packet.get("token").is_none());
    }

    #[test]
    fn native_stream_decodes_only_expected_scoped_events() {
        let output = json!({"event": "output", "payload": {
            "sessionId": "pty-1", "dataBase64": "G1sySg=="
        }});
        assert!(matches!(
            parse_stream_event(&output).unwrap(),
            StreamEvent::Output { session_id, data }
                if session_id == "pty-1" && data == b"\x1b[2J"
        ));
        assert!(matches!(
            parse_stream_event(&json!({"event":"outputResyncRequired", "payload":{"sessionId":"pty-1"}})).unwrap(),
            StreamEvent::ResyncRequired { session_id } if session_id=="pty-1"
        ));
        assert!(parse_stream_event(&json!({"event": "mcpUiCommand", "payload": {}})).is_none());
        assert!(matches!(
            parse_stream_event(&json!({"event":"output","payload":{
                "sessionId":"pty-1","dataBase64":"not!base64"
            }})).unwrap(),
            StreamEvent::Error { session_id, .. } if session_id=="pty-1"
        ));
        assert!(parse_stream_event(
            &json!({"event":"output", "payload":{"sessionId":"bad\nhandle", "dataBase64":"YQ=="}})
        )
        .is_none());
        assert!(matches!(
            parse_stream_event(&json!({
                "event":"error",
                "payload":{"sessionId":"pty-1", "error":"Session moved"}
            })).unwrap(),
            StreamEvent::Error { session_id, reason }
                if session_id=="pty-1" && reason=="Session moved"
        ));
    }

    #[test]
    fn native_stream_commands_are_no_start_and_carry_only_scoped_session_data() {
        let cmd = observe_request(17, "pty-1", "ws-1", "tab-1", "view-1");
        assert_eq!(cmd["id"], 17);
        assert_eq!(cmd["type"], "observe");
        assert_eq!(cmd["payload"]["sessionId"], "pty-1");
        assert_eq!(cmd["payload"]["workspaceId"], "ws-1");
        assert_eq!(cmd["payload"]["tabId"], "tab-1");
        assert_eq!(cmd["payload"]["viewLeaseId"], "view-1");
        assert!(cmd.get("token").is_none());
        let args = native_stream_cli_args(std::path::Path::new("C:/runtime"));
        assert!(args.iter().any(|a| a == "--no-start"));
        assert!(args.iter().any(|a| a == "stream"));
        assert!(!args.iter().any(|a| a == "runtime-host" || a == "attach"));
    }
}
