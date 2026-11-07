//! Read-only native GPUI terminal session explorer.
//! Terminal process, PTY, authentication and session lifetime remain owned by
//! the existing Alera Runtime Host; this frontend uses the CLI --no-start mode.
use crate::runtime_bridge::RuntimeCliBridge;
use serde_json::Value;
use std::ffi::OsString;
use std::io;

pub const TERMINAL_READ_BYTES: u64 = 8 * 1024;
pub const MAX_TERMINALS: usize = 128;
const MAX_OPEN_TABS: usize = 8;
const MAX_TRANSCRIPT_BYTES: usize = 48 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalSession {
    pub handle: String,
    pub workspace_id: String,
    pub tab_id: String,
    pub running: bool,
    pub agent_type: Option<String>,
}

impl TerminalSession {
    pub fn label(&self) -> String {
        let name = self.agent_type.as_deref().unwrap_or("Terminal");
        let state = if self.running { "●" } else { "○" };
        format!("{state} {name} · {}", self.handle)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalChunk {
    pub handle: String,
    pub cursor: u64,
    pub next_cursor: u64,
    pub truncated: bool,
    pub running: bool,
    pub text: String,
}

impl RuntimeCliBridge {
    pub fn list_terminals(&self) -> io::Result<Vec<TerminalSession>> {
        let response = self.terminal_json(&[OsString::from("list")])?;
        parse_terminal_list(&response)
    }

    pub fn read_terminal(&self, handle: &str, cursor: Option<u64>) -> io::Result<TerminalChunk> {
        if !valid_handle(handle) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid terminal handle",
            ));
        }
        let mut args = vec![
            OsString::from("read"),
            OsString::from(format!("--handle={handle}")),
            OsString::from("--max-bytes"),
            OsString::from(TERMINAL_READ_BYTES.to_string()),
        ];
        if let Some(cursor) = cursor {
            args.push(OsString::from("--cursor"));
            args.push(OsString::from(cursor.to_string()));
        }
        let response = self.terminal_json(&args)?;
        parse_terminal_chunk(&response, handle)
    }
}

pub fn parse_terminal_list(value: &Value) -> io::Result<Vec<TerminalSession>> {
    let items = value
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("CLI terminal list has no items"))?;
    if items.len() > MAX_TERMINALS {
        return Err(invalid("Too many terminal sessions for preview"));
    }
    let mut sessions = Vec::with_capacity(items.len());
    for item in items {
        let handle = item
            .get("handle")
            .and_then(Value::as_str)
            .filter(|handle| valid_handle(handle))
            .ok_or_else(|| invalid("Terminal session has invalid handle"))?;
        let workspace_id = item
            .get("workspaceId")
            .and_then(Value::as_str)
            .filter(|id| id.len() <= 256)
            .ok_or_else(|| invalid("Terminal session has no workspaceId"))?;
        let tab_id = item
            .get("tabId")
            .and_then(Value::as_str)
            .filter(|id| id.len() <= 256)
            .ok_or_else(|| invalid("Terminal session has no tabId"))?;
        let running = item
            .get("running")
            .and_then(Value::as_bool)
            .ok_or_else(|| invalid("Terminal session has no running state"))?;
        let agent_type = item
            .get("agentType")
            .and_then(Value::as_str)
            .filter(|agent| agent.len() <= 64)
            .map(ToString::to_string);
        if sessions
            .iter()
            .any(|session: &TerminalSession| session.handle == handle)
        {
            return Err(invalid("Duplicate terminal session handle"));
        }
        sessions.push(TerminalSession {
            handle: handle.to_string(),
            workspace_id: workspace_id.to_string(),
            tab_id: tab_id.to_string(),
            running,
            agent_type,
        });
    }
    Ok(sessions)
}

