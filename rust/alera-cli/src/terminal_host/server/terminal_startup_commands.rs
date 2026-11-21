use alera_core::runtime::WorkspaceTabRecord;
use serde_json::Value;

use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::orchestration::agent_profile_launch_snapshot::{
    AgentInitialDeliveryMechanismV1, AgentInitialDeliveryReplayV1, AgentProfileLaunchSnapshotV1,
    AGENT_PROFILE_LAUNCH_SNAPSHOT_KEY,
};

use crate::terminal_host::orchestration::managed_agent_launch::ManagedAgentLaunch;

use super::agent_title_state::AgentTitleState;

/// Only user-owned, long-lived Agent tabs participate in persistent resume.
/// A one-shot automation or setup command must retain its exit/cleanup policy.
pub(super) fn user_agent_recovery_allowed(tab: &WorkspaceTabRecord) -> bool {
    tab.payload["automationOwned"] != true && !auto_closes_on_success(tab)
}

/// A Host terminal handle is not a provider conversation. Only a provider
/// UUID bound to a tab (via a Hook or a caller-selected Claude UUID) may
/// authorize resume. Never use --last across multiple Agent tabs.
pub(super) fn provider_resume_id(tab: &WorkspaceTabRecord) -> Option<(&str, String)> {
    let agent = tab_agent_type(tab)?;
    if !matches!(agent, "codex" | "claude") {
        return None;
    }
    let state = AgentTitleState::read(tab)?;
    if state.agent.as_deref() != Some(agent) {
        return None;
    }
    let id = uuid::Uuid::parse_str(state.native_id.as_deref()?).ok()?;
    Some((agent, id.to_string()))
}

/// A Codex Hook's exact rollout file must still contain the saved native
/// session ID before we relaunch a restored tab. If that local record was
/// removed, fail closed and keep the tab instead of starting a fresh chat.
/// Legacy checkpoints created before transcript proofs remain supported.
pub(super) fn validate_codex_resume_checkpoint(tab: &WorkspaceTabRecord) -> HostResult<()> {
    let Some(("codex", id)) = provider_resume_id(tab) else {
        return Ok(());
    };
    let Some(transcript_path) =
        AgentTitleState::read(tab).and_then(|state| state.codex_transcript_path)
    else {
        return Ok(());
    };
    if !super::codex_resume_identity::verified_transcript(&id, &transcript_path, None) {
        return Err(HostError::state(
            "Codex saved session transcript is missing or no longer matches its native ID. Keeping the Agent tab; automatic resume is blocked.",
        ));
    }
    Ok(())
}

/// Only append a resume command to the known provider executable, not to
/// custom profile wrappers whose argument grammar is unknown.
pub(super) fn configure_provider_resume(
    launch: &mut ManagedAgentLaunch,
    agent: &str,
    native_id: &str,
) -> bool {
    let filename = launch
        .executable
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let known_cli = match agent {
        "codex" => matches!(
            filename.as_str(),
            "codex" | "codex.exe" | "codex.cmd" | "codex.bat"
        ),
        "claude" => matches!(
            filename.as_str(),
            "claude" | "claude.exe" | "claude.cmd" | "claude.bat"
        ),
        _ => false,
    };
    if !known_cli || uuid::Uuid::parse_str(native_id).is_err() {
        return false;
    }
    if launch.arguments.iter().any(|arg| {
        matches!(
            arg.as_str(),
            "resume" | "--resume" | "--last" | "--session-id"
        ) || arg.starts_with("--resume=")
            || arg.starts_with("--session-id=")
            || (agent == "claude" && matches!(arg.as_str(), "-r" | "--continue" | "-c"))
    }) {
        return false;
    }
    match agent {
        "codex" => {
            // --no-daemon is a global flag: retain it BEFORE the resume
            // subcommand even when the original profile listed it later.
            let no_daemon = launch.arguments.iter().any(|arg| arg == "--no-daemon");
            if no_daemon {
                launch.arguments.retain(|arg| arg != "--no-daemon");
            }
            launch.arguments.insert(0, native_id.to_owned());
            launch.arguments.insert(0, "resume".to_owned());
            if no_daemon {
                launch.arguments.insert(0, "--no-daemon".to_owned());
            }
        }
        "claude" => {
            launch.arguments.push("--resume".to_owned());
            launch.arguments.push(native_id.to_owned());
        }
        _ => return false,
    }
    true
}

