use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use alera_core::runtime::{RuntimeAiAssistSettings, RuntimeStore};
use serde_json::{json, Value};
use tokio::sync::oneshot;

use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::protocol::{error_response, ok_response};

pub(super) use super::ai_assist_command_execution::run_command;
use super::ai_assist_command_execution::run_command_capture;
use super::ai_assist_fx_plan::plan_fx_command;
use super::ai_assist_grok_plan::plan_grok_command;
use super::ai_assist_model_defaults::default_model;
use super::ai_assist_open_code::open_code_run_arguments;
use super::ai_assist_workspace_identity::{parse_workspace_identity, workspace_identity_prompt};
use super::host_service_requests::required_non_blank;
use super::{ServerActor, ServerCommand};

const MAX_ARGV_PROMPT_BYTES: usize = 24_000;
pub(super) const SUPPORTED_AGENTS: [&str; 12] = [
    "codex",
    "claude",
    "copilot",
    "cursor",
    "agy",
    "opencode",
    "opencode2",
    "pi",
    "amp",
    "grok",
    "fx",
    "custom",
];

static ACTIVE_GENERATIONS: OnceLock<Mutex<HashMap<String, oneshot::Sender<()>>>> = OnceLock::new();

pub(super) struct AiAssistCommandPlan {
    pub(super) binary: String,
    pub(super) arguments: Vec<String>,
    pub(super) stdin_payload: Option<String>,
    pub(super) label: String,
    pub(super) environment: HashMap<String, String>,
    pub(super) temporary_directory: Option<PathBuf>,
}

struct AiAssistWorkspaceIdentityContext {
    repo_path: String,
    settings: RuntimeAiAssistSettings,
}

struct AiAssistWorkspaceIdentityQuery {
    runtime_store: RuntimeStore,
}

impl AiAssistWorkspaceIdentityQuery {
    fn new(runtime_store: &RuntimeStore) -> Self {
        Self {
            runtime_store: runtime_store.clone(),
        }
    }

    async fn load(&self, project_id: &str) -> HostResult<AiAssistWorkspaceIdentityContext> {
        let project = self
            .runtime_store
            .find_project(project_id)
            .await
            .map_err(|error| HostError::state(error.to_string()))?
            .ok_or_else(|| HostError::state(format!("Project not found: {project_id}")))?;
        let settings = self
            .runtime_store
            .effective_ai_assist_settings()
            .await
            .map_err(|error| HostError::state(error.to_string()))?;
        Ok(AiAssistWorkspaceIdentityContext {
            repo_path: project.repo_path,
            settings,
        })
    }
}