pub fn parse_terminal_chunk(value: &Value, expected_handle: &str) -> io::Result<TerminalChunk> {
    let handle = value
        .get("handle")
        .and_then(Value::as_str)
        .filter(|handle| *handle == expected_handle)
        .ok_or_else(|| invalid("CLI terminal output handle mismatch"))?;
    let cursor = value
        .get("cursor")
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid("CLI terminal output missing cursor"))?;
    let next_cursor = value
        .get("nextCursor")
        .and_then(Value::as_u64)
        .filter(|next| *next >= cursor)
        .ok_or_else(|| invalid("CLI terminal output cursor went backwards"))?;
    let text = value
        .get("text")
        .and_then(Value::as_str)
        .filter(|text| text.len() <= (TERMINAL_READ_BYTES as usize + 64))
        .ok_or_else(|| invalid("CLI terminal text missing or exceeds preview limit"))?;
    let truncated = value
        .get("truncated")
        .and_then(Value::as_bool)
        .ok_or_else(|| invalid("CLI terminal output missing truncated state"))?;
    let running = value
        .get("running")
        .and_then(Value::as_bool)
        .ok_or_else(|| invalid("CLI terminal output missing running state"))?;
    Ok(TerminalChunk {
        handle: handle.to_string(),
        cursor,
        next_cursor,
        truncated,
        running,
        text: text.to_string(),
    })
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn valid_handle(handle: &str) -> bool {
    !handle.is_empty()
        && handle.len() <= 128
        && handle
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum EscapeMode {
    #[default]
    Text,
    Esc,
    Csi,
    Osc,
    OscEsc,
    ControlString,
    ControlEsc,
}

/// A conservative, bounded terminal *text transcript* decoder.
/// This is not a VT terminal emulator: styling, cursor movement and TUI
/// redraws are intentionally not emulated. Strips OSC52, OSC8, DCS and CSI.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct TextDecoder {
    mode: EscapeMode,
}

impl TextDecoder {
    fn append(&mut self, into: &mut String, text: &str) {
        for character in text.chars() {
            use EscapeMode::*;
            match self.mode {
                Text => match character {
                    '\u{1b}' => self.mode = Esc,
                    '\u{009b}' => self.mode = Csi,
                    '\u{009d}' => self.mode = Osc,
                    '\u{0090}' | '\u{0098}' | '\u{009e}' | '\u{009f}' => self.mode = ControlString,
                    '\r' => into.push('\n'),
                    '\n' | '\t' => into.push(character),
                    c if !c.is_control() => into.push(c),
                    _ => {}
                },
                Esc => {
                    self.mode = match character {
                        '[' => Csi,
                        ']' => Osc,
                        'P' | 'X' | '^' | '_' => ControlString,
                        _ => Text,
                    };
                }
                Csi => {
                    if ('@'..='~').contains(&character) {
                        self.mode = Text;
                    } else if character == '\u{1b}' {
                        self.mode = Esc;
                    }
                }
                Osc => match character {
                    '\u{0007}' | '\u{009c}' => self.mode = Text,
                    '\u{1b}' => self.mode = OscEsc,
                    _ => {}
                },
                OscEsc => {
                    self.mode = if character == '\\' || character == '\u{009c}' {
                        Text
                    } else if character == '\u{1b}' {
                        OscEsc
                    } else {
                        Osc
                    };
                }
                ControlString => {
                    if character == '\u{1b}' {
                        self.mode = ControlEsc;
                    } else if character == '\u{009c}' {
                        self.mode = Text;
                    }
                }
                ControlEsc => {
                    self.mode = if character == '\\' || character == '\u{009c}' {
                        Text
                    } else if character == '\u{1b}' {
                        ControlEsc
                    } else {
                        ControlString
                    };
                }
            }
        }
        if into.len() > MAX_TRANSCRIPT_BYTES {
            let mut remove = into.len() - MAX_TRANSCRIPT_BYTES;
            while !into.is_char_boundary(remove) {
                remove += 1;
            }
            into.drain(..remove);
        }
    }
}

#[derive(Clone, Debug)]
pub struct TerminalTab {
    pub handle: String,
    pub tab_id: String,
    pub workspace_id: String,
    pub title: String,
    pub running: bool,
    pub output: String,
    pub next_cursor: Option<u64>,
    generation: u64,
    decoder: TextDecoder,
}

impl TerminalTab {
    fn new(session: &TerminalSession, generation: u64) -> Self {
        Self {
            handle: session.handle.clone(),
            tab_id: session.tab_id.clone(),
            workspace_id: session.workspace_id.clone(),
            title: session
                .agent_type
                .clone()
                .unwrap_or_else(|| "Terminal".into()),
            running: session.running,
            output: String::new(),
            next_cursor: None,
            generation,
            decoder: TextDecoder::default(),
        }
    }

