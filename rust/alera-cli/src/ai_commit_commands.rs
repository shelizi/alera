//! Narrow AI Commit executor for Native GPUI, using the same authoritative
//! Runtime Host AI Assist settings that Flutter Classic edits.
//! No Host startup, custom shell commands, arbitrary agent launches, or
//! repository writes. A live controller stdin pipe is required for cancellation.
use std::process::Stdio;
use std::time::Duration;

#[cfg(windows)]
use alera_core::child_process::windowless_async_command;
use alera_core::git::{prepare_staged_commit_prompt, verify_staged_commit_prompt};
use alera_core::process_shell::windowless_async_shell_command;
use alera_core::runtime::{validate_ai_assist_settings, RuntimeAiAssistSettings};
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use crate::cli::{AiAssistAction, AiAssistCommand, AiCommitMessageArgs};
use crate::runtime_host_client::RuntimeHostRpcClient;

const MAX_OUTPUT_BYTES: u64 = 64 * 1024;
const MAX_ERROR_BYTES: u64 = 4096;

#[derive(Debug, PartialEq, Eq)]
struct AgentPlan {
    program: String,
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
    // Match Classic: send reasoning effort only when the user selected one.
    let thinking = thinking.map(String::as_str);
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
        program: if agent == "codex" { "codex" } else { "claude" }.into(),
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

// Windows Agent binaries are commonly .cmd launchers. Killing only cmd.exe
// can orphan the real Node/Agent process. Mirror Alera's process-session
// descendant termination before reaping the CLI-controlled child.
async fn terminate_agent_process_tree(child: &mut tokio::process::Child) {
    #[cfg(windows)]
    if let Some(pid) = child.id() {
        let mut command = windowless_async_command("taskkill.exe");
        command
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let _ = tokio::time::timeout(Duration::from_secs(4), command.status()).await;
    }
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        // Safe: a new process group is created for this invocation only.
        unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
    }
    let _ = child.kill().await;
}

async fn call_read_only_agent(plan: AgentPlan, prompt: String) -> Result<String> {
    call_read_only_agent_with_controller(plan, prompt, tokio::io::stdin()).await
}