impl ServerActor {
    /// Discovery and tests run in the Host's real CLI environment.
    /// No caller-controlled commands, prompts, paths or argument lists.
    pub(super) fn start_ai_assist_model_diagnostic(
        &mut self, client_id: u64, request_id: i64, payload: &Value, test: bool,
    ) -> HostResult<()> {
        let discovery_plan = if test {
            if payload["confirmTest"] != true {
                return Err(HostError::state("Explicit model test confirmation required"));
            }
            None
        } else {
            Some(model_discovery_plan(&required_non_blank(payload, "agent")?)?)
        };
        let expected_agent = if test {
            Some(required_non_blank(payload, "expectedAgent")?)
        } else { None };
        let expected_model = if test {
            Some(payload["expectedModel"].as_str().ok_or_else(||
                HostError::format("Expected model is required"))?.to_owned())
        } else { None };
        let expected_thinking = payload["expectedThinking"].as_str().map(str::to_owned);
        let store = self.runtime_store.clone();
        let inbox = self.inbox.clone();
        let dir = self.runtime_dir.join("ai-assist-diagnostics").join(uuid::Uuid::new_v4().to_string());
        let task = async move {
            let started = std::time::Instant::now();
            let result = async {
                let (cancel, cancel_rx) = oneshot::channel();
                let (plan, agent, model, timeout) = if let Some(plan) = discovery_plan {
                    (plan, String::new(), String::new(), 60)
                } else {
                    let settings = store.effective_ai_assist_settings().await
                        .map_err(|_| HostError::state("Unable to read AI Assist settings"))?;
                    let (agent, model) = summary_model_identity(&settings);
                    if Some(agent.as_str()) != expected_agent.as_deref()
                        || Some(model.as_str()) != expected_model.as_deref()
                        || summary_model_thinking(&settings, &model) != expected_thinking.as_deref() {
                        return Err(HostError::state("Host summary settings differ from this screen; save or refresh settings and retry"));
                    }
                    let plan = super::agent_report_summary::plan_report_command(
                        &settings, "Reply with exactly ALERA_MODEL_TEST_OK and nothing else. This is a synthetic connectivity test. Do not use tools or read files.")?;
                    (plan, agent, model, settings.timeout_seconds)
                };
                tokio::fs::create_dir_all(&dir).await
                    .map_err(|_| HostError::state("Unable to prepare AI Assist diagnostic"))?;
                let output = run_command_capture(plan, &dir.to_string_lossy(), timeout, cancel_rx).await;
                drop(cancel);
                let output = output.map_err(|error| HostError::state(safe_ai_diagnostic_error(&error)))?;
                if test {
                    if output.stdout.trim() != "ALERA_MODEL_TEST_OK" {
                        return Err(HostError::state("Model responded but did not pass the synthetic generation test"));
                    }
                    Ok(json!({"agent":agent,"model":model,"passed":true,
                        "elapsedMs":started.elapsed().as_millis() as u64}))
                } else {
                    Ok(json!({"stdout":output.stdout,"stderr":output.stderr,
                        "elapsedMs":started.elapsed().as_millis() as u64}))
                }
            }.await;
            let _ = tokio::fs::remove_dir_all(&dir).await;
            let _ = inbox.send(ServerCommand::AiAssistFinished {client_id,request_id,result});
        };
        self.deferred_admission.schedule(
            super::deferred_admission::DeferredRequestClass::Bulk,
            if test {"aiText.agentWorkReport.testModel"} else {"aiText.models.discover"},
            Some(client_id), task,
        )?;
        Ok(())
    }

    pub(super) fn start_ai_assist_workspace_identity(
        &mut self,
        client_id: u64,
        request_id: i64,
        payload: &Value,
    ) -> HostResult<()> {
        let operation_id = required_non_blank(payload, "operationId")?;
        let project_id = required_non_blank(payload, "projectId")?;
        let initial_prompt = required_non_blank(payload, "prompt")?;
        let query = AiAssistWorkspaceIdentityQuery::new(&self.runtime_store);
        let inbox = self.inbox.clone();
        let (cancel_tx, cancel_rx) = oneshot::channel();
        let mut active = active_generations()
            .lock()
            .map_err(|_| HostError::state("AI Assist state is unavailable."))?;
        if active.contains_key(&operation_id) {
            return Err(HostError::state(
                "AI Assist is already running for this operation.",
            ));
        }
        active.insert(operation_id.clone(), cancel_tx);
        drop(active);
        let task_operation_id = operation_id.clone();
        let task = async move {
            let result = async {
                let context = query.load(&project_id).await?;
                generate_workspace_identity(
                    &context.repo_path,
                    &initial_prompt,
                    context.settings,
                    cancel_rx,
                )
                .await
            }
            .await;
            if let Ok(mut active) = active_generations().lock() {
                active.remove(&task_operation_id);
            }
            let _ = inbox.send(ServerCommand::AiAssistFinished {
                client_id,
                request_id,
                result,
            });
        };
        if let Err(error) = self.deferred_admission.schedule(
            super::deferred_admission::DeferredRequestClass::Bulk,
            "aiText.workspaceIdentity.generate",
            Some(client_id),
            task,
        ) {
            if let Ok(mut active) = active_generations().lock() {
                active.remove(&operation_id);
            }
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn cancel_ai_assist(&mut self, payload: &Value) -> HostResult<Value> {
        let operation_id = required_non_blank(payload, "operationId")?;
        let canceled = active_generations()
            .lock()
            .map_err(|_| HostError::state("AI Assist state is unavailable."))?
            .remove(&operation_id)
            .is_some_and(|sender| sender.send(()).is_ok());
        Ok(json!({"canceled": canceled}))
    }

    pub(super) fn handle_ai_assist_finished(
        &mut self,
        client_id: u64,
        request_id: i64,
        result: HostResult<Value>,
    ) {
        match result {
            Ok(value) => self.client_write(client_id, ok_response(request_id, value)),
            Err(error) => self.client_write(client_id, error_response(request_id, &error)),
        }
    }
}

fn model_discovery_plan(agent: &str) -> HostResult<AiAssistCommandPlan> {
    let (binary, args): (&str, &[&str]) = match agent {
        "codex" => ("codex", &["debug", "models"]),
        "pi" => ("pi", &["--list-models"]),
        "cursor" => ("cursor-agent", &["--list-models"]),
        "agy" => ("agy", &["models"]),
        "opencode" => ("opencode", &["models"]),
        "opencode2" => ("opencode2", &["models"]),
        "grok" => ("grok", &["models"]),
        "devin" => ("devin", &["models", "list"]),
        "fx" => ("fx", &["models", "--json"]),
        _ => return Err(HostError::format("This agent does not support dynamic model discovery")),
    };
    Ok(AiAssistCommandPlan {
        binary: binary.into(), arguments: args.iter().map(|arg| (*arg).into()).collect(),
        stdin_payload: None, label: agent.into(), environment: HashMap::new(),
        temporary_directory: None,
    })
}

pub(super) fn summary_model_identity(settings: &RuntimeAiAssistSettings) -> (String, String) {
    let operation = settings.prompt_settings_by_operation.get("agentWorkReport");
    let agent = operation.and_then(|value| value.agent.as_deref()).unwrap_or(&settings.agent);
    let model = operation.and_then(|value| value.model.as_deref())
        .filter(|value| !value.trim().is_empty())
        .or_else(|| settings.selected_model_by_agent.get(agent).map(String::as_str)
            .filter(|value| !value.trim().is_empty()))
        .unwrap_or_else(|| default_model(agent));
    (agent.to_owned(), model.to_owned())
}

fn summary_model_thinking<'a>(settings: &'a RuntimeAiAssistSettings, model: &str) -> Option<&'a str> {
    settings.selected_thinking_by_operation.get("agentWorkReport")
        .and_then(|values| values.get(model))
        .or_else(|| settings.selected_thinking_by_model.get(model))
        .map(String::as_str).filter(|value| !value.trim().is_empty())
}