    fn apply(&mut self, chunk: TerminalChunk) -> bool {
        if self.handle != chunk.handle {
            return false;
        }
        if let Some(last) = self.next_cursor {
            if chunk.next_cursor < last {
                // Host restarted / scrollback reset; next read starts from
                // the fresh tail, rather than using an invalid old cursor.
                self.output.clear();
                self.decoder = TextDecoder::default();
                self.next_cursor = None;
                self.running = chunk.running;
                return true;
            }
            if chunk.next_cursor == last {
                self.running = chunk.running;
                return false;
            }
        }
        if chunk.truncated {
            self.decoder = TextDecoder::default();
            self.output
                .push_str("\n[Earlier terminal scrollback was truncated]\n");
        }
        self.decoder.append(&mut self.output, &chunk.text);
        self.next_cursor = Some(chunk.next_cursor);
        self.running = chunk.running;
        true
    }
}

/// Correlates a read with exactly one local preview tab and cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalReadTicket {
    pub handle: String,
    pub cursor: Option<u64>,
    generation: u64,
}

/// One tick per 1500 ms; errors back off to avoid spawning busy-loop CLI reads.
#[derive(Debug, Clone)]
pub struct TerminalPollGate {
    enabled: bool,
    failures: u32,
    ticks_to_retry: u32,
}

impl Default for TerminalPollGate {
    fn default() -> Self {
        Self {
            enabled: true,
            failures: 0,
            ticks_to_retry: 0,
        }
    }
}

impl TerminalPollGate {
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn toggle(&mut self) -> bool {
        self.enabled = !self.enabled;
        self.reset();
        self.enabled
    }
    pub fn tick(&mut self) -> bool {
        if !self.enabled {
            return false;
        }
        if self.ticks_to_retry > 0 {
            self.ticks_to_retry -= 1;
            return false;
        }
        true
    }
    pub fn reset(&mut self) {
        self.failures = 0;
        self.ticks_to_retry = 0;
    }
    pub fn failed(&mut self) {
        self.failures = self.failures.saturating_add(1);
        self.ticks_to_retry = 1u32 << self.failures.saturating_sub(1).min(4);
    }
}

#[derive(Debug, Default)]
pub struct TerminalDock {
    pub sessions: Vec<TerminalSession>,
    pub tabs: Vec<TerminalTab>,
    pub active_tab: Option<usize>,
    pub status: String,
    next_generation: u64,
}

impl TerminalDock {
    pub fn update_sessions(&mut self, sessions: Vec<TerminalSession>) {
        for tab in &mut self.tabs {
            if let Some(session) = sessions.iter().find(|item| {
                item.handle == tab.handle
                    && item.tab_id == tab.tab_id
                    && item.workspace_id == tab.workspace_id
            }) {
                tab.running = session.running;
            } else {
                tab.running = false;
            }
        }
        self.sessions = sessions;
        self.status = format!("{} existing terminal(s)", self.sessions.len());
    }

    pub fn open(&mut self, handle: &str) -> bool {
        let Some(session) = self
            .sessions
            .iter()
            .find(|session| session.handle == handle)
        else {
            return false;
        };
        if let Some(index) = self.tabs.iter().position(|tab| tab.handle == handle) {
            if self.tabs[index].tab_id != session.tab_id
                || self.tabs[index].workspace_id != session.workspace_id
            {
                // Explicitly selecting a new session with a reused handle
                // must not silently attach an old transcript.
                self.next_generation = self.next_generation.wrapping_add(1);
                self.tabs[index] = TerminalTab::new(session, self.next_generation);
            }
            self.active_tab = Some(index);
            return true;
        }
        if self.tabs.len() >= MAX_OPEN_TABS {
            self.status = format!("Terminal preview limit reached ({MAX_OPEN_TABS})");
            return false;
        }
        self.next_generation = self.next_generation.wrapping_add(1);
        self.tabs
            .push(TerminalTab::new(session, self.next_generation));
        self.active_tab = Some(self.tabs.len() - 1);
        true
    }

