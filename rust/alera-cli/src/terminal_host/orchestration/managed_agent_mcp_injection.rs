//! Opt-in, per-spawn Alera MCP adapter configuration for managed Agent Profiles.
//!
//! Never mutate ~/.codex/config.toml, ~/.claude.json or other global client
//! configuration. The Host-issued per-PTY capability remains an environment
//! variable, never a command argument, tab payload or persisted config file.
//! Opaque Command-mode profiles and unrecognized adapters are left intact.

use std::path::Path;

use super::managed_agent_launch::ManagedAgentLaunch;

const SERVER_NAME: &str = "alera-workbench";
const OPT_IN_ENV: &str = "ALERA_MCP_PROFILE_INJECTION";

/// The alpha is disabled by default until supported platform acceptance.
pub fn enabled_by_opt_in() -> bool {
    std::env::var(OPT_IN_ENV).as_deref() == Ok("1")
}

/// Extend already resolved *managed* argv, before the initial prompt. Never
/// parse or rewrite opaque Command-mode launch lines.
pub fn inject_managed_agent_mcp(
    agent_type: &str,
    launch: &mut ManagedAgentLaunch,
    sidecar_exe: &Path,
    runtime_dir: &Path,
) -> bool {
    if !sidecar_exe.is_file() || !runtime_dir.is_dir() {
        return false;
    }
    let exe = sidecar_exe.to_string_lossy().into_owned();
    let runtime = runtime_dir.to_string_lossy().into_owned();
    let args = vec![
        "mcp".to_string(),
        "--runtime-dir".to_string(),
        runtime,
        "serve".to_string(),
    ];
    match agent_type {
        "codex" => {
            if launch
                .arguments
                .iter()
                .any(|arg| arg.contains(&format!("mcp_servers.{SERVER_NAME}")))
            {
                return false;
            }
            let Ok(command) = serde_json::to_string(&exe) else {
                return false;
            };
            let Ok(argv) = serde_json::to_string(&args) else {
                return false;
            };
            launch.arguments.extend([
                "--config".into(),
                format!("mcp_servers.{SERVER_NAME}.command={command}"),
                "--config".into(),
                format!("mcp_servers.{SERVER_NAME}.args={argv}"),
            ]);
            true
        }
        "claude" => {
            if launch
                .arguments
                .iter()
                .any(|arg| arg == "--mcp-config" || arg.starts_with("--mcp-config="))
            {
                // An explicit managed per-profile MCP config wins. Do not
                // trample unrelated servers or silently replace its path.
                return false;
            }
            let config = serde_json::json!({
                "mcpServers": {
                    SERVER_NAME: {
                        "command": exe,
                        "args": args,
                    }
                }
            });
            launch.arguments.push("--mcp-config".into());
            launch.arguments.push(config.to_string());
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal_host::orchestration::agent_profile_launch_snapshot::AgentInitialDeliveryMechanismV1;
    use crate::terminal_host::orchestration::agent_startup_command::append_initial_prompt_argument_for;

    fn cli() -> ManagedAgentLaunch {
        ManagedAgentLaunch {
            executable: "codex".into(),
            arguments: vec!["--search".into()],
        }
    }

    #[test]
    fn codex_profile_adds_only_per_launch_config_without_global_mutation() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("alera.exe");
        std::fs::write(&exe, b"stub").unwrap();
        let mut command = cli();
        assert!(inject_managed_agent_mcp(
            "codex",
            &mut command,
            &exe,
            dir.path()
        ));
        assert_eq!(command.arguments.len(), 5);
        assert_eq!(command.arguments[1], "--config");
        assert!(command.arguments[2].starts_with("mcp_servers.alera-workbench.command="));
        assert_eq!(command.arguments[3], "--config");
        assert!(command.arguments[4].contains("mcp_servers.alera-workbench.args="));
        assert!(command.arguments[4].contains("--runtime-dir"));
        assert!(!inject_managed_agent_mcp(
            "codex",
            &mut command,
            &exe,
            dir.path()
        ));
    }

    #[test]
    fn claude_profile_adds_json_without_replacing_explicit_user_config() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("alera.exe");
        std::fs::write(&exe, b"stub").unwrap();
        let mut command = ManagedAgentLaunch {
            executable: "claude".into(),
            arguments: vec![],
        };
        assert!(inject_managed_agent_mcp(
            "claude",
            &mut command,
            &exe,
            dir.path()
        ));
        assert_eq!(command.arguments[0], "--mcp-config");
        let config: serde_json::Value = serde_json::from_str(&command.arguments[1]).unwrap();
        assert_eq!(
            config["mcpServers"][SERVER_NAME]["command"],
            exe.to_string_lossy().as_ref()
        );
        assert_eq!(config["mcpServers"][SERVER_NAME]["args"][0], "mcp");
        assert!(!inject_managed_agent_mcp(
            "claude",
            &mut command,
            &exe,
            dir.path()
        ));
    }

    #[test]
    fn unknown_adapters_and_absent_sidecar_fail_closed_without_mutating_argv() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.exe");
        let mut launch = cli();
        assert!(!inject_managed_agent_mcp(
            "codex",
            &mut launch,
            &missing,
            dir.path()
        ));
        assert!(!inject_managed_agent_mcp(
            "untrusted",
            &mut launch,
            dir.path(),
            dir.path()
        ));
        assert_eq!(launch.arguments, vec!["--search"]);
    }

    #[test]
    fn claude_vararg_mcp_flag_does_not_consume_initial_prompt() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("alera.exe");
        std::fs::write(&exe, b"stub").unwrap();
        let mut launch = ManagedAgentLaunch {
            executable: "claude".into(),
            arguments: vec!["--permission-mode".into(), "manual".into()],
        };
        assert!(inject_managed_agent_mcp(
            "claude",
            &mut launch,
            &exe,
            dir.path()
        ));
        append_initial_prompt_argument_for(
            &AgentInitialDeliveryMechanismV1::PositionalAfterTerminator,
            &mut launch.arguments,
            "- Inspect the current workspace",
        );
        assert_eq!(
            &launch.arguments[launch.arguments.len() - 2..],
            ["--", "- Inspect the current workspace",]
        );
        let index = launch
            .arguments
            .iter()
            .position(|arg| arg == "--mcp-config")
            .unwrap();
        assert_eq!(index, 2);
        assert_eq!(launch.arguments[index + 2], "--");
        assert_eq!(launch.arguments[0], "--permission-mode");
    }

    #[test]
    fn codex_mcp_options_precede_existing_terminated_prompt() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("alera.exe");
        std::fs::write(&exe, b"stub").unwrap();
        let mut launch = cli();
        assert!(inject_managed_agent_mcp(
            "codex",
            &mut launch,
            &exe,
            dir.path()
        ));
        append_initial_prompt_argument_for(
            &AgentInitialDeliveryMechanismV1::PositionalAfterTerminator,
            &mut launch.arguments,
            "Use alera.list_projects in read-only mode",
        );
        assert_eq!(
            &launch.arguments[launch.arguments.len() - 2..],
            ["--", "Use alera.list_projects in read-only mode",]
        );
        assert!(launch.arguments[..launch.arguments.len() - 2]
            .iter()
            .any(|arg| arg.starts_with("mcp_servers.alera-workbench.args=")));
    }
}
