//! Pure, deterministic Agent Hook notification decisions for GPUI.
//! No OS callbacks or second Hook receiver: consume Host-confirmed presence only.

use crate::terminal::{AgentActivityState, TerminalSession};
use crate::workbench_panes::{PaneSlot, WorkbenchPaneLayout};
use crate::workbench_tabs::{WorkbenchTabIdentity, WorkbenchTabKind, WorkbenchTabs};
use std::collections::{HashMap, HashSet};

/// Match Classic's per-session/state throttle and burst batching policy.
pub const NOTIFICATION_COOLDOWN_MS: u64 = 60_000;
pub const NOTIFICATION_COALESCE_MS: u64 = 3_000;
pub const NOTIFICATION_MAX_COALESCE_MS: u64 = 10_000;
const MAX_NOTICE_SESSIONS: usize = 128;

/// Apply the runtime Host's per-Provider Hook switches before notification
/// tracking; absent/unknown/malformed configuration always means OFF.
/// Classic's Hook settings are independent of its portable notification
/// preferences. A running PTY alone is not proof Hooks are enabled.
pub fn enabled_notice_sessions(
    sessions: &[TerminalSession],
    hook_switches: Option<&HashMap<String, bool>>,
) -> Vec<TerminalSession> {
    let Some(enabled) = hook_switches else {
        return Vec::new();
    };
    sessions
        .iter()
        .filter(|session| {
            session.running
                && session
                    .agent_type
                    .as_deref()
                    .is_some_and(|name| enabled.get(&name.to_ascii_lowercase()) == Some(&true))
        })
        // A disabled provider may have hundreds of live terminals. Cap
        // *eligible* sessions only, or it can starve enabled Agents later
        // in the same authenticated Host snapshot.
        .take(MAX_NOTICE_SESSIONS)
        .cloned()
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct AgentIdentity {
    session_id: String,
    workspace_id: String,
    tab_id: String,
    agent_type: String,
}

impl AgentIdentity {
    fn of(session: &TerminalSession) -> Option<Self> {
        let agent_type = session.agent_type.as_ref()?;
        if !session.running {
            return None;
        }
        Some(Self {
            session_id: session.handle.clone(),
            workspace_id: session.workspace_id.clone(),
            tab_id: session.tab_id.clone(),
            agent_type: agent_type.clone(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentNotice {
    pub session_id: String,
    pub workspace_id: String,
    pub tab_id: String,
    pub agent_type: String,
    pub state: AgentActivityState,
}

/// UI navigation identity only. Unlike a Hook notification, focusing a
/// verified running Host Tab must not require an Agent state update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentNavigationTarget {
    pub session_id: String,
    pub workspace_id: String,
    pub tab_id: String,
    pub agent_type: String,
}

impl From<AgentNotice> for AgentNavigationTarget {
    fn from(notice: AgentNotice) -> Self {
        Self {
            session_id: notice.session_id,
            workspace_id: notice.workspace_id,
            tab_id: notice.tab_id,
            agent_type: notice.agent_type,
        }
    }
}
#[derive(Clone)]
struct ObservedAgent {
    identity: AgentIdentity,
    state: Option<AgentActivityState>,
    state_started_at: Option<String>,
}

/// Emits only *new* attention transitions in an existing, exact Host session.
/// The first snapshot is baseline only, so reconnect cannot replay every
/// already-waiting Agent as a desktop notification.
#[derive(Default)]
pub struct AgentNotificationTracker {
    initialized: bool,
    observed: HashMap<String, ObservedAgent>,
    last_notified_at: HashMap<(AgentIdentity, AgentActivityState), u64>,
}

impl AgentNotificationTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// `now_ms` is monotonic (provided by the caller), not Host wall time.
    /// This pure rule layer does not own OS notification delivery.
    pub fn observe(
        &mut self,
        sessions: &[TerminalSession],
        now_ms: u64,
        include_finished: bool,
        enabled: bool,
    ) -> Vec<AgentNotice> {
        let mut next = HashMap::new();
        let mut candidates = Vec::new();
        for session in sessions.iter().take(MAX_NOTICE_SESSIONS) {
            let Some(identity) = AgentIdentity::of(session) else {
                continue;
            };
            let state = session.agent_state;
            if self.initialized && enabled {
                let prior = self.observed.get(&identity.session_id);
                let changed_in_same_session = prior.is_some_and(|old| {
                    old.identity == identity
                        && (old.state != state
                            || old
                                .state_started_at
                                .as_deref()
                                .zip(session.agent_state_started_at.as_deref())
                                .is_some_and(|(before, after)| before != after))
                });
                if changed_in_same_session {
                    if let Some(state) = state.filter(|state| {
                        matches!(
                            state,
                            AgentActivityState::Waiting | AgentActivityState::Blocked
                        ) || (*state == AgentActivityState::Done
                            && include_finished
                            && !session.interrupted)
                    }) {
                        let key = (identity.clone(), state);
                        let cooled_down = self.last_notified_at.get(&key).is_none_or(|last| {
                            now_ms.saturating_sub(*last) >= NOTIFICATION_COOLDOWN_MS
                        });
                        if cooled_down {
                            self.last_notified_at.insert(key, now_ms);
                            candidates.push(AgentNotice {
                                session_id: identity.session_id.clone(),
                                workspace_id: identity.workspace_id.clone(),
                                tab_id: identity.tab_id.clone(),
                                agent_type: identity.agent_type.clone(),
                                state,
                            });
                        }
                    }
                }
            }
            next.insert(
                identity.session_id.clone(),
                ObservedAgent {
                    identity,
                    state,
                    state_started_at: session.agent_state_started_at.clone(),
                },
            );
        }
        let live = next
            .values()
            .map(|entry| entry.identity.clone())
            .collect::<HashSet<_>>();
        self.last_notified_at
            .retain(|(identity, _), _| live.contains(identity));
        self.observed = next;
        self.initialized = true;
        candidates
    }
}

/// Group notifications in a bounded window; only one latest state per Host
/// terminal survives a burst. No timers or OS-specific side effects here.
#[derive(Default)]
pub struct AgentNoticeCoalescer {
    pending: HashMap<String, AgentNotice>,
    first_enqueue_ms: Option<u64>,
    last_enqueue_ms: Option<u64>,
}

impl AgentNoticeCoalescer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enqueue(&mut self, notices: Vec<AgentNotice>, now_ms: u64) {
        if notices.is_empty() {
            return;
        }
        self.first_enqueue_ms.get_or_insert(now_ms);
        self.last_enqueue_ms = Some(now_ms);
        for notice in notices {
            if self.pending.len() >= MAX_NOTICE_SESSIONS
                && !self.pending.contains_key(&notice.session_id)
            {
                continue;
            }
            self.pending.insert(notice.session_id.clone(), notice);
        }
    }

    pub fn discard(&mut self, session_id: &str) {
        self.pending.remove(session_id);
        if self.pending.is_empty() {
            self.first_enqueue_ms = None;
            self.last_enqueue_ms = None;
        }
    }

    pub fn clear(&mut self) {
        self.pending.clear();
        self.first_enqueue_ms = None;
        self.last_enqueue_ms = None;
    }

    pub fn drain_if_ready(&mut self, now_ms: u64) -> Vec<AgentNotice> {
        let (Some(first), Some(last)) = (self.first_enqueue_ms, self.last_enqueue_ms) else {
            return Vec::new();
        };
        if now_ms.saturating_sub(last) < NOTIFICATION_COALESCE_MS
            && now_ms.saturating_sub(first) < NOTIFICATION_MAX_COALESCE_MS
        {
            return Vec::new();
        }
        let mut notices = self
            .pending
            .drain()
            .map(|(_, notice)| notice)
            .collect::<Vec<_>>();
        notices.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        self.first_enqueue_ms = None;
        self.last_enqueue_ms = None;
        notices
    }
}

/// Native must not silently take over Classic's Windows notifications.
/// Caller must obtain an explicit exclusive Host delivery claim before
/// passing `native_owns_delivery=true`. Without it the policy fails closed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AgentNotificationPolicy {
    pub hooks_enabled: bool,
    pub notifications_enabled: bool,
    pub include_finished: bool,
    pub native_owns_delivery: bool,
}

impl AgentNotificationPolicy {
    pub fn allowed(self) -> bool {
        self.hooks_enabled && self.notifications_enabled && self.native_owns_delivery
    }
}

/// Revalidates a notice against the *current* authenticated Host inventory;
/// the handle alone is not enough because it may have been recycled.
/// Deliberately allows a state change from Waiting to Done: clicking a
/// historical notice may still focus the same live Agent, not a new one.
fn validated_agent_identity<'a>(
    session_id: &str,
    workspace_id: &str,
    tab_id: &str,
    agent_type: &str,
    sessions: &'a [TerminalSession],
) -> Option<&'a TerminalSession> {
    sessions.iter().find(|session| {
        session.running
            && session.handle == session_id
            && session.workspace_id == workspace_id
            && session.tab_id == tab_id
            && session.agent_type.as_deref() == Some(agent_type)
    })
}