/// Command-mode Agent Profiles may use a bare canonical CLI without Managed
/// launch metadata. Resume only those exact simple commands: parsing arbitrary
/// shell scripts or appending flags to wrappers could execute a different
/// command. Existing custom commands retain their original behavior.
pub(super) fn resume_bare_agent_command(
    command: &str,
    agent: &str,
    native_id: &str,
    shell: &str,
) -> Option<String> {
    let executable = command.trim();
    if !matches!(
        executable.to_ascii_lowercase().as_str(),
        "codex" | "codex.exe" | "codex.cmd" | "claude" | "claude.exe" | "claude.cmd"
    ) {
        return None;
    }
    let mut launch = ManagedAgentLaunch {
        executable: executable.to_owned(),
        arguments: if agent == "codex" {
            vec!["--no-daemon".to_owned()]
        } else {
            Vec::new()
        },
    };
    configure_provider_resume(&mut launch, agent, native_id).then(|| {
        crate::terminal_host::orchestration::managed_launch_shell_rendering::render_managed_launch(
            &launch, shell,
        )
    })
}

/// Claude Code supports choosing a new native UUID before its first prompt.
/// Persisting this UUID per Tab removes the first-Hook timing race. Only the
/// genuine Claude CLI may receive this flag; custom wrappers stay untouched.
pub(super) fn configure_claude_new_session(
    launch: &mut ManagedAgentLaunch,
    native_id: &str,
) -> bool {
    let mut resume_probe = launch.clone();
    if !configure_provider_resume(&mut resume_probe, "claude", native_id) {
        return false;
    }
    launch.arguments.push("--session-id".to_owned());
    launch.arguments.push(native_id.to_owned());
    true
}

pub(super) fn seed_claude_native_session(
    tab: &mut WorkspaceTabRecord,
    launch: &ManagedAgentLaunch,
) -> Option<String> {
    if tab_agent_type(tab) != Some("claude") {
        return None;
    }
    let mut state = AgentTitleState::read(tab).unwrap_or_else(|| AgentTitleState::new(false));
    if state.native_id.is_some() || state.closed {
        return None;
    }
    let id = uuid::Uuid::new_v4().to_string();
    let mut probe = launch.clone();
    if !configure_claude_new_session(&mut probe, &id) {
        return None;
    }
    state.observe("claude", Some(&id), "", 0);
    state.write(tab);
    Some(id)
}

pub(super) fn terminal_session_id(tab: &WorkspaceTabRecord) -> String {
    tab.payload
        .get("terminalSessionId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(&tab.id)
        .to_string()
}

pub(super) fn initial_command(tab: &WorkspaceTabRecord) -> HostResult<Option<String>> {
    if let Some(snapshot) = agent_profile_launch_snapshot(tab)? {
        return Ok(snapshot.command().map(str::to_string));
    }
    Ok(tab
        .payload
        .get("initialCommand")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string))
}

pub(super) fn auto_closes_on_success(tab: &WorkspaceTabRecord) -> bool {
    tab.payload
        .get("autoCloseOnSuccess")
        .and_then(Value::as_bool)
        == Some(true)
}

pub(super) fn auto_close_setup_command(command: &str, interactive_shell: &str) -> String {
    let executable = interactive_shell
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(interactive_shell)
        .to_ascii_lowercase();
    match executable.as_str() {
        "cmd" | "cmd.exe" => format!("{command} & exit /b"),
        "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe" => {
            format!("& {{ {command} }}; exit $LASTEXITCODE")
        }
        _ => format!("exec {command}"),
    }
}

pub(super) fn initial_managed_agent_launch(
    tab: &WorkspaceTabRecord,
) -> HostResult<Option<crate::terminal_host::orchestration::managed_agent_launch::ManagedAgentLaunch>>
{
    if let Some(snapshot) = agent_profile_launch_snapshot(tab)? {
        return Ok(snapshot.managed_launch());
    }
    tab.payload
        .get("initialManagedAgentLaunch")
        .filter(|value| !value.is_null())
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| HostError::format(format!("invalid managed agent launch: {error}")))
}

pub(super) fn delivers_initial_command_once(tab: &WorkspaceTabRecord) -> bool {
    tab.payload
        .get("initialCommandOnce")
        .and_then(Value::as_bool)
        == Some(true)
}

pub(super) fn delivers_initial_prompt_once(tab: &WorkspaceTabRecord) -> bool {
    if let Ok(Some(snapshot)) = agent_profile_launch_snapshot(tab) {
        return snapshot.initial_delivery.replay == AgentInitialDeliveryReplayV1::Once;
    }
    tab.payload
        .get("initialPromptOnce")
        .and_then(Value::as_bool)
        == Some(true)
}

