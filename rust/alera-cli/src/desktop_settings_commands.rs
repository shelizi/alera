//! Human-owned CLI bridge for Classic/GPUI shared settings. Never starts
//! a Runtime Host and never prints whole configuration or secret material.
use serde_json::{json, Value};

use crate::cli::{DesktopSettingsAction, DesktopSettingsCommand};
use crate::runtime_host_client::RuntimeHostRpcClient;
use crate::{print_error, print_value, runtime_dir, USAGE_EXIT_CODE};

/// The CLI supports only a narrow Native pinned-tab readback; never emit a
/// WorkspaceTabRecord payload that may include Agent instructions or tokens.
fn native_pin_projection(record: &Value) -> Option<Value> {
    let id = record.get("id")?.as_str()?;
    let workspace_id = record.get("workspaceId")?.as_str()?;
    let updated_at = record.get("updatedAt")?.as_str()?;
    let payload = record.get("payload")?.as_object()?;
    let session = payload.get("terminalSessionId")?.as_str()?;
    if id.is_empty() || workspace_id.is_empty() || session.is_empty() || updated_at.is_empty() {
        return None;
    }
    Some(json!({
        "id": id,
        "workspaceId": workspace_id,
        "updatedAt": updated_at,
        "pinned": payload.get("pinned") == Some(&json!(true)),
        "terminalSessionId": session,
    }))
}

// The exact Hook CAS contract is compiled by both this CLI and the
// small isolated offline test harness; no duplicated production logic.
#[path = "agent_hook_cas_contract.rs"]
mod hook_cas_contract;
use hook_cas_contract::{
    native_agent_hook_cas_payload, native_agent_hook_cas_readback, native_agent_hooks_projection,
};

