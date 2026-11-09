//! Human-owned CLI bridge for Classic/GPUI shared settings. Never starts
//! a Runtime Host and never prints whole configuration or secret material.
use alera_core::runtime::SHARED_DESKTOP_PATHS;
use serde_json::{json, Value};

use crate::cli::{DesktopSettingsAction, DesktopSettingsCommand};
use crate::runtime_host_client::RuntimeHostRpcClient;
use crate::{print_error, print_value, runtime_dir, USAGE_EXIT_CODE};

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
        DesktopSettingsAction::Set {
            path,
            expected,
            value,
        } => {
            if !SHARED_DESKTOP_PATHS.contains(&path.as_str()) {
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
            print_value(&data, command.output.json, "Shared desktop settings ready");
            0
        }
        Err(error) => print_error(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

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
        assert!(!SHARED_DESKTOP_PATHS.contains(&"agents.secret"));
    }
}