pub(super) fn replays_initial_prompt_on_restart(tab: &WorkspaceTabRecord) -> HostResult<bool> {
    Ok(match agent_profile_launch_snapshot(tab)? {
        Some(snapshot) => {
            snapshot.initial_delivery.replay == AgentInitialDeliveryReplayV1::OnRestart
        }
        None => !delivers_initial_prompt_once(tab),
    })
}

pub(super) fn initial_prompt(tab: &WorkspaceTabRecord) -> Option<String> {
    tab.payload
        .get("initialPrompt")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(super) fn pending_agent_type(tab: &WorkspaceTabRecord) -> Option<&str> {
    tab.payload
        .pointer("/pendingAgentPrompt/agent")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
}

/// Which agent the tab launches, for deciding the shape of its initial prompt.
///
/// The fallbacks read tabs written by an older host, which named the agent only
/// inside the payload it was going to deliver later. The app attaches to
/// whichever sidecar is already running, so both spellings have to resolve.
pub(super) fn tab_agent_type(tab: &WorkspaceTabRecord) -> Option<&str> {
    if let Some(agent_type) = tab
        .payload
        .pointer("/agentProfileLaunchV1/agentType")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        return Some(agent_type);
    }
    [
        "/agentType",
        "/pendingAgentPrompt/agent",
        "/orchestrationSpawn/agent",
    ]
    .into_iter()
    .find_map(|pointer| {
        tab.payload
            .pointer(pointer)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
    })
}

pub(super) fn initial_delivery_mechanism(
    tab: &WorkspaceTabRecord,
) -> HostResult<Option<AgentInitialDeliveryMechanismV1>> {
    Ok(agent_profile_launch_snapshot(tab)?.map(|snapshot| snapshot.initial_delivery.mechanism))
}

pub(super) fn agent_profile_id(tab: &WorkspaceTabRecord) -> Option<&str> {
    tab.payload
        .pointer("/agentProfileLaunchV1/profile/id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            tab.payload
                .get("agentProfileId")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
        })
}

