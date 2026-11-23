//! Native GPUI terminal session explorer and explicitly requested input.
//! Terminal process, PTY, authentication and session lifetime remain owned by
//! the existing Alera Runtime Host; this frontend uses CLI --no-start only.
use crate::runtime_bridge::RuntimeCliBridge;
use crate::session::TerminalLayout;
use crate::terminal_output_budget::TerminalParseBudget;
use crate::vt_screen::{VtScreen, DEFAULT_SCROLLBACK_LINES};
use serde_json::Value;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

// A 1.5s / 8 KiB polling budget throttled busy terminals to roughly 5 KiB/s.
// Stay within the existing Host 64 KiB read window and bounded local screen.
pub const TERMINAL_READ_BYTES: u64 = 64 * 1024;
pub const MAX_TERMINALS: usize = 128;
/// The Host session list is global. It is not a view of the open workspace.
pub const HOST_SESSION_LIST_SCOPE: &str =
    "Existing Host sessions · all workspaces · input only to selected session";
const MAX_OPEN_TABS: usize = 8;
const MAX_TRANSCRIPT_BYTES: usize = 48 * 1024;
const MAX_HOST_WORKSPACES: usize = 256;
const MAX_HOST_PATH_BYTES: usize = 4096;

/// Host-confirmed Agent Hook status. This is independent of the PTY process
/// running flag: a live session may be waiting for user approval, blocked, or
/// finished with the current turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AgentActivityState {
    Working,
    Waiting,
    Blocked,
    Done,
}

impl AgentActivityState {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "working" => Some(Self::Working),
            "waiting" => Some(Self::Waiting),
            "blocked" => Some(Self::Blocked),
            "done" => Some(Self::Done),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Waiting => "waiting",
            Self::Blocked => "blocked",
            Self::Done => "done",
        }
    }
}

/// Bounded text derived from an authenticated Host Agent presence snapshot.
/// Unlike status-only push events, this is never forwarded to the PTY sidecar.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentContextOverview {
    pub goal: String,
    pub progress: String,
    pub timeline: Vec<AgentContextMilestone>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentContextMilestone {
    pub kind: String,
    pub text: String,
    pub at: String,
}

impl AgentContextOverview {
    fn text(value: Option<&Value>, max: usize) -> String {
        value
            .and_then(Value::as_str)
            .unwrap_or_default()
            .chars()
            .filter(|ch| !ch.is_control())
            .take(max)
            .collect::<String>()
            .trim()
            .to_owned()
    }

    pub fn parse(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let goal = Self::text(object.get("goal"), 600);
        let progress = Self::text(object.get("progress"), 420);
        let timeline = object
            .get("timeline")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .take(16)
            .filter_map(|item| {
                let kind = item.get("kind")?.as_str()?;
                if !matches!(kind, "goal" | "instruction" | "report") {
                    return None;
                }
                let text = Self::text(item.get("text"), 260);
                let at = item.get("at")?.as_str()?;
                if text.is_empty() || at.len() > 64 || agent_host_utc_time_key(at).is_none() {
                    return None;
                }
                Some(AgentContextMilestone {
                    kind: kind.to_owned(),
                    text,
                    at: at.to_owned(),
                })
            })
            .collect::<Vec<_>>();
        if goal.is_empty() && progress.is_empty() && timeline.is_empty() {
            return None;
        }
        Some(Self {
            goal,
            progress,
            timeline,
        })
    }
}

#[cfg(test)]
mod agent_context_overview_tests {
    use super::*;

    #[test]
    fn only_a_verified_running_agent_receives_bounded_context() {
        let long = "任".repeat(900);
        let at = "2026-11-21T12:00:00+00:00";
        let value = serde_json::json!({
            "items": [
                {
                    "handle": "pty-one", "workspaceId": "ws-one",
                    "tabId": "tab-one", "running": true,
                    "agentType": "claude", "agentState": "working",
                    "contextOverview": {
                        "goal": long, "progress": "設定功能對齊中",
                        "timeline": [
                            {"kind":"report", "text":"已補設定驗證", "at":at},
                            {"kind":"toolInput", "text":"SECRET", "at":at},
                            {"kind":"instruction", "text":"補齊 UI", "at":"invalid"},
                        ]
                    }
                },
                {
                    "handle": "pty-two", "workspaceId":"ws-two",
                    "tabId":"tab-two", "running": false,
                    "agentType": "claude", "agentState":"done",
                    "contextOverview": {"goal":"SHOULD NOT DISPLAY"}
                },
                {
                    "handle": "pty-three", "workspaceId":"ws-three",
                    "tabId":"tab-three", "running":true,
                    "agentType":null, "agentState":null,
                    "contextOverview": {"goal":"SHOULD NOT DISPLAY"}
                }
            ]
        });
        let sessions = parse_terminal_list(&value).unwrap();
        let context = sessions[0].agent_context.as_ref().unwrap();
        assert_eq!(context.goal.chars().count(), 600);
        assert_eq!(context.progress, "設定功能對齊中");
        assert_eq!(context.timeline.len(), 1);
        assert_eq!(context.timeline[0].text, "已補設定驗證");
        assert!(sessions[1].agent_context.is_none());
        assert!(sessions[2].agent_context.is_none());
    }

    #[test]
    fn host_context_delivers_evolving_goal_and_two_model_reports_to_right_sidebar() {
        let at = "2026-11-22T08:00:00+00:00";
        let newer = "2026-11-22T09:00:00+00:00";
        let reply = serde_json::json!({
            "items": [{
                "handle":"active-agent","workspaceId":"project-worktree",
                "tabId":"agent-tab","running":true,"agentType":"codex",
                "agentState":"done",
                "contextOverview":{
                    "goal":"修正 Agent Context｜目前重點：顯示完成回覆與最新匯報",
                    "progress":"已加入回覆摘要與時間線，測試通過",
                    "initialGoal":"修正 Agent Context",
                    "currentFocus":"顯示完成回覆與最新匯報",
                    "timeline":[
                        {"kind":"report","text":"已加入回覆摘要與時間線，測試通過","at":newer},
                        {"kind":"instruction","text":"顯示完成回覆與最新匯報","at":newer},
                        {"kind":"report","text":"第一輪已完成","at":at},
                        {"kind":"toolInput","text":"PRIVATE TOOL OUTPUT","at":at}
                    ]
                }
            }]
        });
        let sessions = parse_terminal_list(&reply).unwrap();
        let context = sessions[0].agent_context.as_ref().unwrap();
        assert!(context.goal.contains("目前重點"));
        assert!(context.progress.contains("測試通過"));
        assert_eq!(context.timeline.len(), 3);
        assert_eq!(context.timeline[0].kind, "report");
        assert_eq!(context.timeline[2].text, "第一輪已完成");
        assert!(!context
            .timeline
            .iter()
            .any(|event| event.text.contains("PRIVATE")));
    }

    #[test]
    fn malformed_context_is_treated_as_unavailable() {
        assert!(AgentContextOverview::parse(&serde_json::json!(null)).is_none());
        assert!(AgentContextOverview::parse(&serde_json::json!({"timeline":[]})).is_none());
    }
}

/// Sanitized, status-only Host push. No Prompt, tool input, tokens or PTY
/// bytes cross this model. Exact Host identities prevent recycled-handle
/// events from changing a different Agent Tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPresenceChange {
    pub session_id: String,
    pub workspace_id: String,
    pub tab_id: String,
    pub agent_type: Option<String>,
    pub state: Option<AgentActivityState>,
    pub state_started_at: String,
    pub updated_at: String,
    pub interrupted: bool,
    /// Non-sensitive invalidation bit. Private Context text never crosses status stream.
    pub context_changed: bool,
    pub removed: bool,
}

/// All Host Agent timestamps are UTC; the Host JSON encoder may choose Z
/// while the delta encoder emits +00:00. Canonicalize fractional seconds so
/// lexicographic ordering means chronological ordering in UTC.
pub(crate) fn agent_host_utc_time_key(raw: &str) -> Option<String> {
    let trimmed = raw
        .strip_suffix('Z')
        .or_else(|| raw.strip_suffix("+00:00"))?;
    let (seconds, fraction) = trimmed.split_once('.').unwrap_or((trimmed, ""));
    let bytes = seconds.as_bytes();
    if bytes.len() != 19
        || !bytes.iter().enumerate().all(|(i, byte)| match i {
            4 | 7 => *byte == b'-',
            10 => *byte == b'T',
            13 | 16 => *byte == b':',
            _ => byte.is_ascii_digit(),
        })
        || fraction.len() > 9
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    Some(format!("{seconds}.{fraction:0<9}"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalSession {
    pub handle: String,
    pub workspace_id: String,
    pub tab_id: String,
    pub running: bool,
    pub agent_type: Option<String>,
    /// None means unconfirmed/unknown, never implicitly 'working'.
    pub agent_state: Option<AgentActivityState>,
    pub interrupted: bool,
    /// Host revision of the current state. Used to reject stale async lists.
    pub agent_state_started_at: Option<String>,
    pub agent_updated_at: Option<String>,
    pub agent_context: Option<AgentContextOverview>,
}

impl TerminalSession {
    pub fn label(&self) -> String {
        let name = self.agent_type.as_deref().unwrap_or("Terminal");
        let state = if self.running { "●" } else { "○" };
        let activity = self
            .agent_state
            .map(|state| {
                if state == AgentActivityState::Done && self.interrupted {
                    " · interrupted"
                } else {
                    match state {
                        AgentActivityState::Working => " · working",
                        AgentActivityState::Waiting => " · waiting",
                        AgentActivityState::Blocked => " · blocked",
                        AgentActivityState::Done => " · done",
                    }
                }
            })
            .unwrap_or("");
        format!("{state} {name}{activity} · {}", self.handle)
    }

    /// Menu snapshots must not attach a recycled handle to the wrong Host
    /// workspace/tab. A valid snapshot and the latest authenticated list
    /// must agree on all three identifiers and both be running.
    pub fn matches_running_identity(&self, current: &TerminalSession) -> bool {
        self.running
            && current.running
            && self.handle == current.handle
            && self.workspace_id == current.workspace_id
            && self.tab_id == current.tab_id
    }
}

/// Uses the existing authenticated no-start Host Terminal list as a bounded
/// status-refresh fallback. Live Agents refresh every 3s, otherwise every 15s.
/// This does not create a new Host, Hook receiver or Terminal session.
pub fn agent_presence_refresh_due(tick: u64, sessions: &[TerminalSession]) -> bool {
    let active_agent = sessions
        .iter()
        .any(|session| session.running && session.agent_type.is_some());
    tick % if active_agent { 2 } else { 10 } == 1
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
        session.running
            && session.handle == self.handle
            && session.workspace_id == self.workspace_id
            && session.tab_id == self.tab_id
    }
}

pub fn list_terminal_verb() -> Vec<OsString> {
    vec![OsString::from("list")]
}

pub fn read_terminal_verb(handle: &str, cursor: Option<u64>) -> io::Result<Vec<OsString>> {
    read_terminal_verb_bounded(handle, cursor, TERMINAL_READ_BYTES)
}

pub fn read_terminal_verb_bounded(
    handle: &str,
    cursor: Option<u64>,
    max_bytes: u64,
) -> io::Result<Vec<OsString>> {
    if !valid_handle(handle) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid terminal handle",
        ));
    }
    if !(4 * 1024..=TERMINAL_READ_BYTES).contains(&max_bytes) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Terminal read budget must be 4–64 KiB",
        ));
    }
    let mut args = vec![
        OsString::from("read"),
        OsString::from(format!("--handle={handle}")),
        OsString::from("--max-bytes"),
        OsString::from(max_bytes.to_string()),
    ];
    if let Some(cursor) = cursor {
        args.push(OsString::from("--cursor"));
        args.push(OsString::from(cursor.to_string()));
    }
    Ok(args)
}

