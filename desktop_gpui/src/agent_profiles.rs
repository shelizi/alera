//! Minimal display-only projection of existing Host-owned Agent Profiles.
//! Do not retain commands, managed configurations, credentials or prompts
//! returned by the Host's full profile catalog.
use crate::terminal::TerminalSession;
use serde_json::Value;
use std::io;

const MAX_PROFILES: usize = 96;
const MAX_NAME_BYTES: usize = 256;
const MAX_TYPE_BYTES: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProfileRow {
    pub id: String,
    pub name: String,
    pub agent_type: String,
    pub managed: bool,
    /// Host revision for optimistic edits; never a command or credential.
    pub revision: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchedAgentTab {
    pub workspace_id: String,
    pub profile_id: String,
    pub tab_id: String,
}

pub fn safe_runtime_id(value: &str, max_len: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

pub fn validate_profile_name(name: &str) -> io::Result<String> {
    let name = name.trim();
    if name.is_empty() || name.len() > MAX_NAME_BYTES || name.chars().any(|c| c.is_control()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Profile name must be 1–256 bytes without control characters",
        ));
    }
    Ok(name.to_owned())
}

/// Only secret-free reference counts and Host identity are retained in GPUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProfileRemovalImpact {
    pub profile_id: String,
    pub revision: i64,
    pub is_default: bool,
    pub has_automation_policy: bool,
    pub automation_count: usize,
    pub tab_count: usize,
    pub execution_policy_count: usize,
    pub blocking_reference_count: usize,
}

impl AgentProfileRemovalImpact {
    pub fn has_blocking_references(&self) -> bool {
        self.blocking_reference_count > 0
            || self.automation_count > 0
            || self.tab_count > 0
            || self.execution_policy_count > 0
    }

    pub fn removal_message(&self, name: &str) -> String {
        if self.has_blocking_references() {
            return format!(
                "{name} is referenced by {} automation(s), {} tab(s), {} execution policy run(s). Remove these references before deleting.",
                self.automation_count, self.tab_count, self.execution_policy_count,
            );
        }
        let default_note = if self.is_default {
            " The shared default will be cleared."
        } else {
            ""
        };
        let policy_note = if self.has_automation_policy {
            " Its automation policy will be cleared."
        } else {
            ""
        };
        format!("{name} has no blocking references. Deletion cannot be undone.{default_note}{policy_note}")
    }
}