fn agent_profile_launch_snapshot(
    tab: &WorkspaceTabRecord,
) -> HostResult<Option<AgentProfileLaunchSnapshotV1>> {
    let snapshot: Option<AgentProfileLaunchSnapshotV1> = tab
        .payload
        .get(AGENT_PROFILE_LAUNCH_SNAPSHOT_KEY)
        .filter(|value| !value.is_null())
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| {
            HostError::format(format!("invalid agent profile launch snapshot: {error}"))
        })?;
    if snapshot
        .as_ref()
        .is_some_and(|snapshot| snapshot.version != 1)
    {
        return Err(HostError::format(
            "unsupported agent profile launch snapshot version".to_string(),
        ));
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    fn tab(payload: Value) -> WorkspaceTabRecord {
        let now = Utc::now();
        WorkspaceTabRecord {
            id: "tab-1".to_string(),
            workspace_id: "workspace-1".to_string(),
            kind: "terminal".to_string(),
            title: "Agent".to_string(),
            created_at: now,
            updated_at: now,
            payload,
        }
    }

    fn tab_with_native_session(agent: &str, native: &str) -> WorkspaceTabRecord {
        let mut tab = tab(json!({"agentType": agent}));
        let mut state = AgentTitleState::new(true);
        state.observe(agent, Some(native), "User task", 0);
        state.write(&mut tab);
        tab
    }

    #[test]
    fn codex_resumes_exact_provider_session_not_host_or_title_uuid() {
        let id = "550e8400-e29b-41d4-a716-446655440000";
        let tab = tab_with_native_session("codex", id);
        assert_ne!(tab.payload["agentTitleConversationId"].as_str(), Some(id));
        let (agent, restored_id) = provider_resume_id(&tab).unwrap();
        assert_eq!((agent, restored_id.as_str()), ("codex", id));

        let mut launch = ManagedAgentLaunch {
            executable: r"C:\tools\codex.cmd".into(),
            arguments: vec!["--model".into(), "gpt-6".into()],
        };
        assert!(configure_provider_resume(&mut launch, agent, &restored_id));
        assert_eq!(launch.arguments, vec!["resume", id, "--model", "gpt-6"]);
    }

    #[test]
    fn codex_global_no_daemon_precedes_resume_and_tab_identity_survives_json() {
        let id = "550e8400-e29b-41d4-a716-446655440000";
        let tab = tab_with_native_session("codex", id);
        let saved_json = serde_json::to_string(&tab).unwrap();
        let reopened: WorkspaceTabRecord = serde_json::from_str(&saved_json).unwrap();
        assert_eq!(
            provider_resume_id(&reopened),
            Some(("codex", id.to_string()))
        );

        let mut daemon_launch = ManagedAgentLaunch {
            executable: "codex".into(),
            arguments: vec!["--model".into(), "gpt-6".into(), "--no-daemon".into()],
        };
        assert!(configure_provider_resume(&mut daemon_launch, "codex", id));
        assert_eq!(
            daemon_launch.arguments,
            vec!["--no-daemon", "resume", id, "--model", "gpt-6"]
        );
    }

    #[test]
    fn claude_resumes_only_own_conversation_and_preserves_existing_flags() {
        let first = "550e8400-e29b-41d4-a716-446655440001";
        let second = "550e8400-e29b-41d4-a716-446655440002";
        let a = tab_with_native_session("claude", first);
        let b = tab_with_native_session("claude", second);
        assert_ne!(provider_resume_id(&a), provider_resume_id(&b));

        let (agent, id) = provider_resume_id(&b).unwrap();
        let mut launch = ManagedAgentLaunch {
            executable: "/usr/local/bin/claude".into(),
            arguments: vec!["--model".into(), "sonnet".into()],
        };
        assert!(configure_provider_resume(&mut launch, agent, &id));
        assert_eq!(
            launch.arguments,
            vec!["--model", "sonnet", "--resume", second]
        );
    }

    #[test]
    fn fresh_claude_tabs_preassign_distinct_native_session_ids_before_first_hook() {
        let mut first = tab(json!({"agentType": "claude"}));
        let mut second = tab(json!({"agentType": "claude"}));
        second.id = "tab-2".to_owned();
        let launch = ManagedAgentLaunch {
            executable: "claude.cmd".to_owned(),
            arguments: vec!["--model".into(), "sonnet".into()],
        };

        let first_id = seed_claude_native_session(&mut first, &launch).unwrap();
        let second_id = seed_claude_native_session(&mut second, &launch).unwrap();
        assert_ne!(first_id, second_id);
        assert!(uuid::Uuid::parse_str(&first_id).is_ok());
        assert_eq!(
            provider_resume_id(&first),
            Some(("claude", first_id.clone()))
        );
        assert_eq!(provider_resume_id(&second), Some(("claude", second_id)));
        assert_eq!(seed_claude_native_session(&mut first, &launch), None);

        let mut new_launch = launch.clone();
        assert!(configure_claude_new_session(&mut new_launch, &first_id));
        assert_eq!(
            new_launch.arguments,
            vec!["--model", "sonnet", "--session-id", first_id.as_str()]
        );

        let restored: WorkspaceTabRecord =
            serde_json::from_value(serde_json::to_value(first).unwrap()).unwrap();
        let (_, saved_id) = provider_resume_id(&restored).unwrap();
        let mut resumed = launch;
        assert!(configure_provider_resume(&mut resumed, "claude", &saved_id));
        assert_eq!(
            resumed.arguments,
            vec!["--model", "sonnet", "--resume", saved_id.as_str()]
        );
    }

    #[test]
    fn claude_seeding_does_not_override_custom_commands_or_explicit_sessions() {
        for (exe, args) in [
            ("claude-wrapper", vec![]),
            ("claude", vec!["--session-id".into(), "custom".into()]),
            ("claude", vec!["--resume".into(), "existing".into()]),
            ("codex", vec![]),
        ] {
            let mut tab = tab(json!({"agentType":"claude"}));
            let launch = ManagedAgentLaunch {
                executable: exe.into(),
                arguments: args,
            };
            assert_eq!(seed_claude_native_session(&mut tab, &launch), None);
            assert!(provider_resume_id(&tab).is_none());
        }
    }

    #[test]
    fn agent_resume_retains_user_tabs_but_not_automation_or_setup_tabs() {
        let id = "550e8400-e29b-41d4-a716-446655440079";
        let ordinary = tab_with_native_session("claude", id);
        assert!(user_agent_recovery_allowed(&ordinary));

        let mut automation = ordinary.clone();
        automation.payload["automationOwned"] = json!(true);
        assert!(!user_agent_recovery_allowed(&automation));

        let mut setup = ordinary;
        setup.payload["autoCloseOnSuccess"] = json!(true);
        assert!(!user_agent_recovery_allowed(&setup));
    }

    #[test]
    fn malformed_or_mismatched_provider_ids_never_authorize_resume() {
        for id in [
            "",
            "../private",
            "--last",
            "$(whoami)",
            "not-a-provider-uuid",
        ] {
            let tab = tab_with_native_session("codex", id);
            assert_eq!(provider_resume_id(&tab), None);
        }
        let mut mismatch = tab_with_native_session("codex", "550e8400-e29b-41d4-a716-446655440003");
        mismatch.payload["agentType"] = json!("claude");
        assert!(provider_resume_id(&mismatch).is_none());
        assert!(provider_resume_id(&tab(json!({"agentType":"pi"}))).is_none());
    }

    #[test]
    fn custom_wrappers_and_existing_resume_switches_are_never_rewritten() {
        let id = "550e8400-e29b-41d4-a716-446655440004";
        for (exe, agent, args) in [
            ("custom-agent-wrapper", "codex", vec![]),
            ("claude", "claude", vec!["--continue".to_owned()]),
            ("codex", "codex", vec!["resume".to_owned(), id.to_owned()]),
            ("codex", "codex", vec!["--last".to_owned()]),
        ] {
            let mut launch = ManagedAgentLaunch {
                executable: exe.into(),
                arguments: args.clone(),
            };
            assert!(!configure_provider_resume(&mut launch, agent, id));
            assert_eq!(launch.arguments, args);
        }
    }

    #[test]
    fn closed_provider_session_retains_resume_id_after_restart() {
        let id = "550e8400-e29b-41d4-a716-446655440006";
        let mut saved = tab_with_native_session("claude", id);
        saved.payload["agentTitleStateV1"]["closed"] = json!(true);
        let reopened: WorkspaceTabRecord =
            serde_json::from_value(serde_json::to_value(saved).unwrap()).unwrap();
        assert_eq!(
            provider_resume_id(&reopened),
            Some(("claude", id.to_owned()))
        );
    }

    #[test]
    fn command_profiles_resume_only_bare_provider_commands() {
        let id = "550e8400-e29b-41d4-a716-446655440005";
        assert_eq!(
            resume_bare_agent_command("claude", "claude", id, "pwsh.exe"),
            Some(format!("'claude' '--resume' '{id}'"))
        );
        assert_eq!(
            resume_bare_agent_command("codex", "codex", id, "pwsh.exe"),
            Some(format!("'codex' '--no-daemon' 'resume' '{id}'"))
        );
        for command in [
            "ccs claude",
            "claude --print",
            "codex --model o3",
            "claude && echo hi",
        ] {
            assert_eq!(
                resume_bare_agent_command(command, "claude", id, "pwsh.exe"),
                None
            );
        }
    }

    #[test]
    fn legacy_agent_tab_launch_fields_remain_readable() {
        let tab = tab(json!({
            "initialCommand": "codex --search",
            "agentType": "codex",
            "agentProfileId": "legacy-profile",
            "initialPromptOnce": true
        }));

        assert_eq!(
            initial_command(&tab).unwrap().as_deref(),
            Some("codex --search")
        );
        assert_eq!(tab_agent_type(&tab), Some("codex"));
        assert_eq!(agent_profile_id(&tab), Some("legacy-profile"));
        assert!(delivers_initial_prompt_once(&tab));
        assert!(initial_delivery_mechanism(&tab).unwrap().is_none());
    }

    #[test]
    fn setup_command_replaces_posix_shell() {
        assert_eq!(
            auto_close_setup_command("/bin/sh \"/tmp/setup.sh\"", "/bin/zsh"),
            "exec /bin/sh \"/tmp/setup.sh\""
        );
    }

    #[test]
    fn setup_command_preserves_cmd_exit_status() {
        assert_eq!(
            auto_close_setup_command("cmd /d /c \"C:\\\\setup.cmd\"", "cmd.exe"),
            "cmd /d /c \"C:\\\\setup.cmd\" & exit /b"
        );
    }

    #[test]
    fn setup_command_preserves_powershell_exit_status() {
        assert_eq!(
            auto_close_setup_command("cmd /d /c \"C:\\\\setup.cmd\"", "pwsh.exe"),
            "& { cmd /d /c \"C:\\\\setup.cmd\" }; exit $LASTEXITCODE"
        );
    }
}