impl RuntimeCliBridge {
    /// Read only already-registered Host history. Consent is supplied by the
    /// Native view on each deliberate click, and transcript text uses stdin.
    pub fn search_agent_history(
        &self,
        text: &str,
        provider: Option<&str>,
        account_id: Option<&str>,
        project_id: Option<&str>,
        workspace_id: Option<&str>,
    ) -> io::Result<Value> {
        if text.trim().is_empty()
            || text.chars().count() > 256
            || text.len() > 1024
            || text.contains('\0')
            || provider.is_some_and(|value| !matches!(value, "claude" | "codex"))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid or oversized historical Agent search",
            ));
        }
        let mut args = vec![
            OsString::from("history-search"),
            OsString::from("--stdin"),
            OsString::from("--consent-index-user-text"),
        ];
        for (flag, value) in [
            ("--provider", provider),
            ("--account-id", account_id),
            ("--project-id", project_id),
            ("--workspace-id", workspace_id),
        ] {
            if let Some(value) = value {
                if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Invalid history scope",
                    ));
                }
                args.push(OsString::from(flag));
                args.push(OsString::from(value));
            }
        }
        self.terminal_json_with_input(&args, text.as_bytes())
    }

    /// Provider-native transcripts are never modified by this operation.
    pub fn clear_agent_history_search_cache(&self) -> io::Result<()> {
        self.terminal_json(&[OsString::from("history-clear")])?;
        Ok(())
    }

    /// Native exact history resume is a user-confirmed Host mutation. No
    /// caller-supplied paths, shell snippets, prompts, or ambient Host startup.
    pub fn resume_exact_agent_history(
        &self,
        provider: &str,
        account_id: &str,
        project_id: &str,
        workspace_id: &str,
        native_id: &str,
        profile_id: &str,
        mutation_id: &str,
    ) -> io::Result<Value> {
        if !matches!(provider, "claude" | "codex")
            || account_id.is_empty()
            || account_id.len() > 256
            || account_id.chars().any(char::is_control)
            || uuid::Uuid::parse_str(native_id).is_err()
            || uuid::Uuid::parse_str(mutation_id).is_err()
            || [project_id, workspace_id, profile_id]
                .iter()
                .any(|id| id.is_empty() || id.len() > 256 || id.chars().any(char::is_control))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Exact Resume requires a verified default account and canonical identity",
            ));
        }
        let args = vec![
            OsString::from("history-resume"),
            OsString::from("--provider"),
            OsString::from(provider),
            OsString::from("--account-id"),
            OsString::from(account_id),
            OsString::from("--project-id"),
            OsString::from(project_id),
            OsString::from("--workspace-id"),
            OsString::from(workspace_id),
            OsString::from("--session-id"),
            OsString::from(native_id),
            OsString::from("--profile-id"),
            OsString::from(profile_id),
            OsString::from("--client-mutation-id"),
            OsString::from(mutation_id),
            OsString::from("--confirm-account"),
            OsString::from("--confirm-resume"),
        ];
        self.terminal_json(&args)
    }

    /// Look up an already saved native Agent Tab after UI restart without
    /// starting a Host, running a CLI Agent or touching its history files.
    pub fn locate_exact_agent_history(
        &self,
        provider: &str,
        account_id: &str,
        project_id: &str,
        workspace_id: &str,
        native_id: &str,
    ) -> io::Result<Value> {
        if !matches!(provider, "codex" | "claude")
            || [account_id, project_id, workspace_id].iter().any(|value| {
                value.is_empty() || value.len() > 256 || value.chars().any(char::is_control)
            })
            || !uuid::Uuid::parse_str(native_id).is_ok_and(|parsed| parsed.to_string() == native_id)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "History Tab lookup requires a canonical verified source identity",
            ));
        }
        self.terminal_json(&[
            OsString::from("history-locate"),
            OsString::from("--provider"),
            OsString::from(provider),
            OsString::from("--account-id"),
            OsString::from(account_id),
            OsString::from("--project-id"),
            OsString::from(project_id),
            OsString::from("--workspace-id"),
            OsString::from(workspace_id),
            OsString::from("--session-id"),
            OsString::from(native_id),
        ])
    }

    /// Explicitly fetch a Host-authenticated original resumed Session's
    /// historical user/assistant messages. Never touch the live PTY buffer.
    pub fn read_resumed_agent_messages(
        &self,
        workspace_id: &str,
        tab_id: &str,
        before: Option<u64>,
    ) -> io::Result<Value> {
        if [workspace_id, tab_id]
            .iter()
            .any(|id| id.is_empty() || id.len() > 256 || id.chars().any(char::is_control))
            || before == Some(0)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid original resumed Tab identity/cursor",
            ));
        }
        let mut args = vec![
            OsString::from("history-messages"),
            OsString::from("--workspace-id"),
            OsString::from(workspace_id),
            OsString::from("--tab-id"),
            OsString::from(tab_id),
            OsString::from("--max-messages"),
            OsString::from("30"),
            OsString::from("--confirm-read"),
        ];
        if let Some(cursor) = before {
            args.push(OsString::from("--before"));
            args.push(OsString::from(cursor.to_string()));
        }
        self.terminal_json(&args)
    }

    pub fn list_terminals(&self) -> io::Result<Vec<TerminalSession>> {
        let response = self.terminal_json(&list_terminal_verb())?;
        parse_terminal_list(&response)
    }

    pub fn read_terminal(&self, handle: &str, cursor: Option<u64>) -> io::Result<TerminalChunk> {
        self.read_terminal_bounded(handle, cursor, TERMINAL_READ_BYTES)
    }

    pub fn read_terminal_bounded(
        &self,
        handle: &str,
        cursor: Option<u64>,
        max_bytes: u64,
    ) -> io::Result<TerminalChunk> {
        let args = read_terminal_verb_bounded(handle, cursor, max_bytes)?;
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

    /// Live GPUI Inspector raw keyboard/IME input. The same Host route used
    /// by guarded Agent prompts verifies Workspace + Tab + Session before
    /// writing each PTY byte. No implicit Enter, new Agent or attach.
    pub fn write_verified_agent_raw(
        &self,
        handle: &str,
        workspace_id: &str,
        tab_id: &str,
        text: &str,
    ) -> io::Result<()> {
        let args = verified_agent_raw_verb(handle, workspace_id, tab_id, text)?;
        self.terminal_json_with_input(&args, text.as_bytes())?;
        Ok(())
    }

    /// Full Host identity validation is performed inside the write request.
    /// Never resolve a global dashboard target by handle alone.
    pub fn submit_verified_agent_prompt(
        &self,
        handle: &str,
        workspace_id: &str,
        tab_id: &str,
        prompt: &str,
    ) -> io::Result<()> {
        let args = verified_agent_prompt_verb(handle, workspace_id, tab_id, prompt)?;
        self.terminal_json_with_input(&args, prompt.as_bytes())?;
        Ok(())
    }

    /// Explicit local-only lifecycle control of an exact Host Agent.
    /// Never retries an unknown outcome or opens a new Host process.
    pub fn control_verified_agent(
        &self,
        handle: &str,
        workspace_id: &str,
        tab_id: &str,
        terminate: bool,
    ) -> io::Result<()> {
        let args = verified_agent_control_verb(handle, workspace_id, tab_id, terminate)?;
        self.terminal_json(&args)?;
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

    /// Host-owned direct launch of a canonical installed Agent. Unlike
    /// `tab create`, the authenticated terminal request has no offline-store
    /// fallback. The response is still corroborated by the session list.
    pub fn create_agent_terminal(
        &self,
        workspace_id: &str,
        agent_id: &str,
        executable: &str,
    ) -> io::Result<CreatedHostTerminal> {
        let args = create_agent_terminal_verb(workspace_id, agent_id, executable)?;
        let response = self.terminal_json(&args)?;
        parse_created_agent_terminal(&response, workspace_id, agent_id, executable)
    }

    /// Resize requests can succeed without changing the PTY while a mobile
    /// client holds the viewport lease. Older Hosts respond with {}.
    pub fn resize_terminal(
        &self,
        handle: &str,
        cols: u16,
        rows: u16,
    ) -> io::Result<TerminalResizeOutcome> {
        let args = resize_terminal_verb(handle, cols, rows)?;
        let result = self.terminal_json(&args)?;
        Ok(TerminalResizeOutcome::from_host_payload(&result))
    }
}

/// Host request acceptance is not proof that ConPTY was resized. Older Host
/// binaries return an empty payload; retain their legacy behavior until they
/// are upgraded to report applied vs deferred.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalResizeOutcome {
    Applied,
    Deferred,
    LegacyUnconfirmed,
}

impl TerminalResizeOutcome {
    pub fn from_host_payload(value: &Value) -> Self {
        match value.get("applied").and_then(Value::as_bool) {
            Some(true) => Self::Applied,
            Some(false) => Self::Deferred,
            None => Self::LegacyUnconfirmed,
        }
    }

    pub fn may_update_preview(self) -> bool {
        !matches!(self, Self::Deferred)
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

/// Guard every raw PTY key with the exact original Host source identity.
/// Omit --submit: Escape, arrows, characters and Enter already carry their
/// native bytes and must not inject a second newline.
pub fn verified_agent_raw_verb(
    handle: &str,
    workspace_id: &str,
    tab_id: &str,
    text: &str,
) -> io::Result<Vec<OsString>> {
    if !valid_handle(workspace_id) || !valid_handle(tab_id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid guarded Agent owner",
        ));
    }
    let mut args = write_terminal_verb(handle, text, false)?;
    args.extend([
        OsString::from(format!("--expected-workspace-id={workspace_id}")),
        OsString::from(format!("--expected-tab-id={tab_id}")),
    ]);
    Ok(args)
}

/// List workspaces registered with the running Host. No arguments beyond the
/// verb itself are permitted by the bridge allowlist.
pub fn verified_agent_prompt_verb(
    handle: &str,
    workspace_id: &str,
    tab_id: &str,
    text: &str,
) -> io::Result<Vec<OsString>> {
    if !valid_handle(workspace_id) || !valid_handle(tab_id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid guarded Agent workspace or tab identity",
        ));
    }
    let mut args = write_terminal_verb(handle, text, false)?;
    args.extend([
        OsString::from(format!("--expected-workspace-id={workspace_id}")),
        OsString::from(format!("--expected-tab-id={tab_id}")),
        OsString::from("--submit"),
    ]);
    Ok(args)
}

/// Fixed action and exact Host IDs. Never assemble arbitrary process args.
pub fn verified_agent_control_verb(
    handle: &str,
    workspace_id: &str,
    tab_id: &str,
    terminate: bool,
) -> io::Result<Vec<OsString>> {
    if !valid_handle(handle) || !valid_handle(workspace_id) || !valid_handle(tab_id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid Agent Session, Workspace or Tab identity",
        ));
    }
    let mut args = vec![
        OsString::from("control-agent-verified"),
        OsString::from(format!("--handle={handle}")),
        OsString::from(format!("--expected-workspace-id={workspace_id}")),
        OsString::from(format!("--expected-tab-id={tab_id}")),
        OsString::from("--action"),
        OsString::from(if terminate { "terminate" } else { "interrupt" }),
    ];
    if terminate {
        args.push(OsString::from("--confirm-terminate"));
    }
    Ok(args)
}

#[cfg(test)]
mod gpui_agent_inspector_raw_input_tests {
    use super::*;

    #[test]
    fn escape_and_enter_go_to_exact_host_agent_as_raw_bytes_without_submit() {
        for key in ["\u{1b}", "\r", "a", "繁體"] {
            let args = verified_agent_raw_verb(
                "current-session",
                "original-worktree",
                "original-tab",
                key,
            )
            .expect("valid raw Host key");
            let rendered = args
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            assert_eq!(rendered[0], "write");
            assert!(rendered.contains(&"--stdin".to_owned()));
            assert!(rendered.contains(&"--expected-workspace-id=original-worktree".to_owned()));
            assert!(rendered.contains(&"--expected-tab-id=original-tab".to_owned()));
            assert!(!rendered.contains(&"--enter".to_owned()));
            assert!(!rendered.contains(&"--submit".to_owned()));
            assert!(!rendered.iter().any(|part| part == key));
        }
    }

    #[test]
    fn raw_input_rejects_stale_invalid_identifiers_and_nul_or_large_payloads() {
        assert!(verified_agent_raw_verb("", "workspace", "tab", "a").is_err());
        assert!(verified_agent_raw_verb("session", "workspace\nbad", "tab", "a").is_err());
        assert!(verified_agent_raw_verb("session", "workspace", "tab", "").is_err());
        assert!(verified_agent_raw_verb("session", "workspace", "tab", "\0").is_err());
        assert!(verified_agent_raw_verb("session", "workspace", "tab", &"x".repeat(4097)).is_err());
    }
}

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

/// Windows Terminal clipboard shortcuts. Ordinary Ctrl+C must continue to
/// reach the PTY as SIGINT, while Ctrl+Shift+C copies from the focused pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalClipboardShortcut {
    Copy,
    Paste,
}

pub fn terminal_clipboard_shortcut(
    key: &str,
    control: bool,
    shift: bool,
    alt: bool,
    platform: bool,
) -> Option<TerminalClipboardShortcut> {
    if !control || alt || platform {
        return None;
    }
    match (key.to_ascii_lowercase().as_str(), shift) {
        ("c", true) => Some(TerminalClipboardShortcut::Copy),
        ("v", _) => Some(TerminalClipboardShortcut::Paste),
        _ => None,
    }
}

/// Respect Alacritty's live BRACKETED_PASTE mode, keep PTY clipboard input
/// bounded by the existing 4 KiB Host write limit, and reject NUL bytes.
pub fn terminal_clipboard_payload(text: &str, bracketed: bool) -> Option<String> {
    if text.is_empty() || text.contains('\0') {
        return None;
    }
    let payload = if bracketed {
        format!("\x1b[200~{text}\x1b[201~")
    } else {
        text.to_owned()
    };
    (payload.len() <= 4096).then_some(payload)
}

/// IME owns Unicode commits. Ordinary ASCII keys must use KeyDown because
/// GPUI/Windows may not send EntityInputHandler events for plain typing.
pub fn is_text_input_keystroke(
    key: &str,
    key_char: Option<&str>,
    control: bool,
    alt: bool,
    platform: bool,
) -> bool {
    if control || alt || platform {
        return false;
    }
    key_char.is_some_and(|text| text.chars().any(|ch| !ch.is_ascii() && !ch.is_control()))
        || (key.chars().count() == 1 && key.chars().any(|ch| !ch.is_ascii()))
}

/// One routing decision shared by both Terminal panes. While native IME
/// text is marked, candidate navigation, phonetic ASCII and even shortcuts
/// must not reach the Host PTY. Native Unicode commits take the input handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalKeyDispatch {
    Composing,
    NativeText,
    Clipboard(TerminalClipboardShortcut),
    Pty,
}

pub fn terminal_key_dispatch(
    key: &str,
    key_char: Option<&str>,
    control: bool,
    shift: bool,
    alt: bool,
    platform: bool,
    composing: bool,
    native_ime_mode: bool,
) -> TerminalKeyDispatch {
    if composing {
        TerminalKeyDispatch::Composing
    } else if let Some(shortcut) = terminal_clipboard_shortcut(key, control, shift, alt, platform) {
        TerminalKeyDispatch::Clipboard(shortcut)
    } else if is_text_input_keystroke(key, key_char, control, alt, platform) {
        TerminalKeyDispatch::NativeText
    } else if native_ime_mode
        && !control
        && !alt
        && !platform
        && (key.eq_ignore_ascii_case("space")
            || (key.len() == 1 && key.as_bytes()[0].is_ascii_graphic())
            || key_char.is_some_and(|text| {
                !text.is_empty() && text.chars().all(|ch| ch.is_ascii_graphic() || ch == ' ')
            }))
    {
        // A CJK IME can consume ASCII before TSF emits its first marked preedit.
        TerminalKeyDispatch::NativeText
    } else {
        TerminalKeyDispatch::Pty
    }
}

/// Only suppresses recently forwarded KeyDown ASCII. Paste, unicode
/// injection and Windows IME can deliver text without any preceding KeyDown,
/// which must still reach Host. Owned by the single Native input surface.
#[derive(Debug, Default)]
pub struct TerminalTextCommitGate {
    recent_raw_keys: VecDeque<(String, Instant)>,
}

impl TerminalTextCommitGate {
    const RECENT_WINDOW: Duration = Duration::from_millis(160);
    const MAX_RECENT_KEYS: usize = 64;

    fn expire(&mut self, now: Instant) {
        while self
            .recent_raw_keys
            .front()
            .is_some_and(|(_, time)| now.saturating_duration_since(*time) > Self::RECENT_WINDOW)
        {
            self.recent_raw_keys.pop_front();
        }
    }

    pub fn note_raw_ascii_key(&mut self, text: &str, now: Instant) {
        if text.len() != 1 || !(text.as_bytes()[0].is_ascii_graphic() || text == " ") {
            return;
        }
        self.expire(now);
        if self.recent_raw_keys.len() >= Self::MAX_RECENT_KEYS {
            self.recent_raw_keys.pop_front();
        }
        self.recent_raw_keys.push_back((text.to_owned(), now));
    }

