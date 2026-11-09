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
    let (request, payload) = match command.action {
        DesktopSettingsAction::Get => ("desktopSettings.get", json!({})),
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
                Err(_) => return print_error("expected must be valid JSON scalar"),
            };
            let value: Value = match serde_json::from_str(&value) {
                Ok(v) => v,
                Err(_) => return print_error("value must be valid JSON scalar"),
            };
            if expected.is_array() || expected.is_object() || value.is_array() || value.is_object()
            {
                return print_error("Only JSON scalars are accepted");
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
            let safe = if request == "tab.pin.cas" {
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
