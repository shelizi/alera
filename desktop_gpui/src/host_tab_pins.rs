//! Secret-free, identity-checked Host Tab pin metadata. Never owns a PTY.
use crate::terminal::{RegisteredHostWorkspace, TerminalSession, TerminalTabContextTarget};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use std::io::{self, ErrorKind};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostTabPinSnapshot {
    pub workspace_id: String,
    pub id: String,
    pub updated_at: String,
    pub pinned: bool,
    pub terminal_session_id: Option<String>,
}

impl HostTabPinSnapshot {
    pub fn matches_session(&self, session: &TerminalSession) -> bool {
        self.workspace_id == session.workspace_id
            && self.id == session.tab_id
            && self.terminal_session_id.as_deref() == Some(session.handle.as_str())
    }

    pub fn matches_target(&self, target: &TerminalTabContextTarget) -> bool {
        self.workspace_id == target.workspace_id()
            && self.id == target.tab_id()
            && self.terminal_session_id.as_deref() == Some(target.handle.as_str())
    }

    fn valid(&self) -> bool {
        let safe = |value: &str| {
            !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
        };
        safe(&self.workspace_id)
            && safe(&self.id)
            && self.terminal_session_id.as_deref().is_none_or(safe)
            && (16..=64).contains(&self.updated_at.len())
            && self.updated_at.is_ascii()
            && self.updated_at.contains('T')
    }
}

pub fn parse_pin_snapshot(value: &Value, workspace: &str) -> io::Result<Vec<HostTabPinSnapshot>> {
    let items = value
        .as_array()
        .filter(|items| items.len() <= 256)
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "Invalid Host pin list"))?;
    let mut seen = HashSet::new();
    let mut result = Vec::with_capacity(items.len());
    for item in items {
        let pin: HostTabPinSnapshot = serde_json::from_value(item.clone())
            .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))?;
        if !pin.valid() || pin.workspace_id != workspace || !seen.insert(pin.id.clone()) {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "Host pin has invalid or duplicate identity",
            ));
        }
        result.push(pin);
    }
    Ok(result)
}

/// Query the active root plus other registered Host workspaces that own an
/// attached Native Terminal/Agent preview. A Host session's workspace ID
/// alone is not authorization to query an unregistered or recycled owner.
/// Keep first-seen order so root remains first, and never duplicate queries.
pub fn host_pin_query_workspaces(
    root_workspace: Option<&str>,
    attached_workspace_ids: &[String],
    registered: &[RegisteredHostWorkspace],
) -> Vec<String> {
    let allowed: HashSet<&str> = registered.iter().map(|item| item.id.as_str()).collect();
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for id in root_workspace
        .into_iter()
        .chain(attached_workspace_ids.iter().map(String::as_str))
    {
        if allowed.contains(id) && seen.insert(id) {
            result.push(id.to_owned());
        }
    }
    result
}

/// A late-attached Host tab might appear while an asynchronous pin refresh
/// is in flight. An unqueried registered owner needs a follow-up refresh,
/// never an assumed false pin from the previous scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostPinOwnerCoverage {
    Queried,
    RegisteredButUnqueried,
    Unregistered,
}

pub fn host_pin_owner_coverage(
    workspace_id: &str,
    queried: &[String],
    registered: &[RegisteredHostWorkspace],
) -> HostPinOwnerCoverage {
    if queried.iter().any(|id| id == workspace_id) {
        HostPinOwnerCoverage::Queried
    } else if registered.iter().any(|item| item.id == workspace_id) {
        HostPinOwnerCoverage::RegisteredButUnqueried
    } else {
        HostPinOwnerCoverage::Unregistered
    }
}

