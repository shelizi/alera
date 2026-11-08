//! Local Alera Native MCP UI event relay. The CLI authenticates as the App;
//! this model never reads Host control tokens or speaks raw Host RPC.
use crate::runtime_bridge::RuntimeCliBridge;
use alera_core::child_process::windowless_command;
use serde_json::{json, Value};
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_LINE_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct NativeMcpUiCommand {
    pub request_id: String,
    pub workspace_id: String,
    pub action: String,
    pub arguments: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NativeMcpUiEvent {
    Ready,
    Command(NativeMcpUiCommand),
    Disconnected,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

pub fn parse_native_ui_event(value: &Value, window_id: &str) -> Option<NativeMcpUiEvent> {
    match value.get("kind")?.as_str()? {
        "ready" if value.get("windowId")?.as_str()? == window_id => Some(NativeMcpUiEvent::Ready),
        "command" => {
            let command = value.get("payload")?;
            if command.get("windowId")?.as_str()? != window_id {
                return None;
            }
            let request_id = command.get("requestId")?.as_str()?;
            let workspace_id = command.get("workspaceId")?.as_str()?;
            let action = command.get("action")?.as_str()?;
            let arguments = command.get("arguments")?;
            if !valid_id(request_id)
                || !valid_id(workspace_id)
                || !valid_id(action)
                || !arguments.is_object()
            {
                return None;
            }
            Some(NativeMcpUiEvent::Command(NativeMcpUiCommand {
                request_id: request_id.to_owned(),
                workspace_id: workspace_id.to_owned(),
                action: action.to_owned(),
                arguments: arguments.clone(),
            }))
        }
        _ => None,
    }
}

pub struct NativeMcpUiBridge {
    child: Child,
    stdin: ChildStdin,
    receiver: Receiver<NativeMcpUiEvent>,
    window_id: String,
}

impl RuntimeCliBridge {
    pub fn start_native_mcp_ui(&self, window_id: String) -> io::Result<NativeMcpUiBridge> {
        let (exe, runtime) = self.agent_cli_paths();
        NativeMcpUiBridge::start(exe, runtime, window_id)
    }
}

impl NativeMcpUiBridge {
    /// An isolated Native process has a separate MCP window ID from Classic.
    pub fn new_window_id() -> String {
        let tick = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        format!("native-{}-{tick}", std::process::id())
    }

    pub fn start(cli: &Path, runtime: &Path, window_id: String) -> io::Result<Self> {
        if !cli.is_file() || !valid_id(&window_id) || !window_id.starts_with("native-") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid Native MCP bridge configuration",
            ));
        }
        // Distinct from agent-profile commands; this long-lived relay is
        // registered only while the Native UI window is alive.
        if !runtime.join("host.json").is_file() && !runtime.join("runtime-host.json").is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "MCP UI bridge requires an existing Runtime Host",
            ));
        }
        let mut command = windowless_command(cli);
        let mut child = command
            .arg("native-ui-bridge")
            .arg("--runtime-dir")
            .arg(runtime)
            .arg("--window-id")
            .arg(&window_id)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("MCP bridge stdin unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("MCP bridge stdout unavailable"))?;
        let (tx, receiver) = mpsc::sync_channel(32);
        let reader_window = window_id.clone();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) if line.len() > MAX_LINE_BYTES => break,
                    Ok(_) => {
                        let Ok(value) = serde_json::from_str::<Value>(&line) else {
                            continue;
                        };
                        let Some(event) = parse_native_ui_event(&value, &reader_window) else {
                            continue;
                        };
                        if tx.try_send(event).is_err() {
                            break;
                        }
                    }
                }
            }
            let _ = tx.try_send(NativeMcpUiEvent::Disconnected);
        });
        Ok(Self {
            child,
            stdin,
            receiver,
            window_id,
        })
    }

    pub fn window_id(&self) -> &str {
        &self.window_id
    }

    pub fn take_events(&mut self) -> Vec<NativeMcpUiEvent> {
        let mut events = Vec::new();
        for _ in 0..16 {
            match self.receiver.try_recv() {
                Ok(event) => events.push(event),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    events.push(NativeMcpUiEvent::Disconnected);
                    break;
                }
            }
        }
        events
    }

    /// Called only AFTER GPUI verifies the visible operation. This is a
    /// request-specific receipt, never an arbitrary Host command.
    pub fn complete(
        &mut self,
        request_id: &str,
        applied: bool,
        tab_id: Option<&str>,
        error_code: Option<&str>,
    ) -> io::Result<()> {
        if !valid_id(request_id) || tab_id.is_some_and(|id| !valid_id(id)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid MCP UI receipt ID",
            ));
        }
        let value = json!({"requestId":request_id,"applied":applied,
            "tabId": if applied { tab_id } else { None },
            "errorCode": if applied { None } else { Some(error_code.unwrap_or("ui_not_ready")) }});
        let encoded = serde_json::to_vec(&value).map_err(io::Error::other)?;
        self.stdin.write_all(&encoded)?;
        self.stdin.write_all(b"\n")?;
        self.stdin.flush()
    }
}

impl Drop for NativeMcpUiBridge {
    fn drop(&mut self) {
        // Host owns PTYs and must not be stopped. Killing this ephemeral
        // relay disconnects its App window and invalidates pending commands.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_mcp_ui_events_are_window_bound_and_bounded() {
        let event = json!({"kind":"command", "payload":{
            "requestId":"req_1","windowId":"native-a","workspaceId":"ws_1",
            "action":"reveal_file","arguments":{"relativePath":"src/a.rs"}}});
        assert!(parse_native_ui_event(&event, "native-b").is_none());
        assert!(matches!(parse_native_ui_event(&event,"native-a"),
            Some(NativeMcpUiEvent::Command(NativeMcpUiCommand{action,..})) if action=="reveal_file"));
        assert!(matches!(
            parse_native_ui_event(&json!({"kind":"ready","windowId":"native-a"}), "native-a"),
            Some(NativeMcpUiEvent::Ready)
        ));
        assert!(parse_native_ui_event(&json!({"kind":"command","payload":{"requestId":"../bad", "windowId":"native-a","workspaceId":"ws_1","action":"open_file","arguments":{}}}),"native-a").is_none());
    }

    #[test]
    fn missing_host_cannot_start_native_mcp_or_create_state() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = dir.path().join("runtime");
        let cli = dir.path().join("missing.exe");
        assert!(NativeMcpUiBridge::start(&cli, &runtime, "native-test".into()).is_err());
        assert!(!runtime.exists());
    }
}
