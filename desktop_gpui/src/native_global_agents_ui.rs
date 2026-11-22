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
            provider: "codex".into(),
            account: "secondary".into(),
            project: "project".into(),
            workspace: "workspace".into(),
            native_id: "550e8400-e29b-41d4-a716-446655440001".into(),
        }
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
                    // Leaving History is an explicit newer focus choice, even
                    // when no Workspace or Tab was selected. Retire pending
                    // Host rechecks/deferred focus without discarding the
                    // acknowledgement of an in-flight explicit Resume.
                    view.agent_navigation.cancel();
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
                        view.global_agent_history_located.clear();
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
        .take(128)
        .cloned()
        .collect::<Vec<_>>();
    // One full-width Agent card per row, with stable state sections rather
    // than mixed chronological tiles. Keep original order within each section.
    sessions.sort_by_key(|session| {
        alera_gpui_desktop::agent_status_visual::dashboard_status_group(
            session.agent_state,
            session.interrupted,
        )
        .0
    });

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

    let mut list =
        div()
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
            .child(
                div()
                    .id("global-agent-needs-review-toggle")
                    .cursor_pointer()
                    .text_xs()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(if view.global_agent_needs_review_only {
                        "Show all Agent Sessions".to_owned()
                    } else {
                        format!("Needs You · review Waiting/Blocked ({needs_review_count})")
                    })
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.global_agent_needs_review_only = !view.global_agent_needs_review_only;
                        // A view change must not retain an invisible input target
                        // or a previously armed destructive action.
                        view.global_agent_control_armed = None;
                        view.global_agent_target = None;
                        view.global_agent_submit_status =
                            "Filter changed; select an Agent before sending".into();
                        cx.notify();
                    })),
            )
            .child(div().text_xs().text_color(rgb(MUTED)).child(
                "Host Hook state only · this is a review queue, not proof of pending approval",
            ))
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
            .child(div().text_sm().text_color(rgb(MUTED)).child(
                if view.global_agent_needs_review_only {
                    "No verified Waiting/Blocked Agents need review."
                } else {
                    "No running Agent sessions in the connected Host."
                },
            ))
            .into_any_element();
    }

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
            list = list.child(
                div()
                    .id(("global-agent-state-section", group_rank as usize))
                    .w_full()
                    .mt_2()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().font_weight(gpui::FontWeight::BOLD).child(group_label))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(format!("{count} Agents")),
                    ),
            );
            last_group = Some(group_rank);
        }
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

        list = list.child(
            div()
                .id(("global-agent-session-row", index))
                .w_full()
                .min_w_0()
                .rounded_md()
                .bg(rgb(ELEVATED))
                .px_2()
                .py_2()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .child(agent.clone()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child(format!("Workspace: {workspace_id}")),
                        )
                        .child(div().flex_1())
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child(state_text.clone()),
                        ),
                )
                .child(
                    div()
                        .w_full()
                        .flex()
                        .gap_3()
                        .child(div().min_w_0().flex_1().text_sm().truncate().child(goal))
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .truncate()
                                .child(progress),
                        ),
                )
                .child(timeline_panel)
                // Keep controls on one compact action row, wrapping only when
                // the application window is narrower than the Agent card.
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_3()
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
                                            "Agent identity is stale; refresh Host inventory"
                                                .into();
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
                                    let key =
                                        (card_id.clone(), card_workspace.clone(), card_tab.clone());
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
                                        // The selected original Tab must not remain behind this view.
                                        view.global_agent_maximized = false;
                                        cx.notify();
                                    } else {
                                        view.terminals.status =
                                            "Global Agent selection no longer matches the Host"
                                                .into();
                                        cx.notify();
                                    }
                                })),
                        ),
                ),
        );
    }
    list.into_any_element()
}