pub fn parse_pin_readback(
    value: &Value,
    original: &HostTabPinSnapshot,
    next: bool,
) -> io::Result<HostTabPinSnapshot> {
    let pin: HostTabPinSnapshot = serde_json::from_value(value.clone())
        .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))?;
    if !pin.valid()
        || pin.workspace_id != original.workspace_id
        || pin.id != original.id
        || pin.terminal_session_id != original.terminal_session_id
        || pin.pinned != next
        || pin.updated_at == original.updated_at
    {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "Host pin readback mismatch",
        ));
    }
    Ok(pin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pin_refresh_includes_registered_attached_workspaces_without_trusting_unknown_ids() {
        let registered = [
            RegisteredHostWorkspace {
                id: "root".into(),
                path: "C:/root".into(),
            },
            RegisteredHostWorkspace {
                id: "attached".into(),
                path: "C:/other".into(),
            },
        ];
        let observed = vec![
            "attached".to_owned(),
            "attached".to_owned(),
            "unknown".to_owned(),
            "root".to_owned(),
        ];
        assert_eq!(
            host_pin_query_workspaces(Some("root"), &observed, &registered),
            ["root", "attached"]
        );
        assert_eq!(
            host_pin_query_workspaces(None, &observed, &registered),
            ["attached", "root"]
        );
        assert!(
            host_pin_query_workspaces(Some("unregistered"), &["unknown".into()], &registered)
                .is_empty()
        );
    }

    #[test]
    fn mid_refresh_registered_attachment_requires_catchup_instead_of_false_unpin() {
        let registered = [
            RegisteredHostWorkspace {
                id: "root".into(),
                path: "C:/root".into(),
            },
            RegisteredHostWorkspace {
                id: "newly-attached".into(),
                path: "C:/attached".into(),
            },
        ];
        let old_scope = vec!["root".into()];
        assert_eq!(
            host_pin_owner_coverage("root", &old_scope, &registered),
            HostPinOwnerCoverage::Queried
        );
        assert_eq!(
            host_pin_owner_coverage("newly-attached", &old_scope, &registered),
            HostPinOwnerCoverage::RegisteredButUnqueried,
            "newly attached tabs must not be marked unpinned from old scoped replies"
        );
        assert_eq!(
            host_pin_owner_coverage("unknown", &old_scope, &registered),
            HostPinOwnerCoverage::Unregistered,
            "an unregistered workspace must never authorize another Host query"
        );
    }

    #[test]
    fn only_secret_free_workspace_scoped_pin_snapshots_are_accepted() {
        let item = json!({"workspaceId":"ws","id":"tab",
            "updatedAt":"2026-11-09T14:00:00.000Z",
            "pinned":true,"terminalSessionId":"pty-1"});
        assert!(parse_pin_snapshot(&json!([item.clone()]), "ws").is_ok());
        assert!(parse_pin_snapshot(&json!([item.clone()]), "other").is_err());
        assert!(parse_pin_snapshot(&json!([item.clone(), item.clone()]), "ws").is_err());
        let mut unsafe_item = item.clone();
        unsafe_item["payload"] = json!({"initialCommand":"sensitive"});
        assert!(parse_pin_snapshot(&json!([unsafe_item]), "ws").is_err());
        let session = TerminalSession {
            handle: "pty-1".into(),
            workspace_id: "ws".into(),
            tab_id: "tab".into(),
            running: true,
            agent_type: Some("Codex".into()),
            agent_state: None,
            interrupted: false,
            agent_state_started_at: None,
            agent_updated_at: None,
        };
        let pin = parse_pin_snapshot(&json!([item]), "ws").unwrap().remove(0);
        assert!(pin.matches_session(&session));
        assert!(!pin.matches_session(&TerminalSession {
            handle: "recycled".into(),
            ..session
        }));
    }

    #[test]
    fn pin_readback_refuses_stale_revision_or_recycled_session() {
        let old = parse_pin_snapshot(
            &json!([{"workspaceId":"ws","id":"tab",
            "updatedAt":"2026-11-09T14:00:00.000Z",
            "pinned":false,"terminalSessionId":"pty-1"}]),
            "ws",
        )
        .unwrap()
        .remove(0);
        let next = json!({"workspaceId":"ws","id":"tab",
            "updatedAt":"2026-11-09T14:00:00.001Z",
            "pinned":true,"terminalSessionId":"pty-1"});
        assert!(parse_pin_readback(&next, &old, true).is_ok());
        let mut stale = next.clone();
        stale["updatedAt"] = json!(old.updated_at);
        assert!(parse_pin_readback(&stale, &old, true).is_err());
        let mut recycled = next;
        recycled["terminalSessionId"] = json!("pty-2");
        assert!(parse_pin_readback(&recycled, &old, true).is_err());
    }
}