    pub fn should_forward(&mut self, text: &str, was_marked: bool, now: Instant) -> bool {
        self.expire(now);
        if text.is_empty() {
            return false;
        }
        if was_marked || !text.is_ascii() {
            return true;
        }
        // TextInput may batch characters. Match only exact previously sent
        // sequences, never suppress arbitrary ASCII text without KeyDown.
        for start in 0..self.recent_raw_keys.len() {
            let mut joined = String::new();
            for end in start..self.recent_raw_keys.len() {
                joined.push_str(&self.recent_raw_keys[end].0);
                if joined == text {
                    for _ in start..=end {
                        self.recent_raw_keys.remove(start);
                    }
                    return false;
                }
                if !text.starts_with(&joined) {
                    break;
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod direct_text_input_tests {
    use super::*;

    #[test]
    fn ascii_text_commit_without_keydown_reaches_host_but_duplicate_keydown_does_not() {
        let now = Instant::now();
        let mut gate = TerminalTextCommitGate::default();
        // Unicode SendInput, paste and some Windows IMEs deliver characters
        // through TextInput without a matching KeyDown.
        assert!(gate.should_forward("echo GPUI_NATIVE_OK", false, now));
        assert!(gate.should_forward("a", false, now));
        gate.note_raw_ascii_key("a", now);
        assert!(!gate.should_forward("a", false, now + Duration::from_millis(5)));
        // A later unrelated text event is no longer a duplicate.
        assert!(gate.should_forward("a", false, now + Duration::from_secs(1)));
    }

    #[test]
    fn unicode_commits_and_multi_character_keydown_coalescing_preserve_order() {
        let now = Instant::now();
        let mut gate = TerminalTextCommitGate::default();
        gate.note_raw_ascii_key("e", now);
        gate.note_raw_ascii_key("c", now);
        gate.note_raw_ascii_key("h", now);
        gate.note_raw_ascii_key("o", now);
        assert!(!gate.should_forward("echo", false, now + Duration::from_millis(8)));
        assert!(gate.should_forward("echo", false, now + Duration::from_millis(9)));
        gate.note_raw_ascii_key("x", now);
        assert!(gate.should_forward("貓", true, now + Duration::from_millis(10)));
        assert!(!gate.should_forward("x", false, now + Duration::from_millis(12)));
    }

    #[test]
    fn ascii_keys_reach_raw_host_while_unicode_uses_ime_commit() {
        assert!(!is_text_input_keystroke(
            "a",
            Some("a"),
            false,
            false,
            false
        ));
        assert!(!is_text_input_keystroke(
            "space",
            Some(" "),
            false,
            false,
            false
        ));
        assert!(is_text_input_keystroke(
            "中",
            Some("中"),
            false,
            false,
            false
        ));
        assert!(!is_text_input_keystroke("enter", None, false, false, false));
        assert!(!is_text_input_keystroke(
            "backspace",
            None,
            false,
            false,
            false
        ));
        assert!(!is_text_input_keystroke("a", Some("a"), true, false, false));
        assert!(!is_text_input_keystroke("left", None, false, false, false));
    }
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

/// Never accept generic commands from menu text. The Host derives the
/// launch command from the common descriptor registry and checks overrides.
pub fn create_agent_terminal_verb(
    workspace_id: &str,
    agent_id: &str,
    executable: &str,
) -> io::Result<Vec<OsString>> {
    if !valid_host_id(workspace_id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid Host workspace",
        ));
    }
    if !alera_core::agent_descriptor::agent_descriptor(agent_id)
        .is_some_and(|descriptor| descriptor.id == agent_id)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Unknown Agent type",
        ));
    }
    let path = Path::new(executable);
    if executable.is_empty()
        || executable.len() > 1024
        || executable.chars().any(|ch| ch.is_control())
        || !path.is_absolute()
        || !path.is_file()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid Agent executable",
        ));
    }
    Ok(vec![
        OsString::from("create-agent"),
        OsString::from(format!("--workspace-id={workspace_id}")),
        OsString::from(format!("--agent-id={agent_id}")),
        OsString::from(format!("--executable-path={executable}")),
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
        // The status is trustworthy only for a live, positively identified
        // Agent. A plain Terminal or stopped PTY has no current Hook state.
        let agent_state = (running && agent_type.is_some())
            .then(|| item.get("agentState").and_then(Value::as_str))
            .flatten()
            .and_then(AgentActivityState::parse);
        let interrupted = agent_state == Some(AgentActivityState::Done)
            && item.get("interrupted").and_then(Value::as_bool) == Some(true);
        let agent_state_started_at = agent_state.and_then(|_| {
            item.get("stateStartedAt")
                .and_then(Value::as_str)
                .filter(|value| agent_host_utc_time_key(value).is_some())
                .map(str::to_string)
        });
        let agent_updated_at = agent_state.and_then(|_| {
            item.get("updatedAt")
                .and_then(Value::as_str)
                .filter(|value| agent_host_utc_time_key(value).is_some())
                .map(str::to_string)
        });
        if sessions
            .iter()
            .any(|session: &TerminalSession| session.handle == handle)
        {
            return Err(invalid("Duplicate terminal session handle"));
        }
        let agent_context = if running && agent_type.is_some() {
            item.get("contextOverview")
                .and_then(AgentContextOverview::parse)
        } else {
            None
        };
        sessions.push(TerminalSession {
            handle: handle.to_string(),
            workspace_id: workspace_id.to_string(),
            tab_id: tab_id.to_string(),
            running,
            agent_type,
            agent_state,
            interrupted,
            agent_state_started_at,
            agent_updated_at,
            agent_context,
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

/// Resolve a notification's exact authenticated Host workspace ID to a
/// canonical existing directory. A duplicate ID is ambiguous even when two
/// entries point at the same folder. Never guess from a repo name.
pub fn resolve_registered_host_workspace_path(
    workspace_id: &str,
    registered: &[RegisteredHostWorkspace],
) -> io::Result<PathBuf> {
    if !valid_host_id(workspace_id) {
        return Err(invalid("Invalid Agent notification Host workspace ID"));
    }
    let mut matched = registered.iter().filter(|entry| entry.id == workspace_id);
    let first = matched.next().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Agent workspace is not registered with Host",
        )
    })?;
    if matched.next().is_some() || !first.path.is_absolute() {
        return Err(invalid("Ambiguous or relative Agent workspace path"));
    }
    let canonical = dunce::canonicalize(&first.path)?;
    if !canonical.is_dir() {
        return Err(invalid("Agent workspace directory is not available"));
    }
    Ok(canonical)
}

/// Verify a Dashboard new-Agent target against a fresh authenticated Host
/// snapshot. Both the ID and canonical path must be unique. A renamed,
/// deleted or re-registered checkout must not receive a stale launch intent.
pub fn validate_global_agent_launch_workspace(
    workspace_id: &str,
    expected_root: &Path,
    registered: &[RegisteredHostWorkspace],
) -> io::Result<()> {
    let actual_root = resolve_registered_host_workspace_path(workspace_id, registered)?;
    if actual_root != expected_root
        || select_registered_host_workspace(expected_root, registered)?.as_str() != workspace_id
    {
        return Err(invalid(
            "Global Agent launch target changed; refresh Host workspaces",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod global_agent_launch_workspace_tests {
    use super::*;

    #[test]
    fn launch_target_requires_the_same_unique_registered_workspace() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let root_a = dunce::canonicalize(a.path()).unwrap();
        let root_b = dunce::canonicalize(b.path()).unwrap();
        let registered = vec![
            RegisteredHostWorkspace {
                id: "alera-ws".into(),
                path: root_a.clone(),
            },
            RegisteredHostWorkspace {
                id: "mcp-ws".into(),
                path: root_b.clone(),
            },
        ];
        assert!(validate_global_agent_launch_workspace("mcp-ws", &root_b, &registered).is_ok());
        assert!(validate_global_agent_launch_workspace("mcp-ws", &root_a, &registered).is_err());
        assert!(
            validate_global_agent_launch_workspace("deleted-ws", &root_b, &registered).is_err()
        );
        let mut recycled = registered.clone();
        recycled[1].path = root_a.clone();
        assert!(validate_global_agent_launch_workspace("mcp-ws", &root_b, &recycled).is_err());
        let mut duplicate_id = registered.clone();
        duplicate_id.push(RegisteredHostWorkspace {
            id: "mcp-ws".into(),
            path: root_b.clone(),
        });
        assert!(validate_global_agent_launch_workspace("mcp-ws", &root_b, &duplicate_id).is_err());
        let mut duplicate_path = registered.clone();
        duplicate_path.push(RegisteredHostWorkspace {
            id: "other-ws".into(),
            path: root_b.clone(),
        });
        assert!(
            validate_global_agent_launch_workspace("mcp-ws", &root_b, &duplicate_path).is_err()
        );
    }
}

#[cfg(test)]
mod h03_workspace_notice_tests {
    use super::*;

    #[test]
    fn notification_workspace_routing_requires_exact_unique_host_id_and_existing_directory() {
        let temp = tempfile::tempdir().expect("temp workspace");
        let root = dunce::canonicalize(temp.path()).expect("canonical workspace");
        let registered = vec![RegisteredHostWorkspace {
            id: "h03-workspace".to_owned(),
            path: root.clone(),
        }];
        assert_eq!(
            resolve_registered_host_workspace_path("h03-workspace", &registered).unwrap(),
            root
        );
        assert!(resolve_registered_host_workspace_path("other-workspace", &registered).is_err());
        let mut ambiguous = registered.clone();
        ambiguous.push(registered[0].clone());
        assert!(resolve_registered_host_workspace_path("h03-workspace", &ambiguous).is_err());
        let relative = vec![RegisteredHostWorkspace {
            id: "h03-workspace".to_owned(),
            path: PathBuf::from("relative"),
        }];
        assert!(resolve_registered_host_workspace_path("h03-workspace", &relative).is_err());
    }
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

/// A direct-Agent Host create reply must corroborate the exact requested
/// adapter and executable in its persisted tab payload, in addition to its
/// workspace/tab/session identity. A running PTY without Hook presence may
/// attach only after this stronger verification succeeds.
pub fn parse_created_agent_terminal(
    value: &Value,
    expected_workspace_id: &str,
    expected_agent_id: &str,
    expected_executable: &str,
) -> io::Result<CreatedHostTerminal> {
    let created = parse_created_terminal(value, expected_workspace_id)?;
    let tab = value
        .get("tab")
        .ok_or_else(|| invalid("Agent create response has no tab"))?;
    let payload = tab
        .get("payload")
        .ok_or_else(|| invalid("Agent create response has no payload"))?;
    let exact_agent = payload.get("agentType").and_then(Value::as_str) == Some(expected_agent_id);
    let exact_executable = payload
        .pointer("/initialManagedAgentLaunch/executable")
        .and_then(Value::as_str)
        == Some(expected_executable);
    let no_unrequested_arguments = payload
        .pointer("/initialManagedAgentLaunch/arguments")
        .and_then(Value::as_array)
        .is_some_and(Vec::is_empty);
    let host_spawn_requested = payload.get("spawnOnCreate").and_then(Value::as_bool) == Some(true);
    if !exact_agent || !exact_executable || !no_unrequested_arguments || !host_spawn_requested {
        return Err(invalid(
            "Host Agent tab does not match the requested adapter and executable",
        ));
    }
    Ok(created)
}

/// Separate proven PTY readiness from the Agent Hook identity. A direct
/// Agent's exact Host persisted tab was already authenticated before this
/// function is called; a missing Hook must not leave the workbench showing
/// an endless 'Agent starting' state.
pub fn direct_agent_ready_feedback(
    session: &TerminalSession,
    expected_agent: &str,
) -> Option<&'static str> {
    if !session.running
        || session
            .agent_type
            .as_deref()
            .is_some_and(|reported| reported != expected_agent)
    {
        return None;
    }
    Some(if session.agent_type.is_some() {
        "Host PTY ready; Agent Hook recognized"
    } else {
        "Host PTY ready; Agent Hook not yet detected"
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
    parse_budget: TerminalParseBudget,
    generation: u64,
    decoder: TextDecoder,
    stream_utf8_pending: Vec<u8>,
    stream_attached: bool,
    stream_fallback_pending: bool,
}

/// Host output events contain raw PTY bytes; unlike CLI terminal.read JSON
/// they are not guaranteed to stop at a UTF-8 character boundary.
fn decode_terminal_stream_text(pending: &mut Vec<u8>, bytes: &[u8]) -> String {
    pending.extend_from_slice(bytes);
    let mut output = String::new();
    loop {
        match std::str::from_utf8(pending) {
            Ok(text) => {
                output.push_str(text);
                pending.clear();
                break;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                if valid > 0 {
                    output.push_str(std::str::from_utf8(&pending[..valid]).unwrap_or_default());
                }
                if let Some(invalid) = error.error_len() {
                    output.push('\u{fffd}');
                    pending.drain(..valid + invalid);
                } else {
                    // A valid prefix followed by a partial Unicode sequence:
                    // only retain the handful of incomplete trailing bytes.
                    pending.drain(..valid);
                    break;
                }
            }
        }
    }
    output
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

impl TerminalTabContextTarget {
    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }

    pub fn tab_id(&self) -> &str {
        &self.tab_id
    }
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
    estimate_terminal_viewport_with_line_height(region_width_px, region_height_px, font_px, 1.3)
}

/// Classic TerminalSettings defaults to AleraTokens.space12 for both axes.
/// Keep VT row estimation and Windows IME candidate anchoring in sync with UI.
pub const TERMINAL_PADDING_X: f32 = 12.0;
pub const TERMINAL_PADDING_Y: f32 = 12.0;

pub fn estimate_terminal_viewport_with_line_height(
    region_width_px: f32,
    region_height_px: f32,
    font_px: f32,
    line_height_factor: f32,
) -> Option<(u16, u16)> {
    if !font_px.is_finite()
        || !line_height_factor.is_finite()
        || !(0.8..=2.4).contains(&line_height_factor)
        || !(8.0..=48.0).contains(&font_px)
    {
        return None;
    }
    // Retain the historical API for non-GPUI callers; the native frontend
    // uses the actual shaped font advance through the metrics API below.
    estimate_terminal_viewport_with_cell_metrics(
        region_width_px,
        region_height_px,
        font_px * 0.62,
        font_px * line_height_factor,
    )
}

/// Translate a drawable GPUI Terminal pane into PTY cells using the measured
/// monospace glyph advance. The same metrics must be used for the IME cursor.
/// Avoid assuming all user-selectable monospace families are 0.62em wide.
pub fn estimate_terminal_viewport_with_cell_metrics(
    region_width_px: f32,
    region_height_px: f32,
    cell_width_px: f32,
    row_height_px: f32,
) -> Option<(u16, u16)> {
    if !region_width_px.is_finite()
        || !region_height_px.is_finite()
        || !cell_width_px.is_finite()
        || !row_height_px.is_finite()
        || !(3.0..=64.0).contains(&cell_width_px)
        || !(6.0..=128.0).contains(&row_height_px)
    {
        return None;
    }
    let columns = ((region_width_px - 2.0 * TERMINAL_PADDING_X) / cell_width_px).floor();
    let lines = ((region_height_px - 2.0 * TERMINAL_PADDING_Y) / row_height_px).floor();
    if columns < 20.0 || lines < 5.0 {
        return None;
    }
    Some((columns.min(512.0) as u16, lines.min(200.0) as u16))
}

/// Position the tiny Windows IME input capture near the actual VT cursor,
/// not at the top-left of its Terminal pane. Reuse the same JetBrains Mono
/// metrics as Native's existing viewport estimator and clamp to the visible
/// panel so a narrow/scrolling split cannot strand the candidate popup offscreen.
/// The Host PTY dimensions and cursor are never changed by this calculation.
pub fn terminal_ime_anchor_px(
    cursor: (u16, u16),
    panel_width_px: f32,
    panel_height_px: f32,
    font_px: f32,
) -> Option<(f32, f32)> {
    terminal_ime_anchor_with_line_height(cursor, panel_width_px, panel_height_px, font_px, 1.3)
}

pub fn terminal_ime_anchor_with_line_height(
    cursor: (u16, u16),
    panel_width_px: f32,
    panel_height_px: f32,
    font_px: f32,
    line_height_factor: f32,
) -> Option<(f32, f32)> {
    if !font_px.is_finite()
        || !line_height_factor.is_finite()
        || !(0.8..=2.4).contains(&line_height_factor)
        || !(8.0..=48.0).contains(&font_px)
    {
        return None;
    }
    terminal_ime_anchor_with_cell_metrics(
        cursor,
        panel_width_px,
        panel_height_px,
        font_px * 0.62,
        font_px * line_height_factor,
    )
}

/// Match the IME capture position to the actual GPUI-shaped VT cell advance
/// (rather than the default-family estimate), including resized split panes.
pub fn terminal_ime_anchor_with_cell_metrics(
    cursor: (u16, u16),
    panel_width_px: f32,
    panel_height_px: f32,
    cell_width_px: f32,
    row_height_px: f32,
) -> Option<(f32, f32)> {
    if !panel_width_px.is_finite()
        || !panel_height_px.is_finite()
        || !cell_width_px.is_finite()
        || !row_height_px.is_finite()
        || !(3.0..=64.0).contains(&cell_width_px)
        || !(6.0..=128.0).contains(&row_height_px)
        || panel_width_px < 32.0
        || panel_height_px < 24.0
    {
        return None;
    }
    let visible_cols = ((panel_width_px - 2.0 * TERMINAL_PADDING_X) / cell_width_px)
        .floor()
        .max(1.0);
    let visible_rows = ((panel_height_px - 2.0 * TERMINAL_PADDING_Y) / row_height_px)
        .floor()
        .max(1.0);
    let col = (cursor.1 as f32).min(visible_cols - 1.0);
    let row = (cursor.0 as f32).min(visible_rows - 1.0);
    Some((
        (TERMINAL_PADDING_X + col * cell_width_px).min(panel_width_px - 2.0),
        (TERMINAL_PADDING_Y + row * row_height_px).min(panel_height_px - 2.0),
    ))
}

/// A single, bounded per-Workbench debounce/coalescing gate.  Only visible
/// Host tab identities are observed; each unique viewport is sent at most
/// once until its dimensions change. A failed Host request is not spammed
/// repeatedly while the user is idle.
#[derive(Debug, Default)]
pub struct TerminalAutoResizeGate {
    pending: Vec<(TerminalViewportProposal, std::time::Instant)>,
    attempted: Vec<TerminalViewportProposal>,
    manually_suspended: Vec<TerminalTabContextTarget>,
}

impl TerminalAutoResizeGate {
    pub fn clear(&mut self) {
        self.pending.clear();
        self.attempted.clear();
        self.manually_suspended.clear();
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
        // Invalidate any in-flight automatic response even when debounce
        // already dispatched it before the user chose a manual size.
        if self.attempted.iter().any(|current| &current.target == target) {
            self.manually_suspended.retain(|old| old != target);
            self.manually_suspended.push(target.clone());
        }
    }

    pub fn observe(&mut self, visible: Vec<TerminalViewportProposal>, now: std::time::Instant) {
        // A Host tab can be mirrored in multiple visible panes while owning
        // only one PTY geometry. Avoid constantly resetting debounce when
        // those panes propose different sizes for the same session.
        let mut unique: Vec<TerminalViewportProposal> = Vec::new();
        for proposal in visible {
            if let Some(previous) = unique
                .iter_mut()
                .find(|previous| previous.target == proposal.target)
            {
                previous.cols = previous.cols.min(proposal.cols);
                previous.rows = previous.rows.min(proposal.rows);
            } else {
                unique.push(proposal);
            }
        }
        // A manual preset stays authoritative until the observable pane
        // geometry changes or its Host tab disappears.
        self.manually_suspended.retain(|target| {
            unique.iter().any(|visible| {
                &visible.target == target
                    && self.attempted.iter().any(|previous| previous == visible)
            })
        });
        self.pending
            .retain(|(pending, _)| unique.iter().any(|new| new.target == pending.target));
        self.attempted
            .retain(|attempted| unique.iter().any(|new| new.target == attempted.target));
        // The mixed Workbench split and the inner Terminal split can expose
        // three sessions; never starve the third behind the old take(2) limit.
        for new in unique.into_iter().take(8) {
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

    /// A resize request may return after the window has been resized again.
    /// Only the latest still-visible proposal can change the local VT grid.
    pub fn can_apply_response(&self, request: &TerminalViewportProposal) -> bool {
        !self.manually_suspended.contains(&request.target)
            && self.attempted.iter().any(|attempted| attempted == request)
            && !self.pending.iter().any(|(pending, _)| {
                pending.target == request.target && pending != request
            })
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
            || rows.len() > 200
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
    pub fn view_generation(&self) -> u64 {
        self.generation
    }

    fn new(session: &TerminalSession, generation: u64, scrollback_lines: usize) -> Self {
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
            screen: VtScreen::with_scrollback(24, 80, scrollback_lines),
            parse_budget: TerminalParseBudget::default(),
            generation,
            decoder: TextDecoder::default(),
            stream_utf8_pending: Vec::new(),
            stream_attached: false,
            stream_fallback_pending: false,
        }
    }

    fn apply(&mut self, chunk: TerminalChunk) -> bool {
        if self.handle != chunk.handle {
            return false;
        }
        if self.stream_fallback_pending {
            // A stream snapshot may contain synthetic VT restore bytes rather
            // than a byte-for-byte replay of Host scrollback. Start the next
            // polled read from Host's actual retained cursor, never append
            // it to stale partial stream-rendered output.
            self.output.clear();
            self.screen.reset();
            self.decoder = TextDecoder::default();
            self.stream_utf8_pending.clear();
            self.parse_budget = TerminalParseBudget::default();
            self.stream_fallback_pending = false;
            self.stream_attached = false;
        }
        if let Some(last) = self.next_cursor {
            if chunk.next_cursor < last {
                // Host restarted / scrollback reset; next read starts from
                // the fresh tail, rather than using an invalid old cursor.
                self.output.clear();
                self.decoder = TextDecoder::default();
                self.screen.reset();
                self.stream_utf8_pending.clear();
                self.parse_budget = TerminalParseBudget::default();
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
            self.stream_utf8_pending.clear();
            self.parse_budget = TerminalParseBudget::default();
            self.output
                .push_str("\n[Earlier terminal scrollback was truncated]\n");
        }
        // Replay exactly the Host bytes returned in this chunk into the VT
        // screen before stripping escapes from the optional text transcript.
        let parse_started = Instant::now();
        self.screen.feed(&chunk.text);
        self.decoder.append(&mut self.output, &chunk.text);
        self.parse_budget
            .observe(Instant::now(), chunk.text.len(), parse_started.elapsed());
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
    pub max_bytes: u64,
    generation: u64,
}

/// Host creation confirmation uses the slower 1.5s housekeeping timer.
/// Never issue another create RPC because Host list propagation is delayed:
/// only re-list the existing Host.
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
    OutcomeUnknown {
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

    pub fn is_outcome_unknown(&self) -> bool {
        matches!(self.phase, TerminalCreatePhase::OutcomeUnknown { .. })
    }

    /// Rehydrate a create that was already submitted when the GUI exited.
    /// No Host identity was proven, so recovery only permits listing and
    /// explicit acknowledgement, never another create or auto-attach.
    pub fn restore_outcome_unknown(&mut self) -> bool {
        if self.is_active() {
            return false;
        }
        self.next_request_id = self.next_request_id.wrapping_add(1).max(1);
        self.phase = TerminalCreatePhase::OutcomeUnknown {
            request_id: self.next_request_id,
        };
        true
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

    /// A lost CLI reply cannot prove that the Host did not already create a PTY.
    /// Keep the request blocked until the user reviews existing sessions and
    /// explicitly dismisses the uncertain result.
    pub fn mark_request_outcome_unknown(&mut self, request_id: u64) -> bool {
        if !matches!(self.phase, TerminalCreatePhase::Creating { request_id: active } if active == request_id)
        {
            return false;
        }
        self.phase = TerminalCreatePhase::OutcomeUnknown { request_id };
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
        self.confirm_sessions_with_agent(sessions, None)
    }

    /// Call only after parse_created_agent_terminal corroborates the Host's
    /// persisted Agent type and executable. Hook presence can arrive later.
    /// A running PTY with the exact Host identity and no Hook report is safe
    /// to show, but a stopped session or conflicting Hook type is not.
    pub fn confirm_sessions_with_host_verified_agent(
        &mut self,
        sessions: &[TerminalSession],
        expected_agent: &str,
    ) -> Option<CreatedHostTerminal> {
        let TerminalCreatePhase::Awaiting { created, .. } = &self.phase else {
            return None;
        };
        if !sessions.iter().any(|session| {
            created.matches_live_session(session)
                && session.running
                && session
                    .agent_type
                    .as_deref()
                    .is_none_or(|reported| reported == expected_agent)
        }) {
            return None;
        }
        let confirmed = created.clone();
        self.phase = TerminalCreatePhase::Idle;
        Some(confirmed)
    }

    /// Strict Hook-confirmed adapter matching for callers that have not
    /// verified a direct Agent create response. Ordinary terminals unchanged.
    pub fn confirm_sessions_with_agent(
        &mut self,
        sessions: &[TerminalSession],
        expected_agent: Option<&str>,
    ) -> Option<CreatedHostTerminal> {
        let TerminalCreatePhase::Awaiting { created, .. } = &self.phase else {
            return None;
        };
        if !sessions.iter().any(|session| {
            created.matches_live_session(session)
                && expected_agent.is_none_or(|agent| {
                    session.running && session.agent_type.as_deref() == Some(agent)
                })
        }) {
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

/// Limits foreground read latency without polling the CLI at a high rate while
/// idle or hidden. The caller drives this with one lifecycle-bound 50ms timer.
#[derive(Debug, Clone)]
pub struct TerminalPollCadence {
    last_poll: Instant,
    interactive_until: Instant,
}

impl Default for TerminalPollCadence {
    fn default() -> Self {
        Self::new(Instant::now())
    }
}

impl TerminalPollCadence {
    pub const WAKE_INTERVAL: Duration = Duration::from_millis(50);
    /// Native Host push frames should be rendered on the next display frame,
    /// without spinning a 16ms timer when terminal is hidden or disconnected.
    const VISIBLE_STREAM_INTERVAL: Duration = Duration::from_millis(16);
    const FAST_INTERVAL: Duration = Duration::from_millis(100);
    const IDLE_INTERVAL: Duration = Duration::from_millis(350);
    const HIDDEN_INTERVAL: Duration = Duration::from_millis(1500);
    const ACTIVITY_WINDOW: Duration = Duration::from_secs(3);

    pub fn wake_interval(stream_active: bool, terminal_visible: bool) -> Duration {
        if stream_active && terminal_visible {
            Self::VISIBLE_STREAM_INTERVAL
        } else {
            Self::WAKE_INTERVAL
        }
    }

    pub fn new(now: Instant) -> Self {
        Self {
            last_poll: now.checked_sub(Self::HIDDEN_INTERVAL).unwrap_or(now),
            interactive_until: now,
        }
    }

    pub fn record_activity(&mut self, now: Instant) {
        self.interactive_until = now + Self::ACTIVITY_WINDOW;
    }

    pub fn is_due(&mut self, now: Instant, foreground: bool) -> bool {
        let interval = if !foreground {
            Self::HIDDEN_INTERVAL
        } else if now < self.interactive_until {
            Self::FAST_INTERVAL
        } else {
            Self::IDLE_INTERVAL
        };
        if now.saturating_duration_since(self.last_poll) < interval {
            return false;
        }
        self.last_poll = now;
        true
    }
}

#[cfg(test)]
mod terminal_poll_cadence_tests {
    use super::*;

    #[test]
    fn pushed_output_has_frame_rate_wakeup_only_in_visible_terminal() {
        assert_eq!(
            TerminalPollCadence::wake_interval(true, true),
            Duration::from_millis(16)
        );
        assert_eq!(
            TerminalPollCadence::wake_interval(true, false),
            TerminalPollCadence::WAKE_INTERVAL
        );
        assert_eq!(
            TerminalPollCadence::wake_interval(false, true),
            TerminalPollCadence::WAKE_INTERVAL
        );
    }

    #[test]
    fn foreground_output_is_fast_only_during_activity_and_background_is_throttled() {
        let now = Instant::now();
        let mut cadence = TerminalPollCadence::new(now);
        assert!(cadence.is_due(now, true));
        assert!(!cadence.is_due(now + Duration::from_millis(99), true));
        cadence.record_activity(now + Duration::from_millis(100));
        assert!(cadence.is_due(now + Duration::from_millis(100), true));
        assert!(!cadence.is_due(now + Duration::from_millis(199), true));
        assert!(cadence.is_due(now + Duration::from_millis(200), true));
        assert!(!cadence.is_due(now + Duration::from_millis(400), false));
        assert!(cadence.is_due(now + Duration::from_millis(1700), false));
        assert!(!cadence.is_due(now + Duration::from_millis(1799), true));
        assert!(cadence.is_due(now + Duration::from_millis(1800), true));
        assert!(cadence.is_due(now + Duration::from_millis(3200), true));
        assert!(!cadence.is_due(now + Duration::from_millis(3449), true));
        assert!(cadence.is_due(now + Duration::from_millis(3550), true));
    }
}

/// Gate successive failed Host reads, independent of the normal polling
/// cadence, so a broken CLI cannot create an unbounded read loop.
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

#[derive(Debug)]
pub struct TerminalDock {
    pub sessions: Vec<TerminalSession>,
    pub tabs: Vec<TerminalTab>,
    pub active_tab: Option<usize>,
    pub status: String,
    pub view_mode: TerminalViewMode,
    session_list_stale: bool,
    next_generation: u64,
    workspace_generation: u64,
    scrollback_lines: usize,
}

impl Default for TerminalDock {
    fn default() -> Self {
        Self {
            sessions: Vec::new(),
            tabs: Vec::new(),
            active_tab: None,
            status: String::new(),
            view_mode: TerminalViewMode::default(),
            session_list_stale: false,
            next_generation: 0,
            workspace_generation: 0,
            scrollback_lines: DEFAULT_SCROLLBACK_LINES,
        }
    }
}

impl TerminalDock {
    pub fn for_workspace(generation: u64) -> Self {
        Self {
            workspace_generation: generation,
            ..Self::default()
        }
    }

    /// Native-only visible VT scrollback cap; Host byte retention is separate.
    /// Apply a new selection to all live screen emulators without destroying
    /// output or changing Host session/PTY identity.
    pub fn set_scrollback_lines(&mut self, lines: usize) {
        let lines = lines.clamp(
            crate::vt_screen::MIN_SCROLLBACK_LINES,
            crate::vt_screen::MAX_SCROLLBACK_LINES,
        );
        self.scrollback_lines = lines;
        for tab in &mut self.tabs {
            tab.screen.set_scrollback_lines(lines);
        }
    }

    pub fn scrollback_lines(&self) -> usize {
        self.scrollback_lines
    }

    pub fn workspace_generation(&self) -> u64 {
        self.workspace_generation
    }

    /// A failed CLI inventory is not authoritative evidence of a terminated
    /// Host session. Keep the current preview and running state for display,
    /// but reject new identity-sensitive actions until the Host confirms it.
    pub fn note_session_list_failure(&mut self) {
        self.session_list_stale = true;
    }

    pub fn session_list_is_stale(&self) -> bool {
        self.session_list_stale
    }

    /// Preserve the old screen until a safe CLI read arrives, but force that
    /// read to restore Host scrollback from scratch after stream loss.
    pub fn invalidate_stream_observers(&mut self) {
        for tab in &mut self.tabs {
            if tab.stream_attached {
                tab.next_cursor = None;
                tab.stream_fallback_pending = true;
            }
        }
    }

    fn live_stream_tab_mut(
        &mut self,
        session_id: &str,
        workspace_id: &str,
        tab_id: &str,
    ) -> Option<&mut TerminalTab> {
        if !self.sessions.iter().any(|session| {
            session.running
                && session.handle == session_id
                && session.workspace_id == workspace_id
                && session.tab_id == tab_id
        }) {
            return None;
        }
        self.tabs.iter_mut().find(|tab| {
            tab.handle == session_id && tab.workspace_id == workspace_id && tab.tab_id == tab_id
        })
    }

    /// Replace an existing Terminal's emulator state from an authenticated
    /// Host observation. Never creates a new preview or shell implicitly.
    pub fn apply_stream_snapshot(
        &mut self,
        session_id: &str,
        workspace_id: &str,
        tab_id: &str,
        output_cursor: u64,
        rows: u16,
        cols: u16,
        bytes: &[u8],
    ) -> bool {
        let Some(tab) = self.live_stream_tab_mut(session_id, workspace_id, tab_id) else {
            return false;
        };
        tab.output.clear();
        // Use the original PTY dimensions before replaying absolute cursor
        // moves/hard wraps. This changes only local VT preview geometry.
        tab.screen = VtScreen::with_scrollback(rows, cols, tab.screen.scrollback_lines());
        tab.decoder = TextDecoder::default();
        tab.stream_utf8_pending.clear();
        tab.parse_budget = TerminalParseBudget::default();
        let text = decode_terminal_stream_text(&mut tab.stream_utf8_pending, bytes);
        tab.screen.feed(&text);
        tab.decoder.append(&mut tab.output, &text);
        tab.next_cursor = Some(output_cursor);
        tab.running = true;
        tab.stream_attached = true;
        tab.stream_fallback_pending = false;
        true
    }

    /// Append exactly one ordered Host output frame, retaining UTF-8 split
    /// bytes and using its *byte* count to advance the absolute Host cursor.
    pub fn apply_stream_output(
        &mut self,
        session_id: &str,
        workspace_id: &str,
        tab_id: &str,
        bytes: &[u8],
    ) -> bool {
        if bytes.is_empty() {
            return false;
        }
        let Some(tab) = self.live_stream_tab_mut(session_id, workspace_id, tab_id) else {
            return false;
        };
        let started = Instant::now();
        let text = decode_terminal_stream_text(&mut tab.stream_utf8_pending, bytes);
        tab.screen.feed(&text);
        tab.decoder.append(&mut tab.output, &text);
        tab.parse_budget
            .observe(Instant::now(), bytes.len(), started.elapsed());
        tab.next_cursor = tab
            .next_cursor
            .map(|cursor| cursor.saturating_add(bytes.len() as u64));
        tab.stream_attached = true;
        true
    }

    /// Keep the local visible VT grid in sync after the Host accepts a
    /// normal viewport resize. Never change an unrelated or recycled tab.
    /// This does not create or resize a Host PTY; the Host request ran first.
    pub fn apply_host_accepted_viewport(
        &mut self,
        target: &TerminalTabContextTarget,
        cols: u16,
        rows: u16,
    ) -> bool {
        if !self.context_target_is_live(target) {
            return false;
        }
        let Some(index) = self.index_for_context(target) else {
            return false;
        };
        let Some(tab) = self.tabs.get_mut(index) else {
            return false;
        };
        if !tab.running {
            return false;
        }
        tab.screen.set_preview_size(rows, cols);
        true
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
        !self.session_list_stale
            && self.index_for_context(target).is_some()
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
        let mut sessions = sessions;
        // A snapshot RPC can complete after a newer Host delta. Preserve
        // that newer exact-identity status, but do not leak it to a recycled
        // handle, changed Agent type or a stopped session. A removed Hook
        // leaves a revision tombstone (state=None, updated_at=Some) so an
        // older inventory cannot resurrect a falsely "working" Agent.
        for current in &mut sessions {
            if !current.running {
                continue;
            }
            let Some(old) = self.sessions.iter().find(|old| {
                old.running
                    && old.handle == current.handle
                    && old.workspace_id == current.workspace_id
                    && old.tab_id == current.tab_id
                    && old.agent_type == current.agent_type
            }) else {
                continue;
            };
            let old_time = old
                .agent_updated_at
                .as_deref()
                .and_then(agent_host_utc_time_key);
            let current_time = current
                .agent_updated_at
                .as_deref()
                .and_then(agent_host_utc_time_key);
            if old.agent_state.is_none() {
                // A newer removal outranks a stale/unversioned snapshot.
                // Carry it across a no-status inventory until a verifiably
                // newer Hook state arrives. Equal times fail closed.
                if old_time.as_ref().is_some_and(|previous| {
                    current_time
                        .as_ref()
                        .is_none_or(|incoming| previous >= incoming)
                }) {
                    current.agent_state = None;
                    current.interrupted = false;
                    current.agent_state_started_at = None;
                    current.agent_updated_at = old.agent_updated_at.clone();
                }
            } else if current.agent_state.is_some()
                && old_time
                    .as_ref()
                    .zip(current_time.as_ref())
                    .is_some_and(|(previous, incoming)| previous > incoming)
            {
                // Never restore old activity when the Host explicitly lists
                // no state; only carry a more recent positive Hook state.
                current.agent_state = old.agent_state;
                current.interrupted = old.interrupted;
                current.agent_state_started_at = old.agent_state_started_at.clone();
                current.agent_updated_at = old.agent_updated_at.clone();
            }
        }
        self.session_list_stale = false;
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

    /// Apply redacted authenticated Host deltas to the *existing* exact
    /// identity only. Cannot open/attach a tab, start PTYs or select panes.
    /// Older updates never replace newer state, regardless of event order.
    pub fn apply_agent_presence_changes(&mut self, changes: &[AgentPresenceChange]) -> bool {
        let mut changed = false;
        for change in changes.iter().take(MAX_TERMINALS) {
            let Some(session) = self.sessions.iter_mut().find(|session| {
                session.running
                    && session.handle == change.session_id
                    && session.workspace_id == change.workspace_id
                    && session.tab_id == change.tab_id
            }) else {
                continue;
            };
            // The Host may list a running PTY before its first Agent Hook.
            // A redacted, authenticated delta with the same session/workspace/
            // tab identity may identify it without creating or moving a PTY.
            // Never allow a different Agent adapter to clear or overwrite it.
            if session
                .agent_type
                .as_ref()
                .zip(change.agent_type.as_ref())
                .is_some_and(|(known, incoming)| known != incoming)
            {
                continue;
            }
            // All updates, including removal, must be ordered against the
            // latest confirmed revision. An older "removed" event must not
            // erase a newer Working/Waiting/Blocked/Done event.
            let Some(incoming_time) = agent_host_utc_time_key(&change.updated_at) else {
                continue;
            };
            if session
                .agent_updated_at
                .as_deref()
                .and_then(agent_host_utc_time_key)
                .is_some_and(|previous_time| incoming_time < previous_time)
            {
                continue;
            }
            if change.removed {
                if session.agent_state.is_some()
                    || session.interrupted
                    || session.agent_updated_at.as_deref() != Some(change.updated_at.as_str())
                {
                    session.agent_state = None;
                    session.agent_updated_at = Some(change.updated_at.clone());
                    session.agent_state_started_at = None;
                    session.interrupted = false;
                    changed = true;
                }
                continue;
            }
            if change.state.is_none() || change.agent_type.is_none() {
                continue;
            }
            let next_interrupted =
                change.state == Some(AgentActivityState::Done) && change.interrupted;
            if session.agent_type.is_none()
                || session.agent_state != change.state
                || session.interrupted != next_interrupted
                || session.agent_updated_at.as_deref() != Some(change.updated_at.as_str())
            {
                if session.agent_type.is_none() {
                    session.agent_type = change.agent_type.clone();
                }
                session.agent_state = change.state;
                session.interrupted = next_interrupted;
                session.agent_state_started_at = Some(change.state_started_at.clone());
                session.agent_updated_at = Some(change.updated_at.clone());
                changed = true;
            }
        }
        changed
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
                self.tabs[index] =
                    TerminalTab::new(session, self.next_generation, self.scrollback_lines);
            }
            self.active_tab = Some(index);
            return true;
        }
        if self.tabs.len() >= MAX_OPEN_TABS {
            self.status = format!("Terminal preview limit reached ({MAX_OPEN_TABS})");
            return false;
        }
        self.next_generation = self.next_generation.wrapping_add(1);
        self.tabs.push(TerminalTab::new(
            session,
            self.next_generation,
            self.scrollback_lines,
        ));
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
            max_bytes: tab.parse_budget.max_read_bytes(),
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
        // An authenticated Host stream now owns this preview's VT state.
        // A CLI read started before that stream attached can have the *same*
        // cursor, so cursor/generation equality alone cannot prevent it
        // overwriting fresh stream output. Only a confirmed stream loss
        // unlocks the full retained-Host-read fallback.
        if tab.stream_attached && !tab.stream_fallback_pending {
            return false;
        }
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
    fn host_menu_target_requires_same_live_triple_not_just_reused_handle() {
        let expected = TerminalSession {
            handle: "h1".into(),
            workspace_id: "workspace-a".into(),
            tab_id: "tab-1".into(),
            running: true,
            agent_type: None,
            agent_state: None,
            interrupted: false,
            agent_state_started_at: None,
            agent_updated_at: None,
            agent_context: None,
        };
        assert!(expected.matches_running_identity(&expected));
        let mut current = expected.clone();
        current.tab_id = "tab-2".into();
        assert!(!expected.matches_running_identity(&current));
        current = expected.clone();
        current.workspace_id = "workspace-b".into();
        assert!(!expected.matches_running_identity(&current));
        current = expected.clone();
        current.running = false;
        assert!(!expected.matches_running_identity(&current));
        let mut stopped_snapshot = expected.clone();
        stopped_snapshot.running = false;
        assert!(!stopped_snapshot.matches_running_identity(&expected));
    }

    #[test]
    fn native_ime_candidate_anchor_tracks_vt_cursor_inside_each_visible_pane() {
        let at_origin = terminal_ime_anchor_px((0, 0), 420.0, 470.0, 14.0).unwrap();
        assert_eq!(at_origin, (12.0, 12.0));

        let moved = terminal_ime_anchor_px((11, 20), 420.0, 470.0, 14.0).unwrap();
        assert!((moved.0 - (12.0 + 20.0 * 14.0 * 0.62)).abs() < 0.01);
        assert!((moved.1 - (12.0 + 11.0 * 14.0 * 1.3)).abs() < 0.01);

        let narrow = terminal_ime_anchor_px((99, 239), 220.0, 180.0, 14.0).unwrap();
        assert!(narrow.0 >= 12.0 && narrow.0 <= 218.0);
        assert!(narrow.1 >= 12.0 && narrow.1 <= 178.0);
        assert!(terminal_ime_anchor_px((0, 0), f32::NAN, 100.0, 14.0).is_none());
        assert!(terminal_ime_anchor_px((0, 0), 500.0, f32::INFINITY, 14.0).is_none());
        assert!(terminal_ime_anchor_px((0, 0), 500.0, 500.0, 0.0).is_none());
    }

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
            Some((512, 200))
        );
        let smaller = estimate_terminal_viewport(800.0, 480.0, 13.0).unwrap();
        let larger = estimate_terminal_viewport(1200.0, 780.0, 13.0).unwrap();
        assert!(larger.0 > smaller.0 && larger.1 > smaller.1);
    }

    #[test]
    fn gpui_measured_cell_metrics_keep_pty_and_ime_aligned() {
        let (narrow_cols, _) =
            estimate_terminal_viewport_with_cell_metrics(820.0, 470.0, 9.0, 18.0).unwrap();
        let (wide_cols, _) =
            estimate_terminal_viewport_with_cell_metrics(820.0, 470.0, 11.0, 18.0).unwrap();
        assert!(narrow_cols > wide_cols, "Wider actual font must request fewer PTY columns");
        let cursor = (3, 9);
        let anchor =
            terminal_ime_anchor_with_cell_metrics(cursor, 820.0, 470.0, 11.0, 18.0).unwrap();
        assert_eq!(anchor, (12.0 + 9.0 * 11.0, 12.0 + 3.0 * 18.0));
        assert!(estimate_terminal_viewport_with_cell_metrics(820.0, 470.0, 0.0, 18.0).is_none());
        assert!(terminal_ime_anchor_with_cell_metrics(
            cursor, 820.0, 470.0, f32::NAN, 18.0
        ).is_none());
    }

    #[test]
    fn shared_terminal_line_height_keeps_pty_rows_and_ime_anchor_in_sync() {
        let tight = estimate_terminal_viewport_with_line_height(820.0, 470.0, 14.0, 0.8).unwrap();
        let loose = estimate_terminal_viewport_with_line_height(820.0, 470.0, 14.0, 2.4).unwrap();
        assert_eq!(tight.0, loose.0, "Line height must not change PTY width");
        assert!(
            tight.1 > loose.1,
            "Larger line height reduces Host PTY rows"
        );
        let compact_ime =
            terminal_ime_anchor_with_line_height((11, 20), 820.0, 470.0, 14.0, 0.8).unwrap();
        let loose_ime =
            terminal_ime_anchor_with_line_height((11, 20), 820.0, 470.0, 14.0, 2.4).unwrap();
        assert!(
            loose_ime.1 > compact_ime.1,
            "IME anchor must match visible cursor row"
        );
        assert!(estimate_terminal_viewport_with_line_height(820.0, 470.0, 14.0, 0.7).is_none());
        assert!(
            terminal_ime_anchor_with_line_height((0, 0), 820.0, 470.0, 14.0, f32::NAN).is_none()
        );
        assert_eq!(
            estimate_terminal_viewport_with_line_height(1200.0, 600.0, 13.0, 1.3),
            estimate_terminal_viewport(1200.0, 600.0, 13.0)
        );
    }

    #[test]
    fn wide_viewport_estimation_does_not_letterbox_at_240_columns() {
        let (cols, rows) = estimate_terminal_viewport_with_line_height(3030.0, 920.0, 14.0, 1.3)
            .expect("large Windows viewport");
        assert!(cols > 240, "Host supports more than 240 columns: {cols}");
        assert!(cols <= 512);
        assert!(rows >= 40);
    }

    #[test]
    fn host_accepted_resize_expands_only_the_matching_live_preview() {
        let mut dock = TerminalDock::for_workspace(5);
        assert!(dock.apply_session_list(5, vec![session("pty-1")]));
        assert!(dock.open("pty-1"));
        let current = dock.context_target_at(0).unwrap();
        let mut stale = current.clone();
        stale.tab_id = "different-tab".to_owned();
        assert!(!dock.apply_host_accepted_viewport(&stale, 320, 64));
        assert_eq!(dock.active().unwrap().screen.dimensions(), (24, 80));
        assert!(dock.apply_host_accepted_viewport(&current, 320, 64));
        assert_eq!(dock.active().unwrap().screen.dimensions(), (64, 320));
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
    fn manual_resize_invalidates_inflight_auto_reply_until_geometry_changes() {
        let now = std::time::Instant::now();
        let target = TerminalTabContextTarget {
            handle: "pty".into(),
            workspace_id: "ws".into(),
            tab_id: "tab".into(),
            generation: 1,
            workspace_generation: 1,
        };
        let first = TerminalViewportProposal {
            target: target.clone(), cols: 90, rows: 30,
        };
        let next = TerminalViewportProposal {
            target: target.clone(), cols: 130, rows: 45,
        };
        let mut gate = TerminalAutoResizeGate::default();
        gate.observe(vec![first.clone()], now);
        assert_eq!(
            gate.take_ready(now + std::time::Duration::from_millis(310)),
            vec![first.clone()],
        );
        assert!(gate.can_apply_response(&first));
        gate.suspend_until_changed(&target);
        assert!(!gate.can_apply_response(&first));
        gate.observe(vec![first.clone()], now + std::time::Duration::from_millis(320));
        assert!(gate.take_ready(now + std::time::Duration::from_secs(3)).is_empty());
        gate.observe(vec![next.clone()], now + std::time::Duration::from_secs(4));
        assert_eq!(
            gate.take_ready(now + std::time::Duration::from_millis(4310)),
            vec![next.clone()],
        );
        assert!(gate.can_apply_response(&next));
    }

    #[test]
    fn host_resize_disposition_is_explicit_and_legacy_compatible() {
        assert_eq!(
            TerminalResizeOutcome::from_host_payload(&serde_json::json!({
                "applied": true, "deferred": false, "cols": 100, "rows": 30
            })),
            TerminalResizeOutcome::Applied,
        );
        assert_eq!(
            TerminalResizeOutcome::from_host_payload(&serde_json::json!({
                "applied": false, "deferred": true, "cols": 48, "rows": 22
            })),
            TerminalResizeOutcome::Deferred,
        );
        assert_eq!(
            TerminalResizeOutcome::from_host_payload(&serde_json::json!({})),
            TerminalResizeOutcome::LegacyUnconfirmed,
        );
        assert!(!TerminalResizeOutcome::Deferred.may_update_preview());
        assert!(TerminalResizeOutcome::Applied.may_update_preview());
        assert!(TerminalResizeOutcome::LegacyUnconfirmed.may_update_preview());
    }

    #[test]
    fn stale_host_resize_reply_must_not_overwrite_newer_viewport() {
        let now = std::time::Instant::now();
        let target = TerminalTabContextTarget {
            handle: "same-host".into(),
            workspace_id: "ws".into(),
            tab_id: "tab".into(),
            generation: 4,
            workspace_generation: 7,
        };
        let old = TerminalViewportProposal {
            target: target.clone(), cols: 80, rows: 24,
        };
        let latest = TerminalViewportProposal {
            target, cols: 120, rows: 40,
        };
        let mut gate = TerminalAutoResizeGate::default();
        gate.observe(vec![old.clone()], now);
        assert_eq!(
            gate.take_ready(now + std::time::Duration::from_millis(310)),
            vec![old.clone()],
        );
        assert!(gate.can_apply_response(&old));
        gate.observe(vec![latest.clone()], now + std::time::Duration::from_millis(320));
        assert!(!gate.can_apply_response(&old), "newer pending dimensions win");
        assert!(!gate.can_apply_response(&latest), "not yet sent to Host");
        assert_eq!(
            gate.take_ready(now + std::time::Duration::from_millis(630)),
            vec![latest.clone()],
        );
        assert!(!gate.can_apply_response(&old), "older response must remain stale");
        assert!(gate.can_apply_response(&latest));
        gate.observe(vec![], now + std::time::Duration::from_secs(2));
        assert!(!gate.can_apply_response(&latest), "hidden tab invalidates reply");
    }

    #[test]
    fn three_visible_terminal_previews_each_receive_an_auto_resize() {
        let now = std::time::Instant::now();
        let proposals = (0..3)
            .map(|index| TerminalViewportProposal {
                target: TerminalTabContextTarget {
                    handle: format!("pty-{index}"),
                    workspace_id: "workspace".into(),
                    tab_id: format!("tab-{index}"),
                    generation: 1,
                    workspace_generation: 1,
                },
                cols: 100 - index * 10,
                rows: 30 - index * 5,
            })
            .collect::<Vec<_>>();
        let mut gate = TerminalAutoResizeGate::default();
        gate.observe(proposals.clone(), now);
        assert_eq!(
            gate.take_ready(now + std::time::Duration::from_millis(301)),
            proposals,
        );
    }

    #[test]
    fn duplicated_host_viewport_stabilizes_on_smallest_visible_size() {
        let now = std::time::Instant::now();
        let target = TerminalTabContextTarget {
            handle: "same-host".into(),
            workspace_id: "workspace".into(),
            tab_id: "tab".into(),
            generation: 1,
            workspace_generation: 1,
        };
        let large = TerminalViewportProposal {
            target: target.clone(),
            cols: 145,
            rows: 35,
        };
        let small = TerminalViewportProposal {
            target,
            cols: 90,
            rows: 20,
        };
        let mut gate = TerminalAutoResizeGate::default();
        gate.observe(vec![large.clone(), small.clone()], now);
        // The Host appears in both panes and their enumeration order flips.
        gate.observe(
            vec![small.clone(), large],
            now + std::time::Duration::from_millis(180),
        );
        assert_eq!(
            gate.take_ready(now + std::time::Duration::from_millis(310)),
            vec![small.clone()],
        );
        gate.observe(vec![small], now + std::time::Duration::from_secs(1));
        assert!(gate.take_ready(now + std::time::Duration::from_secs(2)).is_empty());
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
    fn vt_selection_supports_full_200_row_native_pane_without_crossing_snapshot_bounds() {
        let target = TerminalTabContextTarget {
            handle: "pty-large".into(),
            workspace_id: "ws".into(),
            tab_id: "tab".into(),
            generation: 1,
            workspace_generation: 2,
        };
        let rows = (0..200).map(|i| format!("row{i}")).collect::<Vec<_>>();
        let mut selection =
            TerminalLineSelection::begin(target.clone(), Some(8), rows.clone(), 149)
                .expect("full-height Terminal pane row selection");
        assert!(selection.extend(151));
        assert_eq!(selection.selected_text(), "row149\nrow150\nrow151");
        assert!(selection.is_current(&target, Some(8)));
        assert!(
            TerminalLineSelection::begin(target, Some(8), vec!["x".into(); 201], 150).is_none()
        );
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
    fn native_terminal_clipboard_shortcuts_and_bracketed_paste_are_safe() {
        use super::TerminalClipboardShortcut::{Copy, Paste};
        assert_eq!(
            terminal_clipboard_shortcut("c", true, true, false, false),
            Some(Copy)
        );
        assert_eq!(
            terminal_clipboard_shortcut("V", true, true, false, false),
            Some(Paste)
        );
        assert_eq!(
            terminal_clipboard_shortcut("v", true, false, false, false),
            Some(Paste)
        );
        assert_eq!(
            terminal_clipboard_shortcut("c", true, false, false, false),
            None
        );
        assert_eq!(
            terminal_clipboard_shortcut("v", true, true, true, false),
            None
        );
        assert_eq!(
            terminal_clipboard_shortcut("v", true, true, false, true),
            None
        );
        assert_eq!(
            terminal_clipboard_payload("echo 中文", false),
            Some("echo 中文".to_owned())
        );
        assert_eq!(
            terminal_clipboard_payload("one\ntwo", true),
            Some("\x1b[200~one\ntwo\x1b[201~".to_owned())
        );
        assert!(terminal_clipboard_payload("", false).is_none());
        assert!(terminal_clipboard_payload("bad\0payload", false).is_none());
        assert!(terminal_clipboard_payload(&"x".repeat(4096), true).is_none());
    }

    #[test]
    fn native_ime_preedit_blocks_raw_pty_keys_and_clipboard_shortcuts() {
        use TerminalClipboardShortcut::{Copy, Paste};
        use TerminalKeyDispatch::{Clipboard, Composing, NativeText, Pty};

        // Phonetic ASCII and candidate-navigation keys belong to the IME.
        for (key, key_char, ctrl, shift) in [
            ("s", Some("s"), false, false),
            ("space", Some(" "), false, false),
            ("backspace", None, false, false),
            ("enter", None, false, false),
            ("c", Some("c"), true, true),
            ("v", Some("v"), true, true),
            ("c", Some("c"), true, false),
        ] {
            assert_eq!(
                terminal_key_dispatch(key, key_char, ctrl, shift, false, false, true, false),
                Composing,
                "{key} must stay with native IME until it commits"
            );
        }

        // After commit, Host keys and clipboard shortcuts are restored.
        assert_eq!(
            terminal_key_dispatch("s", Some("s"), false, false, false, false, false, false),
            Pty
        );
        assert_eq!(
            terminal_key_dispatch("enter", None, false, false, false, false, false, false),
            Pty
        );
        assert_eq!(
            terminal_key_dispatch("c", None, true, true, false, false, false, false),
            Clipboard(Copy)
        );
        assert_eq!(
            terminal_key_dispatch("v", None, true, true, false, false, false, false),
            Clipboard(Paste)
        );
        assert_eq!(
            terminal_key_dispatch("c", None, true, false, false, false, false, false),
            Pty
        );
        assert_eq!(
            terminal_key_dispatch("中", Some("中"), false, false, false, false, false, false),
            NativeText
        );
    }

    #[test]
    fn active_windows_native_ime_routes_first_phonetic_key_to_text_input() {
        use TerminalClipboardShortcut::Copy;
        use TerminalKeyDispatch::{Clipboard, NativeText, Pty};

        // IME may consume the first phonetic key before GPUI receives a
        // marked-text callback. Do not leak that ASCII key into the PTY.
        assert_eq!(
            terminal_key_dispatch("s", Some("s"), false, false, false, false, false, true),
            NativeText
        );
        assert_eq!(
            terminal_key_dispatch("space", Some(" "), false, false, false, false, false, true),
            NativeText
        );
        // Chinese keyboard present but English/alphanumeric mode selected.
        assert_eq!(
            terminal_key_dispatch("s", Some("s"), false, false, false, false, false, false),
            Pty
        );
        // Non-printable keys and Ctrl+C remain valid PTY controls unless
        // a native marked composition is actively in progress.
        assert_eq!(
            terminal_key_dispatch("enter", None, false, false, false, false, false, true),
            Pty
        );
        assert_eq!(
            terminal_key_dispatch("c", None, true, false, false, false, false, true),
            Pty
        );
        assert_eq!(
            terminal_key_dispatch("c", None, true, true, false, false, false, true),
            Clipboard(Copy)
        );
        assert_eq!(
            terminal_key_dispatch("s", Some("s"), false, false, true, false, false, true),
            Pty
        );
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

        let guarded =
            verified_agent_prompt_verb("term-1", "workspace-1", "tab-1", "修正 Windows 注音")
                .unwrap();
        assert_eq!(guarded[0], "write");
        assert!(guarded.iter().any(|v| v == "--submit"));
        assert!(guarded
            .iter()
            .any(|v| v == "--expected-workspace-id=workspace-1"));
        assert!(guarded.iter().any(|v| v == "--expected-tab-id=tab-1"));
        assert!(!guarded.iter().any(|v| v == "修正 Windows 注音"));
        let routed = terminal_cli_arguments(Path::new("C:/runtime"), &guarded);
        assert!(routed.iter().any(|v| v == "--no-start"));
        assert!(verified_agent_prompt_verb("term-1", "", "tab-1", "prompt").is_err());
        assert!(verified_agent_prompt_verb("term-1", "ws", "../tab", "prompt").is_err());
        for terminate in [false, true] {
            let args = verified_agent_control_verb("term-1", "ws-one", "tab-one", terminate)
                .expect("verified Agent control");
            assert_eq!(args[0], "control-agent-verified");
            assert!(args
                .iter()
                .any(|arg| arg == "--expected-workspace-id=ws-one"));
            assert!(args.iter().any(|arg| arg == "--expected-tab-id=tab-one"));
            assert!(args
                .iter()
                .any(|arg| arg == if terminate { "terminate" } else { "interrupt" }));
            assert_eq!(
                args.iter().any(|arg| arg == "--confirm-terminate"),
                terminate
            );
            let routed = terminal_cli_arguments(Path::new("C:/runtime"), &args);
            assert!(routed.iter().any(|arg| arg == "--no-start"));
        }
        assert!(verified_agent_control_verb("term-1", "ws-one", "../bad", true).is_err());
        assert!(verified_agent_control_verb("", "ws-one", "tab-one", false).is_err());
    }

    #[test]
    fn native_terminal_read_budget_is_scoped_to_the_active_host_tab() {
        use crate::runtime_bridge::terminal_cli_arguments;
        use std::path::Path;

        let mut dock = TerminalDock::for_workspace(6);
        assert!(dock.apply_session_list(6, vec![session("term-1")]));
        assert!(dock.open("term-1"));
        let ticket = dock.active_ticket().expect("confirmed Host session");
        assert_eq!(ticket.max_bytes, 16 * 1024);
        let args = read_terminal_verb_bounded(&ticket.handle, ticket.cursor, ticket.max_bytes)
            .expect("Host read within adaptive budget");
        assert!(args.iter().any(|argument| argument == "16384"));
        assert!(terminal_cli_arguments(Path::new("C:/runtime"), &args)
            .iter()
            .any(|argument| argument == "--no-start"));
        assert!(read_terminal_verb_bounded("term-1", None, 4095).is_err());
        assert!(read_terminal_verb_bounded("term-1", None, 65_537).is_err());
        assert!(read_terminal_verb_bounded("bad\nhandle", None, 8192).is_err());
        assert!(read_terminal_verb("term-1", None)
            .unwrap()
            .iter()
            .any(|argument| argument == "65536"));
    }

    #[test]
    fn native_parse_budget_restarts_after_host_cursor_reset_or_truncation() {
        let s = session("pty-1");
        let started = Instant::now();
        let mut rewind_tab = TerminalTab::new(&s, 1, DEFAULT_SCROLLBACK_LINES);
        rewind_tab
            .parse_budget
            .observe(started, 16 * 1024, Duration::from_millis(1));
        assert_eq!(rewind_tab.parse_budget.max_read_bytes(), 24 * 1024);
        rewind_tab.next_cursor = Some(500);
        assert!(rewind_tab.apply(chunk("pty-1", 0, 10, "reset")));
        assert_eq!(rewind_tab.parse_budget.max_read_bytes(), 16 * 1024);

        let mut truncated_tab = TerminalTab::new(&s, 2, DEFAULT_SCROLLBACK_LINES);
        truncated_tab
            .parse_budget
            .observe(started, 16 * 1024, Duration::from_millis(1));
        assert_eq!(truncated_tab.parse_budget.max_read_bytes(), 24 * 1024);
        let mut truncated = chunk("pty-1", 0, 10, "snapshot");
        truncated.truncated = true;
        assert!(truncated_tab.apply(truncated));
        assert_eq!(truncated_tab.parse_budget.max_read_bytes(), 16 * 1024);
    }

    #[test]
    fn host_push_preserves_split_unicode_and_rejects_recycled_session_identity() {
        let mut dock = TerminalDock::for_workspace(3);
        assert!(dock.apply_session_list(3, vec![session("pty-1")]));
        assert!(dock.open("pty-1"));
        assert!(dock.apply_stream_snapshot(
            "pty-1",
            "workspace-1",
            "tab-pty-1",
            120,
            24,
            80,
            b"hello\n"
        ));
        let unicode = "貓".as_bytes();
        assert!(dock.apply_stream_output("pty-1", "workspace-1", "tab-pty-1", &unicode[..1]));
        assert!(dock.apply_stream_output("pty-1", "workspace-1", "tab-pty-1", &unicode[1..]));
        assert!(dock.active().unwrap().output.contains("貓"));
        assert_eq!(dock.active().unwrap().next_cursor, Some(120 + 3));

        let before = dock.active().unwrap().output.clone();
        assert!(!dock.apply_stream_output("pty-1", "other-worktree", "tab-pty-1", b"stale"));
        assert_eq!(dock.active().unwrap().output, before);
        assert!(!dock.apply_stream_snapshot(
            "pty-1",
            "workspace-1",
            "other-tab",
            999,
            24,
            80,
            b"wrong"
        ));
        assert_eq!(dock.active().unwrap().next_cursor, Some(123));
    }

    #[test]
    fn host_snapshot_replays_at_original_pty_geometry_without_host_resize() {
        let mut dock = TerminalDock::for_workspace(5);
        assert!(dock.apply_session_list(5, vec![session("pty-1")]));
        assert!(dock.open("pty-1"));
        let tab = dock.active().unwrap();
        assert_eq!(tab.screen.dimensions(), (24, 80));
        assert!(dock.apply_stream_snapshot(
            "pty-1",
            "workspace-1",
            "tab-pty-1",
            11,
            36,
            44,
            b"hello\n"
        ));
        assert_eq!(dock.active().unwrap().screen.dimensions(), (36, 44));
    }

    #[test]
    fn disconnected_host_stream_resets_next_cli_read_without_duplicate_snapshot() {
        let mut dock = TerminalDock::for_workspace(4);
        assert!(dock.apply_session_list(4, vec![session("pty-1")]));
        assert!(dock.open("pty-1"));
        assert!(dock.apply_stream_snapshot(
            "pty-1",
            "workspace-1",
            "tab-pty-1",
            500,
            24,
            80,
            b"old state\n"
        ));
        assert!(dock.apply_stream_output("pty-1", "workspace-1", "tab-pty-1", b"new"));
        dock.invalidate_stream_observers();
        let ticket = dock.active_ticket().expect("Host session remains live");
        assert_eq!(
            ticket.cursor, None,
            "recovery must reread authoritative retained output"
        );
        assert!(dock.apply_workspace_read(4, &ticket, chunk("pty-1", 0, 9, "new host\n")));
        assert_eq!(dock.active().unwrap().output, "new host\n");
        assert_eq!(dock.active().unwrap().next_cursor, Some(9));
    }

    #[test]
    fn late_cli_read_must_not_overwrite_authenticated_host_stream_snapshot() {
        let mut dock = TerminalDock::for_workspace(13);
        assert!(dock.apply_session_list(13, vec![session("pty-1")]));
        assert!(dock.open("pty-1"));
        let initial = dock.active_ticket().unwrap();
        assert!(dock.apply_workspace_read(13, &initial, chunk("pty-1", 0, 30, "prior\n")));
        let pending_legacy_read = dock.active_ticket().unwrap();
        assert_eq!(pending_legacy_read.cursor, Some(30));
        assert!(dock.apply_stream_snapshot(
            "pty-1",
            "workspace-1",
            "tab-pty-1",
            30,
            24,
            80,
            b"stream\n"
        ));
        assert!(!dock.apply_workspace_read(
            13,
            &pending_legacy_read,
            chunk("pty-1", 30, 36, "stale\n")
        ));
        assert_eq!(dock.active().unwrap().output, "stream\n");
        dock.invalidate_stream_observers();
        let recovery_read = dock.active_ticket().unwrap();
        assert_eq!(recovery_read.cursor, None);
        assert!(dock.apply_workspace_read(13, &recovery_read, chunk("pty-1", 0, 7, "host\n")));
        assert_eq!(dock.active().unwrap().output, "host\n");
    }

    #[test]
    fn native_scrollback_setting_applies_to_live_and_replayed_host_tabs() {
        let mut dock = TerminalDock::for_workspace(13);
        assert_eq!(dock.scrollback_lines(), DEFAULT_SCROLLBACK_LINES);
        dock.set_scrollback_lines(2_000);
        dock.update_sessions(vec![session("pty-1")]);
        assert!(dock.open("pty-1"));
        let tab = dock.active().unwrap();
        assert_eq!(tab.screen.scrollback_lines(), 2_000);
        let identity = dock.context_target_at(0).unwrap();
        dock.tabs[0].screen.feed("original");
        dock.set_scrollback_lines(300);
        assert_eq!(dock.tabs[0].screen.scrollback_lines(), 300);
        assert!(dock.tabs[0].screen.contents().contains("original"));
        assert!(dock.apply_stream_snapshot(
            &identity.handle,
            &identity.workspace_id,
            &identity.tab_id,
            3,
            24,
            80,
            b"new",
        ));
        assert_eq!(dock.tabs[0].screen.scrollback_lines(), 300);
        dock.update_sessions(vec![session("pty-1"), session("pty-2")]);
        assert!(dock.open("pty-2"));
        assert_eq!(dock.active().unwrap().screen.scrollback_lines(), 300);
        // A different Workspace receives the saved Native cap from Workbench,
        // not from stale tabs in the previous workspace.
        let mut next = TerminalDock::for_workspace(14);
        next.set_scrollback_lines(300);
        assert_eq!(next.scrollback_lines(), 300);
    }

    fn session(handle: &str) -> TerminalSession {
        TerminalSession {
            handle: handle.into(),
            workspace_id: "workspace-1".into(),
            tab_id: format!("tab-{handle}"),
            running: true,
            agent_type: Some("codex".into()),
            agent_state: None,
            interrupted: false,
            agent_state_started_at: None,
            agent_updated_at: None,
            agent_context: None,
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
    fn direct_agent_launch_uses_only_canonical_adapter_with_verified_executable() {
        let current_exe = std::env::current_exe().unwrap();
        let exe = current_exe.to_string_lossy().into_owned();
        let args = create_agent_terminal_verb("ws-123", "codex", &exe).unwrap();
        assert_eq!(args[0], "create-agent");
        assert_eq!(args[1], "--workspace-id=ws-123");
        assert_eq!(args[2], "--agent-id=codex");
        assert_eq!(
            args[3].to_str(),
            Some(format!("--executable-path={exe}").as_str())
        );
        assert!(create_agent_terminal_verb("../bad", "codex", &exe).is_err());
        assert!(create_agent_terminal_verb("ws-123", "unknown", &exe).is_err());
        assert!(create_agent_terminal_verb("ws-123", "claude", "./relative.exe").is_err());
        assert!(create_agent_terminal_verb("ws-123", "claude", "C:/missing.exe").is_err());
    }

    #[test]
    fn direct_agent_reply_must_bind_host_tab_type_and_executable_before_session_attach() {
        let exe = std::env::current_exe().unwrap();
        let exe = exe.to_string_lossy().into_owned();
        let response = serde_json::json!({
            "tab": {
                "id": "tab-pty-1",
                "workspaceId": "workspace-1",
                "kind": "terminal",
                "payload": {
                    "terminalSessionId": "pty-1",
                    "agentType": "codex",
                    "spawnOnCreate": true,
                    "initialManagedAgentLaunch": {"executable": exe, "arguments":[]}
                }
            },
            "attachment": {"sessionId": "pty-1", "workspaceId":"workspace-1", "tabId":"tab-pty-1"}
        });
        let expected =
            parse_created_agent_terminal(&response, "workspace-1", "codex", &exe).unwrap();
        assert_eq!(expected.handle, "pty-1");
        assert_eq!(expected.tab_id, "tab-pty-1");
        for wrong in ["claude", "unknown-agent"] {
            assert!(parse_created_agent_terminal(&response, "workspace-1", wrong, &exe).is_err());
        }
        let mut swapped = response.clone();
        swapped["tab"]["payload"]["initialManagedAgentLaunch"]["executable"] =
            serde_json::json!("C:/wrong-agent.exe");
        assert!(parse_created_agent_terminal(&swapped, "workspace-1", "codex", &exe).is_err());
        swapped = response.clone();
        swapped["tab"]["payload"]["agentType"] = serde_json::Value::Null;
        assert!(parse_created_agent_terminal(&swapped, "workspace-1", "codex", &exe).is_err());
        swapped = response.clone();
        swapped["attachment"]["sessionId"] = serde_json::json!("other-pty");
        assert!(parse_created_agent_terminal(&swapped, "workspace-1", "codex", &exe).is_err());
    }

    #[test]
    fn direct_agent_hook_delay_can_confirm_running_pty_only_after_verified_reply() {
        let mut flow = TerminalCreateFlow::default();
        let request = flow.start_request().unwrap();
        let mut sess = session("pty-no-hook");
        sess.agent_type = None;
        let created = CreatedHostTerminal {
            handle: sess.handle.clone(),
            workspace_id: sess.workspace_id.clone(),
            tab_id: sess.tab_id.clone(),
        };
        assert!(flow.finish_request(request, created.clone()));
        assert_eq!(
            flow.confirm_sessions_with_host_verified_agent(&[sess.clone()], "codex"),
            Some(created.clone()),
            "A Host-verified Agent PTY must not block on delayed Hook presence",
        );
        let mut stopped = sess.clone();
        stopped.running = false;
        let mut flow = TerminalCreateFlow::default();
        let request = flow.start_request().unwrap();
        assert!(flow.finish_request(request, created.clone()));
        assert!(flow
            .confirm_sessions_with_host_verified_agent(&[stopped], "codex")
            .is_none());
        let mut mismatch = sess;
        mismatch.agent_type = Some("claude".into());
        assert!(flow
            .confirm_sessions_with_host_verified_agent(&[mismatch], "codex")
            .is_none());
        assert!(flow.is_waiting());
    }

    #[test]
    fn direct_agent_status_separates_live_host_pty_from_hook_identity() {
        let mut session = session("pty-feedback");
        session.agent_type = None;
        assert_eq!(
            direct_agent_ready_feedback(&session, "codex"),
            Some("Host PTY ready; Agent Hook not yet detected"),
            "A real interactive PTY must not be mislabeled 'Agent starting' forever"
        );
        session.agent_type = Some("codex".into());
        assert_eq!(
            direct_agent_ready_feedback(&session, "codex"),
            Some("Host PTY ready; Agent Hook recognized")
        );
        session.agent_type = Some("claude".into());
        assert!(direct_agent_ready_feedback(&session, "codex").is_none());
        session.running = false;
        assert!(direct_agent_ready_feedback(&session, "claude").is_none());
    }

    #[test]
    fn direct_agent_confirmation_requires_correct_running_agent_not_only_same_host_tab() {
        let mut flow = TerminalCreateFlow::default();
        let request = flow.start_request().unwrap();
        let sess = session("pty-1");
        let created = CreatedHostTerminal {
            handle: sess.handle.clone(),
            workspace_id: sess.workspace_id.clone(),
            tab_id: sess.tab_id.clone(),
        };
        assert!(flow.finish_request(request, created.clone()));
        assert!(flow
            .confirm_sessions_with_agent(&[sess.clone()], Some("claude"))
            .is_none());
        assert!(flow.is_waiting());
        assert_eq!(
            flow.confirm_sessions_with_agent(&[sess], Some("codex")),
            Some(created)
        );
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
    fn host_agent_status_distinguishes_four_states_from_process_running() {
        let items = ["working", "waiting", "blocked", "done"]
            .into_iter()
            .enumerate()
            .map(|(i, state)| {
                json!({
                    "handle":format!("term-{i}"), "workspaceId":"workspace-1",
                    "tabId":format!("tab-{i}"), "running":true,
                    "agentType":"claude", "agentState":state,
                    "interrupted":state == "done"
                })
            })
            .collect::<Vec<_>>();
        let parsed = parse_terminal_list(&json!({"items":items})).unwrap();
        assert_eq!(
            parsed.iter().map(|s| s.agent_state).collect::<Vec<_>>(),
            vec![
                Some(AgentActivityState::Working),
                Some(AgentActivityState::Waiting),
                Some(AgentActivityState::Blocked),
                Some(AgentActivityState::Done),
            ]
        );
        assert!(parsed[1].running, "a waiting agent still owns a live PTY");
        assert!(parsed[1].label().contains("waiting"));
        assert!(parsed[3].interrupted);
        assert!(parsed[3].label().contains("interrupted"));
    }

    #[test]
    fn unproven_or_stopped_agent_status_never_defaults_to_working() {
        let parsed = parse_terminal_list(&json!({"items":[
            {"handle":"p1","workspaceId":"w","tabId":"t1","running":true,
             "agentType":"codex"},
            {"handle":"p2","workspaceId":"w","tabId":"t2","running":true,
             "agentType":"codex","agentState":"future-extension"},
            {"handle":"p3","workspaceId":"w","tabId":"t3","running":false,
             "agentType":"codex","agentState":"working"},
            {"handle":"p4","workspaceId":"w","tabId":"t4","running":true,
             "agentType":null,"agentState":"waiting"}
        ]}))
        .unwrap();
        assert!(parsed.iter().all(|s| s.agent_state.is_none()));
        assert!(parsed.iter().all(|s| !s.label().contains("working")));
    }

    #[test]
    fn active_agent_status_polls_faster_without_spinning_when_host_is_idle() {
        let mut active = session("h1");
        active.agent_state = Some(AgentActivityState::Waiting);
        let mut stopped = active.clone();
        stopped.running = false;
        assert!(agent_presence_refresh_due(1, &[active.clone()]));
        assert!(!agent_presence_refresh_due(2, &[active.clone()]));
        assert!(agent_presence_refresh_due(3, &[active]));
        assert!(!agent_presence_refresh_due(3, &[stopped]));
        assert!(agent_presence_refresh_due(11, &[]));
        assert!(!agent_presence_refresh_due(12, &[]));
    }

    #[test]
    fn h01_late_authenticated_hook_identifies_exact_running_session_without_waiting_for_inventory()
    {
        let mut dock = TerminalDock::for_workspace(17);
        let initial = json!({"items":[
            {"handle":"pty-late","workspaceId":"ws-1","tabId":"tab-A","running":true},
            {"handle":"pty-other","workspaceId":"ws-1","tabId":"tab-B","running":true,
                "agentType":"claude","agentState":"working","stateStartedAt":"2026-11-13T13:00:00Z","updatedAt":"2026-11-13T13:00:00Z"}
        ]});
        assert!(dock.apply_session_list(17, parse_terminal_list(&initial).unwrap()));
        assert!(dock.open("pty-late"));
        assert_eq!(dock.sessions[0].agent_type, None);
        let change = AgentPresenceChange {
            session_id: "pty-late".into(),
            workspace_id: "ws-1".into(),
            tab_id: "tab-A".into(),
            agent_type: Some("codex".into()),
            state: Some(AgentActivityState::Waiting),
            state_started_at: "2026-11-13T13:00:01Z".into(),
            updated_at: "2026-11-13T13:00:02Z".into(),
            interrupted: false,
            context_changed: false,
            removed: false,
        };
        assert!(dock.apply_agent_presence_changes(&[change.clone()]));
        assert_eq!(dock.sessions[0].agent_type.as_deref(), Some("codex"));
        assert_eq!(
            dock.sessions[0].agent_state,
            Some(AgentActivityState::Waiting)
        );
        assert_eq!(
            dock.sessions[1].agent_state,
            Some(AgentActivityState::Working)
        );
        assert_eq!(dock.active_tab, Some(0));
        assert_eq!(dock.tabs.len(), 1);

        let mut wrong_tab = change.clone();
        wrong_tab.tab_id = "tab-B".into();
        wrong_tab.state = Some(AgentActivityState::Blocked);
        assert!(!dock.apply_agent_presence_changes(&[wrong_tab]));
        let mut wrong_adapter = change.clone();
        wrong_adapter.agent_type = Some("claude".into());
        wrong_adapter.state = Some(AgentActivityState::Done);
        assert!(!dock.apply_agent_presence_changes(&[wrong_adapter]));
        assert_eq!(
            dock.sessions[0].agent_state,
            Some(AgentActivityState::Waiting)
        );

        for (moment, state) in [
            ("2026-11-13T13:00:03Z", AgentActivityState::Blocked),
            ("2026-11-13T13:00:04Z", AgentActivityState::Done),
            ("2026-11-13T13:00:05Z", AgentActivityState::Working),
        ] {
            let mut next = change.clone();
            next.state = Some(state);
            next.updated_at = moment.into();
            next.state_started_at = moment.into();
            assert!(dock.apply_agent_presence_changes(&[next]));
            assert_eq!(dock.sessions[0].agent_state, Some(state));
            assert_eq!(dock.active_tab, Some(0));
        }
    }
    #[test]
    fn host_presence_delta_rejects_stale_events_and_old_inventory_snapshots() {
        let mut dock = TerminalDock::for_workspace(7);
        let newer = "2026-11-11T07:00:02+00:00";
        let older = "2026-11-11T07:00:01Z";
        let start = "2026-11-11T07:00:00Z";
        let snapshot = json!({"items":[{
            "handle":"pty-1","workspaceId":"ws-1","tabId":"tab-1",
            "running":true,"agentType":"codex","agentState":"working",
            "stateStartedAt":start,"updatedAt":older
        }]});
        assert!(dock.apply_session_list(7, parse_terminal_list(&snapshot).unwrap()));
        let delta = AgentPresenceChange {
            session_id: "pty-1".into(),
            workspace_id: "ws-1".into(),
            tab_id: "tab-1".into(),
            agent_type: Some("codex".into()),
            state: Some(AgentActivityState::Waiting),
            updated_at: newer.into(),
            state_started_at: start.into(),
            interrupted: false,
            context_changed: false,
            removed: false,
        };
        assert!(dock.apply_agent_presence_changes(&[delta.clone()]));
        assert_eq!(
            dock.sessions[0].agent_state,
            Some(AgentActivityState::Waiting)
        );
        let mut stale = delta.clone();
        stale.state = Some(AgentActivityState::Blocked);
        stale.updated_at = older.into();
        assert!(!dock.apply_agent_presence_changes(&[stale]));
        assert!(dock.apply_session_list(7, parse_terminal_list(&snapshot).unwrap()));
        assert_eq!(
            dock.sessions[0].agent_state,
            Some(AgentActivityState::Waiting),
            "old snapshot must not reverse a newer delta"
        );
        let mut recycled = delta.clone();
        recycled.tab_id = "new-tab-with-recycled-handle".into();
        assert!(!dock.apply_agent_presence_changes(&[recycled]));
        assert_eq!(
            dock.sessions[0].agent_state,
            Some(AgentActivityState::Waiting)
        );
        let mut removed = delta;
        removed.removed = true;
        assert!(dock.apply_agent_presence_changes(&[removed]));
        assert_eq!(dock.sessions[0].agent_state, None);
    }

    #[test]
    fn removed_agent_hook_rejects_stale_remove_and_keeps_newer_revision() {
        let mut dock = TerminalDock::for_workspace(31);
        let snapshot = json!({"items":[{
            "handle":"pty-1","workspaceId":"w1","tabId":"t1","running":true,
            "agentType":"codex","agentState":"working",
            "stateStartedAt":"2026-11-16T12:00:00Z",
            "updatedAt":"2026-11-16T12:00:02Z"
        }]});
        assert!(dock.apply_session_list(31, parse_terminal_list(&snapshot).unwrap()));
        let make_delta =
            |time: &str, removed: bool, state: Option<AgentActivityState>| AgentPresenceChange {
                session_id: "pty-1".into(),
                workspace_id: "w1".into(),
                tab_id: "t1".into(),
                agent_type: Some("codex".into()),
                state,
                state_started_at: time.into(),
                updated_at: time.into(),
                interrupted: false,
                context_changed: false,
                removed,
            };
        assert!(
            !dock.apply_agent_presence_changes(&[make_delta("2026-11-16T12:00:01Z", true, None),]),
            "out-of-order removal cannot clear newer Hook"
        );
        assert_eq!(
            dock.sessions[0].agent_state,
            Some(AgentActivityState::Working)
        );

        let mut spoofed = make_delta("2026-11-16T12:00:05Z", true, None);
        spoofed.agent_type = Some("claude".into());
        assert!(!dock.apply_agent_presence_changes(&[spoofed]));
        assert_eq!(
            dock.sessions[0].agent_state,
            Some(AgentActivityState::Working)
        );

        assert!(dock.apply_agent_presence_changes(&[make_delta(
            "2026-11-16T12:00:06Z",
            true,
            None
        ),]));
        assert_eq!(dock.sessions[0].agent_state, None);
        assert_eq!(
            dock.sessions[0].agent_updated_at.as_deref(),
            Some("2026-11-16T12:00:06Z"),
            "keep revision tombstone without showing a live state"
        );
        assert!(!dock.apply_agent_presence_changes(&[make_delta(
            "2026-11-16T12:00:04Z",
            false,
            Some(AgentActivityState::Waiting)
        ),]));
        assert_eq!(dock.sessions[0].agent_state, None);
        assert!(dock.apply_agent_presence_changes(&[make_delta(
            "2026-11-16T12:00:07Z",
            false,
            Some(AgentActivityState::Working)
        ),]));
        assert_eq!(
            dock.sessions[0].agent_state,
            Some(AgentActivityState::Working)
        );
    }

    #[test]
    fn removed_agent_hook_stays_removed_across_older_and_unversioned_host_lists() {
        let mut dock = TerminalDock::for_workspace(32);
        let old = json!({"items":[{
            "handle":"pty-1","workspaceId":"w1","tabId":"t1","running":true,
            "agentType":"codex","agentState":"working",
            "stateStartedAt":"2026-11-16T12:00:00Z",
            "updatedAt":"2026-11-16T12:00:02Z"
        }]});
        assert!(dock.apply_session_list(32, parse_terminal_list(&old).unwrap()));
        let removed = AgentPresenceChange {
            session_id: "pty-1".into(),
            workspace_id: "w1".into(),
            tab_id: "t1".into(),
            agent_type: Some("codex".into()),
            state: None,
            state_started_at: "2026-11-16T12:00:06Z".into(),
            updated_at: "2026-11-16T12:00:06Z".into(),
            interrupted: false,
            context_changed: false,
            removed: true,
        };
        assert!(dock.apply_agent_presence_changes(&[removed]));
        assert!(dock.apply_session_list(32, parse_terminal_list(&old).unwrap()));
        assert_eq!(dock.sessions[0].agent_state, None);
        let unversioned = json!({"items":[{
            "handle":"pty-1","workspaceId":"w1","tabId":"t1","running":true,
            "agentType":"codex"
        }]});
        assert!(dock.apply_session_list(32, parse_terminal_list(&unversioned).unwrap()));
        assert_eq!(
            dock.sessions[0].agent_updated_at.as_deref(),
            Some("2026-11-16T12:00:06Z")
        );
        assert!(dock.apply_session_list(32, parse_terminal_list(&old).unwrap()));
        assert_eq!(dock.sessions[0].agent_state, None);
        let newer = json!({"items":[{
            "handle":"pty-1","workspaceId":"w1","tabId":"t1","running":true,
            "agentType":"codex","agentState":"done",
            "stateStartedAt":"2026-11-16T12:00:07Z",
            "updatedAt":"2026-11-16T12:00:07Z"
        }]});
        assert!(dock.apply_session_list(32, parse_terminal_list(&newer).unwrap()));
        assert_eq!(
            dock.sessions[0].agent_state,
            Some(AgentActivityState::Done),
            "a genuinely newer state must recover normally"
        );
        let reused = json!({"items":[{
            "handle":"pty-1","workspaceId":"w2","tabId":"t2","running":true,
            "agentType":"claude","agentState":"working",
            "stateStartedAt":"2026-11-16T12:00:01Z",
            "updatedAt":"2026-11-16T12:00:01Z"
        }]});
        assert!(dock.apply_session_list(32, parse_terminal_list(&reused).unwrap()));
        assert_eq!(dock.sessions[0].agent_type.as_deref(), Some("claude"));
        assert_eq!(
            dock.sessions[0].agent_state,
            Some(AgentActivityState::Working),
            "the old session tombstone must not leak to a new Host identity"
        );
    }

    #[test]
    fn isolated_multi_agent_host_payload_to_summary_survives_transitions_and_outage() {
        use crate::agent_status_visual::AgentStateSummary;
        let mut dock = TerminalDock::for_workspace(33);
        let inventory = json!({"kind":"terminals","items":[
            {"handle":"a","workspaceId":"ws1","tabId":"t1","running":true,"agentType":"codex",
             "agentState":"working","updatedAt":"2026-11-16T12:00:01Z"},
            {"handle":"b","workspaceId":"ws2","tabId":"t2","running":true,"agentType":"claude",
             "agentState":"waiting","updatedAt":"2026-11-16T12:00:01Z"},
            {"handle":"c","workspaceId":"ws1","tabId":"t3","running":true,"agentType":"agy",
             "agentState":"blocked","updatedAt":"2026-11-16T12:00:01Z"},
            {"handle":"d","workspaceId":"ws3","tabId":"t4","running":true,"agentType":"copilot",
             "agentState":"done","interrupted":true,"updatedAt":"2026-11-16T12:00:01Z"}
        ]});
        assert!(dock.apply_session_list(33, parse_terminal_list(&inventory).unwrap()));
        assert!(dock.open("a"));
        let original_active = dock.active_tab;
        let original_tabs = dock.tabs.len();
        let summary =
            AgentStateSummary::from_sessions(&dock.sessions, !dock.session_list_is_stale());
        assert_eq!(
            (
                summary.working,
                summary.waiting,
                summary.blocked,
                summary.interrupted,
                summary.done
            ),
            (1, 1, 1, 1, 0)
        );
        let change = AgentPresenceChange {
            session_id: "b".into(),
            workspace_id: "ws2".into(),
            tab_id: "t2".into(),
            agent_type: Some("claude".into()),
            state: Some(AgentActivityState::Working),
            state_started_at: "2026-11-16T12:00:04Z".into(),
            updated_at: "2026-11-16T12:00:04Z".into(),
            interrupted: false,
            context_changed: false,
            removed: false,
        };
        assert!(dock.apply_agent_presence_changes(&[change]));
        let summary =
            AgentStateSummary::from_sessions(&dock.sessions, !dock.session_list_is_stale());
        assert_eq!(
            (
                summary.working,
                summary.waiting,
                summary.blocked,
                summary.interrupted
            ),
            (2, 0, 1, 1)
        );
        dock.note_session_list_failure();
        assert_eq!(
            AgentStateSummary::from_sessions(&dock.sessions, !dock.session_list_is_stale()).total(),
            0
        );
        assert!(dock.apply_session_list(33, parse_terminal_list(&inventory).unwrap()));
        let summary =
            AgentStateSummary::from_sessions(&dock.sessions, !dock.session_list_is_stale());
        assert_eq!(
            (
                summary.working,
                summary.waiting,
                summary.blocked,
                summary.interrupted
            ),
            (2, 0, 1, 1),
            "older list must not undo newer authenticated delta"
        );
        assert_eq!(dock.active_tab, original_active);
        assert_eq!(dock.tabs.len(), original_tabs);
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
    fn terminal_create_cannot_confirm_a_host_session_that_exited_before_attach() {
        let mut flow = TerminalCreateFlow::default();
        let request = flow.start_request().unwrap();
        let created = CreatedHostTerminal {
            handle: "term-1".into(),
            workspace_id: "workspace-1".into(),
            tab_id: "tab-term-1".into(),
        };
        assert!(flow.finish_request(request, created.clone()));

        let mut exited = session("term-1");
        exited.running = false;
        exited.agent_type = None;
        assert!(
            !created.matches_live_session(&exited),
            "a matching Host identity is not proof that its PTY is still running"
        );
        assert!(
            flow.confirm_sessions(&[exited]).is_none(),
            "an exited PTY must never become a successfully attached Terminal"
        );
        assert!(flow.is_waiting(), "keep the existing confirmation request");
        assert!(
            flow.start_request().is_none(),
            "do not spawn a second PTY while the first Host result is unresolved"
        );
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
    fn recovered_create_outcome_never_reissues_a_host_create() {
        let mut flow = TerminalCreateFlow::default();
        assert!(flow.restore_outcome_unknown());
        assert!(flow.is_outcome_unknown());
        assert!(flow.start_request().is_none());
        assert_eq!(flow.tick(), TerminalCreateTick::None);
        assert!(!flow.retry_confirmation());
        assert!(flow.confirm_sessions(&[session("term-1")]).is_none());
        assert!(!flow.restore_outcome_unknown());
        assert!(flow.cancel());
        assert!(flow.start_request().is_some());
    }

    #[test]
    fn ambiguous_terminal_creation_requires_manual_reconciliation_before_another_create() {
        let mut flow = TerminalCreateFlow::default();
        let request = flow.start_request().unwrap();
        assert!(flow.mark_request_outcome_unknown(request));
        assert!(flow.is_outcome_unknown());
        assert!(flow.is_active());
        assert!(
            flow.start_request().is_none(),
            "no blind duplicate Host create RPC"
        );
        assert!(!flow.mark_request_outcome_unknown(request));
        assert!(
            !flow.retry_confirmation(),
            "no unverified Host IDs to retry"
        );
        assert!(flow.confirm_sessions(&[session("term-1")]).is_none());
        assert!(
            flow.cancel(),
            "user deliberately dismisses after inspecting Host"
        );
        let next = flow.start_request().unwrap();
        assert_ne!(next, request);
        assert!(!flow.mark_request_outcome_unknown(request));
        assert!(flow.is_creating());
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
    fn transient_host_inventory_failure_preserves_preview_until_authenticated_recovery() {
        let original = session("pty-recover");
        let mut dock = TerminalDock::for_workspace(7);
        dock.update_sessions(vec![original.clone()]);
        assert!(dock.open("pty-recover"));
        dock.tabs[0].screen.feed("before disconnect");
        let previous_screen = dock.tabs[0].screen.contents();
        let target = dock.context_target_at(0).unwrap();
        assert!(dock.context_target_is_live(&target));

        // A failed CLI list is NOT proof that Host terminated the PTY.
        dock.note_session_list_failure();
        assert_eq!(dock.sessions.len(), 1);
        assert!(dock.tabs[0].running);
        assert_eq!(dock.tabs[0].screen.contents(), previous_screen);
        assert!(!dock.context_target_is_live(&target)); // no stale writes

        assert!(dock.apply_session_list(7, vec![original]));
        assert!(dock.context_target_is_live(&target));
        assert_eq!(dock.tabs[0].screen.contents(), previous_screen);

        // A successful empty Host inventory is definitive; do not resurrect.
        assert!(dock.apply_session_list(7, vec![]));
        assert!(!dock.tabs[0].running);
        assert!(!dock.context_target_is_live(&target));
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
