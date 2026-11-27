//! Global, read-only projection of Host-confirmed Agent sessions.
//!
//! This view is intentionally not tied to the currently selected workspace.
//! It neither starts a Host nor attaches to idle Terminal output. Navigation
//! reuses the existing verified Agent notice / draft-safe workspace switch.
use super::*;

#[derive(Clone, Debug)]
pub(super) struct GlobalHistoryPreview {
    title: String,
    goal: String,
    progress: String,
    match_field: String,
    provider: String,
    account: String,
    project: String,
    workspace: String,
    native_id: String,
}

// Keep readable multi-line previews bounded without truncating to one line.
fn dashboard_card_preview(text: &str, cap: usize, lines: usize) -> String {
    let mut preview = String::new();
    let mut line_count = 1;
    for (index, ch) in text.chars().enumerate() {
        if index >= cap || (ch == '\n' && line_count >= lines) {
            preview.push('…');
            break;
        }
        if ch == '\n' {
            line_count += 1;
        }
        preview.push(ch);
    }
    preview
}

#[cfg(test)]
mod dashboard_card_preview_tests {
    use super::dashboard_card_preview;

    #[test]
    fn keeps_multiline_text_and_unicode_within_preview_bounds() {
        assert_eq!(dashboard_card_preview("目標\n進度", 20, 6), "目標\n進度");
        assert_eq!(dashboard_card_preview("目標\n進度", 20, 1), "目標…");
        assert_eq!(dashboard_card_preview("進度更新", 3, 6), "進度更…");
        assert_eq!(dashboard_card_preview("進度更新", 4, 6), "進度更新");
    }
}

fn dashboard_completion(session: &TerminalSession) -> Option<&str> {
    if alera_gpui_desktop::agent_status_visual::dashboard_status_group(
        session.agent_state,
        session.interrupted,
    )
    .0 != 4
    {
        return None;
    }
    session
        .agent_state_started_at
        .as_deref()
        .or(session.agent_updated_at.as_deref())
}

fn dashboard_identity(session: &TerminalSession) -> (String, String, String) {
    (
        session.handle.clone(),
        session.workspace_id.clone(),
        session.tab_id.clone(),
    )
}

pub(super) fn dashboard_done_unread(
    session: &TerminalSession,
    receipts: &HashMap<(String, String, String), String>,
) -> bool {
    dashboard_completion(session).is_some_and(|generation| {
        receipts
            .get(&dashboard_identity(session))
            .map(String::as_str)
            != Some(generation)
    })
}

fn sort_dashboard_done_peers(
    sessions: &mut [TerminalSession],
    receipts: &HashMap<(String, String, String), String>,
) {
    sessions.sort_by(|a, b| {
        let group = |session: &TerminalSession| {
            alera_gpui_desktop::agent_status_visual::dashboard_status_group(
                session.agent_state,
                session.interrupted,
            )
            .0
        };
        group(a).cmp(&group(b)).then_with(|| {
            if group(a) != 4 {
                return std::cmp::Ordering::Equal;
            }
            let time = |session: &TerminalSession| {
                dashboard_completion(session)
                    .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            };
            dashboard_done_unread(b, receipts)
                .cmp(&dashboard_done_unread(a, receipts))
                .then_with(|| time(b).cmp(&time(a)))
                .then_with(|| a.tab_id.cmp(&b.tab_id))
        })
    });
}

fn history_preview_text(value: Option<&serde_json::Value>, cap: usize) -> String {
    value
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .chars()
        .filter(|ch| !ch.is_control())
        .take(cap)
        .collect::<String>()
}

/// A lost Host ACK must reuse the original mutation UUID after a deliberate
/// second confirmation, never create a different request for the same source.
fn reserve_history_resume_mutation(
    pending: &mut std::collections::HashMap<String, String>,
    identity: &str,
    create_id: impl FnOnce() -> String,
) -> String {
    pending
        .entry(identity.to_owned())
        .or_insert_with(create_id)
        .clone()
}

/// Collision-free source identity for both confirmation and retry state.
/// Never combine untrusted account, project or profile labels with '|'.
fn history_resume_identity(hit: &GlobalHistoryPreview, profile: &str) -> String {
    serde_json::to_string(&[
        hit.provider.as_str(),
        hit.account.as_str(),
        hit.project.as_str(),
        hit.workspace.as_str(),
        hit.native_id.as_str(),
        profile,
    ])
    .expect("history resume identity is a JSON string array")
}

/// Account aliases may include separators: identify the full native tuple
/// without allowing adjacent fields to collide.
fn history_source_identity(hit: &GlobalHistoryPreview) -> String {
    serde_json::to_string(&[
        hit.provider.as_str(),
        hit.account.as_str(),
        hit.project.as_str(),
        hit.workspace.as_str(),
        hit.native_id.as_str(),
    ])
    .expect("history source identity is a JSON array")
}

/// Parse only a Host-confirmed read-only lookup tied to the requested
/// Worktree. This never authorizes a new PTY or chooses an unrelated Tab.
#[derive(Debug, PartialEq, Eq)]
enum LocatedHistoryTab {
    Found { workspace: String, tab_id: String },
    Missing,
}

fn parse_located_history_tab(
    response: &serde_json::Value,
    expected_workspace: &str,
) -> Result<LocatedHistoryTab, &'static str> {
    if response
        .get("noAgentStarted")
        .and_then(serde_json::Value::as_bool)
        != Some(true)
    {
        return Err("History lookup did not prove that no Agent was started");
    }
    match (
        response.get("found").and_then(serde_json::Value::as_bool),
        response.get("tabId").and_then(serde_json::Value::as_str),
        response
            .get("workspaceId")
            .and_then(serde_json::Value::as_str),
    ) {
        (Some(true), Some(tab_id), Some(workspace))
            if !tab_id.is_empty() && tab_id.len() <= 256 && workspace == expected_workspace =>
        {
            Ok(LocatedHistoryTab::Found {
                workspace: workspace.to_owned(),
                tab_id: tab_id.to_owned(),
            })
        }
        (Some(false), None, None) => Ok(LocatedHistoryTab::Missing),
        _ => Err("Host history lookup returned an inconsistent Tab identity"),
    }
}

/// A Host Tab UUID alone is not a terminal attachment. The last
/// authenticated inventory must still contain an explicitly running Agent
/// with the same Worktree, Tab ID and Provider before we schedule navigation.
fn verified_running_history_session<'a>(
    sessions: &'a [TerminalSession],
    workspace: &str,
    tab_id: &str,
    provider: &str,
) -> Option<&'a TerminalSession> {
    sessions.iter().find(|session| {
        session.running
            && session.workspace_id == workspace
            && session.tab_id == tab_id
            && session.agent_type.as_deref() == Some(provider)
    })
}

#[cfg(test)]
mod exact_history_resume_retry_tests {
    use super::*;

    fn sample() -> GlobalHistoryPreview {
        GlobalHistoryPreview {
            title: "Title".into(),
            goal: "Goal".into(),
            progress: "Progress".into(),
            match_field: "goal".into(),
            provider: "codex".into(),
            account: "secondary".into(),
            project: "project".into(),
            workspace: "workspace".into(),
            native_id: "550e8400-e29b-41d4-a716-446655440001".into(),
        }
    }

    #[test]
    fn search_previews_include_progress_and_exact_matched_field() {
        let response = serde_json::json!({
            "hits": [{
                "session": {
                    "title": "Fix Flutter/GPUI parity",
                    "goal": "Move Agent cards to state columns",
                    "progress": "Verified Host session pin ordering",
                    "provider": "claude",
                    "providerAccountId": "account-a",
                    "projectId": "alera",
                    "workspaceId": "wt-one",
                    "providerSessionId": "550e8400-e29b-41d4-a716-446655440001"
                },
                "openAction": "preview_only",
                "matchField": "progress",
                "score": 2
            }],
            "scanTruncated": false
        });
        let (hits, truncated) = parse_history_previews(&response).unwrap();
        assert!(!truncated);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].match_field, "progress");
        assert_eq!(hits[0].progress, "Verified Host session pin ordering");
        assert_eq!(hits[0].workspace, "wt-one");

        let mut invalid = response;
        invalid["hits"][0]["openAction"] = serde_json::json!("resume_exact");
        assert!(parse_history_previews(&invalid).is_err());
    }

    #[test]
    fn located_tab_identity_is_exact_and_profile_independent() {
        let mut first = sample();
        first.account = "one|two".into();
        first.project = "three".into();
        let mut second = sample();
        second.account = "one".into();
        second.project = "two|three".into();
        assert_ne!(
            history_source_identity(&first),
            history_source_identity(&second)
        );
        assert_eq!(
            history_source_identity(&first),
            history_source_identity(&first.clone())
        );
    }

    #[test]
    fn verified_history_navigation_rejects_recycled_or_stopped_host_tabs() {
        let live = TerminalSession {
            handle: "host-session".into(),
            workspace_id: "original-worktree".into(),
            tab_id: "verified-tab".into(),
            running: true,
            agent_type: Some("codex".into()),
            agent_state: None,
            interrupted: false,
            agent_state_started_at: None,
            agent_updated_at: None,
            agent_context: None,
            builtin_mcp_status: None,
        };
        assert!(verified_running_history_session(
            &[live.clone()],
            "original-worktree",
            "verified-tab",
            "codex"
        )
        .is_some());
        for wrong in [
            TerminalSession {
                tab_id: "recycled-tab".into(),
                ..live.clone()
            },
            TerminalSession {
                workspace_id: "other-worktree".into(),
                ..live.clone()
            },
            TerminalSession {
                agent_type: Some("claude".into()),
                ..live.clone()
            },
            TerminalSession {
                running: false,
                ..live.clone()
            },
        ] {
            assert!(
                verified_running_history_session(
                    &[wrong],
                    "original-worktree",
                    "verified-tab",
                    "codex"
                )
                .is_none(),
                "never navigate from a recycled handle or stopped/mismatched Agent"
            );
        }
    }

    #[test]
    fn located_tab_requires_exact_no_start_host_identity() {
        let good = serde_json::json!({
            "found": true,
            "tabId": "saved-tab",
            "workspaceId": "workspace",
            "noAgentStarted": true,
        });
        assert_eq!(
            parse_located_history_tab(&good, "workspace"),
            Ok(LocatedHistoryTab::Found {
                workspace: "workspace".into(),
                tab_id: "saved-tab".into(),
            })
        );
        assert!(parse_located_history_tab(&good, "other-worktree").is_err());
        for bad in [
            serde_json::json!({"found":true,"tabId":"saved-tab","workspaceId":"workspace"}),
            serde_json::json!({"found":true,"tabId":"saved-tab","workspaceId":"workspace","noAgentStarted":false}),
            serde_json::json!({"found":true,"tabId":"","workspaceId":"workspace","noAgentStarted":true}),
            serde_json::json!({"found":false,"tabId":"forged-tab","noAgentStarted":true}),
        ] {
            assert!(parse_located_history_tab(&bad, "workspace").is_err());
        }
        assert_eq!(
            parse_located_history_tab(
                &serde_json::json!({"found": false, "noAgentStarted": true}),
                "workspace",
            ),
            Ok(LocatedHistoryTab::Missing)
        );
    }

    #[test]
    fn ambiguous_ack_reuses_original_id_until_confirmed() {
        let hit = sample();
        let identity = history_resume_identity(&hit, "official-codex");
        let mut pending = std::collections::HashMap::new();
        assert_eq!(
            reserve_history_resume_mutation(&mut pending, &identity, || "mutation-1".into()),
            "mutation-1"
        );
        assert_eq!(
            reserve_history_resume_mutation(&mut pending, &identity, || panic!(
                "must not generate a new UUID after a lost acknowledgement"
            )),
            "mutation-1"
        );
        assert_eq!(pending.remove(&identity).as_deref(), Some("mutation-1"));
        assert_eq!(
            reserve_history_resume_mutation(&mut pending, &identity, || "mutation-2".into()),
            "mutation-2"
        );
    }

    #[test]
    fn source_and_profile_scopes_cannot_alias_even_with_delimiters() {
        let mut a = sample();
        a.account = "a|b".into();
        a.project = "c".into();
        let mut b = sample();
        b.account = "a".into();
        b.project = "b|c".into();
        let first = history_resume_identity(&a, "official");
        let second = history_resume_identity(&b, "official");
        assert_ne!(first, second);
        assert_ne!(first, history_resume_identity(&a, "other-profile"));

        let mut pending = std::collections::HashMap::new();
        reserve_history_resume_mutation(&mut pending, &first, || "mutation-1".into());
        reserve_history_resume_mutation(&mut pending, &second, || "mutation-2".into());
        assert_eq!(pending.len(), 2);
    }
}

fn parse_history_previews(
    value: &serde_json::Value,
) -> Result<(Vec<GlobalHistoryPreview>, bool), &'static str> {
    let raw = value
        .get("hits")
        .and_then(serde_json::Value::as_array)
        .ok_or("Missing Host history results")?;
    if raw.len() > 100 {
        return Err("Host history result budget exceeded");
    }
    let mut hits = Vec::with_capacity(raw.len().min(30));
    for item in raw.iter().take(30) {
        let session = item
            .get("session")
            .and_then(serde_json::Value::as_object)
            .ok_or("Malformed Host history result")?;
        let provider = history_preview_text(session.get("provider"), 32);
        let native_id = history_preview_text(session.get("providerSessionId"), 64);
        if !matches!(provider.as_str(), "claude" | "codex")
            || item.get("openAction").and_then(serde_json::Value::as_str) != Some("preview_only")
            || native_id.is_empty()
        {
            return Err("History result did not pass preview-only validation");
        }
        hits.push(GlobalHistoryPreview {
            title: history_preview_text(session.get("title"), 160),
            goal: history_preview_text(session.get("goal"), 240),
            progress: history_preview_text(session.get("progress"), 240),
            match_field: history_preview_text(item.get("matchField"), 32),
            provider,
            // Codex account IDs can be absolute CODEX_HOME paths. Do not
            // truncate Host identity fields before sending exact Resume.
            account: history_preview_text(session.get("providerAccountId"), 256),
            project: history_preview_text(session.get("projectId"), 256),
            workspace: history_preview_text(session.get("workspaceId"), 256),
            native_id,
        });
    }
    Ok((
        hits,
        value
            .get("scanTruncated")
            .and_then(serde_json::Value::as_bool)
            == Some(true),
    ))
}

#[cfg(test)]
mod gpui_dashboard_parity_tests {
    use super::*;

    #[test]
    fn dashboard_uses_same_detected_agent_choices_as_new_tab_without_profiles() {
        let installed = [
            InstalledAgentCli {
                id: "codex".into(),
                display_name: "Codex".into(),
                executable_path: PathBuf::from("codex.exe"),
            },
            InstalledAgentCli {
                id: "agy".into(),
                display_name: "Antigravity".into(),
                executable_path: PathBuf::from("agy.exe"),
            },
            InstalledAgentCli {
                id: "pi".into(),
                display_name: "Pi".into(),
                executable_path: PathBuf::from("pi.cmd"),
            },
        ];
        let expected: Vec<_> = installed
            .iter()
            .map(|agent| (agent.id.clone(), agent.new_tab_menu_label().to_owned()))
            .collect();
        assert_eq!(dashboard_installed_choices(&installed), expected);
        assert!(dashboard_installed_choices(&[]).is_empty());
    }

    #[test]
    fn dashboard_direct_launch_verifies_same_host_identity_as_new_tab() {
        let receipt = GlobalAgentLaunchReceipt::Installed(
            alera_gpui_desktop::terminal::CreatedHostTerminal {
                handle: "session".into(),
                workspace_id: "workspace".into(),
                tab_id: "tab".into(),
            },
        );
        let mut session = running("session", "workspace", "tab", "2026-11-27T12:00:00Z");
        session.agent_type = None;
        assert!(receipt.matches(&[session.clone()], "pi"));
        session.agent_type = Some("pi".into());
        assert!(receipt.matches(&[session.clone()], "pi"));
        assert!(!receipt.matches(&[session.clone()], "codex"));
        session.workspace_id = "other".into();
        assert!(!receipt.matches(&[session.clone()], "pi"));
        session.workspace_id = "workspace".into();
        session.running = false;
        assert!(!receipt.matches(&[session], "pi"));
    }