/// A missing or mismatched Host impact field must never enable deletion.
pub fn parse_profile_removal_impact(
    value: &Value,
    expected_id: &str,
    expected_revision: i64,
) -> io::Result<AgentProfileRemovalImpact> {
    if !safe_runtime_id(expected_id, 128) || expected_revision < 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid Profile removal target",
        ));
    }
    let flag = |field: &str| {
        value
            .get(field)
            .and_then(Value::as_bool)
            .ok_or_else(|| invalid("Host removal impact omitted required boolean"))
    };
    let count = |field: &str| {
        value
            .get(field)
            .and_then(Value::as_u64)
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| invalid("Host removal impact omitted required count"))
    };
    if value.get("profileId").and_then(Value::as_str) != Some(expected_id)
        || value.get("revision").and_then(Value::as_i64) != Some(expected_revision)
        || !flag("exists")?
    {
        return Err(invalid("Host removal impact identity or revision changed"));
    }
    let list = |field: &str| {
        value
            .get(field)
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("Host removal impact omitted references"))
    };
    let automations = list("automationIds")?;
    let executions = list("executionPolicyRunIds")?;
    let tabs = list("tabs")?;
    if automations.len() > 4096
        || executions.len() > 4096
        || tabs.len() > 4096
        || automations
            .iter()
            .chain(executions.iter())
            .any(|v| v.as_str().is_none_or(|id| !safe_runtime_id(id, 256)))
        || tabs.iter().any(|tab| {
            tab.get("workspaceId")
                .and_then(Value::as_str)
                .is_none_or(|id| !safe_runtime_id(id, 256))
                || tab
                    .get("tabId")
                    .and_then(Value::as_str)
                    .is_none_or(|id| !safe_runtime_id(id, 256))
        })
    {
        return Err(invalid("Host removal impact has invalid references"));
    }
    let blocking_reference_count = count("blockingReferenceCount")?;
    if blocking_reference_count < automations.len() + executions.len() + tabs.len()
        || count("referenceCount")? < blocking_reference_count
    {
        return Err(invalid("Host removal impact reference counts conflict"));
    }
    Ok(AgentProfileRemovalImpact {
        profile_id: expected_id.to_owned(),
        revision: expected_revision,
        is_default: flag("isDefault")?,
        has_automation_policy: flag("hasAutomationPolicy")?,
        automation_count: automations.len(),
        tab_count: tabs.len(),
        execution_policy_count: executions.len(),
        blocking_reference_count,
    })
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub fn parse_profile_catalog(value: &Value) -> io::Result<Vec<AgentProfileRow>> {
    let items = value
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("Agent profile list is missing items"))?;
    if items.len() > MAX_PROFILES {
        return Err(invalid("Host returned more than 96 Agent Profiles"));
    }
    let mut result = Vec::with_capacity(items.len());
    for item in items {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("Agent Profile has no id"))?;
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("Agent Profile has no name"))?;
        let agent_type = item
            .get("agentType")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("Agent Profile has no agent type"))?;
        let launch_mode = item
            .get("launchMode")
            .and_then(Value::as_str)
            .unwrap_or("command");
        let revision = match item.get("revision") {
            None => None,
            Some(value) => Some(
                value
                    .as_i64()
                    .filter(|n| *n >= 0)
                    .ok_or_else(|| invalid("Agent Profile has an invalid Host revision"))?,
            ),
        };
        if !safe_runtime_id(id, 128)
            || name.is_empty()
            || name.len() > MAX_NAME_BYTES
            || name.contains(['\0', '\n', '\r'])
            || agent_type.is_empty()
            || agent_type.len() > MAX_TYPE_BYTES
            || agent_type.contains(['\0', '\n', '\r'])
            || !matches!(launch_mode, "command" | "managed")
        {
            return Err(invalid("Agent Profile has unsafe display fields"));
        }
        if result
            .iter()
            .any(|previous: &AgentProfileRow| previous.id == id)
        {
            return Err(invalid("Host returned duplicate Agent Profile ids"));
        }
        result.push(AgentProfileRow {
            id: id.to_owned(),
            name: name.to_owned(),
            agent_type: agent_type.to_owned(),
            managed: launch_mode == "managed",
            revision,
        });
    }
    Ok(result)
}

pub fn parse_launched_agent(
    value: &Value,
    expected_profile_id: &str,
    expected_workspace_id: &str,
) -> io::Result<LaunchedAgentTab> {
    let id = value
        .get("profileId")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("Agent launch reply has no profile id"))?;
    let workspace_id = value
        .get("workspaceId")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("Agent launch reply has no workspace id"))?;
    let tab_id = value
        .get("tabId")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("Agent launch reply has no tab id"))?;
    if id != expected_profile_id
        || workspace_id != expected_workspace_id
        || !safe_runtime_id(id, 128)
        || !safe_runtime_id(workspace_id, 256)
        || !safe_runtime_id(tab_id, 256)
    {
        return Err(invalid(
            "Agent launch reply does not match the selected profile and registered workspace",
        ));
    }
    Ok(LaunchedAgentTab {
        workspace_id: workspace_id.to_owned(),
        profile_id: id.to_owned(),
        tab_id: tab_id.to_owned(),
    })
}

