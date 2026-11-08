//! Alera Native's narrow Host UI relay.
//!
//! The CLI holds the Host authentication token and registers a LOCAL App
//! connection. Native only receives Host-validated, window-addressed UI
//! commands and sends receipts *after* its GPUI workbench changes state.
//! This is not an arbitrary RPC proxy or an alternate MCP authorization path.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::cli::NativeUiBridgeCommand;
use crate::runtime_host_client::RuntimeHostRpcClient;

const MAX_PENDING_RECEIPTS: usize = 32;
const MAX_RECEIPT_BYTES: usize = 2048;

fn valid_ui_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

fn command_for_window(value: &Value, window_id: &str) -> Option<(String, Value)> {
    let object = value.as_object()?;
    if object.len() != 5
        || object.get("windowId")?.as_str()? != window_id
        || object.get("arguments")?.as_object().is_none()
        || !valid_ui_id(object.get("workspaceId")?.as_str()?)
        || !valid_ui_id(object.get("action")?.as_str()?)
    {
        return None;
    }
    let id = object.get("requestId")?.as_str()?;
    if !valid_ui_id(id) {
        return None;
    }
    Some((id.to_owned(), value.clone()))
}

fn validate_completion(value: &Value, pending: &HashMap<String, Instant>) -> Result<Value> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("Invalid Native UI receipt"))?;
    if object.keys().any(|key| {
        !matches!(
            key.as_str(),
            "requestId" | "applied" | "tabId" | "errorCode"
        )
    }) {
        return Err(anyhow!("Native UI receipt contains unsupported fields"));
    }
    let id = object
        .get("requestId")
        .and_then(Value::as_str)
        .filter(|id| valid_ui_id(id) && pending.contains_key(*id))
        .ok_or_else(|| anyhow!("Unknown Native UI request id"))?;
    let applied = object
        .get("applied")
        .and_then(Value::as_bool)
        .ok_or_else(|| anyhow!("Native UI receipt requires applied boolean"))?;
    let tab = object.get("tabId").and_then(Value::as_str);
    if tab.is_some_and(|id| !valid_ui_id(id)) {
        return Err(anyhow!("Invalid Native UI Tab id"));
    }
    let error = object.get("errorCode").and_then(Value::as_str);
    if error.is_some_and(|code| {
        !matches!(
            code,
            "ui_not_ready"
                | "not_visible"
                | "invalid_path"
                | "file_missing"
                | "tab_missing"
                | "workspace_missing"
                | "git_compare_failed"
                | "ui_operation_failed"
        )
    }) {
        return Err(anyhow!("Invalid Native UI failure code"));
    }
    if (applied && error.is_some()) || (!applied && tab.is_some()) {
        return Err(anyhow!("Native UI receipt success/failure fields conflict"));
    }
    // Host requires optional fields to be omitted, not present as JSON null.
    let mut result = json!({"requestId": id, "applied": applied});
    if applied {
        if let Some(tab) = tab {
            result["tabId"] = json!(tab);
        }
    } else {
        result["errorCode"] = json!(error.unwrap_or("ui_not_ready"));
    }
    Ok(result)
}

async fn send_line(output: &mut tokio::io::Stdout, value: &Value) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > 16 * 1024 {
        return Err(anyhow!("Native UI event exceeds 16 KiB"));
    }
    output.write_all(&bytes).await?;
    output.write_all(b"\n").await?;
    output.flush().await?;
    Ok(())
}

pub(crate) async fn run(args: NativeUiBridgeCommand) -> i32 {
    match run_checked(args).await {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("Alera Native UI bridge unavailable: {error}");
            1
        }
    }
}