    #[test]
    fn done_unread_precedes_read_by_completion_not_updated_at() {
        use alera_gpui_desktop::terminal::AgentActivityState;
        let mut old = running("old", "w", "old", "2026-11-27T12:50:00Z");
        old.agent_state = Some(AgentActivityState::Done);
        old.agent_state_started_at = Some("2026-11-27T12:01:00Z".into());
        let mut new = running("new", "w", "new", "2026-11-27T12:03:00Z");
        new.agent_state = Some(AgentActivityState::Done);
        new.agent_state_started_at = Some("2026-11-27T12:03:00Z".into());
        let mut read = running("read", "w", "read", "2026-11-27T12:04:00Z");
        read.agent_state = Some(AgentActivityState::Done);
        read.agent_state_started_at = Some("2026-11-27T12:04:00Z".into());
        let receipts = HashMap::from([(dashboard_identity(&read), "2026-11-27T12:04:00Z".into())]);
        let mut sessions = vec![read.clone(), old, new];
        sort_dashboard_done_peers(&mut sessions, &receipts);
        assert_eq!(
            sessions
                .iter()
                .map(|s| s.handle.as_str())
                .collect::<Vec<_>>(),
            vec!["new", "old", "read"]
        );
        read.agent_updated_at = Some("2026-11-27T12:55:00Z".into());
        assert!(!dashboard_done_unread(&read, &receipts));
        read.agent_state_started_at = Some("2026-11-27T12:56:00Z".into());
        assert!(dashboard_done_unread(&read, &receipts));
    }

    fn running(handle: &str, workspace: &str, tab: &str, updated: &str) -> TerminalSession {
        TerminalSession {
            handle: handle.to_owned(),
            workspace_id: workspace.to_owned(),
            tab_id: tab.to_owned(),
            running: true,
            agent_type: Some("claude".into()),
            agent_state: None,
            interrupted: false,
            agent_state_started_at: None,
            agent_updated_at: Some(updated.into()),
            agent_context: None,
            builtin_mcp_status: None,
        }
    }

    #[test]
    fn persisted_pin_sorts_first_within_host_state_instead_of_by_age() {
        let mut sessions = vec![
            running("new", "workspace-a", "tab-new", "2026-11-23T06:00:00Z"),
            running("old", "workspace-b", "tab-pinned", "2026-11-22T06:00:00Z"),
            running("other", "workspace-a", "tab-other", "2026-11-21T06:00:00Z"),
        ];
        let pins = HashMap::from([(
            ("workspace-b".into(), "tab-pinned".into()),
            HostTabPinSnapshot {
                workspace_id: "workspace-b".into(),
                id: "tab-pinned".into(),
                updated_at: "2026-11-22T06:00:00Z".into(),
                pinned: true,
                terminal_session_id: Some("old".into()),
            },
        )]);
        sort_dashboard_sessions(&mut sessions, &pins);
        assert_eq!(
            sessions
                .iter()
                .map(|row| row.tab_id.as_str())
                .collect::<Vec<_>>(),
            ["tab-pinned", "tab-new", "tab-other"],
        );
    }

    #[test]
    fn recycled_handle_and_foreign_workspace_cannot_inherit_a_pin() {
        let session = running("current", "workspace-a", "tab-1", "2026-11-23T06:00:00Z");
        let mut pins = HashMap::from([(
            ("workspace-a".into(), "tab-1".into()),
            HostTabPinSnapshot {
                workspace_id: "workspace-a".into(),
                id: "tab-1".into(),
                updated_at: "2026-11-22T06:00:00Z".into(),
                pinned: true,
                terminal_session_id: Some("recycled".into()),
            },
        )]);
        assert!(!dashboard_session_is_pinned(&session, &pins));
        pins.insert(
            ("workspace-b".into(), "tab-1".into()),
            HostTabPinSnapshot {
                workspace_id: "workspace-b".into(),
                id: "tab-1".into(),
                updated_at: "2026-11-22T06:00:00Z".into(),
                pinned: true,
                terminal_session_id: Some("current".into()),
            },
        );
        assert!(!dashboard_session_is_pinned(&session, &pins));
        assert!(!GlobalAgentPromptTarget {
            handle: "current".into(),
            workspace_id: "workspace-b".into(),
            tab_id: "tab-1".into(),
            agent_type: "claude".into(),
        }
        .is_live(&[session]));
    }
}

/// A pinned status-card must represent the same Host session, not a
/// recycled Tab ID or an earlier Workspace owner.
fn dashboard_session_is_pinned(
    session: &TerminalSession,
    pins: &HashMap<(String, String), HostTabPinSnapshot>,
) -> bool {
    pins.get(&(session.workspace_id.clone(), session.tab_id.clone()))
        .filter(|pin| pin.matches_session(session))
        .is_some_and(|pin| pin.pinned)
}

/// Flutter uses this exact ordering: state columns first, then persisted
/// Agent pins, then newest Host update within each column.
fn sort_dashboard_sessions(
    sessions: &mut [TerminalSession],
    pins: &HashMap<(String, String), HostTabPinSnapshot>,
) {
    sessions.sort_by(|a, b| {
        let a_group = alera_gpui_desktop::agent_status_visual::dashboard_status_group(
            a.agent_state,
            a.interrupted,
        )
        .0;
        let b_group = alera_gpui_desktop::agent_status_visual::dashboard_status_group(
            b.agent_state,
            b.interrupted,
        )
        .0;
        a_group
            .cmp(&b_group)
            .then_with(|| {
                dashboard_session_is_pinned(b, pins).cmp(&dashboard_session_is_pinned(a, pins))
            })
            .then_with(|| b.agent_updated_at.cmp(&a.agent_updated_at))
            .then_with(|| a.tab_id.cmp(&b.tab_id))
    });
}

impl Workbench {
    /// Keep the sidebar, board and visible Terminal on the same receipt
    /// stream. Never acknowledge a background Tab behind the maximized board.
    pub(super) fn sync_visible_agent_completion(
        &mut self,
        visible_terminal: Option<&(String, String, String)>,
    ) {
        if self.terminals.session_list_is_stale() {
            return;
        }
        let live: HashSet<_> = self
            .terminals
            .sessions
            .iter()
            .filter(|session| session.running)
            .map(dashboard_identity)
            .collect();
        self.agent_completion_acknowledgements
            .retain(|identity, _| live.contains(identity));
        let inspected = self.global_agent_inspector.as_ref().map(|state| {
            (
                state.target.handle.clone(),
                state.target.workspace_id.clone(),
                state.target.tab_id.clone(),
            )
        });
        let Some(identity) = inspected.as_ref().or(visible_terminal) else {
            return;
        };
        if let Some(session) = self.terminals.sessions.iter().find(|session| {
            session.running
                && session.agent_type.is_some()
                && &dashboard_identity(session) == identity
        }) {
            if let Some(epoch) = dashboard_completion(session) {
                let receipt = self
                    .agent_completion_acknowledgements
                    .entry(identity.clone());
                use std::collections::hash_map::Entry;
                match receipt {
                    Entry::Occupied(mut entry) if entry.get() != epoch => {
                        entry.insert(epoch.to_owned());
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(epoch.to_owned());
                    }
                    _ => {}
                }
            }
        }
    }

