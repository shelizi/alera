//! Native GPUI terminal session explorer and explicitly requested input.
//! Terminal process, PTY, authentication and session lifetime remain owned by
//! the existing Alera Runtime Host; this frontend uses CLI --no-start only.
use crate::runtime_bridge::RuntimeCliBridge;
use crate::session::TerminalLayout;
use crate::vt_screen::VtScreen;
use serde_json::Value;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

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

impl CreatedHostTerminal {
    /// A Host creation reply is not sufficient to attach to a PTY: the next
    /// authenticated session list must corroborate all three identities.
    pub fn matches_live_session(&self, session: &TerminalSession) -> bool {
        session.handle == self.handle
            && session.workspace_id == self.workspace_id
            && session.tab_id == self.tab_id
    }
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

    /// Request only explicit dimensions for an already known Host terminal.
    /// Host mobile-driver lease semantics may defer actual PTY resizing.
    pub fn resize_terminal(&self, handle: &str, cols: u16, rows: u16) -> io::Result<()> {
        let args = resize_terminal_verb(handle, cols, rows)?;
        self.terminal_json(&args)?;
        Ok(())
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
/// Encode native key-down as raw Host PTY input. Terminal typing and
/// command-bar submit are intentionally distinct; no implicit Enter here.
pub fn encode_terminal_key(
    key: &str,
    key_char: Option<&str>,
    control: bool,
    alt: bool,
    platform: bool,
) -> Option<String> {
    if platform || alt {
        return None;
    }
    let lower = key.to_ascii_lowercase();
    if control {
        if lower.len() == 1 {
            let byte = lower.as_bytes()[0];
            if byte.is_ascii_lowercase() {
                return Some(String::from(char::from(byte - b'a' + 1)));
            }
        }
        return None;
    }
    let sequence = match lower.as_str() {
        "enter" | "return" => "\r",
        "backspace" => "\u{7f}",
        "tab" => "\t",
        "escape" | "esc" => "\u{1b}",
        "up" | "arrowup" => "\u{1b}[A",
        "down" | "arrowdown" => "\u{1b}[B",
        "right" | "arrowright" => "\u{1b}[C",
        "left" | "arrowleft" => "\u{1b}[D",
        "home" => "\u{1b}[H",
        "end" => "\u{1b}[F",
        "delete" => "\u{1b}[3~",
        "pageup" => "\u{1b}[5~",
        "pagedown" => "\u{1b}[6~",
        _ => {
            let printable = key_char.or_else(|| (key.chars().count() == 1).then_some(key))?;
            if printable.is_empty()
                || printable.len() > 128
                || printable.chars().any(char::is_control)
            {
                return None;
            }
            return Some(printable.to_owned());
        }
    };
    Some(sequence.to_owned())
}

pub fn resize_terminal_verb(handle: &str, cols: u16, rows: u16) -> io::Result<Vec<OsString>> {
    if !valid_handle(handle) || !(20..=512).contains(&cols) || !(5..=200).contains(&rows) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid Host terminal viewport",
        ));
    }
    Ok(vec![
        OsString::from("resize"),
        OsString::from(format!("--handle={handle}")),
        OsString::from(format!("--cols={cols}")),
        OsString::from(format!("--rows={rows}")),
    ])
}

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
            .filter(|id| valid_host_id(id))
            .ok_or_else(|| invalid("Terminal session has no workspaceId"))?;
        let tab_id = item
            .get("tabId")
            .and_then(Value::as_str)
            .filter(|id| valid_host_id(id))
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

/// Resolve exactly one Host-registered workspace for this already-canonical
/// project path. Never infer an ID from a repo name, Git branch or CLI argv.
/// No Host mutation or process start occurs here.
pub fn select_registered_host_workspace(
    root: &Path,
    registered: &[RegisteredHostWorkspace],
) -> io::Result<String> {
    let active = dunce::canonicalize(root)?;
    if active != root || !active.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Active workspace must be a canonical existing directory",
        ));
    }
    let mut selected: Option<&str> = None;
    for entry in registered {
        // Workspace paths from an authenticated Host are still untrusted
        // identity data. Relative paths must never resolve from GPUI's CWD.
        if !entry.path.is_absolute() {
            continue;
        }
        if dunce::canonicalize(&entry.path).ok().as_deref() != Some(active.as_path()) {
            continue;
        }
        if selected.is_some() {
            return Err(invalid(
                "Multiple Host workspaces match this project; terminal create refused",
            ));
        }
        selected = Some(&entry.id);
    }
    selected.map(ToString::to_string).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Current project is not registered with the running Alera Host",
        )
    })
}

