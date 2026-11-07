//! Native GPUI terminal session explorer and explicitly requested input.
//! Terminal process, PTY, authentication and session lifetime remain owned by
//! the existing Alera Runtime Host; this frontend uses CLI --no-start only.
use crate::runtime_bridge::RuntimeCliBridge;
use crate::vt_screen::VtScreen;
use serde_json::Value;
use std::ffi::OsString;
use std::io;
use std::path::PathBuf;

pub const TERMINAL_READ_BYTES: u64 = 8 * 1024;
pub const MAX_TERMINALS: usize = 128;
/// The Host session list is global. It is not a view of the open workspace.
pub const HOST_SESSION_LIST_SCOPE: &str =
    "Existing Host sessions · all workspaces · input only to selected session";
const MAX_OPEN_TABS: usize = 8;
const MAX_TRANSCRIPT_BYTES: usize = 48 * 1024;
const MAX_HOST_WORKSPACES: usize = 256;
const MAX_HOST_PATH_BYTES: usize = 4096;

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

/// A workspace already registered with the running Runtime Host. `id` is the
/// Host-assigned identifier accepted by `terminal create`; `path` is the
/// workspace root reported by the Host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisteredHostWorkspace {
    pub id: String,
    pub path: PathBuf,
}

/// Identity returned after the Host creates a terminal tab inside a registered
/// workspace. `handle` is the session id used by terminal list/read/write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatedHostTerminal {
    pub handle: String,
    pub workspace_id: String,
    pub tab_id: String,
}

pub fn list_terminal_verb() -> Vec<OsString> {
    vec![OsString::from("list")]
}

pub fn read_terminal_verb(handle: &str, cursor: Option<u64>) -> io::Result<Vec<OsString>> {
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
    Ok(args)
}

impl RuntimeCliBridge {
    pub fn list_terminals(&self) -> io::Result<Vec<TerminalSession>> {
        let response = self.terminal_json(&list_terminal_verb())?;
        parse_terminal_list(&response)
    }

    pub fn read_terminal(&self, handle: &str, cursor: Option<u64>) -> io::Result<TerminalChunk> {
        let args = read_terminal_verb(handle, cursor)?;
        let response = self.terminal_json(&args)?;
        parse_terminal_chunk(&response, handle)
    }

    /// Write to one explicitly selected Host session using the existing CLI.
    /// This never creates, attaches, owns or terminates a PTY.
    pub fn write_terminal(&self, handle: &str, text: &str, enter: bool) -> io::Result<()> {
        let args = write_terminal_verb(handle, text, enter)?;
        self.terminal_json_with_input(&args, text.as_bytes())?;
        Ok(())
    }

    /// List workspaces already registered with the running Host. Uses
    /// --no-start only; a missing Host is an error, never an implicit start.
    pub fn list_registered_workspaces(&self) -> io::Result<Vec<RegisteredHostWorkspace>> {
        let response = self.terminal_json(&workspaces_verb())?;
        parse_registered_workspaces(&response)
    }

    /// Create one Host-owned terminal tab inside an already-registered
    /// workspace and return its session identity. The Host owns the PTY; this
    /// bridge only relays the explicit request through the existing CLI.
    pub fn create_terminal(&self, workspace_id: &str) -> io::Result<CreatedHostTerminal> {
        let args = create_terminal_verb(workspace_id)?;
        let response = self.terminal_json(&args)?;
        parse_created_terminal(&response, workspace_id)
    }
}

/// Construct an explicit write to an already-selected Host terminal.
/// No shell is involved. Text is sent via the child stdin pipe, never argv
/// (which could be visible to unrelated process-list observers).
pub fn write_terminal_verb(handle: &str, text: &str, enter: bool) -> io::Result<Vec<OsString>> {
    const MAX_INPUT_BYTES: usize = 4 * 1024;
    if !valid_handle(handle) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid terminal handle",
        ));
    }
    if text.is_empty() || text.len() > MAX_INPUT_BYTES || text.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Terminal input must be nonempty, NUL-free and at most 4 KiB",
        ));
    }
    let mut args = vec![
        OsString::from("write"),
        OsString::from(format!("--handle={handle}")),
        OsString::from("--stdin"),
    ];
    if enter {
        args.push(OsString::from("--enter"));
    }
    Ok(args)
}

/// List workspaces registered with the running Host. No arguments beyond the
/// verb itself are permitted by the bridge allowlist.
pub fn workspaces_verb() -> Vec<OsString> {
    vec![OsString::from("workspaces")]
}

