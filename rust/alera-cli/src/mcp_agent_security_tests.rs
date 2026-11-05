//! Run environment attacks in child test processes so concurrent tests never
//! observe mutated process-global environment variables.

use super::*;
use crate::terminal_host::protocol::{
    PROTOCOL_VERSION as HOST_PROTOCOL_VERSION, RUNTIME_HOST_BOOTSTRAP_CAPABILITY,
    RUNTIME_HOST_CAPABILITY, RUNTIME_HOST_MANAGED_WORKSPACE_CAPABILITY,
};
use alera_core::child_process::windowless_async_command;
use tokio::net::TcpListener;

#[tokio::test]
async fn agent_observations_reject_spoofed_environment_and_foreign_targets() {
    for scenario in [
        "missing",
        "forged",
        "swapped",
        "foreign-target",
        "same-workspace",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        std::fs::write(
            dir.path().join("runtime-host.json"),
            serde_json::to_vec(&json!({
                "protocolVersion": HOST_PROTOCOL_VERSION,
                "port": listener.local_addr().unwrap().port(), "token": "test-host",
                "runtimeCapabilities": [RUNTIME_HOST_CAPABILITY, RUNTIME_HOST_BOOTSTRAP_CAPABILITY, RUNTIME_HOST_MANAGED_WORKSPACE_CAPABILITY],
            }))
            .unwrap(),
        )
        .unwrap();
        let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            let mut requests = Vec::new();
            // Denied calls may fail locally before opening any connection.
            // Stop explicitly after the isolated child exits, instead of
            // waiting for a fixed number of RPC connections.
            loop {
                let (stream, _) = tokio::select! {
                    _ = &mut stop_rx => break,
                    accepted = listener.accept() => accepted.unwrap(),
                };
                let (reader, mut writer) = stream.into_split();
                let mut lines = BufReader::new(reader).lines();
                while let Some(line) = lines.next_line().await.unwrap() {
                    let request: Value = serde_json::from_str(&line).unwrap();
                    let kind = request["type"].as_str().unwrap();
                    requests.push(kind.to_string());
                    let payload = &request["payload"];
                    let authorized = payload["sessionId"] == "sender"
                        && payload["workspaceId"] == "workspace"
                        && payload["capability"] == "s".repeat(64);
                    let result = match kind {
                        "hello" => json!({}),
                        "agentControl.scope" => json!({
                            "authorized": authorized, "sessionId": "sender",
                            "workspaceId": "workspace", "crossWorkspace": false,
                        }),
                        // Deliberately reproduce the old live-handle check:
                        // a guessed foreign handle and workspace both exist.
                        "orchestration.terminalShow" => json!({
                            "running": true, "workspaceId": if payload["handle"] == "foreign" {
                                "other-workspace"
                            } else { "workspace" },
                        }),
                        "orchestration.terminals" => json!({"items": [{
                            "handle": if payload["workspace"] == "other-workspace" {
                                "foreign"
                            } else { "recipient" },
                            "workspaceId": payload["workspace"],
                            "agentType": "codex", "agentState": "working",
                        }]}),
                        "terminal.read" => json!({"text": "private-output"}),
                        _ => panic!("unexpected request: {kind}"),
                    };
                    let mut reply = serde_json::to_vec(&json!({
                        "id": request["id"], "ok": true, "payload": result,
                    }))
                    .unwrap();
                    reply.push(b'\n');
                    writer.write_all(&reply).await.unwrap();
                }
            }
            requests
        });
        let mut command = windowless_async_command(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "mcp::agent_security_tests::agent_observation_child",
                "--nocapture",
            ])
            .env("ALERA_MCP_SECURITY_SCENARIO", scenario)
            .env("ALERA_MCP_SECURITY_RUNTIME", dir.path())
            .env(
                "ALERA_TERMINAL_HANDLE",
                if scenario == "foreign-target" || scenario == "same-workspace" {
                    "sender"
                } else {
                    "foreign"
                },
            )
            .env(
                "ALERA_WORKSPACE_ID",
                if scenario == "foreign-target" || scenario == "same-workspace" {
                    "workspace"
                } else {
                    "other-workspace"
                },
            )
            .env_remove("ALERA_MCP_UI_TOKEN")
            .kill_on_drop(true);
        match scenario {
            "forged" => {
                command.env("ALERA_MCP_UI_TOKEN", "x".repeat(64));
            }
            "swapped" | "foreign-target" | "same-workspace" => {
                command.env("ALERA_MCP_UI_TOKEN", "s".repeat(64));
            }
            _ => {}
        }
        let output = tokio::time::timeout(Duration::from_secs(30), command.output())
            .await
            .unwrap()
            .unwrap();
        assert!(
            output.status.success(),
            "{scenario}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let _ = stop_tx.send(());
        let requests = tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap();
        if matches!(scenario, "missing" | "forged" | "swapped") {
            assert!(
                requests
                    .iter()
                    .all(|kind| matches!(kind.as_str(), "hello" | "agentControl.scope")),
                "denied callers must not query inventory or output: {requests:?}"
            );
        }
        if scenario == "foreign-target" {
            assert!(!requests.iter().any(|kind| kind == "terminal.read"));
        }
    }
}

#[tokio::test]
async fn agent_observation_child() {
    let Ok(scenario) = std::env::var("ALERA_MCP_SECURITY_SCENARIO") else {
        return;
    };
    let runtime = RuntimeDirArgs {
        runtime_dir: Some(std::env::var("ALERA_MCP_SECURITY_RUNTIME").unwrap()),
    };
    for name in [
        "alera.search_agents",
        "alera.get_agent_activity",
        "alera.read_agent_output",
    ] {
        let args = if name == "alera.search_agents" {
            json!({})
        } else {
            json!({"handle": if scenario == "same-workspace" { "recipient" } else { "foreign" }})
        };
        let result = execute_tool(name, &args, &runtime, &mut None).await;
        if scenario == "same-workspace"
            || (scenario == "foreign-target" && name == "alera.search_agents")
        {
            let value = result.unwrap();
            assert!(!value.to_string().contains("foreign"));
        } else {
            assert!(
                result.is_err(),
                "{name} leaked cross-workspace data for {scenario}: {result:?}"
            );
        }
    }
}