    pub fn active(&self) -> Option<&TerminalTab> {
        self.active_tab.and_then(|index| self.tabs.get(index))
    }

    pub fn activate(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() {
            return false;
        }
        self.active_tab = Some(index);
        true
    }

    pub fn read_request(&self) -> Option<(String, Option<u64>)> {
        self.active()
            .map(|tab| (tab.handle.clone(), tab.next_cursor))
    }

    pub fn active_ticket(&self) -> Option<TerminalReadTicket> {
        let tab = self.active()?;
        if !self.sessions.iter().any(|session| {
            session.handle == tab.handle
                && session.tab_id == tab.tab_id
                && session.workspace_id == tab.workspace_id
        }) {
            return None;
        }
        Some(TerminalReadTicket {
            handle: tab.handle.clone(),
            cursor: tab.next_cursor,
            generation: tab.generation,
        })
    }

    pub fn apply_ticket_chunk(
        &mut self,
        ticket: &TerminalReadTicket,
        chunk: TerminalChunk,
    ) -> bool {
        if ticket.handle != chunk.handle {
            return false;
        }
        if !self.sessions.iter().any(|session| {
            session.handle == ticket.handle
                && self.tabs.iter().any(|tab| {
                    tab.handle == ticket.handle
                        && tab.tab_id == session.tab_id
                        && tab.workspace_id == session.workspace_id
                })
        }) {
            // Session was lost on Host disconnect; reject queued output.
            return false;
        }
        let Some(tab) = self.tabs.iter().find(|tab| tab.handle == ticket.handle) else {
            return false;
        };
        if ticket.generation != tab.generation || ticket.cursor != tab.next_cursor {
            return false;
        }
        self.apply_chunk(chunk)
    }

