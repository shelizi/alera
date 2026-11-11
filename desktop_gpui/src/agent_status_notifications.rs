//! Pure, deterministic Agent Hook notification decisions for GPUI.
//! No OS callbacks or second Hook receiver: consume Host-confirmed presence only.

use crate::terminal::{AgentActivityState, TerminalSession};
use std::collections::{HashMap, HashSet};

/// Match Classic's per-session/state throttle and burst batching policy.
pub const NOTIFICATION_COOLDOWN_MS: u64 = 60_000;
pub const NOTIFICATION_COALESCE_MS: u64 = 3_000;
pub const NOTIFICATION_MAX_COALESCE_MS: u64 = 10_000;
const MAX_NOTICE_SESSIONS: usize = 128;

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

#[derive(Clone)]
struct ObservedAgent {
    identity: AgentIdentity,
    state: Option<AgentActivityState>,
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
                let changed_in_same_session =
                    prior.is_some_and(|old| old.identity == identity && old.state != state);
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
                ObservedAgent { identity, state },
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::{AgentActivityState, TerminalSession};

    fn session(handle: &str, tab: &str, state: Option<AgentActivityState>) -> TerminalSession {
        TerminalSession {
            handle: handle.into(),
            workspace_id: "workspace-a".into(),
            tab_id: tab.into(),
            running: true,
            agent_type: Some("codex".into()),
            agent_state: state,
            interrupted: false,
        }
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
}