pub fn validated_notice_target<'a>(
    notice: &AgentNotice,
    sessions: &'a [TerminalSession],
) -> Option<&'a TerminalSession> {
    validated_agent_identity(
        &notice.session_id,
        &notice.workspace_id,
        &notice.tab_id,
        &notice.agent_type,
        sessions,
    )
}

/// A running Host Agent without Hook activity is still an authentic navigation
/// target. This checks the same full identity as Toast navigation without
/// manufacturing a fake Working/Done state.
pub fn validated_agent_navigation_target<'a>(
    target: &AgentNavigationTarget,
    sessions: &'a [TerminalSession],
) -> Option<&'a TerminalSession> {
    validated_agent_identity(
        &target.session_id,
        &target.workspace_id,
        &target.tab_id,
        &target.agent_type,
        sessions,
    )
}

/// Resolve a verified Host identity to an existing Agent preview and pane.
fn validated_agent_pane_identity(
    session_id: &str,
    workspace_id: &str,
    tab_id: &str,
    agent_type: &str,
    sessions: &[TerminalSession],
    tabs: &WorkbenchTabs,
    panes: &WorkbenchPaneLayout,
) -> Option<(WorkbenchTabIdentity, PaneSlot)> {
    validated_agent_identity(session_id, workspace_id, tab_id, agent_type, sessions)?;
    for pane in [PaneSlot::Primary, PaneSlot::Secondary] {
        for identity in panes.tabs_in(pane) {
            let WorkbenchTabIdentity::Host(target) = identity else {
                continue;
            };
            if target.handle == session_id
                && target.workspace_id() == workspace_id
                && target.tab_id() == tab_id
                && tabs.entries().iter().any(|tab| {
                    &tab.identity == identity && tab.kind == WorkbenchTabKind::Agent && tab.running
                })
            {
                return Some((identity.clone(), pane));
            }
        }
    }
    None
}