/// Parse the real Host `terminal create` reply:
/// `{tab:{id,workspaceId,payload:{terminalSessionId}},attachment:{sessionId}}`.
/// Older Hosts may also include attachment.workspaceId/tabId instead of the
/// tab payload's terminalSessionId. Accept either *corroborated* identity
/// shape; never trust the bare attachment sessionId or mismatched fields.
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
        .map(|value| {
            value
                .as_str()
                .filter(|id| valid_host_id(id))
                .ok_or_else(|| invalid("CLI create response attachment has invalid workspaceId"))
        })
        .transpose()?;
    let attachment_tab = attachment
        .get("tabId")
        .map(|value| {
            value
                .as_str()
                .filter(|id| valid_host_id(id))
                .ok_or_else(|| invalid("CLI create response attachment has invalid tabId"))
        })
        .transpose()?;
    let tab_session = tab
        .get("payload")
        .and_then(|payload| payload.get("terminalSessionId"))
        .map(|value| {
            value
                .as_str()
                .filter(|id| valid_handle(id))
                .ok_or_else(|| invalid("CLI create response tab has invalid terminalSessionId"))
        })
        .transpose()?;
    if tab_workspace != expected_workspace_id
        || attachment_workspace.is_some_and(|id| id != expected_workspace_id)
        || attachment_tab.is_some_and(|id| id != tab_id)
        || tab_session.is_some_and(|id| id != handle)
    {
        return Err(invalid(
            "CLI create response does not match the requested workspace",
        ));
    }
    // The active Host omits attachment.workspaceId/tabId; its tab payload
    // independently binds the session to the already verified workspace/tab.
    // Legacy replies need both attachment IDs if that payload is absent.
    if tab_session != Some(handle)
        && !(attachment_workspace == Some(expected_workspace_id) && attachment_tab == Some(tab_id))
    {
        return Err(invalid(
            "CLI create response has no corroborated terminal identity",
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

/// Stable target for a Terminal tab context action. The same opaque Host
/// handle can be recycled; indexes alone do not identify a session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalTabContextTarget {
    pub handle: String,
    workspace_id: String,
    tab_id: String,
    generation: u64,
    workspace_generation: u64,
}

/// An auto-resize request is bound to one exact still-live Host tab, not to
/// whichever terminal happens to be selected when the background task runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalViewportProposal {
    pub target: TerminalTabContextTarget,
    pub cols: u16,
    pub rows: u16,
}

/// Translate the drawable terminal content region (not the whole window)
/// into bounded VT dimensions. JetBrains Mono is rendered at font_px, with
/// GPUI row line-height = 1.3 * font_px. Rounding down avoids overflow.
pub fn estimate_terminal_viewport(
    region_width_px: f32,
    region_height_px: f32,
    font_px: f32,
) -> Option<(u16, u16)> {
    if !region_width_px.is_finite()
        || !region_height_px.is_finite()
        || !font_px.is_finite()
        || !(8.0..=48.0).contains(&font_px)
    {
        return None;
    }
    let columns = ((region_width_px - 24.0) / (font_px * 0.62)).floor();
    let lines = ((region_height_px - 16.0) / (font_px * 1.3)).floor();
    if columns < 20.0 || lines < 5.0 {
        return None;
    }
    Some((columns.min(240.0) as u16, lines.min(100.0) as u16))
}

/// A single, bounded per-Workbench debounce/coalescing gate.  Only visible
/// Host tab identities are observed; each unique viewport is sent at most
/// once until its dimensions change. A failed Host request is not spammed
/// repeatedly while the user is idle.
#[derive(Debug, Default)]
pub struct TerminalAutoResizeGate {
    pending: Vec<(TerminalViewportProposal, std::time::Instant)>,
    attempted: Vec<TerminalViewportProposal>,
}

impl TerminalAutoResizeGate {
    pub fn clear(&mut self) {
        self.pending.clear();
        self.attempted.clear();
    }

    /// Explicit manual viewport presets win over a pending auto-resize until
    /// the displayed pane size or font changes again.
    pub fn suspend_until_changed(&mut self, target: &TerminalTabContextTarget) {
        if let Some((current, _)) = self
            .pending
            .iter()
            .find(|(current, _)| &current.target == target)
        {
            let current = current.clone();
            self.attempted.retain(|old| &old.target != target);
            self.attempted.push(current);
        }
        self.pending
            .retain(|(current, _)| &current.target != target);
    }

    pub fn observe(&mut self, visible: Vec<TerminalViewportProposal>, now: std::time::Instant) {
        self.pending
            .retain(|(pending, _)| visible.iter().any(|new| new.target == pending.target));
        self.attempted
            .retain(|attempted| visible.iter().any(|new| new.target == attempted.target));
        for new in visible.into_iter().take(2) {
            if self.attempted.iter().any(|attempted| attempted == &new) {
                self.pending
                    .retain(|(pending, _)| pending.target != new.target);
                continue;
            }
            if let Some((current, since)) = self
                .pending
                .iter_mut()
                .find(|(current, _)| current.target == new.target)
            {
                if *current != new {
                    *current = new;
                    *since = now;
                }
            } else {
                self.pending.push((new, now));
            }
        }
    }

    pub fn take_ready(&mut self, now: std::time::Instant) -> Vec<TerminalViewportProposal> {
        const DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(300);
        let mut ready = Vec::new();
        self.pending.retain(|(change, since)| {
            if now.saturating_duration_since(*since) >= DEBOUNCE {
                ready.push(change.clone());
                false
            } else {
                true
            }
        });
        for change in &ready {
            self.attempted
                .retain(|previous| previous.target != change.target);
            self.attempted.push(change.clone());
        }
        ready
    }
}

/// VT screen selection is a visible-row selection, not a shell command and
/// not scrollback. Keeping a text snapshot prevents later Host redraws from
/// silently changing what Copy means; a changed Host read cursor invalidates
/// it. This first implementation selects whole rows, not character slices.
#[derive(Debug, Clone)]
pub struct TerminalLineSelection {
    target: TerminalTabContextTarget,
    output_cursor: Option<u64>,
    rows: Vec<String>,
    anchor_row: usize,
    current_row: usize,
    dragging: bool,
}

impl TerminalLineSelection {
    pub fn begin(
        target: TerminalTabContextTarget,
        output_cursor: Option<u64>,
        rows: Vec<String>,
        row: usize,
    ) -> Option<Self> {
        if rows.is_empty()
            || rows.len() > 100
            || row >= rows.len()
            || rows.iter().any(|text| text.len() > 4096)
        {
            return None;
        }
        Some(Self {
            target,
            output_cursor,
            rows,
            anchor_row: row,
            current_row: row,
            dragging: true,
        })
    }

    pub fn is_current(&self, target: &TerminalTabContextTarget, cursor: Option<u64>) -> bool {
        self.target == *target && self.output_cursor == cursor
    }

    pub fn extend(&mut self, row: usize) -> bool {
        if !self.dragging || row >= self.rows.len() {
            return false;
        }
        let changed = self.current_row != row;
        self.current_row = row;
        changed
    }

    pub fn finish(&mut self) {
        self.dragging = false;
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    pub fn includes_row(&self, row: usize) -> bool {
        (self.anchor_row.min(self.current_row)..=self.anchor_row.max(self.current_row))
            .contains(&row)
    }

    pub fn selected_text(&self) -> String {
        let from = self.anchor_row.min(self.current_row);
        let to = self.anchor_row.max(self.current_row);
        self.rows[from..=to]
            .iter()
            .map(|row| row.trim_end_matches(' '))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// A late reply for PTY A may not discard pending keys for PTY B. Only
/// the exact still-live active session may receive queued raw input; a
/// failure for the *same* session stops its own queue.
pub fn raw_key_reply_may_drain(
    sent: &TerminalTabContextTarget,
    queued: Option<&TerminalTabContextTarget>,
    active: Option<&TerminalTabContextTarget>,
    active_is_live: bool,
    send_succeeded: bool,
) -> bool {
    match (queued, active) {
        (Some(queued), Some(active)) if active_is_live && queued == active => {
            sent != active || send_succeeded
        }
        _ => false,
    }
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

/// The native UI polls every 1.5 seconds. Never issue another create RPC just
/// because Host list propagation is delayed: only re-list the existing Host.
pub const TERMINAL_CONFIRM_TIMEOUT_TICKS: u8 = 20;
const TERMINAL_CONFIRM_REFRESH_INTERVAL_TICKS: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalCreateTick {
    None,
    RefreshSessions,
    TimedOut,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum TerminalCreatePhase {
    #[default]
    Idle,
    Creating {
        request_id: u64,
    },
    Awaiting {
        created: CreatedHostTerminal,
        ticks: u8,
    },
    TimedOut {
        created: CreatedHostTerminal,
    },
}

/// Local view-only lifecycle for a Host-owned Terminal creation RPC.
/// Cancel only invalidates UI callbacks; it never terminates or creates a PTY.
#[derive(Debug, Default)]
pub struct TerminalCreateFlow {
    phase: TerminalCreatePhase,
    next_request_id: u64,
}

impl TerminalCreateFlow {
    pub fn is_active(&self) -> bool {
        !matches!(self.phase, TerminalCreatePhase::Idle)
    }

    pub fn is_creating(&self) -> bool {
        matches!(self.phase, TerminalCreatePhase::Creating { .. })
    }

    pub fn is_waiting(&self) -> bool {
        matches!(self.phase, TerminalCreatePhase::Awaiting { .. })
    }

    pub fn is_timed_out(&self) -> bool {
        matches!(self.phase, TerminalCreatePhase::TimedOut { .. })
    }

    /// Returns a unique local request ID; repeated clicks while pending or
    /// timed out are ignored instead of spawning duplicate Host PTYs.
    pub fn start_request(&mut self) -> Option<u64> {
        if self.is_active() {
            return None;
        }
        self.next_request_id = self.next_request_id.wrapping_add(1).max(1);
        let request_id = self.next_request_id;
        self.phase = TerminalCreatePhase::Creating { request_id };
        Some(request_id)
    }

    /// Reject any result from an operation canceled/replaced in the meantime.
    pub fn finish_request(&mut self, request_id: u64, created: CreatedHostTerminal) -> bool {
        if !matches!(self.phase, TerminalCreatePhase::Creating { request_id: active } if active == request_id)
        {
            return false;
        }
        self.phase = TerminalCreatePhase::Awaiting { created, ticks: 0 };
        true
    }

    pub fn fail_request(&mut self, request_id: u64) -> bool {
        if !matches!(self.phase, TerminalCreatePhase::Creating { request_id: active } if active == request_id)
        {
            return false;
        }
        self.phase = TerminalCreatePhase::Idle;
        true
    }

    /// Only an exact triple identity from an authenticated Host list can
    /// complete attachment. A failed/missing list preserves this expectation.
    pub fn confirm_sessions(
        &mut self,
        sessions: &[TerminalSession],
    ) -> Option<CreatedHostTerminal> {
        let TerminalCreatePhase::Awaiting { created, .. } = &self.phase else {
            return None;
        };
        if !sessions
            .iter()
            .any(|session| created.matches_live_session(session))
        {
            return None;
        }
        let confirmed = created.clone();
        self.phase = TerminalCreatePhase::Idle;
        Some(confirmed)
    }

    /// Advance once per native 1500ms tick. 20 ticks = 30s timeout.
    /// Refresh requests are read-only list operations, never create RPCs.
    pub fn tick(&mut self) -> TerminalCreateTick {
        let TerminalCreatePhase::Awaiting { created, ticks } = &mut self.phase else {
            return TerminalCreateTick::None;
        };
        *ticks = ticks.saturating_add(1);
        if *ticks >= TERMINAL_CONFIRM_TIMEOUT_TICKS {
            let created = created.clone();
            self.phase = TerminalCreatePhase::TimedOut { created };
            return TerminalCreateTick::TimedOut;
        }
        if *ticks % TERMINAL_CONFIRM_REFRESH_INTERVAL_TICKS == 0 {
            TerminalCreateTick::RefreshSessions
        } else {
            TerminalCreateTick::None
        }
    }

    /// After timeout the user can retry confirmation; no second Host create.
    pub fn retry_confirmation(&mut self) -> bool {
        let TerminalCreatePhase::TimedOut { created } = &self.phase else {
            return false;
        };
        self.phase = TerminalCreatePhase::Awaiting {
            created: created.clone(),
            ticks: 0,
        };
        true
    }

    /// Dismiss all waiting/creation UI state and reject its future callbacks.
    /// Does not send any Host session termination or process signal.
    pub fn cancel(&mut self) -> bool {
        if !self.is_active() {
            return false;
        }
        self.phase = TerminalCreatePhase::Idle;
        true
    }
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

    pub fn context_target_at(&self, index: usize) -> Option<TerminalTabContextTarget> {
        let tab = self.tabs.get(index)?;
        Some(TerminalTabContextTarget {
            handle: tab.handle.clone(),
            workspace_id: tab.workspace_id.clone(),
            tab_id: tab.tab_id.clone(),
            generation: tab.generation,
            workspace_generation: self.workspace_generation,
        })
    }

    pub fn index_for_context(&self, target: &TerminalTabContextTarget) -> Option<usize> {
        if target.workspace_generation != self.workspace_generation {
            return None;
        }
        self.tabs.iter().position(|tab| {
            tab.handle == target.handle
                && tab.workspace_id == target.workspace_id
                && tab.tab_id == target.tab_id
                && tab.generation == target.generation
        })
    }

    /// Host-owned split pane assignment requires the same still-running
    /// session in the last authenticated Host list.
    pub fn context_target_is_live(&self, target: &TerminalTabContextTarget) -> bool {
        self.index_for_context(target).is_some()
            && self.sessions.iter().any(|session| {
                session.running
                    && session.handle == target.handle
                    && session.workspace_id == target.workspace_id
                    && session.tab_id == target.tab_id
            })
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

    /// Recreate only preview tabs whose three Host identities still match.
    /// Keep their saved order and selected tab without touching Host PTYs.
    /// Once the user has opened a tab explicitly, saved state cannot replace it.
    pub fn restore_previews_from_host_layout(&mut self, layout: &TerminalLayout) -> usize {
        if !self.tabs.is_empty() || self.sessions.is_empty() {
            return 0;
        }
        self.view_mode = if layout.transcript_view {
            TerminalViewMode::Transcript
        } else {
            TerminalViewMode::Screen
        };
        let mut recovered = Vec::new();
        for saved in layout.tabs.iter().take(MAX_OPEN_TABS) {
            let exact_identity_is_live = self.sessions.iter().any(|live| {
                live.handle == saved.handle
                    && live.tab_id == saved.tab_id
                    && live.workspace_id == saved.workspace_id
            });
            if exact_identity_is_live
                && !recovered
                    .iter()
                    .any(|handle: &String| handle == &saved.handle)
                && self.open(&saved.handle)
            {
                recovered.push(saved.handle.clone());
            }
        }
        if let Some(active) = &layout.active_handle {
            if recovered.iter().any(|handle| handle == active) {
                self.open(active);
            }
        }
        recovered.len()
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
        self.ticket_at(self.active_tab?)
    }

    /// A second split viewport can poll a different tab without changing
    /// focus, while retaining the exact stale-response identity checks.
    pub fn ticket_at(&self, index: usize) -> Option<TerminalReadTicket> {
        let tab = self.tabs.get(index)?;
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
    fn terminal_auto_viewport_fits_minimum_and_bounds_dimensions() {
        assert_eq!(estimate_terminal_viewport(24.0, 160.0, 13.0), None);
        assert_eq!(estimate_terminal_viewport(500.0, 10.0, 13.0), None);
        assert_eq!(estimate_terminal_viewport(f32::NAN, 500.0, 13.0), None);
        assert_eq!(estimate_terminal_viewport(500.0, f32::INFINITY, 13.0), None);
        assert_eq!(estimate_terminal_viewport(500.0, 200.0, 0.0), None);
        assert_eq!(
            estimate_terminal_viewport(1200.0, 600.0, 13.0),
            Some((145, 34))
        );
        assert_eq!(
            estimate_terminal_viewport(50_000.0, 50_000.0, 13.0),
            Some((240, 100))
        );
        let smaller = estimate_terminal_viewport(800.0, 480.0, 13.0).unwrap();
        let larger = estimate_terminal_viewport(1200.0, 780.0, 13.0).unwrap();
        assert!(larger.0 > smaller.0 && larger.1 > smaller.1);
    }

    #[test]
    fn auto_resize_coalesces_visible_host_identity_and_new_dimensions() {
        let start = std::time::Instant::now();
        let tab = TerminalTabContextTarget {
            handle: "term-a".into(),
            workspace_id: "ws".into(),
            tab_id: "tab".into(),
            generation: 4,
            workspace_generation: 7,
        };
        let first = TerminalViewportProposal {
            target: tab.clone(),
            cols: 80,
            rows: 24,
        };
        let second = TerminalViewportProposal {
            target: tab.clone(),
            cols: 120,
            rows: 36,
        };
        let mut gate = TerminalAutoResizeGate::default();
        gate.observe(vec![first.clone()], start);
        assert!(gate
            .take_ready(start + std::time::Duration::from_millis(299))
            .is_empty());
        gate.observe(
            vec![second.clone()],
            start + std::time::Duration::from_millis(100),
        );
        assert!(gate
            .take_ready(start + std::time::Duration::from_millis(350))
            .is_empty());
        assert_eq!(
            gate.take_ready(start + std::time::Duration::from_millis(401)),
            vec![second.clone()]
        );
        gate.observe(
            vec![second.clone()],
            start + std::time::Duration::from_millis(420),
        );
        assert!(gate
            .take_ready(start + std::time::Duration::from_millis(1600))
            .is_empty());
        let mut recycled = tab.clone();
        recycled.tab_id = "new-tab".into();
        let changed = TerminalViewportProposal {
            target: recycled,
            cols: 120,
            rows: 36,
        };
        gate.observe(
            vec![changed.clone()],
            start + std::time::Duration::from_millis(1700),
        );
        assert_eq!(
            gate.take_ready(start + std::time::Duration::from_millis(2100)),
            vec![changed]
        );
        gate.observe(vec![], start + std::time::Duration::from_millis(2200));
        gate.observe(
            vec![second.clone()],
            start + std::time::Duration::from_millis(2300),
        );
        assert_eq!(
            gate.take_ready(start + std::time::Duration::from_millis(2600)),
            vec![second]
        );
        gate.clear();
        assert!(gate
            .take_ready(start + std::time::Duration::from_millis(3000))
            .is_empty());
    }

    #[test]
    fn manual_resize_preserves_preset_until_pane_geometry_changes() {
        let target = TerminalTabContextTarget {
            handle: "term".into(),
            workspace_id: "ws".into(),
            tab_id: "tab".into(),
            generation: 1,
            workspace_generation: 2,
        };
        let start = std::time::Instant::now();
        let first = TerminalViewportProposal {
            target: target.clone(),
            cols: 84,
            rows: 25,
        };
        let second = TerminalViewportProposal {
            target: target.clone(),
            cols: 96,
            rows: 30,
        };
        let mut gate = TerminalAutoResizeGate::default();
        gate.observe(vec![first.clone()], start);
        gate.suspend_until_changed(&target);
        gate.observe(vec![first], start + std::time::Duration::from_secs(2));
        assert!(gate
            .take_ready(start + std::time::Duration::from_secs(4))
            .is_empty());
        gate.observe(
            vec![second.clone()],
            start + std::time::Duration::from_secs(5),
        );
        assert_eq!(
            gate.take_ready(start + std::time::Duration::from_millis(5300)),
            vec![second]
        );
    }

    #[test]
    fn vt_line_selection_copies_only_snapshot_rows_for_exact_host_and_cursor() {
        let target = TerminalTabContextTarget {
            handle: "pty-test".into(),
            workspace_id: "ws-1".into(),
            tab_id: "tab-1".into(),
            generation: 9,
            workspace_generation: 4,
        };
        let rows = vec!["first ".into(), "中文字  ".into(), "last".into()];
        let mut selection =
            TerminalLineSelection::begin(target.clone(), Some(41), rows, 1).unwrap();
        assert!(selection.is_current(&target, Some(41)));
        assert_eq!(selection.selected_text(), "中文字");
        assert!(selection.extend(0));
        assert_eq!(selection.selected_text(), "first\n中文字");
        assert!(selection.includes_row(0));
        assert!(selection.includes_row(1));
        assert!(!selection.includes_row(2));
        selection.finish();
        assert!(!selection.extend(2));
        assert!(
            !selection.is_current(&target, Some(42)),
            "New Host output invalidates old screen selection"
        );
        let recycled = TerminalTabContextTarget {
            tab_id: "tab-2".into(),
            ..target.clone()
        };
        assert!(
            !selection.is_current(&recycled, Some(41)),
            "Never copy a recycled Host handle"
        );
        assert!(TerminalLineSelection::begin(target.clone(), None, vec!["ok".into()], 3).is_none());
        assert!(TerminalLineSelection::begin(target, None, vec!["long".repeat(1030)], 0).is_none());
    }

    #[test]
    fn bounded_resize_verb_never_starts_host_and_rejects_unsafe_geometry() {
        use crate::runtime_bridge::terminal_cli_arguments;
        let args = resize_terminal_verb("term-1", 120, 36).unwrap();
        assert_eq!(args[0], "resize");
        assert_eq!(args[1], "--handle=term-1");
        let routed = terminal_cli_arguments(Path::new("C:/runtime"), &args);
        assert!(routed.iter().any(|arg| arg == "--no-start"));
        for dims in [(0, 24), (19, 24), (80, 4), (513, 24), (80, 201)] {
            assert!(resize_terminal_verb("term-1", dims.0, dims.1).is_err());
        }
        assert!(resize_terminal_verb("bad\nhandle", 80, 24).is_err());
    }

    #[test]
    fn raw_native_terminal_key_encoding_preserves_pty_control_and_unicode() {
        assert_eq!(
            encode_terminal_key("up", None, false, false, false),
            Some("\u{1b}[A".into())
        );
        assert_eq!(
            encode_terminal_key("backspace", None, false, false, false),
            Some("\u{7f}".into())
        );
        assert_eq!(
            encode_terminal_key("enter", None, false, false, false),
            Some("\r".into())
        );
        assert_eq!(
            encode_terminal_key("c", None, true, false, false),
            Some("\u{3}".into())
        );
        assert_eq!(
            encode_terminal_key("d", None, true, false, false),
            Some("\u{4}".into())
        );
        assert_eq!(
            encode_terminal_key("a", Some("中"), false, false, false),
            Some("中".into())
        );
        assert!(encode_terminal_key("v", Some("v"), false, false, true).is_none());
        assert!(encode_terminal_key("a", Some("a"), false, true, false).is_none());
        assert!(encode_terminal_key("key", Some("\0"), false, false, false).is_none());
    }

    #[test]
    fn late_raw_key_reply_cannot_discard_keys_for_a_new_active_host() {
        let first = TerminalTabContextTarget {
            handle: "pty-a".into(),
            workspace_id: "ws".into(),
            tab_id: "tab-a".into(),
            generation: 1,
            workspace_generation: 7,
        };
        let second = TerminalTabContextTarget {
            handle: "pty-b".into(),
            tab_id: "tab-b".into(),
            ..first.clone()
        };
        // B keys arrive while A's asynchronous CLI write is still running.
        // A's success OR failure must permit the still-live B queue to flush.
        assert!(raw_key_reply_may_drain(
            &first,
            Some(&second),
            Some(&second),
            true,
            true,
        ));
        assert!(raw_key_reply_may_drain(
            &first,
            Some(&second),
            Some(&second),
            true,
            false,
        ));
        // A failure for the currently active A is not silently ignored.
        assert!(!raw_key_reply_may_drain(
            &first,
            Some(&first),
            Some(&first),
            true,
            false,
        ));
        assert!(raw_key_reply_may_drain(
            &first,
            Some(&first),
            Some(&first),
            true,
            true,
        ));
        // Neither a stale tab identity nor a stopped session can receive
        // queued keyboard data, even if the Host reuses the same handle.
        let reused = TerminalTabContextTarget {
            tab_id: "new-tab-a".into(),
            ..first.clone()
        };
        assert!(!raw_key_reply_may_drain(
            &first,
            Some(&first),
            Some(&reused),
            true,
            true,
        ));
        assert!(!raw_key_reply_may_drain(
            &first,
            Some(&second),
            Some(&second),
            false,
            true,
        ));
        assert!(!raw_key_reply_may_drain(
            &first,
            None,
            Some(&second),
            true,
            true,
        ));
    }

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
    fn terminal_list_must_reject_empty_or_control_laden_host_identity() {
        let valid = json!({"items":[{"handle":"term-1","workspaceId":"ws-1","tabId":"tab-1","running":true}]});
        assert_eq!(parse_terminal_list(&valid).unwrap().len(), 1);
        for bad_workspace in ["", "ws\nother", "ws\0other", "ws\\other"] {
            let reply = json!({"items":[{"handle":"term-1","workspaceId":bad_workspace,"tabId":"tab-1","running":true}]});
            assert!(
                parse_terminal_list(&reply).is_err(),
                "accepted workspace id {bad_workspace:?}"
            );
        }
        for bad_tab in ["", "tab\rspoof", "tab/other"] {
            let reply = json!({"items":[{"handle":"term-1","workspaceId":"ws-1","tabId":bad_tab,"running":true}]});
            assert!(
                parse_terminal_list(&reply).is_err(),
                "accepted tab id {bad_tab:?}"
            );
        }
    }

    #[test]
    fn registered_workspace_selection_is_canonical_exact_and_unique() {
        let project = tempfile::tempdir().unwrap();
        let unrelated = tempfile::tempdir().unwrap();
        let project_root = dunce::canonicalize(project.path()).unwrap();
        let unrelated_root = dunce::canonicalize(unrelated.path()).unwrap();
        let registered = vec![
            RegisteredHostWorkspace {
                id: "ws-unrelated".into(),
                path: unrelated_root.clone(),
            },
            RegisteredHostWorkspace {
                id: "ws-project".into(),
                path: project_root.join("."),
            },
        ];
        assert_eq!(
            select_registered_host_workspace(&project_root, &registered).unwrap(),
            "ws-project"
        );
        assert_eq!(
            select_registered_host_workspace(&unrelated_root, &registered).unwrap(),
            "ws-unrelated"
        );
        assert_eq!(
            select_registered_host_workspace(&project_root, &registered[..1])
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
        let ambiguous = vec![
            registered[1].clone(),
            RegisteredHostWorkspace {
                id: "ws-alias".into(),
                path: project_root.clone(),
            },
        ];
        assert_eq!(
            select_registered_host_workspace(&project_root, &ambiguous)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn restored_preview_requires_three_matching_ids_and_keeps_saved_order() {
        use crate::session::{TerminalLayout, TerminalLayoutTab};
        let mut dock = TerminalDock::for_workspace(4);
        dock.apply_session_list(4, vec![session("one"), session("two"), session("reused")]);
        let layout = TerminalLayout {
            tabs: vec![
                TerminalLayoutTab {
                    handle: "two".into(),
                    workspace_id: "workspace-1".into(),
                    tab_id: "tab-two".into(),
                },
                TerminalLayoutTab {
                    handle: "reused".into(),
                    workspace_id: "workspace-1".into(),
                    tab_id: "old-tab".into(),
                },
                TerminalLayoutTab {
                    handle: "one".into(),
                    workspace_id: "workspace-1".into(),
                    tab_id: "tab-one".into(),
                },
                TerminalLayoutTab {
                    handle: "wrong-ws".into(),
                    workspace_id: "elsewhere".into(),
                    tab_id: "tab-wrong-ws".into(),
                },
            ],
            active_handle: Some("one".into()),
            transcript_view: true,
        };
        assert_eq!(dock.restore_previews_from_host_layout(&layout), 2);
        assert_eq!(dock.tabs.len(), 2);
        assert_eq!(dock.tabs[0].handle, "two");
        assert_eq!(dock.tabs[1].handle, "one");
        assert_eq!(dock.active().unwrap().handle, "one");
        assert_eq!(dock.view_mode, TerminalViewMode::Transcript);
        assert_eq!(
            dock.sessions.len(),
            3,
            "restore must not terminate Host sessions"
        );
        assert_eq!(
            dock.restore_previews_from_host_layout(&layout),
            0,
            "do not replace user-opened tabs"
        );
    }

    #[test]
    fn created_terminal_confirmation_matches_workspace_tab_and_session() {
        let created = CreatedHostTerminal {
            handle: "term-1".into(),
            workspace_id: "workspace-1".into(),
            tab_id: "tab-term-1".into(),
        };
        let valid = session("term-1");
        assert!(created.matches_live_session(&valid));
        let mut wrong = valid.clone();
        wrong.tab_id = "old-tab".into();
        assert!(!created.matches_live_session(&wrong));
        wrong = valid.clone();
        wrong.workspace_id = "workspace-2".into();
        assert!(!created.matches_live_session(&wrong));
        wrong = valid;
        wrong.handle = "term-2".into();
        assert!(!created.matches_live_session(&wrong));
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
    fn parses_actual_host_creation_without_attachment_workspace_or_tab_fields() {
        // The live CLI Host returns session identity in attachment and the
        // workspace/tab correlation in tab + tab.payload, NOT attachment.
        let value = json!({
            "tab":{
                "id":"tab-live","workspaceId":"ws-live","kind":"terminal",
                "payload":{"source":"mobile","terminalSessionId":"session-live"}
            },
            "attachment":{
                "sessionId":"session-live","created":true,"running":true,
                "snapshotCols":80,"snapshotRows":24
            }
        });
        let created = parse_created_terminal(&value, "ws-live").unwrap();
        assert_eq!(created.handle, "session-live");
        assert_eq!(created.workspace_id, "ws-live");
        assert_eq!(created.tab_id, "tab-live");

        let mut wrong_session = value.clone();
        wrong_session["tab"]["payload"]["terminalSessionId"] = json!("other-session");
        assert!(parse_created_terminal(&wrong_session, "ws-live").is_err());

        let mut missing_proof = value.clone();
        missing_proof["tab"]["payload"] = json!({});
        assert!(parse_created_terminal(&missing_proof, "ws-live").is_err());

        let mut mismatched_workspace = value.clone();
        mismatched_workspace["attachment"]["workspaceId"] = json!("other-workspace");
        assert!(parse_created_terminal(&mismatched_workspace, "ws-live").is_err());

        let mut mismatched_tab = value.clone();
        mismatched_tab["attachment"]["tabId"] = json!("other-tab");
        assert!(parse_created_terminal(&mismatched_tab, "ws-live").is_err());
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
    fn terminal_create_confirm_times_out_without_duplicate_pty_and_can_retry_listing() {
        let mut flow = TerminalCreateFlow::default();
        let request = flow.start_request().expect("first create allowed");
        assert!(
            flow.start_request().is_none(),
            "double click must not create twice"
        );
        let created = CreatedHostTerminal {
            handle: "term-1".into(),
            workspace_id: "workspace-1".into(),
            tab_id: "tab-term-1".into(),
        };
        assert!(flow.finish_request(request, created));
        assert!(
            flow.start_request().is_none(),
            "awaiting Host list is not a new create"
        );
        let mut refreshes = 0usize;
        for tick in 1..=TERMINAL_CONFIRM_TIMEOUT_TICKS {
            match flow.tick() {
                TerminalCreateTick::RefreshSessions => refreshes += 1,
                TerminalCreateTick::TimedOut => assert_eq!(tick, TERMINAL_CONFIRM_TIMEOUT_TICKS),
                TerminalCreateTick::None => {}
            }
        }
        assert!(refreshes > 0 && refreshes < TERMINAL_CONFIRM_TIMEOUT_TICKS as usize);
        assert!(flow.is_timed_out());
        assert!(
            flow.start_request().is_none(),
            "timeout must not start second PTY"
        );
        assert_eq!(flow.tick(), TerminalCreateTick::None);
        assert!(
            flow.retry_confirmation(),
            "retry only relists existing Host session"
        );
        assert!(flow.is_waiting());
    }

    #[test]
    fn terminal_create_cancel_drops_stale_host_callback_without_killing_session() {
        let mut flow = TerminalCreateFlow::default();
        let old_request = flow.start_request().unwrap();
        assert!(flow.cancel());
        let fresh_request = flow.start_request().unwrap();
        assert_ne!(old_request, fresh_request);
        let created = CreatedHostTerminal {
            handle: "term-old".into(),
            workspace_id: "workspace-1".into(),
            tab_id: "tab-old".into(),
        };
        assert!(!flow.finish_request(old_request, created.clone()));
        assert!(flow.is_creating());
        assert!(flow.finish_request(fresh_request, created));
        assert!(
            flow.cancel(),
            "cancel detaches UI expectation, never kills Host PTY"
        );
        assert!(flow.confirm_sessions(&[session("term-old")]).is_none());
        assert!(!flow.cancel());
    }

    #[test]
    fn terminal_create_confirmation_requires_all_three_id_fields() {
        let mut flow = TerminalCreateFlow::default();
        let id = flow.start_request().unwrap();
        let created = CreatedHostTerminal {
            handle: "term-1".into(),
            workspace_id: "workspace-1".into(),
            tab_id: "tab-term-1".into(),
        };
        assert!(flow.finish_request(id, created));
        let mut spoofed = session("term-1");
        spoofed.tab_id = "previous-tab".into();
        assert!(flow.confirm_sessions(&[spoofed]).is_none());
        assert!(flow.is_waiting());
        assert!(flow.confirm_sessions(&[session("term-2")]).is_none());
        assert!(flow.confirm_sessions(&[session("term-1")]).is_some());
        assert!(!flow.is_active());
        assert!(flow.start_request().is_some());
    }

    #[test]
    fn terminal_create_rpc_failure_resets_state_but_rejects_stale_failures() {
        let mut flow = TerminalCreateFlow::default();
        let first = flow.start_request().unwrap();
        assert!(flow.fail_request(first));
        assert!(
            !flow.is_active(),
            "failure must reenable an intentional new request"
        );
        let second = flow.start_request().unwrap();
        assert_ne!(first, second);
        assert!(
            !flow.fail_request(first),
            "late failure must not cancel a new request"
        );
        assert!(flow.is_creating());
        assert!(flow.fail_request(second));
        assert!(!flow.is_active());
    }

    #[test]
    fn terminal_create_timeout_requires_explicit_retry_before_attach() {
        let mut flow = TerminalCreateFlow::default();
        let request = flow.start_request().unwrap();
        let created = CreatedHostTerminal {
            handle: "term-1".into(),
            workspace_id: "workspace-1".into(),
            tab_id: "tab-term-1".into(),
        };
        assert!(flow.finish_request(request, created));
        for _ in 0..TERMINAL_CONFIRM_TIMEOUT_TICKS {
            flow.tick();
        }
        assert!(flow.is_timed_out());
        assert!(flow.confirm_sessions(&[session("term-1")]).is_none());
        assert!(flow.retry_confirmation());
        assert_eq!(
            flow.confirm_sessions(&[session("term-1")])
                .map(|terminal| terminal.handle),
            Some("term-1".into())
        );
        assert!(!flow.is_active());
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

    #[test]
    fn terminal_context_tracks_host_identity_through_reorder_and_rejects_recycled_handle() {
        let mut dock = TerminalDock::for_workspace(11);
        dock.update_sessions(vec![
            session("term-1"),
            session("term-2"),
            session("term-3"),
        ]);
        for name in ["term-1", "term-2", "term-3"] {
            assert!(dock.open(name));
        }
        assert!(dock.activate(2));
        let context = dock.context_target_at(0).unwrap();
        assert_eq!(context.handle, "term-1");
        assert_eq!(dock.index_for_context(&context), Some(0));
        assert!(dock.context_target_is_live(&context));
        assert!(dock.move_tab(0, 1));
        assert_eq!(dock.index_for_context(&context), Some(1));
        assert_eq!(dock.active().unwrap().handle, "term-3");

        // Right-click "Close to Right" affects its clicked tab, while the
        // Host inventory remains intact and no PTY is terminated.
        let count = dock.close_to_right(dock.index_for_context(&context).unwrap());
        assert_eq!(count, 1);
        assert_eq!(dock.active().unwrap().handle, "term-1");
        assert_eq!(dock.sessions.len(), 3);

        dock.close(dock.index_for_context(&context).unwrap());
        assert!(dock.index_for_context(&context).is_none());
        assert!(dock.open("term-1"));
        assert!(dock.index_for_context(&context).is_none());

        // Even a Host-reused opaque handle with a different Tab ID must
        // never become the target of a stale context menu.
        let mut reused = session("term-1");
        reused.tab_id = "new-host-tab".into();
        dock.update_sessions(vec![reused, session("term-2")]);
        assert!(!dock.context_target_is_live(&dock.context_target_at(1).unwrap()));
        assert!(dock.open("term-1"));
        assert!(dock.index_for_context(&context).is_none());
        assert_eq!(dock.sessions.len(), 2);
    }

    #[test]
    fn context_target_cannot_cross_workspaces_or_select_stopped_host() {
        let mut old = TerminalDock::for_workspace(1);
        old.update_sessions(vec![session("shared")]);
        old.open("shared");
        let token = old.context_target_at(0).unwrap();
        let mut next = TerminalDock::for_workspace(2);
        next.update_sessions(vec![session("shared")]);
        next.open("shared");
        assert!(next.index_for_context(&token).is_none());
        assert!(!next.context_target_is_live(&token));
        next.sessions[0].running = false;
        assert!(!next.context_target_is_live(&next.context_target_at(0).unwrap()));
    }
}
