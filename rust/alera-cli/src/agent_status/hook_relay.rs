use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use serde_json::Value;

#[cfg(test)]
use serde_json::json;

const COMPLETION_EVENTS: &[&str] = &[
    "Stop",
    "StopFailure",
    "StopCancelled",
    "SessionEnd",
    "Interrupt",
    "stop",
    "sessionEnd",
    "SessionIdle",
    "agent_settled",
    "session_shutdown",
    "agent.end",
    "ErrorOccurred",
];
const REQUEST_TIMEOUT: Duration = Duration::from_secs(1);
const CONNECT_TIMEOUT: Duration = Duration::from_millis(250);

struct HookRequest {
    port: u16,
    token: String,
    agent: String,
    terminal: String,
    workspace: String,
    tab: String,
    event: Option<String>,
    version: Option<String>,
}

impl HookRequest {
    fn from_environment(get: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let required = |name| get(name).filter(|value| !value.is_empty());
        let agent = required("ALERA_AGENT_TYPE")?;
        if !agent
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return None;
        }
        let token = required("ALERA_AGENT_HOOK_TOKEN")?;
        if !token.bytes().all(|byte| (32..127).contains(&byte)) {
            return None;
        }
        Some(Self {
            port: required("ALERA_AGENT_HOOK_PORT")?
                .parse::<u16>()
                .ok()
                .filter(|port| *port != 0)?,
            token,
            agent,
            terminal: required("ALERA_TERMINAL_SESSION_ID")?,
            workspace: required("ALERA_WORKSPACE_ID")?,
            tab: required("ALERA_TAB_ID")?,
            event: get("ALERA_AGENT_HOOK_EVENT"),
            version: get("ALERA_AGENT_HOOK_VERSION"),
        })
    }

    fn body(&self, input: &[u8]) -> Option<Vec<u8>> {
        let input = std::str::from_utf8(input)
            .ok()?
            .trim_start_matches('\u{feff}');
        let payload: Value = if input.trim().is_empty() {
            Value::Object(Default::default())
        } else {
            serde_json::from_str(input).ok()?
        };
        // Borrow the parsed payload to avoid cloning its entire JSON tree.
        // Field order preserves the existing serde_json map's wire representation.
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Envelope<'a> {
            hook_event_name: &'a Option<String>,
            payload: &'a Value,
            tab_id: &'a str,
            terminal_session_id: &'a str,
            version: &'a Option<String>,
            workspace_id: &'a str,
        }
        serde_json::to_vec(&Envelope {
            hook_event_name: &self.event,
            payload: &payload,
            tab_id: &self.tab,
            terminal_session_id: &self.terminal,
            version: &self.version,
            workspace_id: &self.workspace,
        })
        .ok()
    }

    fn attempts(&self) -> usize {
        if self.event.as_deref().is_some_and(|event| {
            COMPLETION_EVENTS
                .iter()
                .any(|completion| event.eq_ignore_ascii_case(completion))
        }) {
            3
        } else {
            1
        }
    }

    fn post(&self, body: &[u8]) -> std::io::Result<bool> {
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, self.port));
        let mut stream = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT)?;
        let remaining = || {
            deadline
                .checked_duration_since(Instant::now())
                .filter(|duration| !duration.is_zero())
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::TimedOut, "hook deadline"))
        };
        let header = format!(
            "POST /hook/{} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json; charset=utf-8\r\nX-Alera-Agent-Hook-Token: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.agent, self.port, self.token, body.len()
        );
        for mut bytes in [header.as_bytes(), body] {
            while !bytes.is_empty() {
                stream.set_write_timeout(Some(remaining()?))?;
                let written = stream.write(bytes)?;
                if written == 0 {
                    return Err(std::io::ErrorKind::WriteZero.into());
                }
                bytes = &bytes[written..];
            }
        }
        // Only the status line is needed; never retain a response payload.
        let mut reader = BufReader::new(stream);
        let mut status = Vec::new();
        while status.len() < 1024 {
            reader.get_ref().set_read_timeout(Some(remaining()?))?;
            let available = reader.fill_buf()?;
            if available.is_empty() {
                break;
            }
            let end = available.iter().position(|byte| *byte == b'\n');
            let length = end
                .map_or(available.len(), |index| index + 1)
                .min(1024 - status.len());
            status.extend_from_slice(&available[..length]);
            reader.consume(length);
            if end.is_some() {
                break;
            }
        }
        Ok(std::str::from_utf8(&status)
            .ok()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse::<u16>().ok())
            .is_some_and(|code| (200..300).contains(&code)))
    }

    fn deliver(&self, body: &[u8]) {
        for attempt in 0..self.attempts() {
            if matches!(self.post(body), Ok(true)) {
                break;
            }
            if attempt + 1 < self.attempts() {
                std::thread::sleep(Duration::from_millis(50 * (attempt as u64 + 1)));
            }
        }
    }
}

/// Observational hooks remain silent and successful even if the host is gone.
pub fn run_hook_relay() {
    let Some(request) = HookRequest::from_environment(|key| std::env::var(key).ok()) else {
        return;
    };
    let mut input = Vec::new();
    if std::io::stdin().lock().read_to_end(&mut input).is_err() {
        return;
    }
    let Some(body) = request.body(&input) else {
        return;
    };
    drop(input);
    request.deliver(&body);
}

#[cfg(test)]
#[path = "hook_relay_tests.rs"]
mod tests;
