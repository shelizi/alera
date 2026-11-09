//! Narrow AI Commit executor for Native GPUI, using the same authoritative
//! Runtime Host AI Assist settings that Flutter Classic edits.
//! No Host startup, custom shell commands, arbitrary agent launches, or
//! repository writes. A live controller stdin pipe is required for cancellation.
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use alera_core::child_process::windowless_async_command;
use alera_core::git::{prepare_staged_commit_prompt, verify_staged_commit_prompt};
use alera_core::runtime::{validate_ai_assist_settings, RuntimeAiAssistSettings};
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::cli::{AiAssistAction, AiAssistCommand, AiCommitMessageArgs};
use crate::runtime_host_client::RuntimeHostRpcClient;

const MAX_OUTPUT_BYTES: u64 = 64 * 1024;
const MAX_ERROR_BYTES: u64 = 4096;

#[derive(Debug, PartialEq, Eq)]
struct AgentPlan {
    program: &'static str,
    args: Vec<String>,
    agent_label: &'static str,
    timeout_seconds: u64,
}

fn configured_agent_plan(settings: &RuntimeAiAssistSettings) -> Result<AgentPlan> {
    if !settings.enabled {
        bail!("AI Assist is disabled in Classic settings");
    }
    validate_ai_assist_settings(settings)?;
    let operation = settings.prompt_settings_by_operation.get("commitMessage");
    let agent = operation
        .and_then(|settings| settings.agent.as_deref())
        .unwrap_or(settings.agent.as_str());
    let model = operation
        .and_then(|settings| settings.model.as_deref())
        .or_else(|| {
            settings
                .selected_model_by_agent
                .get(agent)
                .map(String::as_str)
        })
        .unwrap_or(match agent {
            "codex" => "gpt-5.5",
            "claude" => "sonnet",
            _ => "",
        });
    // Do not attempt to interpret or run custom commands from the profile.
    // Other registered agents need their own verified read-only adapter.
    if !matches!(agent, "codex" | "claude") {
        bail!("Native AI Commit currently supports read-only Codex or Claude only. Select one in Classic AI Assist settings.");
    }
    if model.trim().is_empty() || model.len() > 128 || model.chars().any(|c| c.is_control()) {
        bail!("AI Assist model selection is invalid");
    }
    let thinking = settings
        .selected_thinking_by_operation
        .get("commitMessage")
        .and_then(|values| values.get(model))
        .or_else(|| settings.selected_thinking_by_model.get(model));
    let thinking = thinking.map(String::as_str).or_else(|| {
        if agent == "codex" || (agent == "claude" && matches!(model, "sonnet" | "opus")) {
            Some("low")
        } else {
            None
        }
    });
    if thinking.is_some_and(|value| !matches!(value, "low" | "medium" | "high" | "xhigh" | "max")) {
        bail!("AI Assist thinking level is unsupported for Native");
    }
    let args = match agent {
        "codex" => {
            let mut args = vec![
                "exec".into(),
                "--ephemeral".into(),
                "--skip-git-repo-check".into(),
                "-s".into(),
                "read-only".into(),
                "--model".into(),
                model.into(),
            ];
            if let Some(thinking) = thinking {
                args.extend(["-c".into(), format!("model_reasoning_effort={thinking}")]);
            }
            args
        }
        "claude" => {
            let mut args = vec![
                "-p".into(),
                "--output-format".into(),
                "text".into(),
                "--model".into(),
                model.into(),
                "--permission-mode".into(),
                "plan".into(),
                "--strict-mcp-config".into(),
                "--mcp-config".into(),
                "{\"mcpServers\":{}}".into(),
                "--tools".into(),
                "".into(),
                "--no-chrome".into(),
            ];
            if let Some(thinking) = thinking {
                args.extend(["--effort".into(), thinking.into()]);
            }
            args
        }
        _ => unreachable!("allowlist enforced"),
    };
    Ok(AgentPlan {
        program: if agent == "codex" { "codex" } else { "claude" },
        args,
        agent_label: if agent == "codex" { "Codex" } else { "Claude" },
        timeout_seconds: settings.timeout_seconds,
    })
}

