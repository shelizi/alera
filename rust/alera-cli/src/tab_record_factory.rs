use alera_core::runtime::WorkspaceTabRecord;
use chrono::Utc;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::cli::TabCreateArgs;

const SUPPORTED_TAB_KINDS: &[&str] = &["terminal", "editor", "markdownViewer", "pdf", "gitDiff"];

pub(crate) fn tab_from_args(args: TabCreateArgs) -> Result<WorkspaceTabRecord, String> {
    let now = Utc::now();
    let id = Uuid::new_v4().to_string();
    let kind = SUPPORTED_TAB_KINDS
        .iter()
        .copied()
        .find(|supported| *supported == args.kind)
        .ok_or_else(|| {
            format!(
                "Unsupported tab kind \"{}\". Supported kinds: {}.",
                args.kind,
                SUPPORTED_TAB_KINDS.join(", ")
            )
        })?;
    if (args.command.is_some() || args.spawn) && kind != "terminal" {
        return Err("--command and --spawn are only supported for terminal tabs.".to_string());
    }
    let mut payload = serde_json::Map::new();
    if kind == "terminal" {
        payload.insert("terminalSessionId".to_string(), json!(id));
        if let Some(command) = args
            .command
            .as_deref()
            .map(str::trim)
            .filter(|command| !command.is_empty())
        {
            payload.insert("initialCommand".to_string(), json!(command));
        }
        if args.spawn {
            payload.insert("spawnOnCreate".to_string(), json!(true));
        }
    }
    Ok(WorkspaceTabRecord {
        id: id.clone(),
        workspace_id: args.workspace_id,
        kind: kind.to_string(),
        title: args.title,
        created_at: now,
        updated_at: now,
        payload: Value::Object(payload),
    })
}

/// Bounded, profile-free Native launcher. The canonical registry determines
/// which Agent may launch; executable path is a single argv value never
/// interpolated into shell text, and must be an existing absolute file.
pub(crate) fn agent_tab_from_args(
    args: crate::cli::TerminalCreateAgentArgs,
) -> Result<WorkspaceTabRecord, String> {
    let valid_id = |id: &str| {
        !id.is_empty()
            && id.len() <= 256
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
    };
    if !valid_id(&args.workspace_id) {
        return Err("Invalid registered Host workspace id".into());
    }
    let descriptor = alera_core::agent_descriptor::agent_descriptor(&args.agent_id)
        .filter(|desc| desc.id == args.agent_id)
        .ok_or("Unsupported installed Agent id")?;
    let executable = std::path::Path::new(&args.executable_path);
    if args.executable_path.is_empty()
        || args.executable_path.len() > 4096
        || args.executable_path.contains(['\0', '\n', '\r'])
        || !executable.is_absolute()
        || !executable.is_file()
    {
        return Err("Agent executable must be an existing absolute file".into());
    }
    let now = Utc::now();
    let tab_id = Uuid::new_v4().to_string();
    Ok(WorkspaceTabRecord {
        id: tab_id.clone(),
        workspace_id: args.workspace_id,
        kind: "terminal".to_string(),
        title: descriptor.display_name.to_string(),
        created_at: now,
        updated_at: now,
        payload: json!({
            "terminalSessionId": tab_id,
            "agentType": descriptor.id,
            "spawnOnCreate": true,
            "exitShellOnAgentExit": true,
            "initialManagedAgentLaunch": {
                "executable": args.executable_path,
                "arguments": [],
            }
        }),
    })
}

#[cfg(test)]
mod agent_direct_launch_tests {
    use super::*;
    use crate::cli::TerminalCreateAgentArgs;

    #[test]
    fn direct_agent_tab_uses_host_owned_spawn_and_canonical_descriptor_not_profile() {
        let exe = std::env::current_exe().unwrap();
        let tab = agent_tab_from_args(TerminalCreateAgentArgs {
            workspace_id: "ws_valid".into(),
            agent_id: "claude".into(),
            executable_path: exe.to_string_lossy().into_owned(),
        })
        .expect("valid local agent");
        assert_eq!(tab.workspace_id, "ws_valid");
        assert_eq!(tab.kind, "terminal");
        assert_eq!(tab.title, "Claude Code");
        assert_eq!(tab.payload["agentType"], "claude");
        assert_eq!(tab.payload["spawnOnCreate"], true);
        assert_eq!(tab.payload["exitShellOnAgentExit"], true);
        assert!(tab.payload["initialCommand"].is_null());
        assert!(tab.payload["agentProfileId"].is_null());
        assert_eq!(tab.payload["terminalSessionId"], tab.id);
        assert_eq!(
            tab.payload["initialManagedAgentLaunch"]["executable"],
            exe.to_string_lossy().as_ref()
        );
    }

    #[test]
    fn direct_agent_rejects_unknown_adapter_unsafe_workspace_and_invalid_override() {
        let exe = std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        for (workspace_id, agent_id, executable_path) in [
            ("../wrong", "codex", exe.as_str()),
            ("ws", "echo --do-not-run", exe.as_str()),
            ("ws", "codex", "./relative-script.cmd"),
            ("ws", "codex", "C:/missing/private.exe"),
        ] {
            assert!(agent_tab_from_args(TerminalCreateAgentArgs {
                workspace_id: workspace_id.into(),
                agent_id: agent_id.into(),
                executable_path: executable_path.into(),
            })
            .is_err());
        }
    }
}