/// Resolve a clicked Toast to the exact already-open Agent preview and pane.
/// This is a read-only projection: only the UI caller may focus the returned
/// identity after the Host validates the complete session/workspace/tab tuple.
/// Never infer the pane from whatever Terminal happens to be active.
pub fn validated_notice_pane_target(
    notice: &AgentNotice,
    sessions: &[TerminalSession],
    tabs: &WorkbenchTabs,
    panes: &WorkbenchPaneLayout,
) -> Option<(WorkbenchTabIdentity, PaneSlot)> {
    validated_agent_pane_identity(
        &notice.session_id,
        &notice.workspace_id,
        &notice.tab_id,
        &notice.agent_type,
        sessions,
        tabs,
        panes,
    )
}

pub fn validated_agent_navigation_pane_target(
    target: &AgentNavigationTarget,
    sessions: &[TerminalSession],
    tabs: &WorkbenchTabs,
    panes: &WorkbenchPaneLayout,
) -> Option<(WorkbenchTabIdentity, PaneSlot)> {
    validated_agent_pane_identity(
        &target.session_id,
        &target.workspace_id,
        &target.tab_id,
        &target.agent_type,
        sessions,
        tabs,
        panes,
    )
}

/// Pure H5 delivery boundary. Keeps Classic-like transition history even
/// while notifications are disabled or owned by Classic; pending batches
/// are discarded on revocation, Agent recovery or Host identity change.
#[derive(Default)]
pub struct AgentNotificationDelivery {
    tracker: AgentNotificationTracker,
    coalescer: AgentNoticeCoalescer,
}