    pub fn apply_chunk(&mut self, chunk: TerminalChunk) -> bool {
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.handle == chunk.handle) else {
            // Async response for a closed tab is stale.
            return false;
        };
        let applied = tab.apply(chunk);
        if applied {
            self.status = "Read-only terminal output updated".into();
        }
        applied
    }

    pub fn close(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        self.tabs.remove(index);
        self.active_tab = match self.active_tab {
            None => None,
            Some(_) if self.tabs.is_empty() => None,
            Some(active) if active == index => Some(index.min(self.tabs.len() - 1)),
            Some(active) if active > index => Some(active - 1),
            Some(active) => Some(active),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn session(handle: &str) -> TerminalSession {
        TerminalSession {
            handle: handle.into(),
            workspace_id: "workspace-1".into(),
            tab_id: format!("tab-{handle}"),
            running: true,
            agent_type: Some("codex".into()),
        }
    }

    fn chunk(handle: &str, cursor: u64, next: u64, text: &str) -> TerminalChunk {
        TerminalChunk {
            handle: handle.into(),
            cursor,
            next_cursor: next,
            truncated: false,
            running: true,
            text: text.into(),
        }
    }

    #[test]
    fn parses_actual_host_terminal_list_shape() {
        let value = json!({
            "kind": "terminals",
            "items": [
                {"handle":"term-1", "workspaceId":"workspace-1", "tabId":"tab-1",
                 "running":true, "agentType":"codex", "agentState":"working"},
                {"handle":"term-2", "workspaceId":"workspace-2", "tabId":"tab-2",
                 "running":false, "agentType":null}
            ]
        });
        let items = parse_terminal_list(&value).unwrap();
        assert_eq!(items.len(), 2);
        assert!(items[0].label().contains("codex"));
        assert!(!items[1].running);
        assert!(parse_terminal_list(&json!({"items":[{"handle": "bad\\nvalue"}]})).is_err());
        assert!(parse_terminal_list(&json!({"kind":"terminals"})).is_err());
    }

    #[test]
    fn parses_cursor_and_rejects_host_handle_spoofing() {
        let value = json!({
            "handle":"term-1", "cursor":42, "nextCursor":50,
            "truncated":false,"running":true,"text":"八個"
        });
        let entry = parse_terminal_chunk(&value, "term-1").unwrap();
        assert_eq!(entry.next_cursor, 50);
        assert!(parse_terminal_chunk(&value, "term-2").is_err());
        assert!(parse_terminal_chunk(
            &json!({"handle":"term-1","cursor":50,"nextCursor":40,"text":""}),
            "term-1"
        )
        .is_err());
    }

    #[test]
    fn keeps_multiple_preview_tabs_and_ignores_stale_response() {
        let mut dock = TerminalDock::default();
        dock.update_sessions(vec![session("term-1"), session("term-2")]);
        assert!(dock.open("term-1"));
        assert!(dock.open("term-2"));
        assert_eq!(dock.active_tab, Some(1));
        assert!(dock.apply_chunk(chunk("term-1", 0, 3, "abc")));
        assert_eq!(dock.tabs[0].output, "abc");
        dock.close(0);
        assert_eq!(dock.active_tab, Some(0));
        assert!(!dock.apply_chunk(chunk("term-1", 3, 6, "def")));
        assert_eq!(dock.tabs[0].handle, "term-2");
    }

    #[test]
    fn replay_ignores_duplicates_and_resets_cursors_on_host_restart() {
        let mut dock = TerminalDock::default();
        dock.update_sessions(vec![session("term-1")]);
        dock.open("term-1");
        assert!(dock.apply_chunk(chunk("term-1", 0, 10, "first")));
        assert!(!dock.apply_chunk(chunk("term-1", 0, 10, "duplicate")));
        assert_eq!(dock.active().unwrap().output, "first");
        assert!(dock.apply_chunk(chunk("term-1", 0, 4, "")));
        assert_eq!(dock.read_request().unwrap().1, None);
        assert!(dock.active().unwrap().output.is_empty());
        assert!(dock.apply_chunk(chunk("term-1", 0, 6, "fresh")));
        assert_eq!(dock.active().unwrap().output, "fresh");
    }

    #[test]
    fn strips_ansi_osc52_dcs_and_handles_split_controls() {
        let mut dock = TerminalDock::default();
        dock.update_sessions(vec![session("term-1")]);
        dock.open("term-1");
        dock.apply_chunk(chunk("term-1", 0, 5, "safe\u{1b}]52;secret"));
        dock.apply_chunk(chunk("term-1", 5, 10, "clipboard\u{1b}"));
        dock.apply_chunk(chunk("term-1", 10, 15, "\\world\u{1b}[31m紅色\u{1b}[0m\n"));
        dock.apply_chunk(chunk("term-1", 15, 20, "\u{1b}Pprivate\u{1b}\\normal"));
        let output = &dock.active().unwrap().output;
        assert_eq!(output, "safeworld紅色\nnormal");
        assert!(!output.contains("clipboard"));
        assert!(!output.contains("private"));
    }

    #[test]
    fn terminal_transcript_is_bounded_and_utf8_safe() {
        let mut dock = TerminalDock::default();
        dock.update_sessions(vec![session("term-1")]);
        dock.open("term-1");
        for i in 1..=65 {
            dock.apply_chunk(chunk("term-1", (i - 1) * 900, i * 900, &"你".repeat(300)));
        }
        let output = &dock.active().unwrap().output;
        assert!(output.len() <= MAX_TRANSCRIPT_BYTES);
        assert!(output.chars().all(|c| c == '你'));
    }

    #[test]
    fn old_terminal_handles_are_not_silently_reattached_to_new_sessions() {
        let mut dock = TerminalDock::default();
        dock.update_sessions(vec![session("term-1")]);
        dock.open("term-1");
        dock.update_sessions(vec![session("term-2")]);
        // Keep the old read-only transcript for inspection, but the caller
        // must explicitly choose a new handle; it is not taken over.
        assert_eq!(dock.active().unwrap().handle, "term-1");
        assert!(!dock.open("absent"));
        assert!(dock.open("term-2"));
        assert_eq!(dock.active().unwrap().handle, "term-2");
    }

    #[test]
    fn missing_host_and_malformed_handle_fail_before_process_spawn() {
        let runtime = tempfile::tempdir().unwrap();
        let bridge = RuntimeCliBridge::new(runtime.path().join("missing-cli.exe"), runtime.path());
        assert_eq!(
            bridge.list_terminals().unwrap_err().kind(),
            io::ErrorKind::NotConnected,
        );
        assert_eq!(
            bridge
                .read_terminal("bad\nhandle", None)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput,
        );
        assert!(!runtime.path().join("host.json").exists());
        assert!(!runtime.path().join("runtime-host.json").exists());
    }

    #[test]
    fn auto_poll_gate_backs_off_and_can_be_disabled_or_reset() {
        let mut gate = TerminalPollGate::default();
        assert!(gate.enabled());
        assert!(gate.tick());
        gate.failed();
        assert!(!gate.tick());
        assert!(gate.tick());
        gate.failed();
        assert!(!gate.tick());
        assert!(!gate.tick());
        assert!(gate.tick());
        for _ in 0..8 {
            gate.failed();
        }
        for _ in 0..16 {
            assert!(!gate.tick());
        }
        assert!(gate.tick());
        assert!(!gate.toggle());
        for _ in 0..20 {
            assert!(!gate.tick());
        }
        assert!(gate.toggle());
        assert!(gate.tick());
        gate.failed();
        gate.reset();
        assert!(gate.tick());
    }

    #[test]
    fn stale_async_response_never_populates_reopened_same_handle() {
        let mut dock = TerminalDock::default();
        dock.update_sessions(vec![session("term-1")]);
        assert!(dock.open("term-1"));
        let old = dock.active_ticket().unwrap();
        dock.close(0);
        assert!(dock.open("term-1"));
        let new = dock.active_ticket().unwrap();
        assert_ne!(new, old);
        assert!(!dock.apply_ticket_chunk(&old, chunk("term-1", 0, 5, "stale")));
        assert!(dock.active().unwrap().output.is_empty());
        assert!(dock.apply_ticket_chunk(&new, chunk("term-1", 0, 5, "fresh")));
        assert_eq!(dock.active().unwrap().output, "fresh");
        assert!(!dock.apply_ticket_chunk(&new, chunk("term-1", 0, 10, "duplicate")));
        assert_eq!(dock.active().unwrap().output, "fresh");
    }

    #[test]
    fn disconnected_host_preserves_transcript_and_cancels_new_reads() {
        let mut dock = TerminalDock::default();
        dock.update_sessions(vec![session("term-1")]);
        assert!(dock.open("term-1"));
        assert!(dock.apply_chunk(chunk("term-1", 0, 4, "keep")));
        let ticket = dock.active_ticket().unwrap();
        dock.update_sessions(Vec::new());
        assert!(dock.active_ticket().is_none());
        assert!(!dock.apply_ticket_chunk(&ticket, chunk("term-1", 4, 8, "stale")));
        assert_eq!(dock.active().unwrap().output, "keep");
        assert!(!dock.active().unwrap().running);
        // Reconnecting with the same Host session lets the existing tab
        // advance from its previous cursor.
        dock.update_sessions(vec![session("term-1")]);
        let new = dock.active_ticket().unwrap();
        assert_eq!(new.cursor, Some(4));
        assert_eq!(new, ticket);
        assert!(dock.apply_ticket_chunk(&new, chunk("term-1", 4, 8, "more")));
        assert_eq!(dock.active().unwrap().output, "keepmore");
    }

    #[test]
    fn reused_handle_with_different_tab_requires_explicit_new_selection() {
        let mut dock = TerminalDock::default();
        dock.update_sessions(vec![session("term-1")]);
        dock.open("term-1");
        let prior = dock.active_ticket().unwrap();
        assert!(dock.apply_chunk(chunk("term-1", 0, 4, "old!")));
        let mut replacement = session("term-1");
        replacement.tab_id = "completely-new-tab".into();
        dock.update_sessions(vec![replacement]);
        assert!(dock.active_ticket().is_none());
        assert!(!dock.active().unwrap().running);
        assert_eq!(dock.active().unwrap().output, "old!");
        assert!(!dock.apply_ticket_chunk(&prior, chunk("term-1", 4, 8, "bad!")));
        assert!(dock.open("term-1"));
        let next = dock.active_ticket().unwrap();
        assert_ne!(prior, next);
        assert!(dock.active().unwrap().output.is_empty());
        assert!(dock.apply_ticket_chunk(&next, chunk("term-1", 0, 4, "new!")));
        assert_eq!(dock.active().unwrap().output, "new!");
    }
}