/// Explicit terminal creation inside an existing Host workspace. The CLI
/// still requires --no-start and an already-running Host; this bridge never
/// starts a runtime, reads a token, or spawns a shell itself.
pub fn create_terminal_verb(workspace_id: &str) -> io::Result<Vec<OsString>> {
    if !valid_host_id(workspace_id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid workspace id",
        ));
    }
    Ok(vec![
        OsString::from("create"),
        OsString::from(format!("--workspace-id={workspace_id}")),
    ])
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

/// Parse `terminal workspaces` output: `{items:[{id,path}]}`. Ids and paths
/// are bounded, duplicate ids are rejected, and the item count is capped so a
/// malformed Host reply cannot hand the UI an unbounded list.
pub fn parse_registered_workspaces(value: &Value) -> io::Result<Vec<RegisteredHostWorkspace>> {
    let items = value
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("CLI workspace list has no items"))?;
    if items.len() > MAX_HOST_WORKSPACES {
        return Err(invalid("Too many registered workspaces for preview"));
    }
    let mut workspaces = Vec::with_capacity(items.len());
    for item in items {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| valid_host_id(id))
            .ok_or_else(|| invalid("Registered workspace has invalid id"))?;
        let path = item
            .get("path")
            .and_then(Value::as_str)
            .filter(|path| valid_host_path(path))
            .ok_or_else(|| invalid("Registered workspace has invalid path"))?;
        if workspaces
            .iter()
            .any(|workspace: &RegisteredHostWorkspace| workspace.id == id)
        {
            return Err(invalid("Duplicate registered workspace id"));
        }
        workspaces.push(RegisteredHostWorkspace {
            id: id.to_string(),
            path: PathBuf::from(path),
        });
    }
    Ok(workspaces)
}

