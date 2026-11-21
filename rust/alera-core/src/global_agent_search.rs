//! UI-neutral search projection for Alera Agent sessions.
//!
//! The Host must authorize and normalize records before passing them here.
//! Only bounded summaries and milestones are searchable. Never index raw PTY
//! output, tool payloads, credentials or provider transcript files in this model.
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

const MAX_QUERY_CHARS: usize = 256;
const MAX_CANDIDATES: usize = 20_000;
const MAX_RESULTS: usize = 100;
const MAX_EVENTS: usize = 16;
const MAX_FIELD_CHARS: usize = 1_024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionSearchRecord {
    pub runtime_instance_id: String,
    pub project_id: String,
    pub workspace_id: String,
    pub tab_id: String,
    pub host_session_id: String,
    pub provider: String,
    pub provider_account_id: Option<String>,
    pub profile_id: Option<String>,
    pub provider_session_id: Option<String>,
    /// The Host has verified this provider ID belongs to this workspace,
    /// profile and account, not merely observed an untrusted transcript.
    pub provider_identity_verified: bool,
    pub title: String,
    pub goal: String,
    pub progress: String,
    pub milestones: Vec<String>,
    pub updated_at_ms: u64,
    pub running: bool,
    pub source_exists: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionOpenAction {
    FocusRunning,
    ResumeExact,
    PreviewOnly,
    SourceUnavailable,
}

impl AgentSessionSearchRecord {
    pub fn open_action(&self) -> SessionOpenAction {
        if self.running
            && !self.runtime_instance_id.is_empty()
            && !self.workspace_id.is_empty()
            && !self.tab_id.is_empty()
            && !self.host_session_id.is_empty()
        {
            return SessionOpenAction::FocusRunning;
        }
        if !self.source_exists {
            return SessionOpenAction::SourceUnavailable;
        }
        if self.provider_identity_verified
            && matches!(self.provider.as_str(), "claude" | "codex")
            && self
                .provider_session_id
                .as_deref()
                .is_some_and(|id| Uuid::parse_str(id).is_ok())
            && !self.workspace_id.is_empty()
            && !self.project_id.is_empty()
        {
            return SessionOpenAction::ResumeExact;
        }
        SessionOpenAction::PreviewOnly
    }

    fn identity(&self) -> AgentSessionIdentity {
        // Typed identity avoids string separator collisions and prevents two
        // different accounts or profiles from becoming one search hit.
        if let Some(id) = self.provider_session_id.as_deref() {
            if Uuid::parse_str(id).is_ok() {
                // Untrusted historical UUIDs allow deduplication, not resume.
                return AgentSessionIdentity::Provider {
                    runtime: self.runtime_instance_id.clone(),
                    project: self.project_id.clone(),
                    workspace: self.workspace_id.clone(),
                    provider: self.provider.clone(),
                    account: self.provider_account_id.clone(),
                    profile: self.profile_id.clone(),
                    native: id.to_owned(),
                };
            }
        }
        AgentSessionIdentity::Host {
            runtime: self.runtime_instance_id.clone(),
            workspace: self.workspace_id.clone(),
            tab: self.tab_id.clone(),
            session: self.host_session_id.clone(),
        }
    }
}

#[derive(Debug, Hash, PartialEq, Eq)]
enum AgentSessionIdentity {
    Provider {
        runtime: String,
        project: String,
        workspace: String,
        provider: String,
        account: Option<String>,
        profile: Option<String>,
        native: String,
    },
    Host {
        runtime: String,
        workspace: String,
        tab: String,
        session: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionSearchQuery {
    pub text: String,
    pub project_id: Option<String>,
    pub workspace_id: Option<String>,
    pub provider: Option<String>,
    pub max_results: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionMatchField {
    Title,
    Goal,
    Progress,
    Milestone,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionSearchHit {
    pub session: AgentSessionSearchRecord,
    pub score: u32,
    pub match_field: SessionMatchField,
    pub open_action: SessionOpenAction,
}

fn searchable_text(value: &str) -> String {
    value
        .chars()
        .take(MAX_FIELD_CHARS)
        .flat_map(char::to_lowercase)
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn words(value: &str) -> Vec<String> {
    searchable_text(value)
        .split_whitespace()
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Exact technical names and full phrases are preferred to generic token overlap.
/// A missing term is not a hard rejection so mixed Chinese/English queries remain
/// useful before local semantic re-ranking becomes available.
fn score_record(
    record: &AgentSessionSearchRecord,
    text: &str,
    terms: &[String],
) -> Option<(u32, SessionMatchField)> {
    let fields = [
        (SessionMatchField::Title, 140, record.title.as_str()),
        (SessionMatchField::Goal, 110, record.goal.as_str()),
        (SessionMatchField::Progress, 85, record.progress.as_str()),
    ];
    let mut best = (0, SessionMatchField::Title);
    let mut matched_terms = HashSet::new();
    for (field, weight, source) in fields.into_iter().chain(
        record
            .milestones
            .iter()
            .take(MAX_EVENTS)
            .map(|value| (SessionMatchField::Milestone, 55, value.as_str())),
    ) {
        let haystack = searchable_text(source);
        if haystack.is_empty() {
            continue;
        }
        let mut score = 0_u32;
        if !text.is_empty() && haystack.contains(text) {
            score += weight * 2;
        }
        for term in terms {
            if haystack.contains(term) {
                matched_terms.insert(term.as_str());
                score += weight / (terms.len() as u32).max(1);
            }
        }
        if score > best.0 {
            best = (score, field);
        }
    }
    if best.0 == 0 {
        return None;
    }
    let coverage = matched_terms.len() as u32 * 20 / (terms.len() as u32).max(1);
    // Running sessions get a modest boost, not enough to outrank an exact
    // historical technical match.
    Some((best.0 + coverage + u32::from(record.running) * 5, best.1))
}

/// Bounded, deterministic cross-project query. The caller supplies only
/// authenticated, access-filtered records; this function never reads disks,
/// starts Agents or changes any session state.
pub fn search_agent_sessions(
    records: impl IntoIterator<Item = AgentSessionSearchRecord>,
    query: &AgentSessionSearchQuery,
) -> Vec<AgentSessionSearchHit> {
    let text = query.text.chars().take(MAX_QUERY_CHARS).collect::<String>();
    let terms = words(&text);
    let normalized = searchable_text(&text);
    let max_results = query.max_results.clamp(1, MAX_RESULTS);
    let mut positions: HashMap<AgentSessionIdentity, usize> = HashMap::new();
    let mut hits: Vec<AgentSessionSearchHit> = Vec::new();
    for record in records.into_iter().take(MAX_CANDIDATES) {
        if query
            .project_id
            .as_ref()
            .is_some_and(|id| id != &record.project_id)
            || query
                .workspace_id
                .as_ref()
                .is_some_and(|id| id != &record.workspace_id)
            || query
                .provider
                .as_ref()
                .is_some_and(|id| id != &record.provider)
        {
            continue;
        }
        if terms.is_empty() {
            continue;
        }
        let Some((score, match_field)) = score_record(&record, &normalized, &terms) else {
            continue;
        };
        let identity = record.identity();
        let hit = AgentSessionSearchHit {
            open_action: record.open_action(),
            session: record,
            score,
            match_field,
        };
        if let Some(&index) = positions.get(&identity) {
            let current: &AgentSessionSearchHit = &hits[index];
            // A live, exact-identity Host session always outranks the same
            // provider conversation in the historical index, regardless of
            // input ordering or a longer archived summary.
            let replace = (hit.open_action == SessionOpenAction::FocusRunning
                && current.open_action != SessionOpenAction::FocusRunning)
                || ((hit.open_action == SessionOpenAction::FocusRunning)
                    == (current.open_action == SessionOpenAction::FocusRunning)
                    && (hit.score, hit.session.updated_at_ms)
                        > (current.score, current.session.updated_at_ms));
            if replace {
                hits[index] = hit;
            }
        } else {
            positions.insert(identity, hits.len());
            hits.push(hit);
        }
    }
    hits.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| b.session.updated_at_ms.cmp(&a.session.updated_at_ms))
            .then_with(|| a.session.host_session_id.cmp(&b.session.host_session_id))
    });
    hits.truncate(max_results);
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(provider: &str, running: bool) -> AgentSessionSearchRecord {
        AgentSessionSearchRecord {
            runtime_instance_id: "host-1".into(),
            project_id: "alera".into(),
            workspace_id: "worktree-1".into(),
            tab_id: "tab-1".into(),
            host_session_id: "pty-handle-1".into(),
            provider: provider.into(),
            provider_account_id: Some("account-default".into()),
            profile_id: Some("profile-default".into()),
            provider_session_id: Some("334c91c7-1050-44e4-a0e1-2df5f2cbb668".into()),
            provider_identity_verified: true,
            title: "Windows IME fix".into(),
            goal: "Fix Windows 注音候選視窗 in native editor".into(),
            progress: "Verified IMM API message handling".into(),
            milestones: vec!["Passed native regression tests".into()],
            updated_at_ms: 42,
            running,
            source_exists: true,
        }
    }

    fn query(text: &str) -> AgentSessionSearchQuery {
        AgentSessionSearchQuery {
            text: text.into(),
            max_results: 20,
            ..Default::default()
        }
    }

    #[test]
    fn matches_terms_and_mixed_chinese_english() {
        let hits = search_agent_sessions(vec![sample("claude", true)], &query("Windows 注音"));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].open_action, SessionOpenAction::FocusRunning);
        assert!(hits[0].score > 0);
    }

    #[test]
    fn technical_exact_match_ranks_above_generic_history() {
        let mut vague = sample("claude", false);
        vague.host_session_id = "old".into();
        vague.tab_id = "old".into();
        vague.provider_session_id = Some("0b47f13b-4d13-45d7-a6ed-19a596a53d9b".into());
        vague.title = "Windows work".into();
        vague.goal = "General Windows cleanup".into();
        vague.progress = String::new();
        let hits = search_agent_sessions(vec![vague, sample("codex", true)], &query("IMM API"));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].session.provider, "codex");
    }

    #[test]
    fn filter_project_and_provider_without_active_workspace() {
        let hits = search_agent_sessions(
            vec![sample("claude", false), sample("codex", false)],
            &AgentSessionSearchQuery {
                provider: Some("claude".into()),
                ..query("windows")
            },
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].session.provider, "claude");
    }

    #[test]
    fn native_resume_requires_verified_uuid_and_scope() {
        let mut doc = sample("claude", false);
        assert_eq!(doc.open_action(), SessionOpenAction::ResumeExact);
        doc.provider_identity_verified = false;
        assert_eq!(doc.open_action(), SessionOpenAction::PreviewOnly);
        doc.provider_identity_verified = true;
        doc.provider_session_id = Some("last".into());
        assert_eq!(doc.open_action(), SessionOpenAction::PreviewOnly);
        doc.provider_session_id = Some("334c91c7-1050-44e4-a0e1-2df5f2cbb668".into());
        doc.source_exists = false;
        assert_eq!(doc.open_action(), SessionOpenAction::SourceUnavailable);
    }

    #[test]
    fn terminated_unknown_provider_is_preview_only() {
        let doc = sample("unknown", false);
        assert_eq!(doc.open_action(), SessionOpenAction::PreviewOnly);
    }

    #[test]
    fn dedupes_verified_provider_session_in_same_workspace() {
        let mut another = sample("claude", false);
        another.tab_id = "tab-2".into();
        another.host_session_id = "pty-2".into();
        assert_eq!(
            search_agent_sessions(vec![sample("claude", true), another], &query("Windows")).len(),
            1
        );
    }

    #[test]
    fn live_record_replaces_older_history_even_if_history_was_first() {
        let mut history = sample("claude", false);
        history.title = "Windows IME candidate window deep repair".into();
        history.updated_at_ms = 4;
        let live = sample("claude", true);
        let hits = search_agent_sessions(vec![history, live], &query("Windows"));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].open_action, SessionOpenAction::FocusRunning);
    }

    #[test]
    fn same_provider_uuid_with_different_account_is_not_deduped() {
        let mut other = sample("claude", false);
        other.provider_account_id = Some("account-secondary".into());
        let hits = search_agent_sessions(vec![sample("claude", false), other], &query("Windows"));
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn unverified_history_has_distinct_identities_but_is_preview_only() {
        let mut first = sample("claude", false);
        first.provider_identity_verified = false;
        first.tab_id.clear();
        first.host_session_id.clear();
        let mut second = first.clone();
        second.provider_session_id = Some("7e27d863-b5a5-40a0-8caf-f589c237cdd1".into());
        let hits = search_agent_sessions(vec![first, second], &query("Windows"));
        assert_eq!(hits.len(), 2);
        assert!(hits
            .iter()
            .all(|hit| hit.open_action == SessionOpenAction::PreviewOnly));
    }

    #[test]
    fn empty_query_and_oversized_limit_never_list_all_history() {
        assert!(search_agent_sessions(vec![sample("claude", true)], &query(" ")).is_empty());
        let q = AgentSessionSearchQuery {
            max_results: usize::MAX,
            ..query("windows")
        };
        assert_eq!(
            search_agent_sessions(vec![sample("claude", true)], &q).len(),
            1
        );
    }
}