/// Use the authenticated Profile launch's exact Host tab to attach a live PTY.
/// Hooks are optional; PTY readiness never proves provider authentication.
pub fn matching_agent_handle<'a>(
    pending: &LaunchedAgentTab,
    sessions: &'a [TerminalSession],
    expected_agent_type: &str,
) -> Option<&'a str> {
    if expected_agent_type.is_empty() {
        return None;
    }
    let exact_live_tab = |session: &&TerminalSession| {
        session.running
            && session.workspace_id == pending.workspace_id
            && session.tab_id == pending.tab_id
    };
    sessions
        .iter()
        .filter(exact_live_tab)
        .find(|session| session.agent_type.as_deref() == Some(expected_agent_type))
        .or_else(|| {
            sessions
                .iter()
                .filter(exact_live_tab)
                .find(|session| session.agent_type.is_none())
        })
        .map(|session| session.handle.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn profile_removal_preview_is_revision_guarded_and_fails_closed_on_references() {
        let safe = json!({
            "profileId": "prof_1", "exists": true, "revision": 2,
            "isDefault": false, "hasAutomationPolicy": false,
            "automationIds": [], "executionPolicyRunIds": [], "tabs": [],
            "referenceCount": 0, "blockingReferenceCount": 0
        });
        let preview = parse_profile_removal_impact(&safe, "prof_1", 2).unwrap();
        assert!(!preview.has_blocking_references());
        assert!(parse_profile_removal_impact(&safe, "prof_2", 2).is_err());
        assert!(parse_profile_removal_impact(&safe, "prof_1", 3).is_err());
        let blocked = json!({
            "profileId": "prof_1", "exists": true, "revision": 2,
            "isDefault": true, "hasAutomationPolicy": false,
            "automationIds": ["auto_1"], "executionPolicyRunIds": [],
            "tabs": [{"workspaceId":"ws_1","tabId":"tab_1"}],
            "referenceCount": 3, "blockingReferenceCount": 2
        });
        let preview = parse_profile_removal_impact(&blocked, "prof_1", 2).unwrap();
        assert!(preview.has_blocking_references());
        assert!(preview.removal_message("Review").contains("automation"));
        assert!(preview.removal_message("Review").contains("tab"));
        assert!(parse_profile_removal_impact(&json!({"profileId":"prof_1"}), "prof_1", 2).is_err());
    }

    #[test]
    fn attached_agent_must_match_the_registered_host_workspace_and_tab() {
        let pending = LaunchedAgentTab {
            workspace_id: "ws_a".into(),
            profile_id: "p_a".into(),
            tab_id: "tab_a".into(),
        };
        let sessions = vec![
            TerminalSession {
                handle: "other_worktree".into(),
                workspace_id: "ws_b".into(),
                tab_id: "tab_a".into(),
                agent_type: Some("codex".into()),
                running: true,
                agent_state: None,
                interrupted: false,
                agent_state_started_at: None,
                agent_updated_at: None,
            },
            TerminalSession {
                handle: "plain_terminal".into(),
                workspace_id: "ws_a".into(),
                tab_id: "tab_a".into(),
                agent_type: None,
                running: true,
                agent_state: None,
                interrupted: false,
                agent_state_started_at: None,
                agent_updated_at: None,
            },
            TerminalSession {
                handle: "correct_agent".into(),
                workspace_id: "ws_a".into(),
                tab_id: "tab_a".into(),
                agent_type: Some("codex".into()),
                running: true,
                agent_state: None,
                interrupted: false,
                agent_state_started_at: None,
                agent_updated_at: None,
            },
        ];
        assert_eq!(
            matching_agent_handle(&pending, &sessions, "codex"),
            Some("correct_agent")
        );
        assert_eq!(
            matching_agent_handle(&pending, &sessions[..2], "codex"),
            Some("plain_terminal")
        );
    }

    #[test]
    fn profile_launch_attaches_authenticated_host_tab_without_optional_hook() {
        let pending = LaunchedAgentTab {
            workspace_id: "ws_ok".into(),
            profile_id: "profile_1".into(),
            tab_id: "tab_unique".into(),
        };
        let session = TerminalSession {
            handle: "pty_running".into(),
            workspace_id: "ws_ok".into(),
            tab_id: "tab_unique".into(),
            agent_type: None,
            running: true,
            agent_state: None,
            interrupted: false,
            agent_state_started_at: None,
            agent_updated_at: None,
        };
        assert_eq!(
            matching_agent_handle(&pending, &[session.clone()], "codex"),
            Some("pty_running"),
            "Host Profile tab must attach without waiting for optional Hook"
        );
        let mut stopped = session.clone();
        stopped.running = false;
        assert!(matching_agent_handle(&pending, &[stopped], "codex").is_none());
        let mut wrong_hook = session.clone();
        wrong_hook.agent_type = Some("claude".into());
        assert!(matching_agent_handle(&pending, &[wrong_hook], "codex").is_none());
        let mut wrong_tab = session;
        wrong_tab.tab_id = "tab_other".into();
        assert!(matching_agent_handle(&pending, &[wrong_tab], "codex").is_none());
    }

    #[test]
    fn editable_profile_catalog_carries_revision_but_never_commands() {
        let rows = parse_profile_catalog(&serde_json::json!({"items":[{
            "id":"profile_1","name":"Review","agentType":"codex",
            "launchMode":"command","revision":4,
            "command":"SECRET_COMMAND","managedConfig":{"secret":"VERY_SECRET"}
        }]}))
        .unwrap();
        assert_eq!(rows[0].revision, Some(4));
        assert!(!format!("{rows:?}").contains("SECRET"));
        assert!(validate_profile_name("  Review Runner  ").is_ok());
        assert!(validate_profile_name("   ").is_err());
        assert!(validate_profile_name("line\nfeed").is_err());
        assert!(validate_profile_name(&"x".repeat(257)).is_err());
        let missing = parse_profile_catalog(&serde_json::json!({"items":[{
            "id":"profile_2","name":"Legacy","agentType":"claude"
        }]}))
        .unwrap();
        assert_eq!(missing[0].revision, None);
        let first = parse_profile_catalog(&json!({"items":[{
            "id":"profile_3","name":"First","agentType":"claude","revision":0
        }]}))
        .unwrap();
        assert_eq!(first[0].revision, Some(0), "Host starts revisions at zero");
        assert!(parse_profile_catalog(&json!({"items":[{
            "id":"profile_4","name":"Invalid","agentType":"claude","revision":-1
        }]}))
        .is_err());
    }

    #[test]
    fn agent_catalog_keeps_only_display_fields_not_commands_or_secrets() {
        let rows = parse_profile_catalog(&json!({"items":[
            {"id":"prof_1","name":"Codex Task","agentType":"codex","launchMode":"managed",
             "command":"dangerous-looking command", "managedConfig":{"apiKey":"SECRET"}},
            {"id":"prof_2","name":"Claude Review","agentType":"claude","launchMode":"command"}
        ]}))
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "prof_1");
        assert!(rows[0].managed);
        assert!(!format!("{rows:?}").contains("SECRET"));
        assert!(!format!("{rows:?}").contains("dangerous-looking"));
        assert!(!rows[1].managed);
    }

    #[test]
    fn agent_catalog_rejects_malformed_duplicate_or_oversize_items() {
        assert!(parse_profile_catalog(&json!({})).is_err());
        assert!(parse_profile_catalog(
            &json!({"items":[{"id":"../unsafe","name":"n","agentType":"x"}]})
        )
        .is_err());
        assert!(parse_profile_catalog(
            &json!({"items":[{"id":"one","name":"bad\nname","agentType":"x"}]})
        )
        .is_err());
        assert!(parse_profile_catalog(&json!({"items":[
            {"id":"one","name":"A","agentType":"x"},
            {"id":"one","name":"B","agentType":"x"}
        ]}))
        .is_err());
        let oversized = (0..100)
            .map(|i| json!({"id":format!("profile_{i}"),"name":"x","agentType":"y"}))
            .collect::<Vec<_>>();
        assert!(parse_profile_catalog(&json!({"items":oversized})).is_err());
    }

    #[test]
    fn agent_launch_requires_same_workspace_profile_and_host_tab() {
        let reply = json!({"workspaceId":"ws_2","profileId":"prof_1","tabId":"tab_3"});
        let launched = parse_launched_agent(&reply, "prof_1", "ws_2").unwrap();
        assert_eq!(launched.tab_id, "tab_3");
        assert!(parse_launched_agent(&reply, "prof_2", "ws_2").is_err());
        assert!(parse_launched_agent(&reply, "prof_1", "ws_1").is_err());
        assert!(parse_launched_agent(
            &json!({"workspaceId":"ws_2","profileId":"prof_1","tabId":"../escape"}),
            "prof_1",
            "ws_2"
        )
        .is_err());
    }
}