async fn run_checked(args: NativeUiBridgeCommand) -> Result<()> {
    if !valid_ui_id(&args.window_id) || !args.window_id.starts_with("native-") {
        return Err(anyhow!("Invalid Native window id"));
    }
    let dir = crate::runtime_dir(&args.runtime);
    let mut client = RuntimeHostRpcClient::connect_native_ui_app(&dir)
        .await?
        .ok_or_else(|| anyhow!("Host not running: Native UI bridge never auto-starts it"))?;
    let register = client
        .request_value("uiControl.register", &json!({"windowId":args.window_id}))
        .await?;
    if register["registered"] != true || register["windowId"] != args.window_id {
        return Err(anyhow!("Host did not confirm Native window registration"));
    }
    let mut stdout = tokio::io::stdout();
    send_line(
        &mut stdout,
        &json!({"kind":"ready","windowId":args.window_id}),
    )
    .await?;
    let stdin = BufReader::new(tokio::io::stdin());
    let mut replies = stdin.lines();
    let mut pending: HashMap<String, Instant> = HashMap::new();
    loop {
        pending.retain(|_, seen| seen.elapsed() < Duration::from_secs(190));
        tokio::select! {
            received = client.next_native_ui_command() => {
                let Some(command) = received? else { break; };
                if let Some((id, payload)) = command_for_window(&command, &args.window_id) {
                    if pending.len() >= MAX_PENDING_RECEIPTS || pending.contains_key(&id) {
                        continue;
                    }
                    pending.insert(id, Instant::now());
                    send_line(&mut stdout, &json!({"kind":"command","payload":payload})).await?;
                }
            }
            line = replies.next_line() => {
                let Some(line) = line? else { break; };
                if line.len() > MAX_RECEIPT_BYTES { continue; }
                let Ok(decoded) = serde_json::from_str::<Value>(&line) else { continue; };
                let Ok(receipt) = validate_completion(&decoded, &pending) else { continue; };
                let Some(request_id) = receipt["requestId"].as_str() else { continue; };
                let request_id = request_id.to_owned();
                // A bad/expired Host receipt cannot turn a failed UI action
                // into applied. Mark the local receipt consumed even if Host
                // returns an error; retrying is never automatic.
                pending.remove(&request_id);
                if let Err(error) = client.request_value("uiControl.complete", &receipt).await {
                    eprintln!("Native UI Host acknowledgment unavailable: {error}");
                }
            }
        }
    }
    let _ = client
        .request_value("uiControl.unregister", &json!({"windowId":args.window_id}))
        .await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn native_ui_bridge_cli_is_private_and_requires_explicit_window_id() {
        use crate::cli::{Cli, Command};
        let cli = Cli::try_parse_from([
            "alera",
            "native-ui-bridge",
            "--runtime-dir",
            "C:/test",
            "--window-id",
            "native-abc",
        ])
        .unwrap();
        let Command::NativeUiBridge(args) = cli.command else {
            panic!("unexpected command")
        };
        assert_eq!(args.window_id, "native-abc");
        assert!(!valid_ui_id("../bad"));
        assert!(!valid_ui_id("x\ny"));
    }

    #[test]
    fn native_bridge_rejects_wrong_window_forged_receipt_and_success_error_mix() {
        let command = json!({
            "requestId":"req_1", "windowId":"native-one", "workspaceId":"ws_1",
            "action":"open_file", "arguments":{"relativePath":"src/main.rs"}
        });
        assert!(command_for_window(&command, "native-two").is_none());
        assert_eq!(
            command_for_window(&command, "native-one").unwrap().0,
            "req_1"
        );
        let mut pending = HashMap::new();
        pending.insert("req_1".to_string(), Instant::now());
        assert!(
            validate_completion(&json!({"requestId":"other","applied":true}), &pending).is_err()
        );
        assert!(validate_completion(
            &json!({"requestId":"req_1","applied":true,"errorCode":"ui_not_ready"}),
            &pending
        )
        .is_err());
        assert!(validate_completion(
            &json!({"requestId":"req_1","applied":false,"tabId":"tab"}),
            &pending
        )
        .is_err());
        let fail = validate_completion(
            &json!({"requestId":"req_1","applied":false,"errorCode":"not_visible"}),
            &pending,
        )
        .unwrap();
        assert_eq!(fail["applied"], false);
        assert_eq!(fail["errorCode"], "not_visible");
    }
}