/// Categorize errors without returning arbitrary provider stderr or secrets.
pub(super) fn safe_ai_diagnostic_error(error: &HostError) -> &'static str {
    let detail = error.wire_message().to_lowercase();
    if detail.contains("timed out") || detail.contains("timeout") {
        "AI Assist timed out; check the model or increase the timeout"
    } else if detail.contains("could not be started") || detail.contains("not recognized") ||
        detail.contains("not found") && !detail.contains("model") ||
        detail.contains("無法辨識") || detail.contains("找不到") {
        "Agent CLI could not start; check installation and Runtime Host PATH"
    } else if ["unauthorized", "authentication", "api key", "401", "403", "login", "not logged", "auth expired"].iter().any(|word| detail.contains(*word)) {
        "Agent authentication failed; check login, account and model permissions"
    } else if ["429", "rate limit", "quota"].iter().any(|word| detail.contains(*word)) {
        "Agent quota or rate limit reached"
    } else if detail.contains("model") {
        "Model request failed; verify the selected model is supported by the account"
    } else if detail.contains("canceled") {
        "AI Assist was canceled"
    } else {
        "Agent CLI failed; check its configuration, login and version"
    }
}

pub(super) fn active_generations() -> &'static Mutex<HashMap<String, oneshot::Sender<()>>> {
    ACTIVE_GENERATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn generate_workspace_identity(
    working_directory: &str,
    initial_prompt: &str,
    settings: RuntimeAiAssistSettings,
    cancel_rx: oneshot::Receiver<()>,
) -> HostResult<Value> {
    if !settings.enabled {
        return Err(HostError::state("AI Assist is disabled."));
    }
    if !SUPPORTED_AGENTS.contains(&settings.agent.as_str()) {
        return Err(HostError::format(
            "The configured AI Assist agent is unsupported.",
        ));
    }
    let prompt = workspace_identity_prompt(
        initial_prompt,
        settings
            .instructions_by_operation
            .get("workspaceIdentity")
            .map(String::as_str)
            .unwrap_or_default(),
    );
    let plan = plan_command(&settings, "workspaceIdentity", &prompt)?;
    let timeout_seconds = settings.timeout_seconds;
    let result = run_command(plan, working_directory, timeout_seconds, cancel_rx).await?;
    parse_workspace_identity(&result)
}