    /// Persist dashboard pins through the existing Host-owned Tab CAS API.
    /// Never mutate local-only pin metadata or a different Agent/Worktree.
    fn toggle_dashboard_agent_pin(
        &mut self,
        target: GlobalAgentPromptTarget,
        cx: &mut Context<Self>,
    ) {
        let pin_identity = (
            target.handle.clone(),
            target.workspace_id.clone(),
            target.tab_id.clone(),
        );
        if self.global_agent_pin_pending.as_ref() == Some(&pin_identity) {
            return;
        }
        if self.host_pins_busy || self.host_pins_loading {
            self.global_agent_pin_feedback =
                Some("Host pin synchronization is in progress; retry shortly".into());
            cx.notify();
            return;
        }
        if self.terminals.session_list_is_stale() || !target.is_live(&self.terminals.sessions) {
            self.global_agent_pin_feedback = Some(
                "Agent identity may have changed; refreshing Host inventory before pinning".into(),
            );
            self.refresh_terminals(cx);
            self.refresh_host_tab_pins(cx);
            cx.notify();
            return;
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.global_agent_pin_feedback =
                Some("Host CLI unavailable; Agent pin has not changed".into());
            cx.notify();
            return;
        };
        let observed = self
            .host_pin_snapshots
            .get(&(target.workspace_id.clone(), target.tab_id.clone()))
            .cloned()
            .filter(|pin| {
                pin.workspace_id == target.workspace_id
                    && pin.id == target.tab_id
                    && pin.terminal_session_id.as_deref() == Some(&target.handle)
            });
        let generation = self.workspace_generation;
        self.global_agent_pin_pending = Some(pin_identity);
        self.global_agent_pin_feedback = Some("Saving Agent pin with the verified Host…".into());
        self.host_pins_busy = true;
        self.host_pins_request = self.host_pins_request.wrapping_add(1);
        let ticket = self.host_pins_request;
        let source = target.clone();
        cx.spawn(async move |view, cx| {
            let response = cx
                .background_spawn(async move {
                    let registered = bridge.list_registered_workspaces()?;
                    if !registered.iter().any(|ws| ws.id == source.workspace_id) {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            "Agent Worktree no longer registered",
                        ));
                    }
                    let existing = bridge
                        .host_tab_pin_list(&source.workspace_id)?
                        .into_iter()
                        .find(|pin| {
                            pin.workspace_id == source.workspace_id
                                && pin.id == source.tab_id
                                && pin.terminal_session_id.as_deref()
                                    == Some(source.handle.as_str())
                        })
                        .ok_or_else(|| {
                            std::io::Error::new(
                                std::io::ErrorKind::NotFound,
                                "Original Agent Tab identity changed",
                            )
                        })?;
                    if observed
                        .as_ref()
                        .is_some_and(|pin| pin.pinned != existing.pinned)
                    {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::AlreadyExists,
                            "Tab pin changed in another Alera window",
                        ));
                    }
                    let next = !existing.pinned;
                    bridge
                        .host_tab_pin_compare_and_set(&existing, next)
                        .map(|pin| (pin, next))
                })
                .await;
            view.update(cx, |view, cx| {
                if view.host_pins_request != ticket {
                    return;
                }
                view.host_pins_busy = false;
                view.global_agent_pin_pending = None;
                if view.workspace_generation != generation {
                    view.global_agent_pin_feedback = Some(
                        "Workspace changed during Host pin update; reloading verified state".into(),
                    );
                    view.refresh_host_tab_pins(cx);
                    cx.notify();
                    return;
                }
                match response {
                    Ok((pin, pinned))
                        if target.is_live(&view.terminals.sessions)
                            && pin.workspace_id == target.workspace_id
                            && pin.id == target.tab_id
                            && pin.terminal_session_id.as_deref()
                                == Some(target.handle.as_str()) =>
                    {
                        view.host_pin_snapshots
                            .insert((pin.workspace_id.clone(), pin.id.clone()), pin);
                        let ids: Vec<_> = view
                            .mixed_tabs
                            .entries()
                            .iter()
                            .filter_map(|entry| match &entry.identity {
                                WorkbenchTabIdentity::Host(owner)
                                    if owner.handle == target.handle
                                        && owner.workspace_id() == target.workspace_id
                                        && owner.tab_id() == target.tab_id =>
                                {
                                    Some(entry.identity.clone())
                                }
                                _ => None,
                            })
                            .collect();
                        for id in ids {
                            view.mixed_tabs.set_pinned(&id, pinned);
                        }
                        view.terminals.status = if pinned {
                            "Agent pinned in Dashboard and Host Tab".into()
                        } else {
                            "Agent unpinned in Dashboard and Host Tab".into()
                        };
                        view.global_agent_pin_feedback = Some(view.terminals.status.clone());
                    }
                    Ok(_) => {
                        view.terminals.status =
                            "Host saved the pin; Agent identity changed. Refresh".into();
                        view.global_agent_pin_feedback = Some(view.terminals.status.clone());
                        view.refresh_host_tab_pins(cx);
                    }
                    Err(error) => {
                        let detail: String = error
                            .to_string()
                            .chars()
                            .filter(|ch| !ch.is_control())
                            .take(160)
                            .collect();
                        view.terminals.status = format!("Agent pin rejected: {detail}");
                        view.global_agent_pin_feedback = Some(view.terminals.status.clone());
                        view.refresh_host_tab_pins(cx);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

impl Workbench {
    fn search_global_agent_history(&mut self, cx: &mut Context<Self>) {
        if self.global_agent_history_busy || !self.global_agent_history_consent {
            self.global_agent_history_status =
                "Enable explicit local indexing before searching".into();
            cx.notify();
            return;
        }
        let input = self.global_agent_history_input.read(cx);
        if input.composing() {
            self.global_agent_history_status = "Finish IME composition before searching".into();
            cx.notify();
            return;
        }
        let text = input.query().trim().to_owned();
        if text.is_empty() || text.chars().count() > 256 || text.len() > 1024 || text.contains('\0')
        {
            self.global_agent_history_status = "Enter 1–256 UTF-8 characters".into();
            cx.notify();
            return;
        }
        let account = self
            .global_agent_history_account_input
            .read(cx)
            .query()
            .trim()
            .to_owned();
        let project = self
            .global_agent_history_project_input
            .read(cx)
            .query()
            .trim()
            .to_owned();
        let workspace = self
            .global_agent_history_workspace_input
            .read(cx)
            .query()
            .trim()
            .to_owned();
        if account.len() > 256 || project.len() > 256 || workspace.len() > 256 {
            self.global_agent_history_status = "Account/Worktree filter too long".into();
            cx.notify();
            return;
        }
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.global_agent_history_status = "Alera CLI not found; no Host started".into();
            cx.notify();
            return;
        };
        let provider = self.global_agent_history_provider.clone();
        self.global_agent_history_busy = true;
        self.global_agent_history_resume_armed = None;
        self.global_agent_history_request = self.global_agent_history_request.wrapping_add(1);
        let ticket = self.global_agent_history_request;
        self.global_agent_history_results.clear();
        self.global_agent_history_located.clear();
        self.global_agent_history_status = "Scanning registered Worktrees; preview only…".into();
        cx.notify();
        cx.spawn(async move |view, cx| {
            let reply = cx
                .background_spawn(async move {
                    bridge.search_agent_history(
                        &text,
                        provider.as_deref(),
                        (!account.is_empty()).then_some(account.as_str()),
                        (!project.is_empty()).then_some(project.as_str()),
                        (!workspace.is_empty()).then_some(workspace.as_str()),
                    )
                })
                .await;
            view.update(cx, |view, cx| {
                if view.global_agent_history_request != ticket {
                    return;
                }
                view.global_agent_history_busy = false;
                match reply {
                    Ok(value) => match parse_history_previews(&value) {
                        Ok((hits, truncated)) => {
                            view.global_agent_history_status = format!(
                                "{} preview-only matches{}",
                                hits.len(),
                                if truncated {
                                    " · scan capped, may be incomplete"
                                } else {
                                    ""
                                }
                            );
                            view.global_agent_history_results = hits;
                        }
                        Err(message) => {
                            view.global_agent_history_status = message.into();
                        }
                    },
                    Err(_) => {
                        view.global_agent_history_status =
                            "Host history search unavailable; no Agent started".into();
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn clear_global_agent_history(&mut self, cx: &mut Context<Self>) {
        if self.global_agent_history_busy {
            return;
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.global_agent_history_status = "Connect an existing Host to clear the cache".into();
            cx.notify();
            return;
        };
        self.global_agent_history_busy = true;
        self.global_agent_history_request = self.global_agent_history_request.wrapping_add(1);
        let ticket = self.global_agent_history_request;
        self.global_agent_history_status = "Clearing only the private search index…".into();
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move { bridge.clear_agent_history_search_cache() })
                .await;
            view.update(cx, |view, cx| {
                if view.global_agent_history_request != ticket {
                    return;
                }
                view.global_agent_history_busy = false;
                if result.is_ok() {
                    view.global_agent_history_results.clear();
                    view.global_agent_history_located.clear();
                    view.global_agent_history_consent = false;
                    view.global_agent_history_resume_armed = None;
                    view.global_agent_history_status =
                        "Search cache cleared. Provider transcripts preserved".into();
                } else {
                    view.global_agent_history_status =
                        "Cache removal unconfirmed; inspect Host before retrying".into();
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

impl Workbench {
    /// Deliberately locate an existing Host-pinned Tab without creating a
    /// terminal or trusting stale UI/session inventory after app restart.
    fn locate_global_agent_history(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.global_agent_history_busy || !self.global_agent_history_consent {
            return;
        }
        let Some(hit) = self.global_agent_history_results.get(index).cloned() else {
            return;
        };
        if hit.account.is_empty() {
            self.global_agent_history_status =
                "Cannot locate a Tab without the original registered account".into();
            cx.notify();
            return;
        }
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.global_agent_history_status = "Alera CLI unavailable; no Host started".into();
            cx.notify();
            return;
        };
        self.global_agent_history_busy = true;
        self.global_agent_history_resume_armed = None;
        self.global_agent_history_request = self.global_agent_history_request.wrapping_add(1);
        let ticket = self.global_agent_history_request;
        self.global_agent_history_status =
            "Locating an existing verified Tab; no Agent will start…".into();
        cx.notify();
        let expected_workspace = hit.workspace.clone();
        let source_key = history_source_identity(&hit);
        cx.spawn(async move |view, cx| {
            let reply = cx
                .background_spawn(async move {
                    bridge.locate_exact_agent_history(
                        &hit.provider,
                        &hit.account,
                        &hit.project,
                        &hit.workspace,
                        &hit.native_id,
                    )
                })
                .await;
            view.update(cx, |view, cx| {
                if view.global_agent_history_request != ticket {
                    return;
                }
                view.global_agent_history_busy = false;
                view.global_agent_history_located.remove(&source_key);
                view.global_agent_history_status = match reply
                    .as_ref()
                    .map_err(|_| "Host unavailable")
                    .and_then(|value| parse_located_history_tab(value, &expected_workspace))
                {
                    Ok(LocatedHistoryTab::Found { workspace, tab_id }) => {
                        view.global_agent_history_located
                            .insert(source_key, (workspace, tab_id.clone()));
                        format!("Existing Tab {tab_id} verified. Choose Open verified running Tab to navigate; no Agent started")
                    }
                    Ok(LocatedHistoryTab::Missing) =>
                        "No verified existing Tab. Use confirmed Resume only if you intend to create one".into(),
                    Err(_) => "Existing Tab lookup unavailable or source changed; no Agent started".into(),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// A second explicit click must recheck the *same* Host-pinned source
    /// before any navigation. A stale history card and an old live inventory
    /// are never authority to focus a Tab, even if a handle was recycled.
    /// A persisted but stopped Tab never triggers an implicit Agent launch.
    fn open_located_global_agent_history(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.global_agent_history_busy || !self.global_agent_history_consent {
            return;
        }
        let Some(hit) = self.global_agent_history_results.get(index).cloned() else {
            return;
        };
        let source_key = history_source_identity(&hit);
        let Some((expected_workspace, expected_tab)) =
            self.global_agent_history_located.get(&source_key).cloned()
        else {
            return;
        };
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.global_agent_history_status =
                "Cannot recheck verified Tab without the existing Host CLI".into();
            cx.notify();
            return;
        };
        self.global_agent_history_busy = true;
        self.global_agent_history_request = self.global_agent_history_request.wrapping_add(1);
        let ticket = self.global_agent_history_request;
        let original_generation = self.workspace_generation;
        let navigation_epoch = self.agent_navigation.epoch();
        self.global_agent_history_status =
            "Rechecking the original Host account and Tab before navigation…".into();
        cx.notify();
        cx.spawn(async move |view, cx| {
            let live_provider = hit.provider.clone();
            let response = cx
                .background_spawn(async move {
                    bridge.locate_exact_agent_history(
                        &hit.provider,
                        &hit.account,
                        &hit.project,
                        &hit.workspace,
                        &hit.native_id,
                    )
                })
                .await;
            view.update(cx, |view, cx| {
                if view.global_agent_history_request != ticket {
                    return;
                }
                view.global_agent_history_busy = false;
                if view.agent_navigation.epoch() != navigation_epoch {
                    view.global_agent_history_status =
                        "Another Tab or Workspace was selected during Host recheck; click Open again"
                            .into();
                    cx.notify();
                    return;
                }
                let verified = response
                    .as_ref()
                    .map_err(|_| "Host unavailable")
                    .and_then(|value| parse_located_history_tab(value, &expected_workspace));
                if !matches!(
                    verified,
                    Ok(LocatedHistoryTab::Found { ref workspace, ref tab_id })
                        if workspace == &expected_workspace && tab_id == &expected_tab
                ) {
                    view.global_agent_history_located.remove(&source_key);
                    view.global_agent_history_status =
                        "Original Host account or Tab changed. Find it again; no Agent started"
                            .into();
                    cx.notify();
                    return;
                }
                if view.workspace_generation != original_generation {
                    view.global_agent_history_status =
                        "Workspace changed during Host recheck; click Open again".into();
                    cx.notify();
                    return;
                }
                if view.terminals.session_list_is_stale() {
                    view.global_agent_history_status =
                        "Live Host inventory is stale. Refresh before opening".into();
                    view.refresh_terminals(cx);
                    cx.notify();
                    return;
                }
                let live = verified_running_history_session(
                    &view.terminals.sessions,
                    &expected_workspace,
                    &expected_tab,
                    &live_provider,
                );
                let Some(live) = live.cloned() else {
                    view.global_agent_history_status =
                        "Verified Tab is saved but not running. Open it explicitly from the original Worktree"
                            .into();
                    cx.notify();
                    return;
                };

                // Reuse the draft-safe Host session/pane navigation path.
                // The next render owns Window; the queue epoch protects an
                // explicit newer Tab or Workspace selection while it defers.
                view.agent_navigation.begin(
                    AgentNavigationTarget {
                        session_id: live.handle,
                        workspace_id: expected_workspace,
                        tab_id: expected_tab,
                        agent_type: live_provider,
                    },
                    AgentNavigationSource::History,
                );
                // An authenticated refresh rechecks the Host before focus.
                // Keep the search visible until focus really succeeds.
                view.refresh_terminals(cx);
                view.global_agent_history_status =
                    "Navigating to exact verified Tab; history stays open if focus fails".into();
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn resume_global_agent_history(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.global_agent_history_busy || !self.global_agent_history_consent {
            return;
        }
        let Some(hit) = self.global_agent_history_results.get(index).cloned() else {
            return;
        };
        if hit.account.is_empty() {
            self.global_agent_history_status =
                "Exact Resume requires a registered Claude/Codex source account".into();
            cx.notify();
            return;
        }
        let input = self.global_agent_history_resume_profile_input.read(cx);
        if input.composing() {
            self.global_agent_history_status =
                "Finish IME composition before selecting a Profile".into();
            cx.notify();
            return;
        }
        let profile = input.query().trim().to_owned();
        if profile.is_empty() || profile.len() > 256 || profile.chars().any(char::is_control) {
            self.global_agent_history_status =
                "Enter the matching official CLI Profile ID from Settings".into();
            cx.notify();
            return;
        }
        let identity = history_resume_identity(&hit, &profile);
        if self.global_agent_history_resume_armed.as_deref() != Some(identity.as_str()) {
            let retrying = self
                .global_agent_history_pending_mutations
                .contains_key(&identity);
            self.global_agent_history_resume_armed = Some(identity);
            self.global_agent_history_status = if retrying {
                "Previous Resume acknowledgement is unknown. Inspect Live Agents first; then confirm the SAME result to reuse its ORIGINAL request ID".into()
            } else {
                format!("Alera will select the configured {} account '{}', and revalidate its native history. Confirm exact Resume on the SAME result; the provider may reject incompatible credentials", hit.provider, hit.account)
            };
            cx.notify();
            return;
        }
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.global_agent_history_status = "Alera CLI unavailable; no Host started".into();
            cx.notify();
            return;
        };
        self.global_agent_history_resume_armed = None;
        self.global_agent_history_busy = true;
        self.global_agent_history_request = self.global_agent_history_request.wrapping_add(1);
        let ticket = self.global_agent_history_request;
        self.global_agent_history_status =
            "Host revalidating native UUID and original registered Worktree…".into();
        cx.notify();
        let mutation_id = reserve_history_resume_mutation(
            &mut self.global_agent_history_pending_mutations,
            &identity,
            || uuid::Uuid::new_v4().to_string(),
        );
        cx.spawn(async move |view, cx| {
            let reply = cx.background_spawn(async move {
                bridge.resume_exact_agent_history(
                    &hit.provider,
                    &hit.account,
                    &hit.project,
                    &hit.workspace,
                    &hit.native_id,
                    &profile,
                    &mutation_id,
                )
            }).await;
            view.update(cx, |view, cx| {
                if view.global_agent_history_request != ticket {
                    return;
                }
                view.global_agent_history_busy = false;
                view.global_agent_history_status = match reply {
                    Ok(value)
                        if value.get("tab")
                            .and_then(|tab| tab.get("id"))
                            .and_then(serde_json::Value::as_str)
                            .is_some() =>
                    {
                        view.global_agent_history_pending_mutations.remove(&identity);
                        "Exact Resume Tab confirmed by Host. Open it from Live Agents; no duplicate launch was made".into()
                    }
                    _ => {
                        "Exact Resume rejected or acknowledgement ambiguous. Inspect Live Agents first; re-confirm the SAME result to reuse the original request ID".into()
                    }
                };
                cx.notify();
            }).ok();
        }).detach();
    }
}

#[derive(Clone, Debug)]
pub(super) struct OriginalConversationMessage {
    role: String,
    text: String,
    timestamp: Option<String>,
}

fn parse_original_conversation_page(
    reply: &serde_json::Value,
    previous_before: Option<u64>,
) -> Result<(Vec<OriginalConversationMessage>, Option<u64>), &'static str> {
    if reply.get("source").and_then(serde_json::Value::as_str) != Some("nativeProvider")
        || reply.get("readOnly").and_then(serde_json::Value::as_bool) != Some(true)
    {
        return Err("Historical transcript is not a read-only native source");
    }
    let rows = reply
        .get("messages")
        .and_then(serde_json::Value::as_array)
        .ok_or("Missing native conversation messages")?;
    if rows.len() > 60 {
        return Err("Too many native conversation messages");
    }
    let mut messages = Vec::with_capacity(rows.len());
    for item in rows {
        let role = item
            .get("role")
            .and_then(serde_json::Value::as_str)
            .filter(|role| matches!(*role, "user" | "assistant"))
            .ok_or("Invalid historical message role")?;
        let text = item
            .get("text")
            .and_then(serde_json::Value::as_str)
            .filter(|text| text.chars().count() <= 8000)
            .ok_or("Invalid historical message content")?;
        let timestamp = item
            .get("timestamp")
            .and_then(serde_json::Value::as_str)
            .filter(|timestamp| timestamp.len() <= 64)
            .map(str::to_owned);
        messages.push(OriginalConversationMessage {
            role: role.to_owned(),
            text: text.to_owned(),
            timestamp,
        });
    }
    let before = if reply.get("hasMore").and_then(serde_json::Value::as_bool) == Some(true) {
        Some(
            reply
                .get("nextBefore")
                .and_then(serde_json::Value::as_u64)
                .filter(|next| *next > 0 && previous_before.is_none_or(|old| *next < old))
                .ok_or("Native conversation pagination cursor is stale")?,
        )
    } else {
        None
    };
    Ok((messages, before))
}

#[cfg(test)]
mod original_conversation_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn native_conversation_read_requires_read_only_source_and_text() {
        let good = json!({"source":"nativeProvider","readOnly":true,"hasMore":true,
        "nextBefore":250,"messages":[
            {"role":"user","text":"question"},
            {"role":"assistant","text":"answer","timestamp":"2026-11-22T01:00:00Z"}
        ]});
        let (messages, next) = parse_original_conversation_page(&good, Some(400)).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].text, "answer");
        assert_eq!(next, Some(250));
        let empty_tool_page = json!({"source":"nativeProvider","readOnly":true,
            "hasMore":true,"nextBefore":300,"messages":[]});
        let (empty, cursor) =
            parse_original_conversation_page(&empty_tool_page, Some(400)).unwrap();
        assert!(empty.is_empty());
        assert_eq!(
            cursor,
            Some(300),
            "tool-only latest page must still permit older original dialogue"
        );
        let mut invalid = good;
        invalid["nextBefore"] = json!(500);
        assert!(parse_original_conversation_page(&invalid, Some(400)).is_err());
        invalid["nextBefore"] = json!(250);
        invalid["messages"][0]["role"] = json!("tool");
        assert!(parse_original_conversation_page(&invalid, Some(400)).is_err());
        invalid["messages"][0]["role"] = json!("user");
        invalid["source"] = json!("untrusted");
        assert!(parse_original_conversation_page(&invalid, Some(400)).is_err());
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct GlobalAgentPromptTarget {
    handle: String,
    workspace_id: String,
    tab_id: String,
    agent_type: String,
}

impl GlobalAgentPromptTarget {
    fn is_live(&self, sessions: &[alera_gpui_desktop::terminal::TerminalSession]) -> bool {
        sessions.iter().any(|session| {
            session.running
                && session.handle == self.handle
                && session.workspace_id == self.workspace_id
                && session.tab_id == self.tab_id
                && session.agent_type.as_deref() == Some(self.agent_type.as_str())
        })
    }
}

#[derive(Debug)]
pub(super) struct GlobalAgentInspectorState {
    target: GlobalAgentPromptTarget,
    /// An independent VT grid, never the Workbench's selected TerminalDock.
    screen: alera_gpui_desktop::vt_screen::VtScreen,
    cursor: Option<u64>,
    read_busy: bool,
    status: String,
    key_queue: std::collections::VecDeque<String>,
    key_sending: bool,
}

impl GlobalAgentInspectorState {
    fn new(target: GlobalAgentPromptTarget) -> Self {
        Self {
            target,
            screen: alera_gpui_desktop::vt_screen::VtScreen::new(50, 160),
            cursor: None,
            read_busy: false,
            status: "Reading the original verified Host Terminal…".into(),
            key_queue: std::collections::VecDeque::new(),
            key_sending: false,
        }
    }
}

impl Workbench {
    /// Sidebar Agent rows follow Classic: focus the verified Host Tab in its
    /// original pane/workspace. Only Dashboard cards open the Inspector modal.
    pub(super) fn open_sidebar_agent_tab(
        &mut self,
        handle: String,
        workspace_id: String,
        tab_id: String,
        agent_type: String,
        cx: &mut Context<Self>,
    ) {
        let target = AgentNavigationTarget {
            session_id: handle,
            workspace_id,
            tab_id,
            agent_type,
        };
        if self.terminals.session_list_is_stale()
            || alera_gpui_desktop::agent_status_notifications::validated_sidebar_navigation_target(
                &target,
                &self.terminals.sessions,
            )
            .is_none()
        {
            self.agent_navigation.cancel();
            self.terminals.status = "Agent identity changed; refresh before opening Tab".into();
            self.refresh_terminals(cx);
            cx.notify();
            return;
        }
        // The shared deferred navigation rechecks the full Host identity,
        // reuses the existing Tab or opens a local view, and never resumes an Agent.
        self.agent_navigation
            .begin(target, AgentNavigationSource::Sidebar);
        cx.notify();
    }

    fn open_global_agent_inspector(
        &mut self,
        target: GlobalAgentPromptTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.terminals.session_list_is_stale() || !target.is_live(&self.terminals.sessions) {
            self.terminals.status = "Agent identity changed; refresh before inspecting".into();
            cx.notify();
            return;
        }
        if let Some(session) = self.terminals.sessions.iter().find(|session| {
            session.handle == target.handle
                && session.workspace_id == target.workspace_id
                && session.tab_id == target.tab_id
        }) {
            if let Some(generation) = dashboard_completion(session) {
                self.agent_completion_acknowledgements
                    .insert(dashboard_identity(session), generation.to_owned());
            }
        }
        self.global_agent_inspector_epoch = self.global_agent_inspector_epoch.wrapping_add(1);
        self.global_agent_inspector = Some(GlobalAgentInspectorState::new(target));
        self.global_agent_control_armed = None;
        self.global_agent_target = None;
        self.global_agent_inspector_input
            .update(cx, |input, cx| input.clear(cx));
        window.focus(
            &self.global_agent_inspector_input.read(cx).focus_handle(cx),
            cx,
        );
        self.refresh_global_agent_inspector(cx);
        cx.notify();
    }

    fn close_global_agent_inspector(&mut self, cx: &mut Context<Self>) {
        self.global_agent_inspector_epoch = self.global_agent_inspector_epoch.wrapping_add(1);
        self.global_agent_inspector = None;
        // The Host remains fully running. Any late read or key ACK is discarded
        // by the Inspector epoch and cannot select a different Agent.
        cx.notify();
    }

    /// A single Workbench timer calls this only while the Modal is visible.
    /// It never creates, attaches, focuses or resizes a Host PTY.
    pub(super) fn refresh_global_agent_inspector(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.global_agent_inspector.as_mut() else {
            return;
        };
        if state.read_busy {
            return;
        }
        if self.terminals.session_list_is_stale() || !state.target.is_live(&self.terminals.sessions)
        {
            state.status = "Agent identity is stale; input is blocked until refreshed".into();
            cx.notify();
            return;
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            state.status = "Existing Host CLI unavailable".into();
            cx.notify();
            return;
        };
        state.read_busy = true;
        let cursor = state.cursor;
        let target = state.target.clone();
        let read_target = target.clone();
        let epoch = self.global_agent_inspector_epoch;
        cx.spawn(async move |view, cx| {
            let reply = cx
                .background_spawn(async move {
                    bridge.read_terminal_bounded(&read_target.handle, cursor, 16 * 1024)
                })
                .await;
            view.update(cx, |view, cx| {
                if view.global_agent_inspector_epoch != epoch {
                    return;
                }
                let Some(state) = view.global_agent_inspector.as_mut() else {
                    return;
                };
                if state.target != target {
                    return;
                }
                state.read_busy = false;
                if view.terminals.session_list_is_stale()
                    || !target.is_live(&view.terminals.sessions)
                {
                    state.status = "Host identity changed; stopped displaying stale output".into();
                    cx.notify();
                    return;
                }
                match reply {
                    Ok(chunk) if chunk.handle == target.handle => {
                        if chunk.truncated {
                            state.screen.reset();
                        }
                        if state.cursor != Some(chunk.next_cursor) || chunk.truncated {
                            state.screen.feed(&chunk.text);
                            state.cursor = Some(chunk.next_cursor);
                        }
                        state.status = if chunk.running {
                            "Live Terminal · type directly · Esc reaches Agent · close with ×"
                                .into()
                        } else {
                            "Terminal is no longer running; input disabled".into()
                        };
                    }
                    _ => state.status = "Cannot read verified Host Terminal; retrying…".into(),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn enqueue_global_agent_inspector_key(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(state) = self.global_agent_inspector.as_mut() else {
            return false;
        };
        if self.terminals.session_list_is_stale()
            || !state.target.is_live(&self.terminals.sessions)
            || text.is_empty()
            || text.len() > 4096
            || text.contains('\0')
        {
            state.status = "Terminal input blocked: invalid text or stale Agent".into();
            cx.notify();
            return false;
        }
        if state.key_queue.len() >= 256
            || state.key_queue.iter().map(String::len).sum::<usize>() + text.len() > 8192
        {
            state.status = "Terminal input queue is full; retry after pending input".into();
            cx.notify();
            return false;
        }
        state.key_queue.push_back(text);
        self.flush_global_agent_inspector_keys(cx);
        true
    }

    fn flush_global_agent_inspector_keys(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.global_agent_inspector.as_mut() else {
            return;
        };
        if state.key_sending {
            return;
        }
        let Some(mut text) = state.key_queue.pop_front() else {
            return;
        };
        // Amortize per-keystroke CLI startup while preserving byte order,
        // including Escape/Enter and finalized IME characters.
        while let Some(next) = state.key_queue.front() {
            if text.len() + next.len() > 1024 {
                break;
            }
            text.push_str(&state.key_queue.pop_front().expect("queued input"));
        }
        let target = state.target.clone();
        if self.terminals.session_list_is_stale() || !target.is_live(&self.terminals.sessions) {
            state.key_queue.clear();
            state.status = "Agent identity changed; queued input discarded".into();
            cx.notify();
            return;
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            state.key_queue.clear();
            state.status = "Host CLI unavailable; queued input discarded".into();
            cx.notify();
            return;
        };
        state.key_sending = true;
        let input_target = target.clone();
        let epoch = self.global_agent_inspector_epoch;
        cx.spawn(async move |view, cx| {
            let sent = cx
                .background_spawn(async move {
                    bridge.write_verified_agent_raw(
                        &input_target.handle,
                        &input_target.workspace_id,
                        &input_target.tab_id,
                        &text,
                    )
                })
                .await;
            view.update(cx, |view, cx| {
                if view.global_agent_inspector_epoch != epoch {
                    return;
                }
                let Some(state) = view.global_agent_inspector.as_mut() else {
                    return;
                };
                if state.target != target {
                    return;
                }
                state.key_sending = false;
                if let Err(error) = sent {
                    state.key_queue.clear();
                    state.status = format!("Host rejected Agent input: {error}");
                } else {
                    view.flush_global_agent_inspector_keys(cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn send_global_agent_inspector_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.global_agent_inspector.is_none() {
            return false;
        }
        if self.global_agent_inspector_input.read(cx).composing()
            || win_ime::focused_native_text_mode()
        {
            // Zhuyin/IME candidate controls belong to the IME until commit.
            return false;
        }
        let Some(bytes) = encode_terminal_key(
            &event.keystroke.key,
            event.keystroke.key_char.as_deref(),
            event.keystroke.modifiers.control,
            event.keystroke.modifiers.alt,
            event.keystroke.modifiers.platform,
        ) else {
            return false;
        };
        let ascii = bytes.len() == 1
            && (bytes.as_bytes()[0].is_ascii_graphic() || bytes == " ")
            && !event.keystroke.modifiers.control
            && !event.keystroke.modifiers.alt;
        let sent = self.enqueue_global_agent_inspector_key(bytes.clone(), cx);
        if sent && ascii {
            self.global_agent_inspector_input.update(cx, |input, _| {
                input.note_terminal_ascii_key(&bytes);
            });
        }
        sent
    }
}

/// Host response classification matters: a failed preflight cannot have
/// launched a PTY, while a lost launch acknowledgement might have done so.
enum GlobalAgentCreateReply {
    NotIssued(&'static str),
    Launched(
        GlobalAgentLaunchReceipt,
        Option<Vec<alera_gpui_desktop::terminal::TerminalSession>>,
    ),
    OutcomeUnknown,
}

#[derive(Debug)]
pub(super) enum GlobalAgentLaunchReceipt {
    Installed(alera_gpui_desktop::terminal::CreatedHostTerminal),
    Profile(LaunchedAgentTab),
}

impl GlobalAgentLaunchReceipt {
    fn matches(&self, sessions: &[TerminalSession], expected: &str) -> bool {
        match self {
            Self::Installed(created) => sessions
                .iter()
                .any(|session| created.matches_host_verified_agent_session(session, expected)),
            Self::Profile(created) => matching_agent_handle(created, sessions, expected).is_some(),
        }
    }
}

fn dashboard_installed_choices(agents: &[InstalledAgentCli]) -> Vec<(String, String)> {
    agents
        .iter()
        .map(|agent| (agent.id.clone(), agent.new_tab_menu_label().to_owned()))
        .collect()
}

impl Workbench {
    fn close_original_conversation(&mut self) {
        self.global_agent_transcript_request = self.global_agent_transcript_request.wrapping_add(1);
        self.global_agent_transcript_target = None;
        self.global_agent_transcript_messages.clear();
        self.global_agent_transcript_before = None;
        self.global_agent_transcript_busy = false;
        self.global_agent_transcript_status.clear();
    }

    /// Explicit-click, read-only provider transcript page. This never calls
    /// terminal.read/write/attach and never starts or resumes a new Agent.
    fn load_original_conversation(
        &mut self,
        target: GlobalAgentPromptTarget,
        older: bool,
        cx: &mut Context<Self>,
    ) {
        if self.global_agent_transcript_busy {
            return;
        }
        if self.terminals.session_list_is_stale()
            || !target.is_live(&self.terminals.sessions)
            || !matches!(target.agent_type.as_str(), "claude" | "codex")
        {
            self.global_agent_transcript_status =
                "Live Agent identity changed; refresh Host before reading history".into();
            cx.notify();
            return;
        }
        if older
            && (self.global_agent_transcript_target.as_ref() != Some(&target)
                || self.global_agent_transcript_before.is_none())
        {
            return;
        }
        let before = if older {
            self.global_agent_transcript_before
        } else {
            None
        };
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.global_agent_transcript_status =
                "Local Alera CLI unavailable; original conversation was not read".into();
            cx.notify();
            return;
        };
        if !older {
            self.close_original_conversation();
            self.global_agent_transcript_target = Some(target.clone());
        }
        self.global_agent_transcript_busy = true;
        self.global_agent_transcript_status =
            "Reading original native user/assistant messages (read only)…".into();
        self.global_agent_transcript_request = self.global_agent_transcript_request.wrapping_add(1);
        let ticket = self.global_agent_transcript_request;
        cx.notify();
        cx.spawn(async move |view, cx| {
            let requested = target.clone();
            let result = cx.background_spawn(async move {
                bridge.read_resumed_agent_messages(
                    &target.workspace_id, &target.tab_id, before
                )
            }).await;
            view.update(cx, |view, cx| {
                if view.global_agent_transcript_request != ticket
                    || view.global_agent_transcript_target.as_ref() != Some(&requested) {
                    return;
                }
                view.global_agent_transcript_busy = false;
                match result {
                    Ok(reply) => match parse_original_conversation_page(&reply, before) {
                        Ok((messages, next_before)) => {
                            if older {
                                // Show earlier messages above the current page;
                                // bound retained plaintext to 360 visible entries.
                                let mut combined = messages;
                                combined.extend(view.global_agent_transcript_messages.drain(..));
                                combined.truncate(360);
                                view.global_agent_transcript_messages = combined;
                            } else {
                                view.global_agent_transcript_messages = messages;
                            }
                            view.global_agent_transcript_before = next_before;
                            view.global_agent_transcript_status =
                                "Original provider-native dialogue · read only · live Terminal unchanged".into();
                        }
                        Err(_) => {
                            view.global_agent_transcript_messages.clear();
                            view.global_agent_transcript_before = None;
                            view.global_agent_transcript_status =
                                "Native history response was invalid; nothing was sent to Agent".into();
                        }
                    },
                    Err(_) => {
                        view.global_agent_transcript_messages.clear();
                        view.global_agent_transcript_before = None;
                        view.global_agent_transcript_status =
                            "Original history unavailable: the saved Claude/Codex Tab lacks a unique Host-verified native Session source".into();
                    }
                }
                cx.notify();
            }).ok();
        }).detach();
    }
}

impl Workbench {
    /// Both interrupt and terminate require two deliberate clicks on the SAME
    /// live Host identity. Switching target or hiding a card disarms the action.
    fn confirm_global_agent_control(
        &mut self,
        target: GlobalAgentPromptTarget,
        terminate: bool,
        cx: &mut Context<Self>,
    ) {
        if self.global_agent_control_busy || self.global_agent_submit_busy {
            return;
        }
        if self.terminals.session_list_is_stale() || !target.is_live(&self.terminals.sessions) {
            self.global_agent_control_armed = None;
            self.global_agent_control_status =
                "Agent identity stale; refresh Host inventory before controlling".into();
            cx.notify();
            return;
        }
        if self.global_agent_control_armed.as_ref() != Some(&(target.clone(), terminate)) {
            self.global_agent_control_armed = Some((target, terminate));
            self.global_agent_control_status = if terminate {
                "Click Confirm Terminate again: this ends the Host PTY and may erase its retained Terminal history. Hide Card to preserve it"
            } else {
                "Click Confirm Interrupt again: only the Agent's current turn is interrupted; the PTY stays running"
            }.into();
            cx.notify();
            return;
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.global_agent_control_armed = None;
            self.global_agent_control_status =
                "Existing Runtime Host CLI unavailable; no action was taken".into();
            cx.notify();
            return;
        };
        self.global_agent_control_armed = None;
        self.global_agent_control_busy = true;
        self.global_agent_control_status =
            "Requesting Host-verified Agent lifecycle control…".into();
        cx.notify();
        cx.spawn(async move |view, cx| {
            let current_target = target.clone();
            let result = cx.background_spawn(async move {
                bridge.control_verified_agent(
                    &target.handle, &target.workspace_id, &target.tab_id, terminate,
                )
            }).await;
            view.update(cx, |view, cx| {
                view.global_agent_control_busy = false;
                if result.is_ok() {
                    view.global_agent_control_status = if terminate {
                        "Host acknowledged Agent Session termination. Refresh the list"
                    } else {
                        "Host accepted Agent interrupt. Check its next Hook state"
                    }.into();
                    if terminate {
                        view.global_agent_hidden.remove(&(
                            current_target.handle.clone(),
                            current_target.workspace_id.clone(),
                            current_target.tab_id.clone(),
                        ));
                        if view.global_agent_target.as_ref() == Some(&current_target) {
                            view.global_agent_target = None;
                        }
                    }
                    view.refresh_terminals(cx);
                } else {
                    // The Host may have executed the action before its response
                    // was lost. Never auto-repeat a terminate or Ctrl-C.
                    view.global_agent_control_status =
                        "Control outcome unconfirmed. Inspect refreshed Host inventory before any retry".into();
                    view.refresh_terminals(cx);
                }
                cx.notify();
            }).ok();
        }).detach();
    }
}

impl Workbench {
    fn open_global_agent_create(&mut self, cx: &mut Context<Self>) {
        self.global_agent_create_open = true;
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        if self.runtime_bridge.is_none() {
            self.global_agent_create_status = "Alera CLI unavailable. No Host started".into();
        } else {
            self.global_agent_create_status =
                "Select a Project, Workspace and installed Agent".into();
            self.refresh_mcp_host_workspaces(cx);
            self.refresh_installed_agent_clis(cx);
            self.refresh_agent_profiles(cx);
        }
        cx.notify();
    }

    /// Restore only Dashboard-owned profile launch tracking. A pending
    /// ordinary Terminal create belongs to its original pane owner and must
    /// never be acknowledged from this panel.
    fn select_global_agent_create_workspace(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.global_agent_create_busy || self.global_agent_create_journal.is_some() {
            self.global_agent_create_status =
                "Resolve the previous Agent creation before selecting another Worktree".into();
            cx.notify();
            return;
        }
        let root = match alera_gpui_desktop::terminal::resolve_registered_host_workspace_path(
            id,
            &self.mcp_host_workspaces,
        ) {
            Ok(root) => root,
            Err(_) => {
                self.global_agent_create_status =
                    "Selected Worktree was removed from Host. Refresh to choose again".into();
                cx.notify();
                return;
            }
        };
        if alera_gpui_desktop::terminal::validate_global_agent_launch_workspace(
            id,
            &root,
            &self.mcp_host_workspaces,
        )
        .is_err()
        {
            self.global_agent_create_status = "Ambiguous Worktree; refresh Host inventory".into();
            cx.notify();
            return;
        }
        let Some(store) = self.session_store.as_ref() else {
            self.global_agent_create_status = "Native recovery storage unavailable".into();
            cx.notify();
            return;
        };
        match store.pending_host_create(&root) {
            Ok(Some(marker)) if marker.is_global_agent_profile() => {
                self.global_agent_create_workspace = Some(id.to_owned());
                self.global_agent_create_journal = Some((root, marker));
                self.global_agent_create_pending = None;
                self.global_agent_create_expected_agent = None;
                self.global_agent_create_inventory_verified = false;
                self.global_agent_create_ack_armed = false;
                self.global_agent_create_status =
                    "Recovered an uncertain Dashboard launch from disk; inspect fresh Host sessions before acknowledging".into();
                self.refresh_terminals(cx);
            }
            Ok(Some(_)) => {
                self.global_agent_create_status =
                    "Another Terminal/Agent creation owns this Worktree recovery marker. Reopen its original pane".into();
            }
            Ok(None) => {
                self.global_agent_create_workspace = Some(id.to_owned());
                self.global_agent_create_status =
                    "Workspace selected. Choose an installed Agent".into();
            }
            Err(_) => {
                self.global_agent_create_status =
                    "Invalid create journal; preserve it and inspect Host sessions".into();
            }
        }
        cx.notify();
    }

    /// Launch without switching the editor or stealing another pane's focus.
    /// A durable per-worktree marker is reserved before the Host RPC.
    fn launch_global_agent(&mut self, cx: &mut Context<Self>) {
        if self.global_agent_create_busy || self.global_agent_create_journal.is_some() {
            self.global_agent_create_status =
                "Previous Agent creation is still pending. Inspect Host sessions; do not retry"
                    .into();
            cx.notify();
            return;
        }
        if self.agent_launch_busy || self.terminal_create_flow.is_active() {
            self.global_agent_create_status =
                "Finish the existing Agent/Terminal create before launching another".into();
            cx.notify();
            return;
        }
        if self.mcp_host_workspaces_loading
            || self.installed_agent_clis_loading
            || (self.global_agent_create_profile.is_some() && self.agent_profiles_loading)
        {
            self.global_agent_create_status =
                "Workspace/Agent inventory is refreshing; try after it completes".into();
            cx.notify();
            return;
        }
        let Some(workspace_id) = self.global_agent_create_workspace.clone() else {
            self.global_agent_create_status = "Select a registered Worktree first".into();
            cx.notify();
            return;
        };
        let valid_project = self
            .global_agent_create_project
            .as_deref()
            .is_some_and(|project| {
                self.runtime_catalog.as_ref().is_some_and(|catalog| {
                    catalog.projects.iter().any(|entry| entry.id == project)
                        && catalog.workspaces.iter().any(|workspace| {
                            workspace.id == workspace_id
                                && workspace.project_id == project
                                && !workspace.is_archived
                        })
                })
            });
        if !valid_project {
            self.global_agent_create_status =
                "Project/Workspace selection changed. Select again.".into();
            cx.notify();
            return;
        }
        let Some(agent_type) = self.global_agent_create_agent.clone() else {
            self.global_agent_create_status = "Select an installed Agent first".into();
            cx.notify();
            return;
        };
        let Some(executable) = self
            .installed_agent_clis
            .iter()
            .find(|agent| agent.id == agent_type)
            .map(|agent| agent.executable_path.to_string_lossy().into_owned())
        else {
            self.global_agent_create_status =
                "Installed Agent changed; refresh before launching".into();
            cx.notify();
            return;
        };
        let profile_id = self.global_agent_create_profile.clone();
        if let Some(id) = profile_id.as_deref() {
            if !self.agent_profiles.iter().any(|profile| {
                profile.id == id
                    && alera_core::agent_descriptor::canonical_agent_id(&profile.agent_type)
                        == Some(agent_type.as_str())
            }) {
                self.global_agent_create_status = "Agent Profile changed; select again".into();
                cx.notify();
                return;
            }
        }
        let root = match alera_gpui_desktop::terminal::resolve_registered_host_workspace_path(
            &workspace_id,
            &self.mcp_host_workspaces,
        ) {
            Ok(root) => root,
            Err(_) => {
                self.global_agent_create_status =
                    "Workspace selection is no longer registered. Refresh Host workspaces".into();
                cx.notify();
                return;
            }
        };
        if alera_gpui_desktop::terminal::validate_global_agent_launch_workspace(
            &workspace_id,
            &root,
            &self.mcp_host_workspaces,
        )
        .is_err()
        {
            self.global_agent_create_status = "Ambiguous Workspace ID or path; refresh".into();
            cx.notify();
            return;
        }
        let input = self.global_agent_create_prompt_input.read(cx);
        if profile_id.is_some() && input.composing() {
            self.global_agent_create_status = "Finish IME composition before launching".into();
            cx.notify();
            return;
        }
        let prompt = input.query().trim().to_owned();
        if profile_id.is_some()
            && (prompt.is_empty() || prompt.len() > 8192 || prompt.contains('\0'))
        {
            self.global_agent_create_status = "Initial Agent task must be 1–8192 bytes".into();
            cx.notify();
            return;
        }
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.global_agent_create_status = "Alera CLI unavailable; no Agent launched".into();
            cx.notify();
            return;
        };
        let Some(store) = self.session_store.as_ref() else {
            self.global_agent_create_status =
                "Cannot launch without durable Native recovery storage".into();
            cx.notify();
            return;
        };
        let journal = match store.reserve_global_agent_profile_create(&root) {
            Ok(record) => record,
            Err(_) => {
                self.global_agent_create_status =
                    "Unresolved Host create marker for this Worktree. Inspect existing sessions; no new PTY issued".into();
                cx.notify();
                return;
            }
        };
        self.global_agent_create_journal = Some((root.clone(), journal));
        self.global_agent_create_pending = None;
        self.global_agent_create_expected_agent = Some(agent_type.clone());
        self.global_agent_create_inventory_verified = false;
        self.global_agent_create_ack_armed = false;
        self.global_agent_create_busy = true;
        self.global_agent_create_request = self.global_agent_create_request.wrapping_add(1);
        let ticket = self.global_agent_create_request;
        self.global_agent_create_status =
            "Checking fresh Host Workspace identity before launch…".into();
        cx.notify();
        cx.spawn(async move |view, cx| {
            let reply = cx.background_spawn(async move {
                let fresh = match bridge.list_registered_workspaces() {
                    Ok(fresh) => fresh,
                    Err(_) => return GlobalAgentCreateReply::NotIssued(
                        "Host Workspace inventory unavailable; no Agent started",
                    ),
                };
                if alera_gpui_desktop::terminal::validate_global_agent_launch_workspace(
                    &workspace_id, &root, &fresh,
                ).is_err() {
                    return GlobalAgentCreateReply::NotIssued(
                        "Target Worktree changed since selection; no Agent started",
                    );
                }
                let launched = match profile_id.as_deref() {
                    Some(id) => bridge.launch_agent_profile(id, &workspace_id, &prompt)
                        .map(GlobalAgentLaunchReceipt::Profile),
                    None => bridge.create_agent_terminal(&workspace_id, &agent_type, &executable)
                        .map(GlobalAgentLaunchReceipt::Installed),
                };
                match launched {
                    Ok(created) => GlobalAgentCreateReply::Launched(
                        created, bridge.list_terminals().ok(),
                    ),
                    Err(_) => GlobalAgentCreateReply::OutcomeUnknown,
                }
            }).await;
            view.update(cx, |view, cx| {
                if view.global_agent_create_request != ticket {
                    return;
                }
                view.global_agent_create_busy = false;
                match reply {
                    GlobalAgentCreateReply::NotIssued(message) => {
                        if let Some((root, journal)) = view.global_agent_create_journal.as_ref() {
                            if view.session_store.as_ref()
                                .is_some_and(|store| store.resolve_host_create(root, journal).is_ok())
                            {
                                view.global_agent_create_journal = None;
                                view.global_agent_create_expected_agent = None;
                                view.global_agent_create_status = message.into();
                            } else {
                                view.global_agent_create_status =
                                    "Preflight rejected; journal could not be cleared. Inspect recovery marker".into();
                            }
                        }
                    }
                    GlobalAgentCreateReply::Launched(created, sessions) => {
                        view.global_agent_create_pending = Some(created);
                        view.global_agent_create_status =
                            "Agent launch returned; verifying exact Host tab (no workspace switch)…".into();
                        if let Some(sessions) = sessions {
                            view.global_agent_create_inventory_verified = true;
                            view.confirm_global_agent_created(&sessions);
                        }
                        view.refresh_terminals(cx);
                    }
                    GlobalAgentCreateReply::OutcomeUnknown => {
                        view.global_agent_create_status =
                            "Host launch outcome unknown. Refresh inventory and inspect before acknowledging; never blindly retry".into();
                        view.refresh_terminals(cx);
                    }
                }
                cx.notify();
            }).ok();
        }).detach();
    }

    /// Host-verified list is the only authority for resolving a launch marker.
    pub(super) fn confirm_global_agent_created(
        &mut self,
        sessions: &[alera_gpui_desktop::terminal::TerminalSession],
    ) {
        let Some(created) = self.global_agent_create_pending.as_ref() else {
            return;
        };
        let Some(expected) = self.global_agent_create_expected_agent.as_deref() else {
            return;
        };
        if !created.matches(sessions, expected) {
            return;
        }
        let (Some((root, journal)), Some(store)) = (
            self.global_agent_create_journal.as_ref(),
            self.session_store.as_ref(),
        ) else {
            self.global_agent_create_status =
                "Agent verified but Native recovery journal is missing".into();
            return;
        };
        if store.resolve_host_create(root, journal).is_err() {
            self.global_agent_create_status =
                "Host Agent verified, but recovery marker could not be cleared; inspect storage"
                    .into();
            return;
        }
        self.global_agent_create_journal = None;
        self.global_agent_create_pending = None;
        self.global_agent_create_expected_agent = None;
        self.global_agent_create_ack_armed = false;
        self.global_agent_create_status =
            "Host confirmed Agent in chosen Worktree. Provider sign-in is not verified".into();
    }

    /// A launch with a lost response has no trustworthy tab ID. Users must
    /// inspect a FRESH authenticated inventory before acknowledging the marker.
    /// Acknowledging never terminates a PTY and does not imply it was created.
    fn acknowledge_global_agent_create(&mut self, cx: &mut Context<Self>) {
        if self.global_agent_create_busy || self.global_agent_create_journal.is_none() {
            return;
        }
        if !self.global_agent_create_inventory_verified || self.terminals.session_list_is_stale() {
            self.global_agent_create_status =
                "Refresh a live Host session inventory before acknowledging unknown creation"
                    .into();
            self.refresh_terminals(cx);
            cx.notify();
            return;
        }
        if !self.global_agent_create_ack_armed {
            self.global_agent_create_ack_armed = true;
            self.global_agent_create_status =
                "Inspect Host sessions first. Click Acknowledge again to clear ONLY recovery tracking, not PTYs".into();
            cx.notify();
            return;
        }
        let (Some((root, journal)), Some(store)) = (
            self.global_agent_create_journal.as_ref(),
            self.session_store.as_ref(),
        ) else {
            return;
        };
        if store.resolve_host_create(root, journal).is_err() {
            self.global_agent_create_status =
                "Recovery marker changed; cannot acknowledge automatically".into();
            cx.notify();
            return;
        }
        self.global_agent_create_journal = None;
        self.global_agent_create_pending = None;
        self.global_agent_create_expected_agent = None;
        self.global_agent_create_ack_armed = false;
        self.global_agent_create_status =
            "Tracking acknowledged after fresh Host inventory. Existing PTYs left untouched".into();
        cx.notify();
    }
}

fn history_filter_badge(
    provider: Option<&str>,
    fields: [&str; 3],
    expanded: bool,
) -> (usize, String) {
    let count = usize::from(provider.is_some())
        + fields
            .into_iter()
            .filter(|value| !value.trim().is_empty())
            .count();
    let arrow = if expanded { "▴" } else { "▾" };
    let label = if count == 0 {
        format!("Filters {arrow}")
    } else {
        format!("Filters ({count}) {arrow}")
    };
    (count, label)
}

#[cfg(test)]
mod compact_history_filter_tests {
    use super::history_filter_badge;

    #[test]
    fn empty_and_whitespace_fields_do_not_count_as_filters() {
        assert_eq!(
            history_filter_badge(None, ["", " \t", "\u{3000}"], false),
            (0, "Filters ▾".into())
        );
        assert_eq!(
            history_filter_badge(None, ["", "", ""], true),
            (0, "Filters ▴".into())
        );
    }

    #[test]
    fn collapsed_badge_retains_provider_and_field_counts() {
        for (provider, fields, count) in [
            (Some("claude"), ["", "", ""], 1),
            (None, ["project", "worktree", ""], 2),
            (None, ["project", "worktree", "account"], 3),
            (Some("codex"), ["project", "worktree", "account"], 4),
        ] {
            assert_eq!(
                history_filter_badge(provider, fields, false),
                (count, format!("Filters ({count}) ▾"))
            );
            assert_eq!(
                history_filter_badge(provider, fields, true),
                (count, format!("Filters ({count}) ▴"))
            );
        }
    }
}

fn history_panel(view: &Workbench, cx: &mut Context<Workbench>) -> gpui::AnyElement {
    let busy = view.global_agent_history_busy;
    let consented = view.global_agent_history_consent;
    let (filter_count, filter_label) = history_filter_badge(
        view.global_agent_history_provider.as_deref(),
        [
            view.global_agent_history_project_input.read(cx).query(),
            view.global_agent_history_workspace_input.read(cx).query(),
            view.global_agent_history_account_input.read(cx).query(),
        ],
        view.global_agent_history_filters_expanded,
    );
    // Keep search and consent visible while results scroll independently.
    let mut chips = div()
        .id("global-history-provider-chips")
        .flex_1()
        .min_w_0()
        .flex()
        .items_center()
        .gap_2();
    for (index, (choice, label)) in [
        (None, "All"),
        (Some("claude"), "Claude"),
        (Some("codex"), "Codex"),
    ]
    .into_iter()
    .enumerate()
    {
        let selected = view.global_agent_history_provider.as_deref() == choice;
        let next = choice.map(str::to_owned);
        chips = chips.child(
            div()
                .id(("global-history-provider-chip", index))
                .cursor_pointer()
                .px_3()
                .py_2()
                .rounded_md()
                .bg(rgb(if selected { ELEVATED } else { PANEL }))
                .border_1()
                .border_color(rgb(BORDER))
                .text_xs()
                .font_weight(if selected {
                    gpui::FontWeight::BOLD
                } else {
                    gpui::FontWeight::NORMAL
                })
                .child(label)
                .on_click(cx.listener(move |view, _, _, cx| {
                    if view.global_agent_history_busy || view.global_agent_history_provider == next
                    {
                        return;
                    }
                    view.global_agent_history_provider = next.clone();
                    view.global_agent_history_results.clear();
                    view.global_agent_history_located.clear();
                    view.global_agent_history_resume_expanded = None;
                    view.global_agent_history_resume_armed = None;
                    view.global_agent_history_clear_armed = false;
                    view.global_agent_history_status =
                        "Provider changed. Search again to refresh previews.".into();
                    cx.notify();
                })),
        );
    }
    let list = div()
        .id("global-agent-history-panel")
        .when(view.global_agent_maximized, |panel| panel.flex_1().min_h_0())
        .when(!view.global_agent_maximized, |panel| panel.h(px(590.0)).flex_shrink_0())
        .px_3()
        .py_2()
        .flex()
        .flex_col()
        .gap_2()
        .border_b_1()
        .border_color(rgb(BORDER))
        .child(
            div().w_full().flex().items_center().gap_3()
                .child(div().font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_sm().child("History"))
                .child(div().flex_1())
                .child(
                    div().id("global-history-return-to-live")
                        .text_xs().cursor_pointer().text_color(rgb(TEXT))
                        .child("← Agents")
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.agent_navigation.cancel();
                            view.global_agent_history_open = false;
                            view.global_agent_history_filters_expanded = false;
                            view.global_agent_history_clear_armed = false;
                            view.global_agent_history_resume_expanded = None;
                            cx.notify();
                        })),
                ),
        )
        .child(
            div().w_full().flex().items_center().gap_2()
                .child(div().flex_1().min_w_0().child(view.global_agent_history_input.clone()))
                .child(
                    div().id("global-history-search")
                        .cursor_pointer().rounded_md().bg(rgb(ELEVATED))
                        .border_1().border_color(rgb(BORDER))
                        .px_3().py_2()
                        .text_xs()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(if busy || !consented { MUTED } else { TEXT }))
                        .child(if busy { "Searching…" } else { "Search" })
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.search_global_agent_history(cx);
                        })),
                )
                .child(
                    div().id("global-history-advanced-toggle")
                        .cursor_pointer().flex_shrink_0().rounded_md()
                        .border_1().border_color(rgb(BORDER))
                        .bg(rgb(if view.global_agent_history_filters_expanded || filter_count > 0 {
                            ELEVATED
                        } else {
                            PANEL
                        }))
                        .px_3().py_2().text_xs().text_color(rgb(TEXT))
                        .child(filter_label)
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.global_agent_history_filters_expanded =
                                !view.global_agent_history_filters_expanded;
                            if !view.global_agent_history_filters_expanded {
                                window.focus(
                                    &view.global_agent_history_input.read(cx).focus_handle(cx),
                                    cx,
                                );
                            }
                            cx.notify();
                        })),
                ),
        )
        .when(view.global_agent_history_filters_expanded, |list| {
            list.child(
                div().id("global-history-advanced-filters")
                    .w_full().p_2().rounded_md().bg(rgb(PANEL))
                    .flex().flex_col().gap_2()
                    .child(
                        div().w_full().flex().items_center().gap_2()
                            .child(chips)
                            .child(
                                div().id("global-history-reset-filters")
                                    .cursor_pointer().flex_shrink_0().text_xs()
                                    .text_color(rgb(if busy { MUTED } else { TEXT }))
                                    .child("Reset Filters")
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        if view.global_agent_history_busy {
                                            return;
                                        }
                                        view.global_agent_history_provider = None;
                                        for input in [
                                            view.global_agent_history_project_input.clone(),
                                            view.global_agent_history_workspace_input.clone(),
                                            view.global_agent_history_account_input.clone(),
                                        ] {
                                            input.update(cx, |input, cx| input.clear(cx));
                                        }
                                        view.global_agent_history_results.clear();
                                        view.global_agent_history_located.clear();
                                        view.global_agent_history_resume_expanded = None;
                                        view.global_agent_history_resume_armed = None;
                                        view.global_agent_history_clear_armed = false;
                                        view.global_agent_history_status =
                                            "Filters reset. Search again to refresh previews.".into();
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div().w_full().flex().items_center().gap_2()
                            .child(div().flex_1().min_w_0().child(view.global_agent_history_project_input.clone()))
                            .child(div().flex_1().min_w_0().child(view.global_agent_history_workspace_input.clone()))
                            .child(div().flex_1().min_w_0().child(view.global_agent_history_account_input.clone())),
                    ),
            )
        })
        .child(
            div().id("global-history-consent").text_xs().cursor_pointer()
                .text_color(rgb(if consented { TEXT } else { MUTED }))
                .child(if consented {
                    "☑ Local index on · private SQLite, not encrypted"
                } else {
                    "☐ Enable local index · private SQLite, not encrypted"
                })
                .on_click(cx.listener(|view, _, _, cx| {
                    if view.global_agent_history_busy { return; }
                    view.global_agent_history_consent = !view.global_agent_history_consent;
                    if !view.global_agent_history_consent {
                        view.global_agent_history_results.clear();
                        view.global_agent_history_located.clear();
                        view.global_agent_history_resume_armed = None;
                        view.global_agent_history_resume_expanded = None;
                    }
                    cx.notify();
                })),
        )
        .child(
            div().w_full().flex().items_center().gap_3()
                .child(div().text_xs().font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(format!("{} results", view.global_agent_history_results.len())))
                .child(div().flex_1())
                .child(
                    div().id("global-history-clear")
                        .cursor_pointer().text_xs()
                        .text_color(rgb(if view.global_agent_history_clear_armed { TEXT } else { MUTED }))
                        .child(if view.global_agent_history_clear_armed {
                            "Confirm clear · originals kept"
                        } else {
                            "Clear cache…"
                        })
                        .on_click(cx.listener(|view, _, _, cx| {
                            if view.global_agent_history_busy { return; }
                            if !view.global_agent_history_clear_armed {
                                view.global_agent_history_clear_armed = true;
                                view.global_agent_history_status =
                                    "Confirm cache deletion by clicking Clear again. Native Claude/Codex transcripts remain untouched.".into();
                                cx.notify();
                            } else {
                                view.global_agent_history_clear_armed = false;
                                view.clear_global_agent_history(cx);
                            }
                        })),
                ),
        )
        .when(!view.global_agent_history_status.is_empty(), |panel| {
            panel.child(
                div()
                    .max_w(px(700.0))
                    .truncate()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(view.global_agent_history_status.clone()),
            )
        });
    let mut results = div()
        .id("global-history-results-scroll")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .gap_2()
        .py_2();
    let profile = view
        .global_agent_history_resume_profile_input
        .read(cx)
        .query()
        .trim()
        .to_owned();
    for (index, hit) in view.global_agent_history_results.iter().enumerate() {
        let source_key = history_source_identity(hit);
        let resume_open =
            view.global_agent_history_resume_expanded.as_deref() == Some(source_key.as_str());
        let mut card = div()
            .id(("global-history-preview", index))
            .bg(rgb(ELEVATED))
            .rounded_md()
            .px_2()
            .py_2()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(hit.title.clone()),
            )
            .child(div().text_xs().text_color(rgb(MUTED)).child(format!(
                "{} · {} · {}",
                hit.provider, hit.account, hit.native_id
            )))
            .child(div().text_xs().text_color(rgb(MUTED)).child(format!(
                "Project {} / Worktree {}",
                hit.project, hit.workspace
            )))
            .child(div().text_xs().child(hit.goal.clone()))
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(hit.progress.clone()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(format!("Matched: {} · preview only", hit.match_field)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(if hit.account.is_empty() {
                        "Preview only · source account unavailable"
                    } else {
                        "Exact Resume available · Host verifies source account, UUID and Worktree"
                    }),
            );
        if !hit.account.is_empty() {
            card = card.child(
                div()
                    .id(("global-history-locate-existing", index))
                    .text_xs()
                    .cursor_pointer()
                    .text_color(rgb(if busy || !consented { MUTED } else { TEXT }))
                    .child("Find existing verified Tab (read only)")
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.locate_global_agent_history(index, cx);
                    })),
            );
            if view
                .global_agent_history_located
                .contains_key(&history_source_identity(hit))
            {
                card = card.child(
                    div()
                        .id(("global-history-open-verified", index))
                        .text_xs()
                        .cursor_pointer()
                        .text_color(rgb(if busy || !consented { MUTED } else { TEXT }))
                        .child("Open verified running Tab")
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.open_located_global_agent_history(index, cx);
                        })),
                );
            }
            let identity = history_resume_identity(hit, &profile);
            let armed =
                view.global_agent_history_resume_armed.as_deref() == Some(identity.as_str());
            card = card.child(
                div()
                    .id(("global-history-resume-toggle", index))
                    .text_xs()
                    .cursor_pointer()
                    .text_color(rgb(TEXT))
                    .child(if resume_open {
                        "▴ Hide Resume options"
                    } else {
                        "▾ Resume Session…"
                    })
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.global_agent_history_resume_expanded =
                            if view.global_agent_history_resume_expanded.as_deref()
                                == Some(source_key.as_str())
                            {
                                None
                            } else {
                                Some(source_key.clone())
                            };
                        view.global_agent_history_resume_armed = None;
                        cx.notify();
                    })),
            );
            if resume_open {
                card = card.child(
                    div().text_xs().text_color(rgb(MUTED)).child(
                        "Official CLI Profile ID from Settings · verify account/Worktree · confirm twice to resume",
                    ),
                )
                .child(view.global_agent_history_resume_profile_input.clone());
                card = card.child(
                    div()
                        .id(("global-history-exact-resume", index))
                        .text_xs()
                        .cursor_pointer()
                        .text_color(rgb(if busy || !consented { MUTED } else { TEXT }))
                        .child(if armed {
                            "Confirm exact Resume of this Session"
                        } else {
                            "Resume exact Session…"
                        })
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.resume_global_agent_history(index, cx);
                        })),
                );
            }
        }
        results = results.child(card);
    }
    list.child(results).into_any_element()
}

/// Overlay above the center Dashboard. This is a real, separately polled VT
/// viewer for the original PTY; it never selects a different Workspace/Tab.
/// The 32px exterior gutter is the only outside-dismiss click target.
pub(super) fn inspector_overlay(view: &Workbench, cx: &mut Context<Workbench>) -> gpui::AnyElement {
    let Some(state) = view.global_agent_inspector.as_ref() else {
        return div().hidden().into_any_element();
    };
    let target = &state.target;
    let verified =
        !view.terminals.session_list_is_stale() && target.is_live(&view.terminals.sessions);
    let session = view.terminals.sessions.iter().find(|session| {
        session.handle == target.handle
            && session.workspace_id == target.workspace_id
            && session.tab_id == target.tab_id
            && session.agent_type.as_deref() == Some(target.agent_type.as_str())
    });
    let (goal, progress, timeline) = session
        .and_then(|session| session.agent_context.as_ref())
        .map(|context| {
            (
                context.goal.clone(),
                context.progress.clone(),
                context.timeline.clone(),
            )
        })
        .unwrap_or_default();
    let mut summary = div()
        .w(px(300.0))
        .flex_shrink_0()
        .min_h_0()
        .border_l_1()
        .border_color(rgb(BORDER))
        .p_3()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child("Overall objective"),
        )
        .child(div().text_sm().child(if goal.is_empty() {
            "No verified goal reported".to_owned()
        } else {
            goal
        }))
        .child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child("Latest progress"),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(MUTED))
                .child(if progress.is_empty() {
                    "No verified progress reported".to_owned()
                } else {
                    progress
                }),
        );
    if !timeline.is_empty() {
        summary = summary.child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(format!("Progress timeline ({})", timeline.len())),
        );
        let mut events = div()
            .id("global-inspector-timeline-events")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_2();
        for (index, event) in timeline.iter().take(12).enumerate() {
            events = events.child(
                div()
                    .id(("global-inspector-timeline", index))
                    .p_2()
                    .rounded_md()
                    .bg(rgb(ELEVATED))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(event.at.clone()),
                    )
                    .child(div().text_xs().child(event.text.clone())),
            );
        }
        summary = summary.child(events);
    }

    let mut screen = div()
        .id("global-agent-inspector-terminal")
        .relative()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .overflow_x_scroll()
        .overflow_y_scroll()
        .p_3()
        .font_family("JetBrains Mono")
        .text_size(px(13.5))
        .bg(rgb(BG))
        .key_context("AleraGpuiTerminalRaw")
        .track_focus(&view.global_agent_inspector_focus)
        .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, _, cx| {
            // Escape is a PTY key, never a modal close shortcut. Suppress
            // bubbling even on a temporarily stale Host identity.
            let is_escape = event.keystroke.key.eq_ignore_ascii_case("escape");
            if view.send_global_agent_inspector_key(event, cx) || is_escape {
                cx.stop_propagation();
            }
        }))
        .on_mouse_down(
            gpui::MouseButton::Left,
            cx.listener(|view, _, window, cx| {
                window.focus(
                    &view.global_agent_inspector_input.read(cx).focus_handle(cx),
                    cx,
                );
            }),
        )
        .on_mouse_up(
            gpui::MouseButton::Left,
            cx.listener(|view, _, window, cx| {
                window.focus(
                    &view.global_agent_inspector_input.read(cx).focus_handle(cx),
                    cx,
                );
            }),
        )
        .child(
            div()
                .absolute()
                .left(px(1.0))
                .top(px(1.0))
                .w(px(1.0))
                .h(px(1.0))
                .child(view.global_agent_inspector_input.clone()),
        );

    for (row_index, row) in state.screen.styled_rows().into_iter().enumerate() {
        let text =
            gpui::StyledText::new(row.text).with_highlights(row.spans.into_iter().map(|span| {
                let style = span.style;
                (
                    span.range,
                    gpui::HighlightStyle {
                        color: Some(rgb(style.fg).into()),
                        background_color: Some(rgb(style.bg).into()),
                        font_weight: style.bold.then_some(gpui::FontWeight::BOLD),
                        font_style: style.italic.then_some(gpui::FontStyle::Italic),
                        underline: style.underline.then_some(gpui::UnderlineStyle {
                            thickness: px(1.0),
                            color: Some(rgb(style.fg).into()),
                            wavy: false,
                        }),
                        ..Default::default()
                    },
                )
            }));
        screen = screen.child(
            div()
                .id(("global-agent-inspector-row", row_index))
                .whitespace_nowrap()
                .line_height(px(17.5))
                .child(text),
        );
    }

    let status = state.status.clone();
    let target_label = format!(
        "{} / {} · {}",
        target.workspace_id, target.tab_id, target.agent_type
    );
    div()
        .id("global-agent-inspector-backdrop")
        .absolute()
        .inset_0()
        .p_8()
        .bg(rgb(PANEL))
        .flex()
        .items_center()
        .justify_center()
        .on_click(cx.listener(|view, _, _, cx| {
            view.close_global_agent_inspector(cx);
        }))
        .child(
            div()
                .id("global-agent-inspector-dialog")
                .w_full()
                .h_full()
                .min_h_0()
                .rounded_lg()
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(BG))
                .flex()
                .flex_col()
                .on_click(cx.listener(|_, _, _, cx| {
                    cx.stop_propagation();
                }))
                .child(
                    div()
                        .h(px(54.0))
                        .px_3()
                        .flex()
                        .items_center()
                        .gap_3()
                        .border_b_1()
                        .border_color(rgb(BORDER))
                        .child(
                            div()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .child(format!("Live Agent Terminal · {target_label}")),
                        )
                        .child(div().flex_1())
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(if verified { TEXT } else { MUTED }))
                                .child(if verified {
                                    "Verified Host session"
                                } else {
                                    "Stale · read-only"
                                }),
                        )
                        .child(
                            div()
                                .id("global-agent-inspector-close")
                                .cursor_pointer()
                                .text_sm()
                                .font_weight(gpui::FontWeight::BOLD)
                                .child("× Close")
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.close_global_agent_inspector(cx);
                                })),
                        ),
                )
                .child(
                    div()
                        .w_full()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .child(screen)
                        .child(summary),
                )
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .border_t_1()
                        .border_color(rgb(BORDER))
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(status),
                ),
        )
        .into_any_element()
}