impl AgentNotificationDelivery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe(
        &mut self,
        sessions: &[TerminalSession],
        now_ms: u64,
        policy: AgentNotificationPolicy,
    ) {
        let allowed = policy.allowed();
        let notices = self
            .tracker
            .observe(sessions, now_ms, policy.include_finished, allowed);
        if !allowed {
            self.coalescer.clear();
            return;
        }
        self.coalescer.pending.retain(|_, notice| {
            validated_notice_target(notice, sessions).is_some_and(|current| {
                current.agent_state == Some(notice.state)
                    && !(notice.state == AgentActivityState::Done && current.interrupted)
            })
        });
        if self.coalescer.pending.is_empty() {
            self.coalescer.first_enqueue_ms = None;
            self.coalescer.last_enqueue_ms = None;
        }
        self.coalescer.enqueue(notices, now_ms);
    }

    pub fn drain(
        &mut self,
        sessions: &[TerminalSession],
        now_ms: u64,
        policy: AgentNotificationPolicy,
    ) -> Vec<AgentNotice> {
        if !policy.allowed() {
            self.coalescer.clear();
            return Vec::new();
        }
        self.coalescer
            .drain_if_ready(now_ms)
            .into_iter()
            .filter(|notice| {
                validated_notice_target(notice, sessions).is_some_and(|current| {
                    current.agent_state == Some(notice.state)
                        && !(notice.state == AgentActivityState::Done && current.interrupted)
                        && (notice.state != AgentActivityState::Done || policy.include_finished)
                })
            })
            .collect()
    }

    pub fn clear(&mut self) {
        self.coalescer.clear();
    }

    /// Reconnecting to an existing Host must not replay already-waiting
    /// Agents as new attention. Keep cooldown history, reset only baseline.
    pub fn on_host_reconnected(&mut self) {
        self.coalescer.clear();
        self.tracker.initialized = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::{AgentActivityState, TerminalDock, TerminalSession};
    use crate::workbench_panes::{PaneSlot, WorkbenchPaneLayout};
    use crate::workbench_tabs::{
        WorkbenchTab, WorkbenchTabIdentity, WorkbenchTabKind, WorkbenchTabs,
    };

    fn session(handle: &str, tab: &str, state: Option<AgentActivityState>) -> TerminalSession {
        TerminalSession {
            handle: handle.into(),
            workspace_id: "workspace-a".into(),
            tab_id: tab.into(),
            running: true,
            agent_type: Some("codex".into()),
            agent_state: state,
            interrupted: false,
            agent_state_started_at: None,
            agent_updated_at: None,
            agent_context: None,
        }
    }

    fn notice_tabs(sessions: &[TerminalSession]) -> (WorkbenchTabs, WorkbenchPaneLayout) {
        let mut dock = TerminalDock::for_workspace(7);
        dock.update_sessions(sessions.to_vec());
        let mut observed = Vec::new();
        for session in sessions {
            assert!(dock.open(&session.handle));
            let index = dock.active_tab.unwrap();
            observed.push(WorkbenchTab {
                identity: WorkbenchTabIdentity::Host(dock.context_target_at(index).unwrap()),
                kind: WorkbenchTabKind::Agent,
                title: session.agent_type.clone().unwrap_or_default(),
                dirty: false,
                running: session.running,
                agent_state: session.agent_state,
                interrupted: session.interrupted,
            });
        }
        let mut tabs = WorkbenchTabs::for_workspace(7);
        let ids = observed
            .iter()
            .map(|tab| tab.identity.clone())
            .collect::<Vec<_>>();
        tabs.reconcile(7, observed, ids.first().cloned());
        let mut panes = WorkbenchPaneLayout::new();
        panes.reconcile(&ids);
        (tabs, panes)
    }

    #[test]
    fn history_navigation_without_hook_status_keeps_exact_host_identity() {
        let live = session("live-session", "saved-history-tab", None);
        let target = AgentNavigationTarget {
            session_id: live.handle.clone(),
            workspace_id: live.workspace_id.clone(),
            tab_id: live.tab_id.clone(),
            agent_type: "codex".into(),
        };
        assert!(live.agent_state.is_none(), "no Hook status was reported");
        assert!(validated_agent_navigation_target(&target, &[live.clone()]).is_some());
        let (tabs, panes) = notice_tabs(&[live.clone()]);
        assert!(
            validated_agent_navigation_pane_target(&target, &[live.clone()], &tabs, &panes)
                .is_some(),
            "opening an existing running tab must not depend on Hooks"
        );

        for wrong in [
            TerminalSession {
                handle: "recycled-handle".into(),
                ..live.clone()
            },
            TerminalSession {
                tab_id: "other-tab".into(),
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
            assert!(validated_agent_navigation_target(&target, &[wrong]).is_none());
        }
        assert!(
            validated_agent_navigation_pane_target(&target, &[], &tabs, &panes).is_none(),
            "stale Host inventories cannot authorize a pane activation"
        );
    }
    #[test]
    fn h03_toast_click_routes_to_exact_secondary_agent_without_changing_other_pane() {
        let primary = session("host-a", "tab-a", Some(AgentActivityState::Working));
        let secondary = session("host-b", "tab-b", Some(AgentActivityState::Waiting));
        let sessions = vec![primary, secondary.clone()];
        let (tabs, mut panes) = notice_tabs(&sessions);
        let secondary_id = tabs.entries()[1].identity.clone();
        let primary_id = tabs.entries()[0].identity.clone();
        assert!(panes.split_with(&secondary_id));
        assert!(panes.activate(&primary_id));
        let before = panes.clone();
        let notice = AgentNotice {
            session_id: secondary.handle,
            workspace_id: secondary.workspace_id,
            tab_id: secondary.tab_id,
            agent_type: "codex".into(),
            state: AgentActivityState::Waiting,
        };
        assert_eq!(
            validated_notice_pane_target(&notice, &sessions, &tabs, &panes),
            Some((secondary_id, PaneSlot::Secondary))
        );
        assert_eq!(panes, before, "lookup never changes pane or Host PTY");
        assert_eq!(panes.active_in(PaneSlot::Primary), Some(&primary_id));
    }

    #[test]
    fn h03_toast_click_rejects_recycled_host_and_nonagent_preview() {
        let live = session("host-a", "tab-a", Some(AgentActivityState::Done));
        let (mut tabs, panes) = notice_tabs(&[live.clone()]);
        let notice = AgentNotice {
            session_id: live.handle.clone(),
            workspace_id: live.workspace_id.clone(),
            tab_id: live.tab_id.clone(),
            agent_type: "codex".into(),
            state: AgentActivityState::Waiting,
        };
        assert!(validated_notice_pane_target(&notice, &[live.clone()], &tabs, &panes).is_some());
        for stale in [
            TerminalSession {
                tab_id: "recycled".into(),
                ..live.clone()
            },
            TerminalSession {
                workspace_id: "another".into(),
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
            assert!(validated_notice_pane_target(&notice, &[stale], &tabs, &panes).is_none());
        }
        let mut nonagent = tabs.entries().to_vec();
        nonagent[0].kind = WorkbenchTabKind::Terminal;
        tabs.reconcile(7, nonagent, None);
        assert!(validated_notice_pane_target(&notice, &[live], &tabs, &panes).is_none());
    }

    #[test]
    fn initial_snapshot_never_spams_existing_waiting_or_done_agents() {
        let mut tracker = AgentNotificationTracker::new();
        assert!(tracker
            .observe(
                &[
                    session("h1", "t1", Some(AgentActivityState::Waiting)),
                    session("h2", "t2", Some(AgentActivityState::Done))
                ],
                1_000,
                true,
                true
            )
            .is_empty());
    }

    #[test]
    fn attention_transitions_are_deduplicated_with_classic_sixty_second_cooldown() {
        let mut tracker = AgentNotificationTracker::new();
        let waiting = session("h1", "t1", Some(AgentActivityState::Waiting));
        let working = session("h1", "t1", Some(AgentActivityState::Working));
        let blocked = session("h1", "t1", Some(AgentActivityState::Blocked));
        assert!(tracker
            .observe(&[working.clone()], 1_000, false, true)
            .is_empty());
        assert_eq!(
            tracker
                .observe(&[waiting.clone()], 2_000, false, true)
                .len(),
            1
        );
        assert!(tracker
            .observe(&[waiting.clone()], 2_100, false, true)
            .is_empty());
        assert!(tracker
            .observe(&[working.clone()], 3_000, false, true)
            .is_empty());
        assert!(tracker
            .observe(&[waiting.clone()], 5_000, false, true)
            .is_empty());
        assert_eq!(tracker.observe(&[blocked], 6_000, false, true).len(), 1);
        assert!(tracker.observe(&[working], 8_000, false, true).is_empty());
        assert_eq!(tracker.observe(&[waiting], 63_000, false, true).len(), 1);
    }

    #[test]
    fn done_only_notifies_when_enabled_and_reused_handle_cannot_inherit_state() {
        let mut tracker = AgentNotificationTracker::new();
        let working = session("h1", "t1", Some(AgentActivityState::Working));
        let done = session("h1", "t1", Some(AgentActivityState::Done));
        let reused = session("h1", "t2", Some(AgentActivityState::Blocked));
        assert!(tracker
            .observe(&[working.clone()], 10, true, true)
            .is_empty());
        assert!(tracker.observe(&[done.clone()], 20, false, true).is_empty());
        assert!(tracker
            .observe(&[working.clone()], 30, true, true)
            .is_empty());
        assert_eq!(tracker.observe(&[done], 40, true, true).len(), 1);
        assert!(tracker
            .observe(&[reused.clone()], 50, true, true)
            .is_empty());
        assert!(tracker
            .observe(
                &[session("h1", "t2", Some(AgentActivityState::Working))],
                60,
                true,
                true
            )
            .is_empty());
        assert_eq!(tracker.observe(&[reused], 70, true, true).len(), 1);
    }

    #[test]
    fn disabled_notifications_update_state_without_replaying_on_reenable() {
        let mut tracker = AgentNotificationTracker::new();
        let working = session("h1", "t1", Some(AgentActivityState::Working));
        let waiting = session("h1", "t1", Some(AgentActivityState::Waiting));
        assert!(tracker
            .observe(&[working.clone()], 0, false, true)
            .is_empty());
        assert!(tracker
            .observe(&[waiting.clone()], 1_000, false, false)
            .is_empty());
        assert!(tracker.observe(&[waiting], 2_000, false, true).is_empty());
        assert!(tracker.observe(&[working], 3_000, false, true).is_empty());
    }

    #[test]
    fn same_attention_state_new_agent_turn_is_not_lost_after_cooldown() {
        let mut tracker = AgentNotificationTracker::new();
        let mut waiting = session("h1", "t1", Some(AgentActivityState::Waiting));
        waiting.agent_state_started_at = Some("2026-11-11T07:00:00Z".into());
        assert!(
            tracker
                .observe(&[waiting.clone()], 0, false, true)
                .is_empty(),
            "initial Host snapshot is baseline, never a notification"
        );
        assert!(
            tracker
                .observe(&[waiting.clone()], 80_000, false, true)
                .is_empty(),
            "same state and same turn may not replay"
        );
        waiting.agent_state_started_at = Some("2026-11-11T07:01:20Z".into());
        assert_eq!(
            tracker
                .observe(&[waiting.clone()], 80_001, false, true)
                .len(),
            1,
            "a new approval in a new turn is a fresh Hook transition"
        );
        waiting.agent_state_started_at = Some("2026-11-11T07:01:21Z".into());
        assert!(
            tracker
                .observe(&[waiting.clone()], 81_000, false, true)
                .is_empty(),
            "new round within cooldown should not spam"
        );
        waiting.agent_state_started_at = Some("2026-11-11T07:03:00Z".into());
        assert_eq!(tracker.observe(&[waiting], 142_000, false, true).len(), 1);
    }

    #[test]
    fn bursts_merge_for_three_seconds_with_ten_second_ceiling() {
        let mut coalescer = AgentNoticeCoalescer::new();
        let mut tracker = AgentNotificationTracker::new();
        let working = session("h1", "t1", Some(AgentActivityState::Working));
        let waiting = session("h1", "t1", Some(AgentActivityState::Waiting));
        let blocked = session("h1", "t1", Some(AgentActivityState::Blocked));
        tracker.observe(&[working.clone()], 0, true, true);
        coalescer.enqueue(tracker.observe(&[waiting], 1_000, true, true), 1_000);
        assert!(coalescer.drain_if_ready(3_999).is_empty());
        coalescer.enqueue(tracker.observe(&[blocked], 3_500, true, true), 3_500);
        assert!(coalescer.drain_if_ready(5_000).is_empty());
        let batch = coalescer.drain_if_ready(6_500);
        assert_eq!(
            batch.len(),
            1,
            "one terminal contributes only latest transition"
        );
        assert_eq!(batch[0].state, AgentActivityState::Blocked);
        assert!(coalescer.drain_if_ready(20_000).is_empty());
    }

    #[test]
    fn continuous_status_churn_cannot_delay_notifications_beyond_ten_seconds() {
        let mut coalescer = AgentNoticeCoalescer::new();
        let notice = AgentNotice {
            session_id: "h1".into(),
            workspace_id: "w1".into(),
            tab_id: "tab1".into(),
            agent_type: "codex".into(),
            state: AgentActivityState::Waiting,
        };
        for now in [1_000, 3_000, 5_000, 7_000, 9_000, 10_500] {
            coalescer.enqueue(vec![notice.clone()], now);
            assert!(coalescer.drain_if_ready(now).is_empty());
        }
        assert_eq!(coalescer.drain_if_ready(11_000), vec![notice]);
    }

    #[test]
    fn resolved_attention_cancels_queued_notification_before_delivery() {
        let mut coalescer = AgentNoticeCoalescer::new();
        coalescer.enqueue(
            vec![AgentNotice {
                session_id: "h1".into(),
                workspace_id: "w1".into(),
                tab_id: "tab1".into(),
                agent_type: "codex".into(),
                state: AgentActivityState::Waiting,
            }],
            1_000,
        );
        coalescer.discard("h1");
        assert!(coalescer.drain_if_ready(12_000).is_empty());
    }

    #[test]
    fn attention_delivery_requires_explicit_native_ownership_and_host_permission() {
        let mut delivery = AgentNotificationDelivery::new();
        let working = session("h1", "t1", Some(AgentActivityState::Working));
        let waiting = session("h1", "t1", Some(AgentActivityState::Waiting));
        let owner = AgentNotificationPolicy {
            hooks_enabled: true,
            notifications_enabled: true,
            include_finished: false,
            native_owns_delivery: true,
        };
        let not_owner = AgentNotificationPolicy {
            native_owns_delivery: false,
            ..owner
        };
        delivery.observe(&[working.clone()], 0, owner);
        delivery.observe(&[waiting.clone()], 1_000, not_owner);
        assert!(
            delivery.drain(&[waiting.clone()], 5_000, owner).is_empty(),
            "Classic-owned transitions may not replay when Native gains ownership"
        );
        delivery.observe(&[working.clone()], 6_000, owner);
        delivery.observe(&[waiting.clone()], 7_000, owner);
        let batch = delivery.drain(&[waiting.clone()], 10_000, owner);
        assert_eq!(batch.len(), 1, "new Native-owned attention is eligible");
        assert_eq!(batch[0].tab_id, "t1");
        assert!(delivery.drain(&[waiting.clone()], 11_000, owner).is_empty());
    }

    #[test]
    fn resolved_or_recycled_agent_cancels_delivery_even_after_coalescing() {
        let mut delivery = AgentNotificationDelivery::new();
        let working = session("h1", "t1", Some(AgentActivityState::Working));
        let waiting = session("h1", "t1", Some(AgentActivityState::Waiting));
        let owner = AgentNotificationPolicy {
            hooks_enabled: true,
            notifications_enabled: true,
            include_finished: false,
            native_owns_delivery: true,
        };
        delivery.observe(&[working.clone()], 0, owner);
        delivery.observe(&[waiting.clone()], 1_000, owner);
        delivery.observe(&[working.clone()], 2_000, owner);
        assert!(delivery.drain(&[working.clone()], 10_000, owner).is_empty());
        delivery.observe(&[waiting.clone()], 65_000, owner);
        let recycled = session("h1", "t2", Some(AgentActivityState::Waiting));
        assert!(
            delivery.drain(&[recycled], 70_000, owner).is_empty(),
            "a new Host tab may reuse the same handle"
        );
        delivery.observe(&[working], 71_000, owner);
        assert!(delivery
            .drain(
                &[waiting],
                75_000,
                AgentNotificationPolicy {
                    notifications_enabled: false,
                    ..owner
                }
            )
            .is_empty());
    }

    #[test]
    fn notice_activation_rechecks_exact_host_identity_before_navigation() {
        let target = AgentNotice {
            session_id: "h1".into(),
            workspace_id: "workspace-a".into(),
            tab_id: "t1".into(),
            agent_type: "codex".into(),
            state: AgentActivityState::Waiting,
        };
        let current = session("h1", "t1", Some(AgentActivityState::Done));
        assert!(
            validated_notice_target(&target, &[current.clone()]).is_some(),
            "still-live same Host session may be focused even after state changes"
        );
        for bad in [
            session("h1", "t2", Some(AgentActivityState::Waiting)),
            TerminalSession {
                workspace_id: "other".into(),
                ..current.clone()
            },
            TerminalSession {
                agent_type: Some("claude".into()),
                ..current.clone()
            },
            TerminalSession {
                running: false,
                ..current.clone()
            },
        ] {
            assert!(
                validated_notice_target(&target, &[bad]).is_none(),
                "stale click must never focus a different Host or workspace"
            );
        }
        assert!(validated_notice_target(&target, &[]).is_none());
    }

    #[test]
    fn per_agent_hook_flags_from_host_filter_native_notifications_and_fail_closed() {
        let codex = session("h1", "t1", Some(AgentActivityState::Working));
        let mut claude = session("h2", "t2", Some(AgentActivityState::Working));
        claude.agent_type = Some("claude".into());
        let mut flags = HashMap::from([("codex".to_string(), true), ("claude".to_string(), false)]);
        assert!(
            enabled_notice_sessions(&[codex.clone(), claude.clone()], None).is_empty(),
            "no Host settings is NOT implicit permission"
        );
        let filtered = enabled_notice_sessions(&[codex.clone(), claude.clone()], Some(&flags));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].handle, "h1");
        let mut delivery = AgentNotificationDelivery::new();
        let owner = AgentNotificationPolicy {
            hooks_enabled: true,
            notifications_enabled: true,
            include_finished: false,
            native_owns_delivery: true,
        };
        delivery.observe(&filtered, 0, owner);
        let mut waiting_codex = codex.clone();
        waiting_codex.agent_state = Some(AgentActivityState::Waiting);
        claude.agent_state = Some(AgentActivityState::Waiting);
        let filtered =
            enabled_notice_sessions(&[waiting_codex.clone(), claude.clone()], Some(&flags));
        delivery.observe(&filtered, 1_000, owner);
        assert_eq!(
            delivery.drain(&filtered, 4_000, owner).len(),
            1,
            "only enabled Codex Hook can notify"
        );
        flags.insert("codex".into(), false);
        let now_disabled =
            enabled_notice_sessions(&[waiting_codex.clone(), claude.clone()], Some(&flags));
        assert!(now_disabled.is_empty());
        delivery.observe(&now_disabled, 5_000, owner);
        assert!(delivery.drain(&now_disabled, 6_000, owner).is_empty());
    }

    #[test]
    fn disabled_provider_flood_cannot_starve_late_enabled_host_agent() {
        let mut all = (0..150)
            .map(|index| {
                let mut item = session(
                    &format!("disabled-{index}"),
                    &format!("tab-{index}"),
                    Some(AgentActivityState::Waiting),
                );
                item.agent_type = Some("claude".into());
                item
            })
            .collect::<Vec<_>>();
        all.push(session(
            "live-codex",
            "real-tab",
            Some(AgentActivityState::Waiting),
        ));
        let flags = HashMap::from([("codex".into(), true), ("claude".into(), false)]);
        let selected = enabled_notice_sessions(&all, Some(&flags));
        assert_eq!(
            selected.len(),
            1,
            "unrelated disabled sessions must not consume the notification cap"
        );
        assert_eq!(selected[0].handle, "live-codex");
    }

    #[test]
    fn reconnect_discards_pending_toasts_and_does_not_replay_waiting_snapshot() {
        let mut delivery = AgentNotificationDelivery::new();
        let working = session("h1", "t1", Some(AgentActivityState::Working));
        let waiting = session("h1", "t1", Some(AgentActivityState::Waiting));
        let owner = AgentNotificationPolicy {
            hooks_enabled: true,
            notifications_enabled: true,
            include_finished: false,
            native_owns_delivery: true,
        };
        delivery.observe(&[working.clone()], 0, owner);
        delivery.observe(&[waiting.clone()], 1_000, owner);
        delivery.on_host_reconnected();
        delivery.observe(&[waiting.clone()], 4_000, owner);
        assert!(
            delivery.drain(&[waiting.clone()], 11_000, owner).is_empty(),
            "reconnect snapshot must never replay the previous Waiting"
        );
        delivery.observe(&[working], 12_000, owner);
        delivery.observe(&[waiting.clone()], 72_000, owner);
        assert_eq!(
            delivery.drain(&[waiting], 75_000, owner).len(),
            1,
            "a genuinely fresh attention after reconnect may notify"
        );
    }
}