pub(super) fn plan_command(
    settings: &RuntimeAiAssistSettings,
    operation: &str,
    prompt: &str,
) -> HostResult<AiAssistCommandPlan> {
    let prompt_settings = settings.prompt_settings_by_operation.get(operation);
    let agent = prompt_settings
        .and_then(|value| value.agent.as_deref())
        .unwrap_or(&settings.agent);
    if agent == "custom" {
        return plan_custom_command(&settings.custom_command, prompt);
    }
    let selected_model = prompt_settings
        .and_then(|value| value.model.as_deref())
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            settings
                .selected_model_by_agent
                .get(agent)
                .map(String::as_str)
                .filter(|value| !value.trim().is_empty())
        });
    let model = selected_model.unwrap_or_else(|| default_model(agent));
    let thinking = settings
        .selected_thinking_by_operation
        .get(operation)
        .and_then(|values| values.get(model))
        .or_else(|| settings.selected_thinking_by_model.get(model))
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty());
    if agent == "fx" {
        return Ok(plan_fx_command(selected_model, prompt));
    }
    let timeout = settings.timeout_seconds;
    let (binary, arguments, stdin_payload, label) = match agent {
        "claude" => (
            "claude",
            vec![
                "-p",
                "--output-format",
                "text",
                "--model",
                model,
                "--permission-mode",
                "plan",
            ]
            .into_iter()
            .map(str::to_string)
            .chain(
                thinking
                    .map(|value| vec!["--effort".to_string(), value.to_string()])
                    .unwrap_or_default(),
            )
            .collect(),
            Some(prompt.to_string()),
            "Claude Code",
        ),
        "codex" => (
            "codex",
            vec![
                "exec".to_string(),
                "--ephemeral".to_string(),
                "--skip-git-repo-check".to_string(),
                "-s".to_string(),
                "read-only".to_string(),
                "--model".to_string(),
                model.to_string(),
            ]
            .into_iter()
            .chain(
                thinking
                    .map(|value| vec!["-c".to_string(), format!("model_reasoning_effort={value}")])
                    .unwrap_or_default(),
            )
            .collect(),
            Some(prompt.to_string()),
            "Codex",
        ),
        "copilot" => (
            "copilot",
            vec![
                "--prompt",
                prompt,
                "--silent",
                "--stream",
                "off",
                "--no-custom-instructions",
                "--model",
                model,
            ]
            .into_iter()
            .map(str::to_string)
            .chain(
                thinking
                    .map(|value| vec!["--effort".to_string(), value.to_string()])
                    .unwrap_or_default(),
            )
            .collect(),
            None,
            "GitHub Copilot",
        ),
        "cursor" => (
            "cursor-agent",
            vec![
                "--print",
                "--mode",
                "ask",
                "--trust",
                "--output-format",
                "text",
                "--model",
                model,
                prompt,
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
            None,
            "Cursor",
        ),
        "agy" => {
            let mut arguments = vec![
                "--sandbox".to_string(),
                "--print-timeout".to_string(),
                format!("{timeout}s"),
            ];
            if selected_model.is_some() {
                arguments.extend(["--model".to_string(), model.to_string()]);
            }
            ("agy", arguments, Some(prompt.to_string()), "Antigravity")
        }
        "opencode" | "opencode2" => (
            agent,
            open_code_run_arguments(model, thinking),
            Some(prompt.to_string()),
            if agent == "opencode2" {
                "OpenCode 2"
            } else {
                "OpenCode"
            },
        ),
        "pi" => (
            "pi",
            vec![
                "--print",
                "--no-session",
                "--no-tools",
                "--no-extensions",
                "--no-skills",
                "--no-context-files",
                "--mode",
                "text",
            ]
            .into_iter()
            .map(str::to_string)
            .chain(
                selected_model
                    .map(|value| vec!["--model".to_string(), value.to_string()])
                    .unwrap_or_default(),
            )
            .chain(
                thinking
                    .map(|value| vec!["--thinking".to_string(), value.to_string()])
                    .unwrap_or_default(),
            )
            .collect(),
            Some(prompt.to_string()),
            "Pi",
        ),
        "amp" => (
            "amp",
            vec![
                "--execute",
                "--no-notifications",
                "--no-ide",
                "--no-jetbrains",
                "--mode",
                model,
            ]
            .into_iter()
            .map(str::to_string)
            .chain(
                thinking
                    .map(|value| vec!["--effort".to_string(), value.to_string()])
                    .unwrap_or_default(),
            )
            .collect(),
            Some(prompt.to_string()),
            "Amp",
        ),
        "grok" => return plan_grok_command(model, thinking, prompt),
        _ => {
            return Err(HostError::format(
                "The configured AI Assist agent is unsupported.",
            ))
        }
    };
    if stdin_payload.is_none() && prompt.len() > MAX_ARGV_PROMPT_BYTES {
        return Err(HostError::format(
            "The selected AI Assist agent cannot receive this prompt safely. Choose an agent that supports stdin.",
        ));
    }
    Ok(AiAssistCommandPlan {
        binary: binary.to_string(),
        arguments,
        stdin_payload,
        label: label.to_string(),
        environment: HashMap::new(),
        temporary_directory: None,
    })
}