pub(super) fn panel(view: &Workbench, cx: &mut Context<Workbench>) -> gpui::AnyElement {
    if view.global_agent_history_open {
        return history_panel(view, cx);
    }
    let fresh = !view.terminals.session_list_is_stale();
    let search = view
        .global_agent_search_input
        .read(cx)
        .query()
        .trim()
        .to_lowercase();
    let needs_review_count = view
        .terminals
        .sessions
        .iter()
        .filter(|session| {
            alera_gpui_desktop::agent_status_visual::AgentVisualState::needs_host_review(
                session, fresh,
            )
        })
        .count();
    let mut sessions = view
        .terminals
        .sessions
        .iter()
        .filter(|session| {
            session.running
                && session.agent_type.is_some()
                && (!view.global_agent_needs_review_only
                    || alera_gpui_desktop::agent_status_visual::AgentVisualState::needs_host_review(
                        session, fresh,
                    ))
                && (view.global_agent_needs_review_only
                    || view.global_agent_show_hidden
                    || !view.global_agent_hidden.contains(&(
                        session.handle.clone(),
                        session.workspace_id.clone(),
                        session.tab_id.clone(),
                    )))
        })
        .filter(|session| {
            if search.is_empty() {
                return true;
            }
            let catalog = view.runtime_catalog.as_ref();
            let workspace = catalog.and_then(|catalog| {
                catalog
                    .workspaces
                    .iter()
                    .find(|workspace| workspace.id == session.workspace_id)
            });
            let project = workspace.and_then(|workspace| {
                catalog.and_then(|catalog| {
                    catalog
                        .projects
                        .iter()
                        .find(|project| project.id == workspace.project_id)
                })
            });
            let context = session.agent_context.as_ref();
            let text = format!(
                "{} {} {} {} {}",
                project.map(|p| p.name.as_str()).unwrap_or(""),
                workspace.map(|w| w.name.as_str()).unwrap_or(""),
                session.agent_type.as_deref().unwrap_or(""),
                context.map(|c| c.goal.as_str()).unwrap_or(""),
                context.map(|c| c.progress.as_str()).unwrap_or("")
            )
            .to_lowercase();
            search.split_whitespace().all(|term| text.contains(term))
        })
        .take(128)
        .cloned()
        .collect::<Vec<_>>();
    // Match Flutter Kanban ordering: status columns, then persisted Host Tab
    // pins at the top of their own column, then most recently updated Agents.
    sort_dashboard_sessions(&mut sessions, &view.host_pin_snapshots);
    sort_dashboard_done_peers(&mut sessions, &view.agent_completion_acknowledgements);

    let mut create_form = div().h(px(0.0)).into_any_element();
    if view.global_agent_create_open {
        let mut form = div()
            .id("global-agent-create-form")
            .flex()
            .flex_col()
            .gap_2()
            .p_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .id("global-agent-refresh-launch-choices")
                    .cursor_pointer()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child("Refresh choices ↻")
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.refresh_mcp_host_workspaces(cx);
                        view.refresh_installed_agent_clis(cx);
                        view.refresh_agent_profiles(cx);
                    })),
            );
        // The RuntimeStore owns Project membership. Only catalog workspaces
        // also registered in the live Host are offered for launch.
        let catalog = view.runtime_catalog.as_ref();
        let registered = |id: &str| view.mcp_host_workspaces.iter().any(|w| w.id == id);
        let projects: Vec<(String, String)> = catalog
            .map(|catalog| {
                catalog
                    .projects
                    .iter()
                    .filter(|project| {
                        catalog.workspaces.iter().any(|workspace| {
                            !workspace.is_archived
                                && workspace.project_id == project.id
                                && registered(&workspace.id)
                        })
                    })
                    .map(|project| (project.id.clone(), project.name.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let workspaces: Vec<(String, String)> = catalog
            .into_iter()
            .flat_map(|catalog| catalog.workspaces.iter())
            .filter(|workspace| {
                !workspace.is_archived
                    && registered(&workspace.id)
                    && view.global_agent_create_project.as_deref()
                        == Some(workspace.project_id.as_str())
            })
            .map(|workspace| (workspace.id.clone(), workspace.name.clone()))
            .collect();
        let agents = dashboard_installed_choices(&view.installed_agent_clis);
        let mut profiles = vec![(String::new(), "Default (same as New Tab)".to_owned())];
        profiles.extend(
            view.agent_profiles
                .iter()
                .filter(|profile| {
                    view.global_agent_create_agent
                        .as_deref()
                        .is_some_and(|agent| {
                            alera_core::agent_descriptor::canonical_agent_id(&profile.agent_type)
                                == Some(agent)
                        })
                })
                .map(|profile| (profile.id.clone(), profile.name.clone())),
        );
        let locked = view.global_agent_create_busy || view.global_agent_create_journal.is_some();
        let stage_count = if profiles.len() > 1 || view.global_agent_create_profile.is_some() {
            4
        } else {
            3
        };
        let selected = [
            view.global_agent_create_project.as_deref(),
            view.global_agent_create_workspace.as_deref(),
            view.global_agent_create_agent.as_deref(),
            Some(view.global_agent_create_profile.as_deref().unwrap_or("")),
        ];
        let choices = [&projects, &workspaces, &agents, &profiles];
        let labels = ["Project", "Workspace", "Agent", "Profile (optional)"];
        let mut selectors = div().w_full().flex().flex_wrap().gap_2();
        for stage in 0..stage_count {
            let current = choices[stage]
                .iter()
                .find(|(id, _)| Some(id.as_str()) == selected[stage]);
            let enabled = !locked
                && (stage != 2 || !view.installed_agent_clis_loading)
                && (stage != 3 || !view.agent_profiles_loading)
                && (stage == 0
                    || choices[stage - 1]
                        .iter()
                        .any(|(id, _)| Some(id.as_str()) == selected[stage - 1]));
            selectors = selectors.child(
                div()
                    .id(("global-create-select", stage))
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .px_2()
                    .py_1()
                    .text_xs()
                    .text_color(rgb(if enabled { TEXT } else { MUTED }))
                    .when(enabled, |element| element.cursor_pointer())
                    .child(format!(
                        "{}: {} ▾",
                        labels[stage],
                        current.map(|(_, name)| name.as_str()).unwrap_or("Select")
                    ))
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if !enabled
                            || view.global_agent_create_busy
                            || view.global_agent_create_journal.is_some()
                        {
                            return;
                        }
                        let stage = stage as u8;
                        if view.global_agent_create_picker == Some(stage) {
                            view.global_agent_create_picker = None;
                        } else {
                            view.global_agent_create_picker = Some(stage);
                            view.global_agent_create_filter_input
                                .update(cx, |input, cx| input.clear(cx));
                            window.focus(
                                &view
                                    .global_agent_create_filter_input
                                    .read(cx)
                                    .focus_handle(cx),
                                cx,
                            );
                        }
                        cx.notify();
                    })),
            );
        }
        form = form.child(selectors);
        if let Some(stage) = view
            .global_agent_create_picker
            .filter(|stage| (*stage as usize) < stage_count)
        {
            let query = view
                .global_agent_create_filter_input
                .read(cx)
                .query()
                .trim()
                .to_lowercase();
            let mut options = div()
                .id("global-create-options")
                .max_h(px(150.0))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_1();
            let mut count = 0;
            for (index, (id, name)) in choices[stage as usize].iter().enumerate() {
                if !query
                    .split_whitespace()
                    .all(|term| name.to_lowercase().contains(term))
                {
                    continue;
                }
                count += 1;
                let id = id.clone();
                let current = selected[stage as usize] == Some(id.as_str());
                options = options.child(
                    div()
                        .id(("global-create-option", index))
                        .cursor_pointer()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .min_w_0()
                        .truncate()
                        .text_xs()
                        .bg(rgb(if current { ELEVATED } else { BG }))
                        .child(name.clone())
                        .on_click(cx.listener(move |view, _, _, cx| {
                            if view.global_agent_create_busy
                                || view.global_agent_create_journal.is_some()
                            {
                                return;
                            }
                            match stage {
                                0 => {
                                    view.global_agent_create_project = Some(id.clone());
                                    view.global_agent_create_workspace = None;
                                    view.global_agent_create_agent = None;
                                    view.global_agent_create_profile = None;
                                }
                                1 => {
                                    view.global_agent_create_workspace = None;
                                    view.global_agent_create_agent = None;
                                    view.global_agent_create_profile = None;
                                    view.select_global_agent_create_workspace(&id, cx);
                                }
                                2 => {
                                    view.global_agent_create_agent = Some(id.clone());
                                    view.global_agent_create_profile = None;
                                }
                                _ => {
                                    view.global_agent_create_profile =
                                        (!id.is_empty()).then(|| id.clone())
                                }
                            }
                            view.global_agent_create_prompt_input
                                .update(cx, |input, cx| input.clear(cx));
                            view.global_agent_create_picker = None;
                            cx.notify();
                        })),
                );
            }
            if count == 0 {
                options = options.child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child("No matching choices"),
                );
            }
            form = form
                .child(view.global_agent_create_filter_input.clone())
                .child(options);
        }
        if view.installed_agent_clis_loading || agents.is_empty() {
            form = form.child(div().text_xs().text_color(rgb(MUTED)).child(
                if view.installed_agent_clis_loading {
                    "Detecting installed Agents…"
                } else {
                    "No installed Agents detected. Check Settings or refresh choices."
                },
            ));
        }
        if view.global_agent_create_profile.is_some() {
            form = form
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child("Initial task (Profile's Host credentials and settings apply)"),
                )
                .child(view.global_agent_create_prompt_input.clone());
        }
        form = form
            .child(
                div()
                    .id("global-agent-create-submit")
                    .cursor_pointer()
                    .text_xs()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(if view.global_agent_create_busy {
                        MUTED
                    } else {
                        TEXT
                    }))
                    .child(if view.global_agent_create_busy {
                        "Launching Agent…"
                    } else {
                        "Launch in selected Worktree"
                    })
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.launch_global_agent(cx);
                    })),
            )
            .child(
                div()
                    .id("global-agent-create-status")
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(view.global_agent_create_status.clone()),
            );
        if view.global_agent_create_journal.is_some() {
            form = form
                .child(
                    div()
                        .id("global-agent-create-recheck")
                        .cursor_pointer()
                        .text_xs()
                        .child("Recheck existing Host sessions (no duplicate launch)")
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.global_agent_create_ack_armed = false;
                            view.refresh_terminals(cx);
                        })),
                )
                .child(
                    div()
                        .id("global-agent-create-acknowledge")
                        .cursor_pointer()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(if view.global_agent_create_ack_armed {
                            "Confirm acknowledge (does NOT stop PTY)"
                        } else {
                            "Acknowledge uncertain launch after inspection"
                        })
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.acknowledge_global_agent_create(cx);
                        })),
                );
        }
        form = form.child(
            div()
                .id("global-agent-create-close")
                .cursor_pointer()
                .text_xs()
                .text_color(rgb(MUTED))
                .child("Hide creator (keeps pending Host operation)")
                .on_click(cx.listener(|view, _, _, cx| {
                    view.global_agent_create_open = false;
                    cx.notify();
                })),
        );
        create_form = form.into_any_element();
    }

    let list = div()
        .id("global-live-agent-sessions")
        .when(view.global_agent_maximized, |list| list.flex_1().min_h_0())
        .when(!view.global_agent_maximized, |list| {
            list.h(px(if view.global_agent_create_open {
                580.0
            } else {
                340.0
            }))
            .flex_shrink_0()
        })
        .when(!view.global_agent_maximized, |panel| {
            panel.overflow_y_scroll()
        })
        .px_3()
        .py_2()
        .flex()
        .flex_col()
        .gap_2()
        .border_b_1()
        .border_color(rgb(BORDER))
        // One compact tool row; column headers own the status/counts.
        .child(
            div()
                .id("global-agent-toolbar")
                .w_full()
                .min_h(px(36.0))
                .flex()
                .items_center()
                .gap_3()
                .border_b_1()
                .border_color(rgb(BORDER))
                .child(
                    div()
                        .text_sm()
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(format!("Agents · {}", sessions.len())),
                )
                .child(
                    div()
                        .w(px(180.0))
                        .min_w_0()
                        .child(view.global_agent_search_input.clone()),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .id("global-agent-toolbar-actions")
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap_2()
                        .overflow_x_scroll()
                        .child(
                            div()
                                .id("global-agent-refresh")
                                .cursor_pointer()
                                .px_2()
                                .py_2()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child("↻")
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.refresh_terminals(cx);
                                    view.refresh_host_tab_pins(cx);
                                })),
                        )
                        .child(
                            div()
                                .id("global-agent-needs-review-toggle")
                                .cursor_pointer()
                                .px_2()
                                .py_2()
                                .rounded_md()
                                .bg(rgb(if view.global_agent_needs_review_only {
                                    ELEVATED
                                } else {
                                    BG
                                }))
                                .text_xs()
                                .text_color(rgb(if view.global_agent_needs_review_only {
                                    TEXT
                                } else {
                                    MUTED
                                }))
                                .child(if view.global_agent_needs_review_only {
                                    "All".to_owned()
                                } else {
                                    format!("Needs you · {needs_review_count}")
                                })
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.global_agent_needs_review_only =
                                        !view.global_agent_needs_review_only;
                                    view.global_agent_collapsed_groups.clear();
                                    view.close_original_conversation();
                                    view.global_agent_control_armed = None;
                                    view.global_agent_target = None;
                                    cx.notify();
                                })),
                        )
                        .when(!view.global_agent_hidden.is_empty(), |actions| {
                            actions.child(
                                div()
                                    .id("global-agent-show-hidden")
                                    .cursor_pointer()
                                    .px_2()
                                    .py_2()
                                    .rounded_md()
                                    .text_xs()
                                    .text_color(rgb(if view.global_agent_show_hidden {
                                        TEXT
                                    } else {
                                        MUTED
                                    }))
                                    .child(if view.global_agent_show_hidden {
                                        "Visible".to_owned()
                                    } else {
                                        format!("Hidden · {}", view.global_agent_hidden.len())
                                    })
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.global_agent_show_hidden =
                                            !view.global_agent_show_hidden;
                                        cx.notify();
                                    })),
                            )
                        })
                        .child(
                            div()
                                .id("global-agent-open-history")
                                .cursor_pointer()
                                .px_2()
                                .py_2()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child("History")
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.global_agent_history_open = true;
                                    view.global_agent_history_filters_expanded = false;
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .id("global-agent-launch-current-workspace")
                                .cursor_pointer()
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .bg(rgb(ELEVATED))
                                .text_xs()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .child("+ Agent")
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.open_global_agent_create(cx);
                                })),
                        ),
                ),
        )
        .when(view.global_agent_pin_feedback.is_some(), |panel| {
            panel.child(
                div()
                    .id("global-agent-pin-feedback")
                    .max_w(px(560.0))
                    .truncate()
                    .text_xs()
                    .text_color(rgb(if view.global_agent_pin_pending.is_some() {
                        MUTED
                    } else {
                        TEXT
                    }))
                    .child(view.global_agent_pin_feedback.clone().unwrap_or_default()),
            )
        })
        .child(create_form)
        .when(!view.global_agent_control_status.is_empty(), |panel| {
            panel.child(
                div()
                    .max_w(px(560.0))
                    .truncate()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(view.global_agent_control_status.clone()),
            )
        });

    if !fresh {
        return list
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(MUTED))
                    .child("Host inventory is stale. Refresh before focusing an Agent."),
            )
            .into_any_element();
    }

    if sessions.is_empty() {
        return list
            .child(div().text_sm().text_color(rgb(MUTED)).child(
                if view.global_agent_needs_review_only {
                    "No verified Waiting/Blocked Agents need review."
                } else {
                    "No running Agent sessions in the connected Host."
                },
            ))
            .into_any_element();
    }

    let mut sections: Vec<(u8, String, usize, Vec<gpui::AnyElement>)> = Vec::new();
    let mut last_group = None;
    for (index, session) in sessions.iter().enumerate() {
        let (group_rank, group_label) =
            alera_gpui_desktop::agent_status_visual::dashboard_status_group(
                session.agent_state,
                session.interrupted,
            );
        if last_group != Some(group_rank) {
            let count = sessions
                .iter()
                .filter(|item| {
                    alera_gpui_desktop::agent_status_visual::dashboard_status_group(
                        item.agent_state,
                        item.interrupted,
                    )
                    .0 == group_rank
                })
                .count();
            sections.push((group_rank, group_label.to_owned(), count, Vec::new()));
            last_group = Some(group_rank);
        }
        if view.global_agent_collapsed_groups.contains(&group_rank) {
            continue;
        }
        let agent = session.agent_type.clone().unwrap_or_default();
        let goal = session
            .agent_context
            .as_ref()
            .map(|context| context.goal.trim())
            .filter(|text| !text.is_empty())
            .unwrap_or("No overall goal reported")
            .to_owned();
        let progress = session
            .agent_context
            .as_ref()
            .map(|context| context.progress.trim())
            .filter(|text| !text.is_empty())
            .unwrap_or("No progress report")
            .to_owned();
        let session_id = session.handle.clone();
        let workspace_id = session.workspace_id.clone();
        let tab_id = session.tab_id.clone();
        // Use the shared RuntimeStore catalog, never raw Host UUIDs as the
        // primary card title. This matches Flutter's Project / Worktree labels.
        let (project_label, workspace_label) = view
            .runtime_catalog
            .as_ref()
            .and_then(|catalog| {
                catalog
                    .workspaces
                    .iter()
                    .find(|workspace| workspace.id == workspace_id)
                    .map(|workspace| {
                        let project = catalog
                            .projects
                            .iter()
                            .find(|project| project.id == workspace.project_id)
                            .map(|project| project.name.clone())
                            .unwrap_or_else(|| workspace.name.clone());
                        (project, workspace.name.clone())
                    })
            })
            .unwrap_or_else(|| {
                let short_id: String = workspace_id.chars().take(8).collect();
                (agent.clone(), format!("Worktree {short_id}"))
            });
        let action_identity = (session_id.clone(), workspace_id.clone(), tab_id.clone());
        let done_generation = dashboard_completion(session).map(str::to_owned);
        let done_unread = dashboard_done_unread(session, &view.agent_completion_acknowledgements);
        let receipt_identity = action_identity.clone();
        let receipt_generation = done_generation.clone();
        let more_actions_visible =
            view.global_agent_actions_expanded.as_ref() == Some(&action_identity);
        let more_actions_key = action_identity.clone();
        let agent_type = agent.clone();
        let inspect_target = GlobalAgentPromptTarget {
            handle: session_id.clone(),
            workspace_id: workspace_id.clone(),
            tab_id: tab_id.clone(),
            agent_type: agent_type.clone(),
        };
        let card_id = session_id.clone();
        let card_workspace = workspace_id.clone();
        let card_tab = tab_id.clone();
        let is_hidden = view.global_agent_hidden.contains(&(
            card_id.clone(),
            card_workspace.clone(),
            card_tab.clone(),
        ));
        let is_pinned = dashboard_session_is_pinned(session, &view.host_pin_snapshots);
        let pin_pending =
            view.global_agent_pin_pending
                .as_ref()
                .is_some_and(|(handle, owner, tab)| {
                    handle == &session_id && owner == &workspace_id && tab == &tab_id
                });
        let pin_target = GlobalAgentPromptTarget {
            handle: session_id.clone(),
            workspace_id: workspace_id.clone(),
            tab_id: tab_id.clone(),
            agent_type: agent.clone(),
        };
        let interrupt_target = GlobalAgentPromptTarget {
            handle: session_id.clone(),
            workspace_id: workspace_id.clone(),
            tab_id: tab_id.clone(),
            agent_type: agent.clone(),
        };
        let terminate_target = interrupt_target.clone();
        let interrupt_armed =
            view.global_agent_control_armed.as_ref() == Some(&(interrupt_target.clone(), false));
        let terminate_armed =
            view.global_agent_control_armed.as_ref() == Some(&(terminate_target.clone(), true));
        let control_busy = view.global_agent_control_busy;
        let timeline_identity = (session_id.clone(), workspace_id.clone(), tab_id.clone());
        let timeline_expanded =
            view.global_agent_expanded_timeline.as_ref() == Some(&timeline_identity);
        let timeline_events = session
            .agent_context
            .as_ref()
            .map(|context| context.timeline.clone())
            .unwrap_or_default();
        let mut timeline_panel = div().flex().flex_col().gap_1();
        if !timeline_events.is_empty() {
            timeline_panel = timeline_panel.child(
                div()
                    .id(("global-agent-timeline-toggle", index))
                    .cursor_pointer()
                    .text_xs()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(format!(
                        "{} Timeline ({})",
                        if timeline_expanded { "▾" } else { "▸" },
                        timeline_events.len()
                    ))
                    .on_click(cx.listener(move |view, _, _, cx| {
                        if view.global_agent_expanded_timeline.as_ref() == Some(&timeline_identity)
                        {
                            view.global_agent_expanded_timeline = None;
                        } else {
                            view.global_agent_expanded_timeline = Some(timeline_identity.clone());
                        }
                        cx.notify();
                    })),
            );
            if timeline_expanded {
                for (event_index, event) in timeline_events.iter().take(16).enumerate() {
                    let label = match event.kind.as_str() {
                        "goal" => "Goal",
                        "instruction" => "Follow-up",
                        "report" => "Agent report",
                        _ => "Update",
                    };
                    timeline_panel = timeline_panel.child(
                        div()
                            .id(("global-agent-timeline-item", index * 16 + event_index))
                            .rounded_md()
                            .bg(rgb(PANEL))
                            .px_2()
                            .py_2()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(MUTED))
                                    .child(format!("{label} · {}", event.at)),
                            )
                            .child(div().min_w_0().text_xs().child(event.text.clone())),
                    );
                }
            }
        }

        let original_target = GlobalAgentPromptTarget {
            handle: session_id.clone(),
            workspace_id: workspace_id.clone(),
            tab_id: tab_id.clone(),
            agent_type: agent.clone(),
        };
        let selected_original =
            view.global_agent_transcript_target.as_ref() == Some(&original_target);
        let original_button_target = original_target.clone();
        let older_button_target = original_target.clone();
        let latest_button_target = original_target.clone();
        let mut original_panel = div().flex().flex_col().gap_2();
        if selected_original {
            original_panel = original_panel
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .child("Original provider conversation · read only"),
                        )
                        .child(div().flex_1())
                        .child(
                            div()
                                .id(("global-agent-original-close", index))
                                .text_xs()
                                .cursor_pointer()
                                .child("Close ×")
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.close_original_conversation();
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(view.global_agent_transcript_status.clone()),
                );
            if view.global_agent_transcript_busy {
                original_panel =
                    original_panel.child(div().text_xs().text_color(rgb(MUTED)).child("Loading…"));
            }
            let mut messages = div()
                .id(("global-agent-original-list", index))
                .max_h(px(430.0))
                .min_h_0()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_2();
            for (message_index, message) in view.global_agent_transcript_messages.iter().enumerate()
            {
                let role = if message.role == "user" {
                    "You"
                } else {
                    "Agent"
                };
                messages =
                    messages.child(
                        div()
                            .id(("global-agent-original-message", index * 64 + message_index))
                            .w_full()
                            .min_w_0()
                            .px_2()
                            .py_2()
                            .rounded_md()
                            .bg(rgb(PANEL))
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_xs().text_color(rgb(MUTED)).child(
                                message.timestamp.as_deref().map_or_else(
                                    || role.to_owned(),
                                    |time| format!("{role} · {time}"),
                                ),
                            ))
                            .child(div().min_w_0().text_sm().child(message.text.clone())),
                    );
            }
            original_panel = original_panel.child(messages).child(
                div()
                    .flex()
                    .gap_3()
                    .child(
                        div()
                            .id(("global-agent-original-older", index))
                            .text_xs()
                            .cursor_pointer()
                            .text_color(rgb(
                                if view.global_agent_transcript_before.is_some()
                                    && !view.global_agent_transcript_busy
                                {
                                    TEXT
                                } else {
                                    MUTED
                                },
                            ))
                            .child("Load older native messages")
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.load_original_conversation(
                                    older_button_target.clone(),
                                    true,
                                    cx,
                                );
                            })),
                    )
                    .child(
                        div()
                            .id(("global-agent-original-latest", index))
                            .text_xs()
                            .cursor_pointer()
                            .child("Latest page")
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.load_original_conversation(
                                    latest_button_target.clone(),
                                    false,
                                    cx,
                                );
                            })),
                    ),
            );
        }

        sections.last_mut().expect("status section").3.push(
            div()
                .id(("global-agent-session-row", index))
                .w_full()
                .min_w_0()
                .rounded_md()
                .border_1()
                .border_color(rgb(if done_unread { 0x36936A } else { BORDER }))
                .bg(rgb(if done_unread { 0x20382D } else { ELEVATED }))
                .px_3()
                .py_3()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_sm()
                                .truncate()
                                .child(project_label),
                        )
                        .child(
                            div()
                                .id(("global-agent-pin-card", index))
                                .text_xs()
                                .cursor_pointer()
                                .text_color(rgb(if pin_pending { MUTED } else { TEXT }))
                                .font_weight(if is_pinned {
                                    gpui::FontWeight::BOLD
                                } else {
                                    gpui::FontWeight::NORMAL
                                })
                                .child(if pin_pending {
                                    "Saving…"
                                } else if is_pinned {
                                    "📌 Unpin"
                                } else {
                                    "📌 Pin"
                                })
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.toggle_dashboard_agent_pin(pin_target.clone(), cx);
                                })),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .truncate()
                        .child(format!(
                            "{} · {}{}",
                            workspace_label,
                            agent,
                            if done_unread {
                                " · ● Unread"
                            } else if done_generation.is_some() {
                                " · Read"
                            } else {
                                ""
                            }
                        )),
                )
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .text_sm()
                        .child(dashboard_card_preview(&goal, 360, 6)),
                )
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .text_sm()
                        .text_color(rgb(MUTED))
                        .child(dashboard_card_preview(&progress, 520, 8)),
                )
                .when(!timeline_events.is_empty(), |card| {
                    card.child(timeline_panel)
                })
                .when(selected_original, |card| card.child(original_panel))
                // Mount secondary actions only on demand; the default
                // footer stays a single row and reserves no hidden space.
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .id(("global-agent-inspect", index))
                                .cursor_pointer()
                                .text_xs()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .child("Inspect")
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    view.open_global_agent_inspector(
                                        inspect_target.clone(),
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            div()
                                .id(("global-agent-open", index))
                                .cursor_pointer()
                                .text_xs()
                                .child("Open Agent")
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    // Read fresh Host state again; never navigate from a stale
                                    // copied row or a recycled handle.
                                    if view.terminals.session_list_is_stale() {
                                        view.terminals.status =
                                            "Host Agent inventory is stale; refresh before opening"
                                                .into();
                                        cx.notify();
                                        return;
                                    }
                                    let current =
                                        view.terminals.sessions.iter().find(|candidate| {
                                            candidate.running
                                                && candidate.handle == session_id
                                                && candidate.workspace_id == workspace_id
                                                && candidate.tab_id == tab_id
                                                && candidate.agent_type.as_deref()
                                                    == Some(agent_type.as_str())
                                        });
                                    if current.is_some() {
                                        view.agent_navigation.begin(
                                            AgentNavigationTarget {
                                                session_id: session_id.clone(),
                                                workspace_id: workspace_id.clone(),
                                                tab_id: tab_id.clone(),
                                                agent_type: agent_type.clone(),
                                            },
                                            AgentNavigationSource::Dashboard,
                                        );
                                        // Keep the Dashboard visible until the Host's
                                        // exact Tab selection is confirmed by navigation.
                                        cx.notify();
                                    } else {
                                        view.terminals.status =
                                            "Global Agent selection no longer matches the Host"
                                                .into();
                                        cx.notify();
                                    }
                                })),
                        )
                        .child(div().flex_1())
                        .child(
                            div()
                                .id(("global-agent-more", index))
                                .cursor_pointer()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child(if more_actions_visible {
                                    "▴ Less"
                                } else {
                                    "⋯ More"
                                })
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.global_agent_control_armed = None;
                                    if view.global_agent_actions_expanded.as_ref()
                                        == Some(&more_actions_key)
                                    {
                                        view.global_agent_actions_expanded = None;
                                    } else {
                                        view.global_agent_actions_expanded =
                                            Some(more_actions_key.clone());
                                    }
                                    cx.notify();
                                })),
                        ),
                )
                .when(more_actions_visible, |card| {
                    card.child(
                        div()
                            .w_full()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .when(done_generation.is_some(), |actions| {
                                actions.child(
                                    div()
                                        .id(("global-agent-done-receipt", index))
                                        .cursor_pointer()
                                        .text_xs()
                                        .child(if done_unread {
                                            "Mark as read"
                                        } else {
                                            "Mark as unread"
                                        })
                                        .on_click(cx.listener(move |view, _, _, cx| {
                                            if view.terminals.session_list_is_stale() {
                                                return;
                                            }
                                            let current =
                                                view.terminals.sessions.iter().find(|session| {
                                                    dashboard_identity(session) == receipt_identity
                                                });
                                            if current.and_then(dashboard_completion)
                                                != receipt_generation.as_deref()
                                            {
                                                return;
                                            }
                                            if done_unread {
                                                if let Some(generation) = &receipt_generation {
                                                    view.agent_completion_acknowledgements.insert(
                                                        receipt_identity.clone(),
                                                        generation.clone(),
                                                    );
                                                }
                                            } else {
                                                view.agent_completion_acknowledgements
                                                    .remove(&receipt_identity);
                                            }
                                            cx.notify();
                                        })),
                                )
                            })
                            .when(matches!(agent.as_str(), "claude" | "codex"), |actions| {
                                actions.child(
                                    div()
                                        .id(("global-agent-original-open", index))
                                        .cursor_pointer()
                                        .text_xs()
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .child("Original conversation · read only")
                                        .on_click(cx.listener(move |view, _, _, cx| {
                                            view.load_original_conversation(
                                                original_button_target.clone(),
                                                false,
                                                cx,
                                            );
                                        })),
                                )
                            })
                            .child(
                                div()
                                    .id(("global-agent-hide-card", index))
                                    .cursor_pointer()
                                    .text_xs()
                                    .text_color(rgb(MUTED))
                                    .child(if is_hidden { "Show" } else { "Hide" })
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        let key = (
                                            card_id.clone(),
                                            card_workspace.clone(),
                                            card_tab.clone(),
                                        );
                                        if view.global_agent_hidden.contains(&key) {
                                            view.global_agent_hidden.remove(&key);
                                        } else {
                                            view.global_agent_hidden.insert(key.clone());
                                            view.global_agent_control_armed = None;
                                            // A card hidden from the dashboard cannot remain
                                            // an invisible destination for subsequent input.
                                            if view.global_agent_target.as_ref().is_some_and(
                                                |target| {
                                                    target.handle == key.0
                                                        && target.workspace_id == key.1
                                                        && target.tab_id == key.2
                                                },
                                            ) {
                                                view.global_agent_target = None;
                                                view.global_agent_submit_status =
                                                    "Hidden card; direct input target cleared"
                                                        .into();
                                            }
                                        }
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id(("global-agent-interrupt", index))
                                    .cursor_pointer()
                                    .text_xs()
                                    .text_color(rgb(if control_busy { MUTED } else { TEXT }))
                                    .child(if interrupt_armed {
                                        "Confirm Interrupt Turn (keeps PTY)"
                                    } else {
                                        "Interrupt Turn · confirmation required"
                                    })
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        view.confirm_global_agent_control(
                                            interrupt_target.clone(),
                                            false,
                                            cx,
                                        );
                                    })),
                            )
                            .child(
                                div()
                                    .id(("global-agent-terminate", index))
                                    .cursor_pointer()
                                    .text_xs()
                                    .text_color(rgb(if control_busy { MUTED } else { TEXT }))
                                    .child(if terminate_armed {
                                        "Confirm Terminate Session (ends PTY)"
                                    } else {
                                        "Terminate Session · confirmation required"
                                    })
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        view.confirm_global_agent_control(
                                            terminate_target.clone(),
                                            true,
                                            cx,
                                        );
                                    })),
                            ),
                    )
                })
                .into_any_element(),
        );
    }
    // A fixed-width independent scroll lane per current Hook state. More
    // statuses scroll sideways rather than squeezing cards into tiny tiles.
    let mut board = div()
        .id("global-agent-kanban-board")
        .w_full()
        .flex_1()
        .min_h_0()
        .overflow_x_scroll()
        .flex()
        .gap_3();
    for (rank, label, count, cards) in sections {
        let collapsed = view.global_agent_collapsed_groups.contains(&rank);
        let header = div()
            .id(("global-agent-state-section", rank as usize))
            .w_full()
            .px_3()
            .py_1()
            .cursor_pointer()
            .border_b_1()
            .border_color(rgb(BORDER))
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(format!("{} {}", if collapsed { "▸" } else { "▾" }, label)),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(format!("{count}")),
            )
            .on_click(cx.listener(move |view, _, _, cx| {
                if !view.global_agent_collapsed_groups.insert(rank) {
                    view.global_agent_collapsed_groups.remove(&rank);
                }
                view.global_agent_target = None;
                view.global_agent_control_armed = None;
                view.close_original_conversation();
                cx.notify();
            }));
        let mut column = div()
            .id(("global-agent-kanban-column", rank as usize))
            .w(px(if collapsed { 132.0 } else { 342.0 }))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .min_h_0()
            .rounded_lg()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(PANEL))
            .child(header);
        if !collapsed {
            column = column.child(
                div()
                    .id(("global-agent-status-cards", rank as usize))
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_2()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .children(cards),
            );
        }
        board = board.child(column);
    }
    list.child(board).into_any_element()
}