fn clean_message(raw: &str) -> Result<String> {
    let raw = raw.replace("\r\n", "\n");
    let mut text = raw.trim();
    if text.starts_with("```") {
        if let Some((_, rest)) = text.split_once('\n') {
            if let Some(inner) = rest.strip_suffix("```") {
                text = inner.trim();
            }
        }
    }
    let mut lines = text.lines();
    let subject = lines
        .next()
        .unwrap_or("")
        .trim()
        .trim_end_matches('.')
        .trim();
    if subject.is_empty() {
        bail!("AI Agent returned an empty commit message");
    }
    let subject: String = subject.chars().take(72).collect();
    let body: String = lines
        .filter(|line| {
            !line
                .trim_start()
                .to_ascii_lowercase()
                .starts_with("co-authored-by:")
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .chars()
        .take(3800)
        .collect();
    Ok(if body.is_empty() {
        subject
    } else {
        format!("{subject}\n\n{body}")
    })
}

async fn call_read_only_agent(plan: AgentPlan, workspace: &Path, prompt: String) -> Result<String> {
    let mut command = windowless_async_command(plan.program);
    command
        .args(&plan.args)
        .current_dir(workspace)
        .env_remove("ALERA_WORKSPACE_ID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // Match Classic's user-shell command environment, including the tool's
    // installed PATH when Alera was launched from a desktop launcher.
    let environment =
        crate::login_shell_environment::login_shell_variables_with_process_overrides().await;
    command.envs(environment);
    if let Some(environment) =
        crate::login_shell_environment::login_shell_command_environment().await
    {
        command.envs(environment);
    }
    // Revoke inherited Host Workspace routing *after* hydrating the user
    // environment, otherwise envs() may silently reintroduce it.
    command.env_remove("ALERA_WORKSPACE_ID");
    let mut child = command
        .spawn()
        .context("Configured AI Agent could not be started")?;
    let send = async {
        let mut input = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("AI Agent stdin unavailable"))?;
        input.write_all(prompt.as_bytes()).await?;
        input.shutdown().await?;
        Ok::<(), anyhow::Error>(())
    };
    match tokio::time::timeout(Duration::from_secs(10), send).await {
        Ok(Ok(())) => {}
        _ => {
            let _ = child.kill().await;
            bail!("AI Agent did not accept the staged prompt");
        }
    }
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("AI Agent stdout unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("AI Agent stderr unavailable"))?;
    let output_reader = tokio::spawn(async move {
        let mut bytes = Vec::new();
        stdout
            .take(MAX_OUTPUT_BYTES + 1)
            .read_to_end(&mut bytes)
            .await?;
        Ok::<Vec<u8>, std::io::Error>(bytes)
    });
    let error_reader = tokio::spawn(async move {
        let mut bytes = Vec::new();
        stderr
            .take(MAX_ERROR_BYTES + 1)
            .read_to_end(&mut bytes)
            .await?;
        Ok::<Vec<u8>, std::io::Error>(bytes)
    });
    // Native keeps stdin open as a cancellation/liveness lease. Closing it
    // aborts this task and kills the Agent, without requiring new Host RPCs.
    let controller = tokio::spawn(async {
        let mut lease = tokio::io::stdin();
        let mut one = [0u8; 1];
        let _ = lease.read(&mut one).await;
    });
    let started = tokio::time::Instant::now();
    let result = loop {
        if controller.is_finished() {
            let _ = child.kill().await;
            bail!("AI Commit generation cancelled (controller disconnected)");
        }
        if started.elapsed() >= Duration::from_secs(plan.timeout_seconds) {
            let _ = child.kill().await;
            bail!("AI Commit generation timed out");
        }
        if let Some(status) = child.try_wait().context("AI Agent wait failed")? {
            break status;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    };
    controller.abort();
    let bytes = output_reader.await.context("AI stdout reader failed")??;
    let _ = error_reader.await;
    if bytes.len() as u64 > MAX_OUTPUT_BYTES {
        bail!("AI Agent output exceeded the 64 KiB limit");
    }
    if !result.success() {
        bail!("Configured AI Agent failed; verify its login and settings");
    }
    clean_message(&String::from_utf8(bytes).context("AI Agent returned invalid text encoding")?)
}

async fn run_command(command: AiAssistCommand) -> Result<Value> {
    if !command.no_start || !command.output.json {
        bail!("AI Commit requires --no-start and --json");
    }
    let AiAssistAction::CommitMessage(args) = command.action;
    let AiCommitMessageArgs {
        workspace,
        expected_head,
        expected_index,
    } = args;
    if expected_head.len() != 40
        || expected_index.len() != 40
        || !expected_head.bytes().all(|c| c.is_ascii_hexdigit())
        || !expected_index.bytes().all(|c| c.is_ascii_hexdigit())
    {
        bail!("AI Commit requires an exact reviewed HEAD and index SHA");
    }
    let runtime = crate::runtime_dir(&command.runtime);
    let mut host = RuntimeHostRpcClient::connect(&runtime)
        .await?
        .ok_or_else(|| {
            anyhow!("A running compatible Runtime Host is required; no Host was started")
        })?;
    let response = host
        .request_value("runtimeSettings.get", &json!({}))
        .await?;
    let encoded = response
        .get("aiTextGeneration")
        .ok_or_else(|| anyhow!("Classic AI Assist settings are unavailable in the running Host"))?;
    if encoded.is_null() {
        bail!("Classic AI Assist settings have not been initialized in the Host");
    }
    let settings: RuntimeAiAssistSettings = serde_json::from_value(encoded.clone())
        .context("Invalid authoritative AI Assist settings")?;
    let settings = settings.normalized();
    let plan = configured_agent_plan(&settings)?;
    let instructions = settings
        .instructions_by_operation
        .get("commitMessage")
        .map(String::as_str)
        .unwrap_or("");
    let workspace = dunce::canonicalize(&workspace).context("Workspace is unavailable")?;
    let prepared = prepare_staged_commit_prompt(&workspace, instructions)
        .map_err(|e| anyhow!(e.to_string()))?;
    if prepared.head_oid != expected_head || prepared.index_tree_oid != expected_index {
        bail!("Staged changes changed after Native preview; regenerate before running AI");
    }
    let agent_label = plan.agent_label;
    let generated = call_read_only_agent(plan, &workspace, prepared.prompt.clone()).await?;
    verify_staged_commit_prompt(&workspace, &prepared).map_err(|e| anyhow!(e.to_string()))?;
    Ok(json!({
        "text": generated, "agentLabel": agent_label,
        "headOid": prepared.head_oid, "indexTreeOid": prepared.index_tree_oid
    }))
}

pub(crate) async fn run(command: AiAssistCommand) -> i32 {
    match run_command(command).await {
        Ok(payload) => {
            println!("{}", payload);
            0
        }
        Err(error) => {
            // No staged patch, prompt, process stderr or secret in error logs.
            eprintln!("Native AI Commit refused: {error}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Cli, Command};
    use clap::Parser;

    #[test]
    fn cli_requires_no_start_and_snapshot_before_generation() {
        let valid = Cli::try_parse_from([
            "alera",
            "ai-assist",
            "--no-start",
            "--json",
            "commit-message",
            "--workspace",
            "/tmp/repo",
            "--expected-head",
            "0123456789012345678901234567890123456789",
            "--expected-index",
            "abcdefabcdefabcdefabcdefabcdefabcdefabcd",
        ])
        .unwrap();
        assert!(matches!(
            valid.command,
            Command::AiAssist(AiAssistCommand { no_start: true, .. })
        ));
        assert!(Cli::try_parse_from([
            "alera",
            "ai-assist",
            "--json",
            "commit-message",
            "--workspace",
            "/tmp/repo",
            "--expected-head",
            "a",
            "--expected-index",
            "b",
        ])
        .is_err());
    }

    #[test]
    fn settings_select_only_read_only_allowlisted_agents_and_operation_override() {
        let mut settings = RuntimeAiAssistSettings::default();
        let plan = configured_agent_plan(&settings).unwrap();
        assert_eq!(plan.program, "codex");
        assert!(plan.args.windows(2).any(|args| args == ["-s", "read-only"]));
        settings.prompt_settings_by_operation.insert(
            "commitMessage".into(),
            alera_core::runtime::RuntimeAiAssistPromptSettings {
                agent: Some("claude".into()),
                model: Some("opus".into()),
            },
        );
        let plan = configured_agent_plan(&settings).unwrap();
        assert_eq!(plan.program, "claude");
        assert!(plan.args.windows(2).any(|args| args == ["--tools", ""]));
        assert!(plan.args.iter().any(|s| s == "--strict-mcp-config"));
        settings.prompt_settings_by_operation.clear();
        settings.agent = "custom".into();
        settings.custom_command = "echo {prompt}".into();
        assert!(configured_agent_plan(&settings).is_err());
        settings.agent = "codex".into();
        settings.enabled = false;
        assert!(configured_agent_plan(&settings).is_err());
    }

    #[test]
    fn output_is_trimmed_and_strips_trailers() {
        let text = clean_message(
            "```\nUpdate API.\n\nWhy this matters\nCo-authored-by: Hidden <h@example.invalid>\n```",
        )
        .unwrap();
        assert_eq!(text, "Update API\n\nWhy this matters");
        assert!(clean_message(" ").is_err());
    }
}