fn plan_custom_command(template: &str, prompt: &str) -> HostResult<AiAssistCommandPlan> {
    let tokens = tokenize_command(template);
    if tokens.is_empty() {
        return Err(HostError::format("The custom AI Assist command is empty."));
    }
    let uses_placeholder = tokens.iter().any(|token| token.contains("{prompt}"));
    let values: Vec<String> = tokens
        .into_iter()
        .map(|token| token.replace("{prompt}", prompt))
        .collect();
    Ok(AiAssistCommandPlan {
        binary: values[0].clone(),
        arguments: values[1..].to_vec(),
        stdin_payload: (!uses_placeholder).then(|| prompt.to_string()),
        label: values[0].clone(),
        environment: HashMap::new(),
        temporary_directory: None,
    })
}

fn tokenize_command(template: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for character in template.chars() {
        match (quote, character) {
            (Some(active), value) if value == active => quote = None,
            (None, '"' | '\'') => quote = Some(character),
            (None, value) if value.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(character),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

#[cfg(test)]
#[path = "ai_assist_requests_tests.rs"]
mod tests;

#[cfg(test)]
mod model_diagnostic_tests {
    use super::*;
    use alera_core::runtime::RuntimeAiAssistPromptSettings;

    #[test]
    fn pi_inherits_native_model_and_explicit_summary_override_wins() {
        let mut settings = RuntimeAiAssistSettings { agent: "pi".into(), ..Default::default() };
        assert_eq!(summary_model_identity(&settings), ("pi".into(), String::new()));
        let plan = super::super::agent_report_summary::plan_report_command(&settings, "test").unwrap();
        assert!(!plan.arguments.iter().any(|arg| arg == "--model"));
        assert!(plan.arguments.iter().any(|arg| arg == "--no-tools"));
        settings.selected_model_by_agent.insert("pi".into(), "provider/global".into());
        settings.prompt_settings_by_operation.insert("agentWorkReport".into(),
            RuntimeAiAssistPromptSettings { agent: Some("pi".into()), model: Some("provider/summary".into()) });
        let plan = super::super::agent_report_summary::plan_report_command(&settings, "test").unwrap();
        assert!(plan.arguments.windows(2).any(|args| args[0] == "--model" && args[1] == "provider/summary"));
        assert_eq!(summary_model_identity(&settings).1, "provider/summary");
        settings.prompt_settings_by_operation.get_mut("agentWorkReport").unwrap().model = Some("bad%PATH%".into());
        assert!(super::super::agent_report_summary::plan_report_command(&settings, "test").is_err());
    }

    #[test]
    fn discovery_is_allowlisted_and_errors_are_redacted() {
        assert_eq!(model_discovery_plan("pi").unwrap().arguments, vec!["--list-models"]);
        assert!(model_discovery_plan("custom").is_err());
        assert!(model_discovery_plan("pi & other-command").is_err());
        assert_eq!(safe_ai_diagnostic_error(&HostError::state("Pi failed: 401 API key [redacted]")),
            "Agent authentication failed; check login, account and model permissions");
    }
}