// Injecting the control pipe lets cancellation and timeout tests exercise a
// real child process without a live Host, account login or workspace mutation.
async fn call_read_only_agent_with_controller<R: AsyncRead + Unpin>(
    plan: AgentPlan,
    prompt: String,
    mut controller: R,
) -> Result<String> {
    // The prompt already contains the entire bounded staged diff. Running in a
    // fresh empty directory avoids automatically reading untracked files,
    // repository instructions and hooks from the actual checkout.
    let isolated_dir = tempfile::Builder::new()
        .prefix("alera-ai-commit-")
        .tempdir()
        .context("Could not create an isolated AI Commit directory")?;
    let mut environment =
        crate::login_shell_environment::login_shell_variables_with_process_overrides().await;
    if let Some(shell_environment) =
        crate::login_shell_environment::login_shell_command_environment().await
    {
        environment.extend(shell_environment);
    }
    // Match Classic's resolution of Windows .cmd/.bat Agent launchers.
    let cwd = isolated_dir.path().to_string_lossy();
    let mut command = windowless_async_shell_command(
        &plan.program,
        &plan.args,
        Some(&cwd),
        environment.get("PATH").map(String::as_str),
    );
    command
        .current_dir(isolated_dir.path())
        .envs(environment)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    // Never inherit Alera Host/PTY identity or hook credentials in the Agent.
    for key in [
        "ALERA_TERMINAL_SESSION_ID",
        "ALERA_WORKSPACE_ID",
        "ALERA_TAB_ID",
        "ALERA_AGENT_HOOK_ENDPOINT",
        "ALERA_AGENT_HOOK_PORT",
        "ALERA_AGENT_HOOK_TOKEN",
        "ALERA_RUNTIME_DIR",
    ] {
        command.env_remove(key);
    }
    let mut child = command
        .spawn()
        .context("Configured AI Agent could not be started")?;
    let started = tokio::time::Instant::now();
    let deadline = started + Duration::from_secs(plan.timeout_seconds);
    // Native holds the CLI stdin write end for the entire operation. Closing
    // it must interrupt even a child blocked while accepting the prompt.
    let mut one = [0u8; 1];
    let disconnected = async {
        let _ = controller.read(&mut one).await;
    };
    tokio::pin!(disconnected);
    let send = async {
        let mut input = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("AI Agent stdin unavailable"))?;
        input.write_all(prompt.as_bytes()).await?;
        input.shutdown().await?;
        Ok::<(), anyhow::Error>(())
    };
    enum SendOutcome {
        Sent(Result<()>),
        Cancelled,
    }
    let sent = tokio::select! {
        _ = &mut disconnected => SendOutcome::Cancelled,
        result = tokio::time::timeout(Duration::from_secs(10), send) =>
            SendOutcome::Sent(result.map_err(|_| anyhow!("AI Agent prompt write timed out")).and_then(|v| v)),
    };
    match sent {
        SendOutcome::Sent(Ok(())) => {}
        SendOutcome::Cancelled => {
            terminate_agent_process_tree(&mut child).await;
            bail!("AI Commit generation cancelled (controller disconnected)");
        }
        SendOutcome::Sent(Err(_)) => {
            terminate_agent_process_tree(&mut child).await;
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
    let mut output_reader = tokio::spawn(async move {
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
    enum Completion {
        Cancelled,
        TimedOut,
        Exited(std::io::Result<std::process::ExitStatus>),
    }
    let completion = tokio::select! {
        _ = &mut disconnected => Completion::Cancelled,
        _ = tokio::time::sleep_until(deadline) => Completion::TimedOut,
        status = child.wait() => Completion::Exited(status),
    };
    let result = match completion {
        Completion::Cancelled => {
            terminate_agent_process_tree(&mut child).await;
            output_reader.abort();
            error_reader.abort();
            bail!("AI Commit generation cancelled (controller disconnected)");
        }
        Completion::TimedOut => {
            terminate_agent_process_tree(&mut child).await;
            output_reader.abort();
            error_reader.abort();
            bail!("AI Commit generation timed out");
        }
        Completion::Exited(result) => result.context("AI Agent wait failed")?,
    };
    // A detached Agent descendant might inherit stdout even after a launcher
    // exits. Do not let draining that inherited pipe bypass cancel/timeout.
    let bytes = tokio::select! {
        output = &mut output_reader => output.context("AI stdout reader failed")??,
        _ = &mut disconnected => {
            output_reader.abort();
            error_reader.abort();
            bail!("AI Commit generation cancelled (controller disconnected)");
        }
        _ = tokio::time::sleep_until(deadline) => {
            output_reader.abort();
            error_reader.abort();
            bail!("AI Commit generation timed out while collecting output");
        }
    };
    error_reader.abort();
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
        expected_branch,
        expected_head,
        expected_index,
    } = args;
    if expected_branch.trim().is_empty()
        || expected_branch.len() > 255
        || expected_head.len() != 40
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
    if prepared.branch != expected_branch
        || prepared.head_oid != expected_head
        || prepared.index_tree_oid != expected_index
    {
        bail!("Staged changes changed after Native preview; regenerate before running AI");
    }
    let agent_label = plan.agent_label;
    let generated = call_read_only_agent(plan, prepared.prompt.clone()).await?;
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
            "--expected-branch",
            "feature",
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
            "--expected-branch",
            "feature",
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
        assert!(!plan
            .args
            .iter()
            .any(|value| value.contains("model_reasoning_effort=")));
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

    // This ignored test doubles as an isolated child process: no real Agent,
    // Host token, checkout, or remote request is needed for lifecycle tests.
    #[test]
    #[ignore]
    fn fixture_agent_waits_for_cancellation() {
        std::thread::sleep(Duration::from_secs(20));
    }

    fn fixture_agent_plan(timeout_seconds: u64) -> AgentPlan {
        AgentPlan {
            program: std::env::current_exe()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            args: vec![
                "--exact".into(),
                "ai_commit_commands::tests::fixture_agent_waits_for_cancellation".into(),
                "--ignored".into(),
                "--nocapture".into(),
            ],
            agent_label: "Fixture",
            timeout_seconds,
        }
    }

    #[tokio::test]
    async fn cancellation_closes_real_agent_process_promptly() {
        let (controller, writer) = tokio::io::duplex(8);
        let started = tokio::time::Instant::now();
        let finish_controller = async move {
            tokio::time::sleep(Duration::from_millis(250)).await;
            drop(writer);
        };
        let operation = call_read_only_agent_with_controller(
            fixture_agent_plan(15),
            "only staged diff".into(),
            controller,
        );
        let (result, ()) = tokio::time::timeout(Duration::from_secs(6), async {
            tokio::join!(operation, finish_controller)
        })
        .await
        .expect("cancellation must not leave a child running");
        assert!(result.unwrap_err().to_string().contains("cancelled"));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn timeout_kills_and_reaps_real_agent_process() {
        let (controller, _writer) = tokio::io::duplex(8);
        let started = tokio::time::Instant::now();
        let result = tokio::time::timeout(
            Duration::from_secs(6),
            call_read_only_agent_with_controller(
                fixture_agent_plan(1),
                "only staged diff".into(),
                controller,
            ),
        )
        .await
        .expect("timeout must not leave a child running");
        assert!(result.unwrap_err().to_string().contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
