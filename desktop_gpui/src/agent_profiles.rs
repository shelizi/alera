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

/// Only an authenticated Host session matching both identifiers and an
/// Agent type can fulfill a Native launch confirmation.
pub fn matching_agent_handle<'a>(
    pending: &LaunchedAgentTab,
    sessions: &'a [TerminalSession],
) -> Option<&'a str> {
    sessions
        .iter()
        .find(|session| {
            session.workspace_id == pending.workspace_id
                && session.tab_id == pending.tab_id
                && session
                    .agent_type
                    .as_deref()
                    .is_some_and(|name| !name.is_empty())
        })
        .map(|session| session.handle.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
            },
            TerminalSession {
                handle: "plain_terminal".into(),
                workspace_id: "ws_a".into(),
                tab_id: "tab_a".into(),
                agent_type: None,
                running: true,
                agent_state: None,
                interrupted: false,
            },
            TerminalSession {
                handle: "correct_agent".into(),
                workspace_id: "ws_a".into(),
                tab_id: "tab_a".into(),
                agent_type: Some("codex".into()),
                running: true,
                agent_state: None,
                interrupted: false,
            },
        ];
        assert_eq!(
            matching_agent_handle(&pending, &sessions),
            Some("correct_agent")
        );
        assert!(matching_agent_handle(&pending, &sessions[..2]).is_none());
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