/// Parse a `terminal create` reply of the form
/// `{tab:{id,workspaceId},attachment:{sessionId,workspaceId,tabId}}` and prove
/// it belongs to the workspace that was requested. A reply for a different
/// workspace or tab is malformed rather than silently attached.
pub fn parse_created_terminal(
    value: &Value,
    expected_workspace_id: &str,
) -> io::Result<CreatedHostTerminal> {
    let tab = value
        .get("tab")
        .ok_or_else(|| invalid("CLI create response has no tab"))?;
    let tab_id = tab
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| valid_host_id(id))
        .ok_or_else(|| invalid("CLI create response tab id is invalid"))?;
    let tab_workspace = tab
        .get("workspaceId")
        .and_then(Value::as_str)
        .filter(|id| valid_host_id(id))
        .ok_or_else(|| invalid("CLI create response tab has no workspaceId"))?;
    let attachment = value
        .get("attachment")
        .ok_or_else(|| invalid("CLI create response has no attachment"))?;
    let handle = attachment
        .get("sessionId")
        .and_then(Value::as_str)
        .filter(|handle| valid_handle(handle))
        .ok_or_else(|| invalid("CLI create response session handle is invalid"))?;
    let attachment_workspace = attachment
        .get("workspaceId")
        .and_then(Value::as_str)
        .filter(|id| valid_host_id(id))
        .ok_or_else(|| invalid("CLI create response attachment has no workspaceId"))?;
    let attachment_tab = attachment
        .get("tabId")
        .and_then(Value::as_str)
        .filter(|id| valid_host_id(id))
        .ok_or_else(|| invalid("CLI create response attachment has no tabId"))?;
    if tab_workspace != expected_workspace_id
        || attachment_workspace != expected_workspace_id
        || attachment_tab != tab_id
    {
        return Err(invalid(
            "CLI create response does not match the requested workspace",
        ));
    }
    Ok(CreatedHostTerminal {
        handle: handle.to_string(),
        workspace_id: expected_workspace_id.to_string(),
        tab_id: tab_id.to_string(),
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

/// Only the exact bounded ASCII identifier syntax accepted by the paired
/// Alera CLI is forwarded, so a workspace cannot appear selectable then fail
/// at the create boundary.
fn valid_host_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 256
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

/// Registered workspace paths are bounded identity data; a malformed Host
/// reply must not hand the UI an unbounded or control-laden string.
fn valid_host_path(path: &str) -> bool {
    !path.is_empty() && path.len() <= MAX_HOST_PATH_BYTES && !path.chars().any(|c| c.is_control())
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

#[derive(Debug)]
pub struct TerminalTab {
    pub handle: String,
    pub tab_id: String,
    pub workspace_id: String,
    pub title: String,
    pub running: bool,
    pub output: String,
    pub next_cursor: Option<u64>,
    pub screen: VtScreen,
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
            screen: VtScreen::default(),
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
                self.screen.reset();
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
            self.screen.reset();
            self.output
                .push_str("\n[Earlier terminal scrollback was truncated]\n");
        }
        // Replay exactly the Host bytes returned in this chunk into the VT
        // screen before stripping escapes from the optional text transcript.
        self.screen.feed(&chunk.text);
        self.decoder.append(&mut self.output, &chunk.text);
        self.next_cursor = Some(chunk.next_cursor);
        self.running = chunk.running;
        true
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum TerminalViewMode {
    #[default]
    Screen,
    Transcript,
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
    pub view_mode: TerminalViewMode,
    next_generation: u64,
    workspace_generation: u64,
}

impl TerminalDock {
    pub fn for_workspace(generation: u64) -> Self {
        Self {
            workspace_generation: generation,
            ..Self::default()
        }
    }

    pub fn workspace_generation(&self) -> u64 {
        self.workspace_generation
    }

    /// Drop a list reply captured for a workspace this dock no longer shows.
    pub fn apply_session_list(&mut self, generation: u64, sessions: Vec<TerminalSession>) -> bool {
        if generation != self.workspace_generation {
            return false;
        }
        self.update_sessions(sessions);
        true
    }

    /// Drop a read reply from an older workspace generation or a replaced session.
    pub fn apply_workspace_read(
        &mut self,
        generation: u64,
        ticket: &TerminalReadTicket,
        chunk: TerminalChunk,
    ) -> bool {
        if generation != self.workspace_generation {
            return false;
        }
        self.apply_ticket_chunk(ticket, chunk)
    }

    pub fn toggle_view_mode(&mut self) {
        self.view_mode = match self.view_mode {
            TerminalViewMode::Screen => TerminalViewMode::Transcript,
            TerminalViewMode::Transcript => TerminalViewMode::Screen,
        };
    }

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
        self.status = format!(
            "{} existing terminal(s) · all workspaces",
            self.sessions.len()
        );
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

    /// Reorder a local preview tab. The tab keeps its transcript, cursor and
    /// VT screen state, and the active selection follows the same terminal
    /// tab rather than the numeric slot. Returns false for out-of-range
    /// indexes. This never touches Host PTYs.
    pub fn move_tab(&mut self, from: usize, to: usize) -> bool {
        if from >= self.tabs.len() || to >= self.tabs.len() {
            return false;
        }
        if from == to {
            return true;
        }
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        if let Some(active) = self.active_tab {
            let shifted = if active == from {
                to
            } else if from < active && active <= to {
                active - 1
            } else if to <= active && active < from {
                active + 1
            } else {
                active
            };
            self.active_tab = Some(shifted);
        }
        true
    }

    /// Activate the next local preview tab, wrapping around. Returns false
    /// when no tabs are open.
    pub fn activate_next(&mut self) -> bool {
        let len = self.tabs.len();
        if len == 0 {
            return false;
        }
        let next = self.active_tab.map_or(0, |active| (active + 1) % len);
        self.active_tab = Some(next);
        true
    }

    /// Activate the previous local preview tab, wrapping around. Returns
    /// false when no tabs are open.
    pub fn activate_previous(&mut self) -> bool {
        let len = self.tabs.len();
        if len == 0 {
            return false;
        }
        let previous = self
            .active_tab
            .map_or(len - 1, |active| (active + len - 1) % len);
        self.active_tab = Some(previous);
        true
    }

    /// Close every local preview tab except `keep`. Returns the number of
    /// tabs closed. Host sessions and PTYs are never terminated.
    pub fn close_others(&mut self, keep: usize) -> usize {
        if keep >= self.tabs.len() {
            return 0;
        }
        let closed = self.tabs.len() - 1;
        let kept = self.tabs.remove(keep);
        self.tabs.clear();
        self.tabs.push(kept);
        self.active_tab = Some(0);
        closed
    }

    /// Close local preview tabs to the right of `keep` (higher indexes).
    /// Returns the number of tabs closed. Host sessions and PTYs are never
    /// terminated.
    pub fn close_to_right(&mut self, keep: usize) -> usize {
        if keep >= self.tabs.len() {
            return 0;
        }
        let closed = self.tabs.len() - keep - 1;
        self.tabs.truncate(keep + 1);
        if let Some(active) = self.active_tab {
            if active > keep {
                self.active_tab = Some(keep);
            }
        }
        closed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn terminal_writes_are_explicit_bounded_and_never_start_host() {
        use crate::runtime_bridge::terminal_cli_arguments;
        use std::path::Path;
        let args = write_terminal_verb("term-1", "echo 貓", true).unwrap();
        assert_eq!(args[0], "write");
        assert!(args.iter().any(|arg| arg == "--enter"));
        assert!(args.iter().any(|arg| arg == "--stdin"));
        assert!(!args.iter().any(|arg| arg == "echo 貓" || arg == "--text"));
        let routed = terminal_cli_arguments(Path::new("C:/runtime"), &args);
        assert!(routed.iter().any(|arg| arg == "--no-start"));
        assert!(!routed.iter().any(|arg| arg == "attach"));
        assert!(!write_terminal_verb("term-1", "echo ok", false)
            .unwrap()
            .iter()
            .any(|arg| arg == "--enter"));
        for handle in ["", "evil\n--handle=other", "../bad"] {
            assert!(write_terminal_verb(handle, "cmd", true).is_err());
        }
        for text in ["", "x\0y"] {
            assert!(write_terminal_verb("term-1", text, true).is_err());
        }
        assert!(write_terminal_verb("term-1", &"a".repeat(4097), true).is_err());
    }

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
    fn workspace_and_create_verbs_are_bounded_and_never_start_host() {
        use crate::runtime_bridge::terminal_cli_arguments;
        use std::path::Path;
        assert_eq!(workspaces_verb(), vec![OsString::from("workspaces")]);
        let args = create_terminal_verb("ws-123").unwrap();
        assert_eq!(args[0], "create");
        assert_eq!(args[1], "--workspace-id=ws-123");
        let routed = terminal_cli_arguments(Path::new("C:/runtime"), &args);
        assert!(routed.iter().any(|arg| arg == "terminal"));
        assert!(routed.iter().any(|arg| arg == "--no-start"));
        assert!(routed.iter().any(|arg| arg == "--json"));
        assert!(!routed.iter().any(|arg| arg == "attach" || arg == "prune"));
        for id in ["", "bad\nid", "bad\0id"] {
            assert!(create_terminal_verb(id).is_err());
        }
        assert!(create_terminal_verb(&"x".repeat(257)).is_err());
        assert!(create_terminal_verb(&"x".repeat(256)).is_ok());
    }

    #[test]
    fn parses_registered_workspace_list_shape() {
        let value = json!({
            "items": [
                {"id":"ws-1", "path":"C:/repo/one"},
                {"id":"ws-2", "path":"/home/user/two"}
            ]
        });
        let items = parse_registered_workspaces(&value).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "ws-1");
        assert_eq!(items[0].path, PathBuf::from("C:/repo/one"));
        assert_eq!(items[1].path, PathBuf::from("/home/user/two"));
        assert!(parse_registered_workspaces(&json!({"items":[{"id":"ws-1"}]})).is_err());
        assert!(parse_registered_workspaces(&json!({"items":[{"id":"ws-1","path":""}]})).is_err());
        assert!(parse_registered_workspaces(&json!({"kind":"workspaces"})).is_err());
        assert!(parse_registered_workspaces(&json!({"items":[
            {"id":"ws-1","path":"/a"},{"id":"ws-1","path":"/b"}
        ]}))
        .is_err());
        let oversized = json!({"items": (0..=MAX_HOST_WORKSPACES)
            .map(|i| json!({"id":format!("ws-{i}"),"path":"/p"}))
            .collect::<Vec<_>>()});
        assert!(parse_registered_workspaces(&oversized).is_err());
    }

    #[test]
    fn parses_created_terminal_and_rejects_workspace_mismatch() {
        let value = json!({
            "tab":{"id":"tab-1","workspaceId":"ws-1"},
            "attachment":{"sessionId":"term-9","workspaceId":"ws-1","tabId":"tab-1"}
        });
        let created = parse_created_terminal(&value, "ws-1").unwrap();
        assert_eq!(created.handle, "term-9");
        assert_eq!(created.workspace_id, "ws-1");
        assert_eq!(created.tab_id, "tab-1");
        // A reply for a different requested workspace is malformed.
        assert!(parse_created_terminal(&value, "ws-2").is_err());
        let spoofed_workspace = json!({
            "tab":{"id":"tab-1","workspaceId":"ws-2"},
            "attachment":{"sessionId":"term-9","workspaceId":"ws-1","tabId":"tab-1"}
        });
        assert!(parse_created_terminal(&spoofed_workspace, "ws-1").is_err());
        let mismatched_tab = json!({
            "tab":{"id":"tab-1","workspaceId":"ws-1"},
            "attachment":{"sessionId":"term-9","workspaceId":"ws-1","tabId":"tab-2"}
        });
        assert!(parse_created_terminal(&mismatched_tab, "ws-1").is_err());
        assert!(parse_created_terminal(
            &json!({"tab":{"id":"tab-1","workspaceId":"ws-1"}}),
            "ws-1"
        )
        .is_err());
        let bad_handle = json!({
            "tab":{"id":"tab-1","workspaceId":"ws-1"},
            "attachment":{"sessionId":"bad\nhandle","workspaceId":"ws-1","tabId":"tab-1"}
        });
        assert!(parse_created_terminal(&bad_handle, "ws-1").is_err());
    }

    #[test]
    fn workspace_and_create_calls_fail_before_spawn_without_host() {
        let runtime = tempfile::tempdir().unwrap();
        let bridge = RuntimeCliBridge::new(runtime.path().join("missing-cli"), runtime.path());
        assert_eq!(
            bridge.create_terminal("bad\nid").unwrap_err().kind(),
            io::ErrorKind::InvalidInput,
        );
        assert_eq!(
            bridge.create_terminal("ws-1").unwrap_err().kind(),
            io::ErrorKind::NotConnected,
        );
        assert_eq!(
            bridge.list_registered_workspaces().unwrap_err().kind(),
            io::ErrorKind::NotConnected,
        );
        assert!(!runtime.path().join("host.json").exists());
        assert!(!runtime.path().join("runtime-host.json").exists());
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
    fn real_vt_screen_tracks_cursor_while_text_log_remains_readable() {
        let mut dock = TerminalDock::default();
        assert_eq!(dock.view_mode, TerminalViewMode::Screen);
        dock.update_sessions(vec![session("term-1")]);
        dock.open("term-1");
        assert!(dock.apply_chunk(chunk("term-1", 0, 10, "hello\x1b[2DXY")));
        let tab = dock.active().unwrap();
        assert!(tab.screen.contents().starts_with("helXY"));
        assert!(!tab.screen.contents().contains("\x1b"));
        assert_eq!(tab.output, "helloXY");
        dock.toggle_view_mode();
        assert_eq!(dock.view_mode, TerminalViewMode::Transcript);
        dock.toggle_view_mode();
        assert_eq!(dock.view_mode, TerminalViewMode::Screen);
    }

    #[test]
    fn truncated_host_scroll_reinitializes_vt_state_and_text_decoder() {
        let mut dock = TerminalDock::default();
        dock.update_sessions(vec![session("term-1")]);
        dock.open("term-1");
        dock.apply_chunk(chunk("term-1", 0, 12, "old\x1b[?1049hAPP"));
        assert!(dock.active().unwrap().screen.alternate_screen());
        let mut snapshot = chunk("term-1", 12, 25, "new\n");
        snapshot.truncated = true;
        dock.apply_chunk(snapshot);
        assert!(!dock.active().unwrap().screen.alternate_screen());
        assert!(dock.active().unwrap().screen.contents().contains("new"));
        assert!(!dock.active().unwrap().screen.contents().contains("APP"));
        assert!(dock.active().unwrap().output.contains("truncated"));
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

    #[test]
    fn move_tab_reorders_tabs_and_keeps_active_identity_and_state() {
        let mut dock = TerminalDock::default();
        dock.update_sessions(vec![
            session("term-1"),
            session("term-2"),
            session("term-3"),
        ]);
        dock.open("term-1");
        dock.open("term-2");
        dock.open("term-3");
        dock.apply_chunk(chunk("term-1", 0, 3, "one"));
        dock.apply_chunk(chunk("term-3", 0, 5, "three\x1b[31m!"));
        assert_eq!(dock.active_tab, Some(2));

        // Drag the active tab to the front: the index follows the terminal.
        assert!(dock.move_tab(2, 0));
        assert_eq!(dock.tabs[0].handle, "term-3");
        assert_eq!(dock.active_tab, Some(0));
        let active = dock.active().unwrap();
        assert_eq!(active.handle, "term-3");
        assert_eq!(active.output, "three!");
        assert_eq!(active.next_cursor, Some(5));
        assert!(active.screen.contents().contains('!'));

        // Drag another tab across the active one; selection still tracks
        // the same terminal tab rather than the numeric slot.
        assert!(dock.move_tab(2, 1));
        assert_eq!(dock.active_tab, Some(0));
        assert_eq!(dock.active().unwrap().handle, "term-3");

        assert!(dock.move_tab(0, 2));
        assert_eq!(dock.active_tab, Some(2));
        assert_eq!(dock.active().unwrap().handle, "term-3");
        assert_eq!(dock.active().unwrap().output, "three!");
        assert!(!dock.move_tab(3, 0));
        assert!(!dock.move_tab(0, 3));
    }

    #[test]
    fn activate_next_and_previous_wrap_around_and_tolerate_empty_dock() {
        let mut dock = TerminalDock::default();
        assert!(!dock.activate_next());
        assert!(!dock.activate_previous());

        dock.update_sessions(vec![session("term-1"), session("term-2")]);
        dock.open("term-1");
        dock.open("term-2");
        assert_eq!(dock.active_tab, Some(1));
        assert!(dock.activate_previous());
        assert_eq!(dock.active_tab, Some(0));
        assert!(dock.activate_previous());
        assert_eq!(dock.active_tab, Some(1));
        assert!(dock.activate_next());
        assert_eq!(dock.active_tab, Some(0));
        assert!(dock.activate_next());
        assert_eq!(dock.active_tab, Some(1));

        // A single remaining tab wraps onto itself without error.
        dock.close(0);
        assert_eq!(dock.tabs.len(), 1);
        assert!(dock.activate_next());
        assert_eq!(dock.active_tab, Some(0));
        assert!(dock.activate_previous());
        assert_eq!(dock.active_tab, Some(0));

        dock.close(0);
        assert!(!dock.activate_next());
        assert!(!dock.activate_previous());
    }

    #[test]
    fn close_others_and_close_to_right_drop_only_local_preview_tabs() {
        let mut dock = TerminalDock::default();
        dock.update_sessions(vec![
            session("term-1"),
            session("term-2"),
            session("term-3"),
        ]);
        dock.open("term-1");
        dock.open("term-2");
        dock.open("term-3");
        dock.activate(0);

        assert_eq!(dock.close_others(1), 2);
        assert_eq!(dock.tabs.len(), 1);
        assert_eq!(dock.tabs[0].handle, "term-2");
        assert_eq!(dock.active_tab, Some(0));
        // Closing previews never removes Host sessions or terminates PTYs.
        assert_eq!(dock.sessions.len(), 3);

        dock.open("term-1");
        dock.open("term-3");
        assert_eq!(dock.tabs.len(), 3);
        assert_eq!(dock.active_tab, Some(2));
        assert_eq!(dock.close_to_right(0), 2);
        assert_eq!(dock.tabs.len(), 1);
        assert_eq!(dock.tabs[0].handle, "term-2");
        assert_eq!(dock.active_tab, Some(0));
        assert_eq!(dock.sessions.len(), 3);

        assert_eq!(dock.close_others(9), 0);
        assert_eq!(dock.close_to_right(9), 0);
        assert_eq!(dock.tabs.len(), 1);
    }

    #[test]
    fn moved_tab_keeps_ticket_valid_but_bulk_close_makes_it_stale() {
        let mut dock = TerminalDock::default();
        dock.update_sessions(vec![
            session("term-1"),
            session("term-2"),
            session("term-3"),
        ]);
        dock.open("term-1");
        dock.open("term-2");
        dock.open("term-3");
        dock.activate(1);
        let moved = dock.active_ticket().unwrap();

        // Reordering preserves generation and cursor, so an in-flight read
        // for the moved tab still applies to the same terminal.
        assert!(dock.move_tab(1, 0));
        assert!(dock.apply_ticket_chunk(&moved, chunk("term-2", 0, 4, "fine")));
        assert_eq!(dock.tabs[0].output, "fine");

        dock.activate(2);
        let dropped = dock.active_ticket().unwrap();
        assert_eq!(dropped.handle, "term-3");
        assert_eq!(dock.close_others(0), 2);
        assert!(!dock.apply_ticket_chunk(&dropped, chunk("term-3", 0, 3, "stale")));
        assert_eq!(dock.tabs.len(), 1);
        assert_eq!(dock.tabs[0].output, "fine");
    }
}