pub async fn run(command: DesktopSettingsCommand) -> i32 {
    if !command.no_start {
        eprintln!("Shared settings require --no-start and an existing Runtime Host");
        return USAGE_EXIT_CODE;
    }
    let runtime_path = runtime_dir(&command.runtime);
    let Some(mut client) = (match RuntimeHostRpcClient::connect(&runtime_path).await {
        Ok(client) => client,
        Err(error) => return print_error(error),
    }) else {
        return print_error("Existing Alera Runtime Host is not running");
    };
    if matches!(
        &command.action,
        DesktopSettingsAction::SidebarOrderGet
            | DesktopSettingsAction::SidebarSortSet { .. }
            | DesktopSettingsAction::SidebarOrderMove { .. }
    ) {
        return crate::sidebar_order_commands::run(
            &mut client,
            &command.action,
            command.output.json,
        )
        .await;
    }
    let (request, payload) = match command.action {
        DesktopSettingsAction::SidebarOrderGet
        | DesktopSettingsAction::SidebarSortSet { .. }
        | DesktopSettingsAction::SidebarOrderMove { .. } => unreachable!("handled above"),
        DesktopSettingsAction::Get => ("desktopSettings.get", json!({})),
        DesktopSettingsAction::AgentHooksGet => ("runtimeSettings.get", json!({})),
        DesktopSettingsAction::AgentHooksCas {
            agent_id,
            expected_revision,
            expected_enabled,
            enabled,
        } => {
            // Read only from the already authenticated, existing Host.
            // On failure never retry blindly or manufacture a revision.
            let current = match client
                .request_value("runtimeSettings.get", &json!({}))
                .await
            {
                Ok(snapshot) => snapshot,
                Err(_) => return print_error("Host Hook settings unavailable"),
            };
            match native_agent_hook_cas_payload(
                &current,
                &agent_id,
                expected_revision,
                expected_enabled,
                enabled,
            ) {
                Ok(Some(payload)) => ("runtimeSettings.update", payload),
                Ok(None) => {
                    let safe = match native_agent_hooks_projection(&current) {
                        Ok(projection) => projection,
                        Err(error) => return print_error(error),
                    };
                    print_value(&safe, command.output.json, "Agent Hook already matches");
                    return 0;
                }
                Err(error) => return print_error(error),
            }
        }
        DesktopSettingsAction::FavoritesGet { root_path } => {
            ("gitFavorites.forWorkspace", json!({"rootPath":root_path}))
        }
        DesktopSettingsAction::FavoritesSeed {
            root_path,
            branches,
        } => {
            let branches: Vec<String> = match serde_json::from_str::<Vec<String>>(&branches) {
                Ok(v) if v.len() <= 80 => v,
                _ => {
                    return print_error("favorites seed requires JSON array of at most 80 branches")
                }
            };
            (
                "gitFavorites.seedNative",
                json!({"rootPath":root_path,"branches":branches}),
            )
        }
        DesktopSettingsAction::FavoritesSet {
            root_path,
            expected,
            value,
        } => {
            let expected: Vec<String> = match serde_json::from_str::<Vec<String>>(&expected) {
                Ok(v) if v.len() <= 80 => v,
                _ => {
                    return print_error(
                        "favorites expected must be a JSON array of at most 80 branches",
                    )
                }
            };
            let value: Vec<String> = match serde_json::from_str::<Vec<String>>(&value) {
                Ok(v) if v.len() <= 80 => v,
                _ => {
                    return print_error(
                        "favorites value must be a JSON array of at most 80 branches",
                    )
                }
            };
            (
                "gitFavorites.casForWorkspace",
                json!({"rootPath":root_path,"expected":expected,"value":value}),
            )
        }
        DesktopSettingsAction::MigrateProfile { profile_id } => (
            "desktopSettings.seedNativeProfile",
            json!({"profileId":profile_id}),
        ),
        DesktopSettingsAction::MigrateExplorer {
            show_hidden,
            show_ignored,
        } => (
            "desktopSettings.seedExplorer",
            json!({"showHidden":show_hidden,"showIgnored":show_ignored}),
        ),
        DesktopSettingsAction::MigrateTwoRow { enabled } => (
            "desktopSettings.seedLocal",
            json!({"values":{"editor.twoRowTabs":enabled}}),
        ),
        DesktopSettingsAction::PinList { workspace_id } => {
            ("tab.pin.list", json!({"workspaceId": workspace_id}))
        }
        DesktopSettingsAction::PinSet {
            workspace_id,
            id,
            expected_updated_at,
            expected_pinned,
            pinned,
            expected_terminal_session_id,
        } => {
            if workspace_id.is_empty()
                || workspace_id.len() > 256
                || id.is_empty()
                || id.len() > 256
                || expected_terminal_session_id.is_empty()
                || expected_terminal_session_id.len() > 256
                || chrono::DateTime::parse_from_rfc3339(&expected_updated_at).is_err()
            {
                return print_error("Invalid Host Tab pin identity or revision");
            }
            (
                "tab.pin.cas",
                json!({
                    "workspaceId": workspace_id,
                    "id": id,
                    "expectedUpdatedAt": expected_updated_at,
                    "expectedPinned": expected_pinned,
                    "pinned": pinned,
                    "expectedTerminalSessionId": expected_terminal_session_id
                }),
            )
        }
        DesktopSettingsAction::Set {
            path,
            expected,
            value,
        } => {
            // Destructive confirmation switches and auto-archiving are
            // explicitly user-facing Classic Settings controls, not
            // available for arbitrary Agent-executable CLI mutation.
            if !alera_core::desktop_settings_contract::cli_writable(&path) {
                return print_error("Unsupported shared desktop setting path");
            }
            let expected: Value = match serde_json::from_str(&expected) {
                Ok(v) => v,
                Err(_) => return print_error("expected must be valid JSON"),
            };
            let value: Value = match serde_json::from_str(&value) {
                Ok(v) => v,
                Err(_) => return print_error("value must be valid JSON"),
            };
            // Classic quota pins are the sole reviewed array-valued CLI
            // setting. Validate both old and new CAS values.
            if !alera_core::desktop_settings_contract::cli_set_value_allowed(&path, &expected)
                || !alera_core::desktop_settings_contract::cli_set_value_allowed(&path, &value)
            {
                return print_error("Only JSON scalars or validated quota pin arrays are accepted");
            }
            (
                "desktopSettings.compareAndSet",
                json!({"path":path,"expectedValue":expected,"value":value}),
            )
        }
    };
    match client.request_value(request, &payload).await {
        Ok(data) => {
            // The Host's CAS response is a complete Tab for Classic. The
            // CLI is Native's low-trust readback, so never print its payload.
            let safe = if request == "runtimeSettings.get" {
                match native_agent_hooks_projection(&data) {
                    Ok(projection) => projection,
                    Err(error) => return print_error(error),
                }
            } else if request == "runtimeSettings.update" {
                match native_agent_hook_cas_readback(&data, &payload) {
                    Ok(projection) => projection,
                    Err(error) => return print_error(error),
                }
            } else if request == "tab.pin.cas" {
                match native_pin_projection(&data) {
                    Some(projection) => projection,
                    None => return print_error("Invalid Host Tab pin readback"),
                }
            } else {
                data
            };
            print_value(&safe, command.output.json, "Shared desktop settings ready");
            0
        }
        Err(error) if request == "runtimeSettings.update" => {
            // Do not print a full Host Settings diagnostic or retry after
            // conflict; the UI must fetch fresh Host truth before another CAS.
            let _ = error;
            print_error("Host Hook CAS rejected; refresh and retry")
        }
        Err(error) => print_error(error),
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use serde_json::json;

    #[test]
    fn native_cli_pin_projection_never_exposes_agent_or_transfer_payload() {
        let full = json!({
            "id": "tab-1",
            "workspaceId": "workspace-1",
            "updatedAt": "2026-11-09T14:00:00.001Z",
            "title": "Agent",
            "payload": {
                "terminalSessionId": "session-1",
                "pinned": true,
                "initialCommand": "sensitive instruction",
                "lastTerminalTabTransfer": {"token": "not-for-cli"},
            }
        });
        let projected = super::native_pin_projection(&full).unwrap();
        assert_eq!(projected.as_object().unwrap().len(), 5);
        assert_eq!(projected["pinned"], true);
        assert!(!projected.to_string().contains("sensitive"));
        assert!(!projected.to_string().contains("not-for-cli"));
        assert!(super::native_pin_projection(&json!({
            "id": "tab-1", "workspaceId": "workspace-1", "updatedAt": "now",
            "payload": {"pinned": true}
        }))
        .is_none());
    }

    #[test]
    fn agent_hook_readback_is_boolean_only_and_never_leaks_runtime_secrets() {
        let host = json!({
            "revision": 19,
            "agentStatusHooks": {"codex": true, "claude": false, "grok": true},
            "aiAssist": {"apiKey":"VERY_SECRET", "prompt":"Do not disclose"},
            "agentQuotas": {"tokens":"PRIVATE"},
            "automation": {"privateKey":"BAD"}
        });
        let projected = super::native_agent_hooks_projection(&host).unwrap();
        assert_eq!(projected["kind"], "agentHookSettings");
        assert_eq!(projected["revision"], 19);
        assert_eq!(projected["agentStatusHooks"]["codex"], true);
        assert_eq!(projected["agentStatusHooks"]["claude"], false);
        assert_eq!(projected.as_object().unwrap().len(), 3);
        let text = projected.to_string();
        for secret in ["VERY_SECRET", "Do not disclose", "PRIVATE", "BAD", "apiKey"] {
            assert!(!text.contains(secret));
        }
        assert!(super::native_agent_hooks_projection(&json!({"revision":19})).is_err());
        assert!(super::native_agent_hooks_projection(&json!({
            "agentStatusHooks":{"codex":true}
        }))
        .is_err());
        assert!(super::native_agent_hooks_projection(&json!({
            "revision":19, "agentStatusHooks": {"codex": "yes"}
        }))
        .is_err());
        assert!(super::native_agent_hooks_projection(&json!({
            "revision":19, "agentStatusHooks": {"bad\nname": true}
        }))
        .is_err());
    }

    #[test]
    fn agent_hook_readback_cli_requires_no_start_and_cannot_mutate_host() {
        let command = crate::cli::Cli::try_parse_from([
            "alera",
            "settings",
            "--no-start",
            "--json",
            "agent-hooks-get",
        ])
        .unwrap();
        assert!(matches!(
            command.command,
            crate::cli::Command::Settings(crate::cli::DesktopSettingsCommand {
                no_start: true,
                action: crate::cli::DesktopSettingsAction::AgentHooksGet,
                ..
            })
        ));
    }

    #[test]
    fn agent_hook_cas_only_mutates_one_switch_and_requires_exact_host_revision() {
        let host = json!({
            "revision": 7,
            "agentStatusHooks": {"codex": false, "claude": true},
            "aiTextGeneration": {"apiKey":"HIDDEN", "prompt":"PRIVATE"},
            "agentQuotas": {"clientSecret":"SECRET"},
        });
        let payload = super::native_agent_hook_cas_payload(&host, "codex", 7, false, true)
            .unwrap()
            .expect("changed Hook must write");
        assert_eq!(
            payload,
            json!({
                "expectedRevision": 7,
                "agentStatusHooks": {"codex": true, "claude": true}
            })
        );
        let text = payload.to_string();
        for secret in ["HIDDEN", "PRIVATE", "SECRET"] {
            assert!(!text.contains(secret));
        }
        assert!(super::native_agent_hook_cas_payload(&host, "codex", 6, false, true).is_err());
        assert!(super::native_agent_hook_cas_payload(&host, "codex", 7, true, false).is_err());
        assert!(super::native_agent_hook_cas_payload(&host, "bad\nagent", 7, false, true).is_err());
        assert!(super::native_agent_hook_cas_payload(&host, "codex", -1, false, true).is_err());
        assert!(
            super::native_agent_hook_cas_payload(&host, "claude", 7, true, true)
                .unwrap()
                .is_none(),
            "no-op cannot create an extra Host update revision"
        );
        assert!(
            super::native_agent_hook_cas_payload(
                &json!({"agentStatusHooks":{"codex":false}}),
                "codex",
                0,
                false,
                true
            )
            .is_err(),
            "no Host revision means no CAS"
        );
    }

    #[test]
    fn agent_hook_cas_cli_requires_observed_value_and_revision() {
        let parsed = crate::cli::Cli::try_parse_from([
            "alera",
            "settings",
            "--no-start",
            "--json",
            "agent-hooks-cas",
            "--agent-id",
            "codex",
            "--expected-revision",
            "7",
            "--expected-enabled",
            "false",
            "--enabled",
            "true",
        ])
        .unwrap();
        assert!(matches!(
            parsed.command,
            crate::cli::Command::Settings(crate::cli::DesktopSettingsCommand {
                no_start: true,
                action: crate::cli::DesktopSettingsAction::AgentHooksCas {
                    expected_revision: 7,
                    expected_enabled: false,
                    enabled: true,
                    ..
                },
                ..
            })
        ));
        assert!(
            crate::cli::Cli::try_parse_from([
                "alera",
                "settings",
                "--no-start",
                "agent-hooks-cas",
                "--agent-id",
                "codex",
                "--enabled",
                "true"
            ])
            .is_err(),
            "blind Hook updates must be impossible from CLI"
        );
    }

    #[test]
    fn cli_requires_explicit_no_start_and_typed_json() {
        let parsed = crate::cli::Cli::try_parse_from([
            "alera",
            "settings",
            "--no-start",
            "--json",
            "set",
            "--path",
            "general.language",
            "--expected",
            "\"system\"",
            "--value",
            "\"traditionalChinese\"",
        ])
        .unwrap();
        assert!(matches!(parsed.command, crate::cli::Command::Settings(_)));
        let rejected = crate::cli::Cli::try_parse_from([
            "alera",
            "settings",
            "--no-start",
            "--json",
            "migrate-profile",
            "--profile-id",
            "prof_alpha",
        ])
        .unwrap();
        assert!(matches!(rejected.command, crate::cli::Command::Settings(_)));
        let explorer = crate::cli::Cli::try_parse_from([
            "alera",
            "settings",
            "--no-start",
            "--json",
            "migrate-explorer",
            "--show-hidden",
            "true",
            "--show-ignored",
            "false",
        ])
        .unwrap();
        assert!(matches!(
            explorer.command,
            crate::cli::Command::Settings(crate::cli::DesktopSettingsCommand {
                action: crate::cli::DesktopSettingsAction::MigrateExplorer {
                    show_hidden: true,
                    show_ignored: false,
                },
                ..
            })
        ));
        let two_row = crate::cli::Cli::try_parse_from([
            "alera",
            "settings",
            "--no-start",
            "--json",
            "migrate-two-row",
            "--enabled",
            "true",
        ])
        .unwrap();
        assert!(matches!(
            two_row.command,
            crate::cli::Command::Settings(crate::cli::DesktopSettingsCommand {
                action: crate::cli::DesktopSettingsAction::MigrateTwoRow { enabled: true },
                ..
            })
        ));
        let pin_list = crate::cli::Cli::try_parse_from([
            "alera",
            "settings",
            "--no-start",
            "--json",
            "pin-list",
            "--workspace-id",
            "ws-1",
        ])
        .unwrap();
        assert!(matches!(
            pin_list.command,
            crate::cli::Command::Settings(crate::cli::DesktopSettingsCommand {
                action: crate::cli::DesktopSettingsAction::PinList { .. },
                no_start: true,
                ..
            })
        ));
        let pin_set = crate::cli::Cli::try_parse_from([
            "alera",
            "settings",
            "--no-start",
            "--json",
            "pin-set",
            "--workspace-id",
            "ws-1",
            "--id",
            "tab-1",
            "--expected-updated-at",
            "2026-11-09T14:00:00.000Z",
            "--expected-pinned",
            "false",
            "--pinned",
            "true",
            "--expected-terminal-session-id",
            "pty-1",
        ])
        .unwrap();
        assert!(matches!(
            pin_set.command,
            crate::cli::Command::Settings(crate::cli::DesktopSettingsCommand {
                action: crate::cli::DesktopSettingsAction::PinSet { pinned: true, .. },
                no_start: true,
                ..
            })
        ));
        assert!(
            crate::cli::Cli::try_parse_from([
                "alera",
                "settings",
                "--no-start",
                "pin-set",
                "--workspace-id",
                "ws-1",
                "--id",
                "tab-1",
                "--expected-updated-at",
                "2026-11-09T14:00:00.000Z",
                "--expected-pinned",
                "false",
                "--pinned",
                "true",
            ])
            .is_err(),
            "pin CAS must have an exact Host session guard"
        );
        for verb in ["favorites-get", "favorites-seed", "favorites-set"] {
            let mut arguments = vec![
                "alera",
                "settings",
                "--no-start",
                "--json",
                verb,
                "--root-path",
                "C:/repo",
            ];
            if verb == "favorites-seed" {
                arguments.extend(["--branches", "[]"]);
            }
            if verb == "favorites-set" {
                arguments.extend(["--expected", "[]", "--value", "[\"main\"]"]);
            }
            assert!(
                crate::cli::Cli::try_parse_from(arguments).is_ok(),
                "cannot parse {verb}"
            );
        }
    }
}
