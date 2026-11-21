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
    provider: String,
    account: String,
    project: String,
    workspace: String,
    native_id: String,
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
        let workspace = self
            .global_agent_history_workspace_input
            .read(cx)
            .query()
            .trim()
            .to_owned();
        if account.len() > 256 || workspace.len() > 256 {
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
        self.global_agent_history_status = "Scanning registered Worktrees; preview only…".into();
        cx.notify();
        cx.spawn(async move |view, cx| {
            let reply = cx
                .background_spawn(async move {
                    bridge.search_agent_history(
                        &text,
                        provider.as_deref(),
                        (!account.is_empty()).then_some(account.as_str()),
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
        let identity = format!(
            "{}|{}|{}|{}|{}|{}",
            hit.provider, hit.account, hit.project, hit.workspace, hit.native_id, profile
        );
        if self.global_agent_history_resume_armed.as_deref() != Some(identity.as_str()) {
            self.global_agent_history_resume_armed = Some(identity);
            self.global_agent_history_status =
                format!("Alera will select the configured {} account '{}', and revalidate its native history. Confirm exact Resume on the SAME result; the provider may reject incompatible credentials", hit.provider, hit.account);
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
        let mutation_id = uuid::Uuid::new_v4().to_string();
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
                        "Exact Resume Tab created by Host. Open it from Live Agents; do not retry while startup is pending".into()
                    }
                    _ => {
                        "Exact Resume rejected or acknowledgement ambiguous. Inspect Live Agents before retrying".into()
                    }
                };
                cx.notify();
            }).ok();
        }).detach();
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

/// Host response classification matters: a failed preflight cannot have
/// launched a PTY, while a lost launch acknowledgement might have done so.
enum GlobalAgentCreateReply {
    NotIssued(&'static str),
    Launched(
        LaunchedAgentTab,
        Option<Vec<alera_gpui_desktop::terminal::TerminalSession>>,
    ),
    OutcomeUnknown,
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
                "Select a Host-registered Workspace, Profile and initial task".into();
            self.refresh_mcp_host_workspaces(cx);
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
                    "Worktree selected. Choose a Host Profile and initial goal".into();
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
        if self.mcp_host_workspaces_loading || self.agent_profiles_loading {
            self.global_agent_create_status =
                "Host Workspace/Profile inventory is refreshing; try after it completes".into();
            cx.notify();
            return;
        }
        let Some(workspace_id) = self.global_agent_create_workspace.clone() else {
            self.global_agent_create_status = "Select a registered Worktree first".into();
            cx.notify();
            return;
        };
        let Some(profile_id) = self.global_agent_create_profile.clone() else {
            self.global_agent_create_status = "Select an Agent Profile first".into();
            cx.notify();
            return;
        };
        let Some(agent_type) = self
            .agent_profiles
            .iter()
            .find(|profile| profile.id == profile_id)
            .map(|profile| profile.agent_type.clone())
        else {
            self.global_agent_create_status =
                "Agent Profile changed; refresh before launching".into();
            cx.notify();
            return;
        };
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
        if input.composing() {
            self.global_agent_create_status = "Finish IME composition before launching".into();
            cx.notify();
            return;
        }
        let prompt = input.query().trim().to_owned();
        if prompt.is_empty() || prompt.len() > 8192 || prompt.contains('\0') {
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
                match bridge.launch_agent_profile(&profile_id, &workspace_id, &prompt) {
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
        if matching_agent_handle(created, sessions, expected).is_none() {
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

impl Workbench {
    fn submit_global_agent_prompt(&mut self, cx: &mut Context<Self>) {
        if self.global_agent_submit_busy || self.terminals.session_list_is_stale() {
            self.global_agent_submit_status =
                "Host inventory is stale or another send is in progress".into();
            cx.notify();
            return;
        }
        let Some(target) = self.global_agent_target.clone() else {
            self.global_agent_submit_status = "Choose a verified Agent row first".into();
            cx.notify();
            return;
        };
        if !target.is_live(&self.terminals.sessions) {
            self.global_agent_submit_status = "Agent identity changed. Select it again".into();
            self.global_agent_target = None;
            cx.notify();
            return;
        }
        let prompt = self.global_agent_prompt_input.read(cx).query().to_owned();
        if prompt.trim().is_empty() || prompt.len() > 4096 || prompt.contains('\0') {
            self.global_agent_submit_status = "Prompt must be nonempty and at most 4 KiB".into();
            cx.notify();
            return;
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.global_agent_submit_status = "Existing Runtime Host CLI is unavailable".into();
            cx.notify();
            return;
        };
        self.global_agent_submit_busy = true;
        self.global_agent_submit_status = "Sending to verified Host Agent…".into();
        cx.notify();
        cx.spawn(async move |view, cx| {
            let sent_target = target.clone();
            let sent_prompt = prompt.clone();
            let result = cx
                .background_spawn(async move {
                    bridge.submit_verified_agent_prompt(
                        &target.handle,
                        &target.workspace_id,
                        &target.tab_id,
                        &prompt,
                    )
                })
                .await;
            view.update(cx, |view, cx| {
                view.global_agent_submit_busy = false;
                if view.global_agent_target.as_ref() != Some(&sent_target) {
                    cx.notify();
                    return;
                }
                view.global_agent_submit_status = if result.is_ok() {
                    // Only clear the input if it wasn't edited during the send.
                    view.global_agent_prompt_input.update(cx, |input, cx| {
                        if input.query() == sent_prompt {
                            input.clear(cx);
                        }
                    });
                    "Host acknowledged Agent input".into()
                } else {
                    // The Host might have enqueued before a response was lost.
                    // Never retry automatically.
                    "Delivery unconfirmed. Refresh Host status before retrying".into()
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

fn history_panel(view: &Workbench, cx: &mut Context<Workbench>) -> gpui::AnyElement {
    let busy = view.global_agent_history_busy;
    let consented = view.global_agent_history_consent;
    let provider = view
        .global_agent_history_provider
        .as_deref()
        .unwrap_or("all");
    let mut list = div()
        .id("global-agent-history-panel")
        .h(px(590.0))
        .flex_shrink_0()
        .overflow_y_scroll()
        .px_3()
        .py_2()
        .flex()
        .flex_col()
        .gap_2()
        .border_b_1()
        .border_color(rgb(BORDER))
        .child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child("Historical Agent Sessions · local FTS5"),
        )
        .child(
            div()
                .id("global-history-return-to-live")
                .text_xs()
                .cursor_pointer()
                .text_color(rgb(TEXT))
                .child("← Back to live Agent dashboard")
                .on_click(cx.listener(|view, _, _, cx| {
                    view.global_agent_history_open = false;
                    cx.notify();
                })),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(MUTED))
                .child("No cloud indexing or automatic Agent resume. Searches match limited native user-message previews."),
        )
        .child(view.global_agent_history_input.clone())
        .child(
            div()
                .id("global-history-provider")
                .text_xs()
                .cursor_pointer()
                .child(format!("Provider: {provider} · click to filter"))
                .on_click(cx.listener(|view, _, _, cx| {
                    view.global_agent_history_provider =
                        match view.global_agent_history_provider.as_deref() {
                            None => Some("claude".into()),
                            Some("claude") => Some("codex".into()),
                            _ => None,
                        };
                    cx.notify();
                })),
        )
        .child(view.global_agent_history_account_input.clone())
        .child(view.global_agent_history_workspace_input.clone())
        .child(view.global_agent_history_resume_profile_input.clone())
        .child(
            div()
                .text_xs()
                .text_color(rgb(MUTED))
                .child("Exact Resume uses the verified source account's CLI config directory, including non-default accounts. Use an official CLI Profile ID; confirm the same result twice. No credentials are copied."),
        )
        .child(
            div()
                .id("global-history-consent")
                .text_xs()
                .cursor_pointer()
                .text_color(rgb(if consented { TEXT } else { MUTED }))
                .child(if consented {
                    "☑ I allow local indexing of limited user-message previews"
                } else {
                    "☐ Opt in: index limited user-message previews to a private local SQLite cache (not encrypted)"
                })
                .on_click(cx.listener(|view, _, _, cx| {
                    if view.global_agent_history_busy { return; }
                    view.global_agent_history_consent = !view.global_agent_history_consent;
                    if !view.global_agent_history_consent {
                        view.global_agent_history_results.clear();
                        view.global_agent_history_resume_armed = None;
                    }
                    cx.notify();
                })),
        )
        .child(
            div()
                .id("global-history-search")
                .text_xs()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .cursor_pointer()
                .text_color(rgb(if busy || !consented { MUTED } else { TEXT }))
                .child(if busy { "Searching…" } else { "Search prior Sessions (preview only)" })
                .on_click(cx.listener(|view, _, _, cx| {
                    view.search_global_agent_history(cx);
                })),
        )
        .child(
            div()
                .id("global-history-clear")
                .text_xs()
                .cursor_pointer()
                .text_color(rgb(MUTED))
                .child("Clear private search index · keep Claude/Codex transcripts")
                .on_click(cx.listener(|view, _, _, cx| {
                    view.clear_global_agent_history(cx);
                })),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(MUTED))
                .child(view.global_agent_history_status.clone()),
        );
    let profile = view
        .global_agent_history_resume_profile_input
        .read(cx)
        .query()
        .trim()
        .to_owned();
    for (index, hit) in view.global_agent_history_results.iter().enumerate() {
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
            .child(div().text_xs().text_color(rgb(MUTED)).child(
                "Preview only · Resume requires independent Host/provider identity verification",
            ));
        if !hit.account.is_empty() {
            let identity = format!(
                "{}|{}|{}|{}|{}|{}",
                hit.provider, hit.account, hit.project, hit.workspace, hit.native_id, profile
            );
            let armed =
                view.global_agent_history_resume_armed.as_deref() == Some(identity.as_str());
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
        list = list.child(card);
    }
    list.into_any_element()
}

pub(super) fn panel(view: &Workbench, cx: &mut Context<Workbench>) -> gpui::AnyElement {
    if view.global_agent_history_open {
        return history_panel(view, cx);
    }
    let fresh = !view.terminals.session_list_is_stale();
    let sessions = view
        .terminals
        .sessions
        .iter()
        .filter(|session| {
            session.running
                && session.agent_type.is_some()
                && (view.global_agent_show_hidden
                    || !view.global_agent_hidden.contains(&(
                        session.handle.clone(),
                        session.workspace_id.clone(),
                        session.tab_id.clone(),
                    )))
        })
        .take(128)
        .cloned()
        .collect::<Vec<_>>();

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
                    .text_xs()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("Launch Agent without switching your editor Workspace"),
            )
            .child(
                div()
                    .id("global-agent-refresh-launch-choices")
                    .cursor_pointer()
                    .text_xs()
                    .text_color(rgb(TEXT))
                    .child("Refresh Host Worktrees and Agent Profiles ↻")
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.refresh_mcp_host_workspaces(cx);
                        view.refresh_agent_profiles(cx);
                    })),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child("Host-registered Worktrees"),
            );

        let mut workspaces = div()
            .id("global-agent-create-workspaces")
            .max_h(px(90.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1();
        for (index, workspace) in view.mcp_host_workspaces.iter().take(96).enumerate() {
            let id = workspace.id.clone();
            let selected = view.global_agent_create_workspace.as_deref() == Some(id.as_str());
            let label = format!("{} · {}", workspace.path.display(), workspace.id);
            workspaces = workspaces.child(
                div()
                    .id(("global-create-workspace", index))
                    .cursor_pointer()
                    .text_xs()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(if selected { TEXT } else { MUTED }))
                    .child(format!("{} {label}", if selected { "●" } else { "○" }))
                    .on_click(cx.listener(move |view, _, _, cx| {
                        if !view.global_agent_create_busy
                            && view.global_agent_create_journal.is_none()
                        {
                            view.select_global_agent_create_workspace(&id, cx);
                        }
                    })),
            );
        }
        form = form.child(workspaces);
        if view.mcp_host_workspaces.is_empty() {
            form = form.child(div().text_xs().text_color(rgb(MUTED)).child(
                if view.mcp_host_workspaces_loading {
                    "Loading Host Workspaces…"
                } else {
                    "No registered Host Worktrees available. Refresh to retry."
                },
            ));
        }

        form = form.child(
            div()
                .text_xs()
                .text_color(rgb(MUTED))
                .child("Agent Profile"),
        );
        let mut profiles = div()
            .id("global-agent-create-profiles")
            .max_h(px(75.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1();
        for (index, profile) in view.agent_profiles.iter().take(96).enumerate() {
            let id = profile.id.clone();
            let selected = view.global_agent_create_profile.as_deref() == Some(id.as_str());
            let label = format!(
                "{} · {} ({})",
                profile.name,
                profile.agent_type,
                if profile.managed {
                    "Managed"
                } else {
                    "Command"
                }
            );
            profiles = profiles.child(
                div()
                    .id(("global-create-profile", index))
                    .cursor_pointer()
                    .text_xs()
                    .text_color(rgb(if selected { TEXT } else { MUTED }))
                    .child(format!("{} {label}", if selected { "●" } else { "○" }))
                    .on_click(cx.listener(move |view, _, _, cx| {
                        if !view.global_agent_create_busy
                            && view.global_agent_create_journal.is_none()
                        {
                            view.global_agent_create_profile = Some(id.clone());
                            cx.notify();
                        }
                    })),
            );
        }
        form = form
            .child(profiles)
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child("Initial task (Profile's Host credentials and settings apply)"),
            )
            .child(view.global_agent_create_prompt_input.clone())
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

    let mut list = div()
        .id("global-live-agent-sessions")
        .h(px(if view.global_agent_create_open {
            580.0
        } else {
            340.0
        }))
        .flex_shrink_0()
        .overflow_y_scroll()
        .px_3()
        .py_2()
        .flex()
        .flex_col()
        .gap_2()
        .border_b_1()
        .border_color(rgb(BORDER))
        .child(
            div()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_sm()
                .child(format!("All Agent Sessions ({})", sessions.len())),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(MUTED))
                .child("Global Host inventory, all registered workspaces"),
        )
        .child(
            div()
                .id("global-agent-open-history")
                .text_xs()
                .cursor_pointer()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child("Search historical Claude / Codex Sessions")
                .on_click(cx.listener(|view, _, _, cx| {
                    view.global_agent_history_open = true;
                    cx.notify();
                })),
        )
        .child(
            div()
                .id("global-agent-show-hidden")
                .text_xs()
                .cursor_pointer()
                .text_color(rgb(MUTED))
                .child(if view.global_agent_show_hidden {
                    format!(
                        "Show only visible cards ({} hidden)",
                        view.global_agent_hidden.len()
                    )
                } else {
                    format!("Show hidden cards ({})", view.global_agent_hidden.len())
                })
                .on_click(cx.listener(|view, _, _, cx| {
                    view.global_agent_show_hidden = !view.global_agent_show_hidden;
                    cx.notify();
                })),
        )
        .child(div().text_xs().text_color(rgb(MUTED)).child(
            view.global_agent_target.as_ref().map_or_else(
                || "Select an Agent's Steer button to target its live session".to_owned(),
                |target| format!("Target: {} / {}", target.workspace_id, target.tab_id),
            ),
        ))
        .child(
            div()
                .id("global-agent-launch-current-workspace")
                .cursor_pointer()
                .text_xs()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child("+ New Agent · choose any registered Worktree")
                .on_click(cx.listener(|view, _, _, cx| {
                    view.open_global_agent_create(cx);
                })),
        )
        .child(create_form)
        .child(view.global_agent_prompt_input.clone())
        .child(
            div()
                .text_xs()
                .text_color(rgb(MUTED))
                .child(view.global_agent_control_status.clone()),
        )
        .child(
            div()
                .id("global-agent-send-instruction")
                .cursor_pointer()
                .text_xs()
                .text_color(rgb(if view.global_agent_submit_busy {
                    MUTED
                } else {
                    TEXT
                }))
                .child(if view.global_agent_submit_busy {
                    "Sending…"
                } else {
                    "Send instruction to selected Agent"
                })
                .on_click(cx.listener(|view, _, _, cx| {
                    view.submit_global_agent_prompt(cx);
                })),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(MUTED))
                .child(view.global_agent_submit_status.clone()),
        );

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
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(MUTED))
                    .child("No running Agent sessions in the connected Host."),
            )
            .into_any_element();
    }

    for (index, session) in sessions.iter().enumerate() {
        let agent = session.agent_type.clone().unwrap_or_default();
        let state = session.agent_state;
        let state_text = if let Some(state) = state {
            if session.interrupted {
                "interrupted".to_owned()
            } else {
                state.label().to_owned()
            }
        } else {
            "state unavailable".to_owned()
        };
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
        let agent_type = agent.clone();
        let steer_id = session_id.clone();
        let steer_workspace = workspace_id.clone();
        let steer_tab = tab_id.clone();
        let steer_agent = agent_type.clone();
        let card_id = session_id.clone();
        let card_workspace = workspace_id.clone();
        let card_tab = tab_id.clone();
        let is_hidden = view.global_agent_hidden.contains(&(
            card_id.clone(),
            card_workspace.clone(),
            card_tab.clone(),
        ));
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

        list = list.child(
            div()
                .id(("global-agent-session-row", index))
                .rounded_md()
                .bg(rgb(ELEVATED))
                .px_2()
                .py_2()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_xs()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(format!("{agent} - {state_text}")),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(format!("Workspace: {workspace_id}")),
                )
                .child(div().text_sm().child(goal))
                .child(div().text_xs().text_color(rgb(MUTED)).child(progress))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(if state.is_some() {
                            "Click to open this Agent"
                        } else {
                            "Waiting for verified Agent state"
                        }),
                )
                .child(
                    div()
                        .id(("global-agent-select-steer", index))
                        .cursor_pointer()
                        .text_xs()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child("Steer · select target")
                        .on_click(cx.listener(move |view, _, _, cx| {
                            let target = GlobalAgentPromptTarget {
                                handle: steer_id.clone(),
                                workspace_id: steer_workspace.clone(),
                                tab_id: steer_tab.clone(),
                                agent_type: steer_agent.clone(),
                            };
                            if view.terminals.session_list_is_stale()
                                || !target.is_live(&view.terminals.sessions)
                            {
                                view.global_agent_target = None;
                                view.global_agent_submit_status =
                                    "Agent identity is stale; refresh Host inventory".into();
                            } else {
                                view.global_agent_target = Some(target);
                                view.global_agent_submit_status =
                                    "Target selected; type an instruction above".into();
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
                            view.confirm_global_agent_control(interrupt_target.clone(), false, cx);
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
                            view.confirm_global_agent_control(terminate_target.clone(), true, cx);
                        })),
                )
                .child(
                    div()
                        .id(("global-agent-hide-card", index))
                        .cursor_pointer()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(if is_hidden {
                            "Show this dashboard card"
                        } else {
                            "Hide card · keep Host Agent running"
                        })
                        .on_click(cx.listener(move |view, _, _, cx| {
                            let key = (card_id.clone(), card_workspace.clone(), card_tab.clone());
                            if view.global_agent_hidden.contains(&key) {
                                view.global_agent_hidden.remove(&key);
                            } else {
                                view.global_agent_hidden.insert(key.clone());
                                view.global_agent_control_armed = None;
                                // A card hidden from the dashboard cannot remain
                                // an invisible destination for subsequent input.
                                if view.global_agent_target.as_ref().is_some_and(|target| {
                                    target.handle == key.0
                                        && target.workspace_id == key.1
                                        && target.tab_id == key.2
                                }) {
                                    view.global_agent_target = None;
                                    view.global_agent_submit_status =
                                        "Hidden card; direct input target cleared".into();
                                }
                            }
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .id(("global-agent-open", index))
                        .cursor_pointer()
                        .text_xs()
                        .child("Open original Agent tab")
                        .on_click(cx.listener(move |view, _, window, cx| {
                            // Read fresh Host state again; never navigate from a stale
                            // copied row or a recycled handle.
                            if view.terminals.session_list_is_stale() {
                                view.terminals.status =
                                    "Host Agent inventory is stale; refresh before opening".into();
                                cx.notify();
                                return;
                            }
                            let current = view.terminals.sessions.iter().find(|candidate| {
                                candidate.running
                                    && candidate.handle == session_id
                                    && candidate.workspace_id == workspace_id
                                    && candidate.tab_id == tab_id
                                    && candidate.agent_type.as_deref() == Some(agent_type.as_str())
                            });
                            if let Some(current) = current {
                                if let Some(current_state) = current.agent_state {
                                    view.activate_agent_notice(
                                        AgentNotice {
                                            session_id: session_id.clone(),
                                            workspace_id: workspace_id.clone(),
                                            tab_id: tab_id.clone(),
                                            agent_type: agent_type.clone(),
                                            state: current_state,
                                        },
                                        window,
                                        cx,
                                    );
                                } else {
                                    view.terminals.status =
                                        "Agent state is unverified; refresh Host status".into();
                                    cx.notify();
                                }
                            } else {
                                view.terminals.status =
                                    "Global Agent selection no longer matches the Host".into();
                                cx.notify();
                            }
                        })),
                ),
        );
    }
    list.into_any_element()
}
