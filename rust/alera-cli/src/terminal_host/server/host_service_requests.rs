use alera_core::runtime::{
    RuntimeAgentQuotaSettings, RuntimeAiAssistSettings, RuntimeAutomationSettings,
    RuntimeMobilePushSettings, RuntimeTextActionsSettings,
};
use serde::Serialize;
use serde_json::{json, Value};

use crate::agent_status::reconcile_agent_integrations;
use crate::host_tools::{
    cli_registration_status, install_cli_registration, install_skill, SkillKind, SkillRunner,
};
use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::protocol::{error_response, event, ok_response};

use super::{ServerActor, ServerCommand};

impl ServerActor {
    pub(super) async fn apply_mobile_runtime_settings(
        &mut self,
        payload: &Value,
    ) -> HostResult<Value> {
        let mut refresh_push_subscriptions = false;
        if let Some(value) = payload.get("workspaceDirectory") {
            let directory = match value {
                Value::String(value) => Some(value.as_str()),
                Value::Null => None,
                _ => {
                    return Err(HostError::format(
                        "workspaceDirectory must be a string or null.",
                    ))
                }
            };
            runtime_value(self.runtime_store.set_workspace_directory(directory).await)?;
        }
        if let Some(value) = payload.get("confirmProjectRemoval") {
            let enabled = value
                .as_bool()
                .ok_or_else(|| HostError::format("confirmProjectRemoval must be a boolean."))?;
            runtime_value(
                self.runtime_store
                    .set_confirm_project_removal(enabled)
                    .await,
            )?;
        }
        if let Some(value) = payload.get("confirmWorkspaceRemoval") {
            let enabled = value
                .as_bool()
                .ok_or_else(|| HostError::format("confirmWorkspaceRemoval must be a boolean."))?;
            runtime_value(
                self.runtime_store
                    .set_confirm_workspace_removal(enabled)
                    .await,
            )?;
        }
        if let Some(value) = payload.get("autoArchiveWorkspacesAfterDays") {
            let days = value.as_i64().ok_or_else(|| {
                HostError::format("autoArchiveWorkspacesAfterDays must be an integer.")
            })?;
            if days < 0 {
                return Err(HostError::format(
                    "autoArchiveWorkspacesAfterDays must be zero or greater.",
                ));
            }
            runtime_value(
                self.runtime_store
                    .set_auto_archive_workspaces_after_days(days)
                    .await,
            )?;
        }
        if let Some(value) = payload.get("defaultAgentProfileId") {
            let profile_id = match value {
                Value::String(value) => Some(value.as_str()),
                Value::Null => None,
                _ => {
                    return Err(HostError::format(
                        "defaultAgentProfileId must be a string or null.",
                    ))
                }
            };
            runtime_value(
                self.runtime_store
                    .set_default_agent_profile_id(profile_id)
                    .await,
            )?;
        }
        if let Some(value) = payload.get("agentStatusHooks") {
            let settings = serde_json::from_value(value.clone()).map_err(|_| {
                HostError::format("agentStatusHooks must contain boolean agent switches.")
            })?;
            runtime_value(
                self.runtime_store
                    .set_agent_status_hook_settings(&settings)
                    .await,
            )?;
            self.agent_presence
                .retain_enabled(&settings.enabled_agents());
            self.schedule_agent_integration_reconcile(settings);
            self.broadcast_agent_presence_changed();
        }
        if let Some(value) = payload.get("agentQuotas") {
            let settings: RuntimeAgentQuotaSettings = serde_json::from_value(value.clone())
                .map_err(|_| HostError::format("agentQuotas is invalid."))?;
            validate_agent_quota_settings(&settings)?;
            runtime_value(self.runtime_store.set_agent_quota_settings(settings).await)?;
            self.agent_quota_cache = None;
        }
        if let Some(value) = payload.get("mobilePushNotifications") {
            let mut settings: RuntimeMobilePushSettings = serde_json::from_value(value.clone())
                .map_err(|_| HostError::format("mobilePushNotifications is invalid."))?;
            // Privacy portable build: preserve the local setting shape but never
            // allow cloud push to become active.
            settings.enabled = false;
            runtime_value(self.runtime_store.set_mobile_push_settings(&settings).await)?;
            self.account_push.push_enabled = false;
            self.account_push.active_subscriptions = 0;
            refresh_push_subscriptions = false;
        }
        if let Some(value) = payload.get("automation") {
            let settings: RuntimeAutomationSettings = serde_json::from_value(value.clone())
                .map_err(|_| HostError::format("automation settings are invalid."))?;
            runtime_value(self.runtime_store.set_automation_settings(settings).await)?;
            self.schedule_autostart_reconcile();
        }
        if refresh_push_subscriptions {
            self.start_push_subscription_sync(None);
        }
        if let Some(value) = payload.get("aiTextGeneration") {
            let settings: RuntimeAiAssistSettings = serde_json::from_value(value.clone())
                .map_err(|_| HostError::format("AI Assist settings are invalid."))?;
            validate_ai_assist_settings(&settings)?;
            let cancel_titles = self
                .agent_title_jobs
                .iter()
                .filter(|(_, job)| {
                    !settings.enabled || (job.automatic && !settings.auto_generate_agent_titles)
                })
                .map(|(tab, _)| tab.clone())
                .collect::<Vec<_>>();
            runtime_value(self.runtime_store.set_ai_assist_settings(settings).await)?;
            let titles_canceled = !cancel_titles.is_empty();
            for tab_id in cancel_titles {
                self.cancel_agent_title_job(&tab_id);
                if let Ok(Some(mut tab)) = self.runtime_store.find_workspace_tab(&tab_id).await {
                    tab.payload["agentTitleStatus"] = json!("idle");
                    let _ = self.runtime_store.upsert_workspace_tab(tab).await;
                }
            }
            if titles_canceled {
                self.broadcast_workspace_tabs_changed(None);
            }
        }
        if let Some(value) = payload.get("textActions") {
            let settings: RuntimeTextActionsSettings = serde_json::from_value(value.clone())
                .map_err(|_| HostError::format("textActions is invalid."))?;
            validate_text_actions_settings(&settings)?;
            runtime_value(self.runtime_store.set_text_actions_settings(settings).await)?;
        }
        let value = runtime_value(self.runtime_store.runtime_settings().await)?;
        self.broadcast_authenticated(event("runtimeSettingsChanged", json!({})));
        Ok(value)
    }

    /// Login-item reconcile runs off the actor on the shared deferred budget.
    /// The job re-reads the persisted settings when it runs, so rapid toggles
    /// coalesce onto the latest state and a reconcile failure can never fail
    /// the settings update that already committed.
    fn schedule_autostart_reconcile(&self) {
        let store = self.runtime_store.clone();
        let runtime_dir = self.runtime_dir.clone();
        if let Err(error) = self.deferred_admission.schedule(
            super::deferred_admission::DeferredRequestClass::Maintenance,
            "automation.autostart.reconcile",
            None,
            async move {
                crate::automation_autostart::reconcile_runtime_autostart(&store, &runtime_dir)
                    .await;
            },
        ) {
            tracing::warn!(
                "autostart reconcile was not admitted: {}",
                error.wire_message()
            );
        }
    }

    pub(super) fn start_cli_registration_request(
        &mut self,
        client_id: u64,
        request_id: i64,
        install: bool,
    ) {
        let runtime_dir = self.runtime_dir.clone();
        let inbox = self.inbox.clone();
        tokio::spawn(async move {
            let result = if install {
                install_cli_registration(&runtime_dir)
                    .await
                    .map_err(|error| HostError::state(error.to_string()))
            } else {
                Ok(cli_registration_status(&runtime_dir).await)
            }
            .and_then(|value| {
                serde_json::to_value(value).map_err(|error| HostError::state(error.to_string()))
            });
            let _ = inbox.send(ServerCommand::HostToolFinished {
                client_id,
                request_id,
                result,
                operation_id: None,
                skill: None,
            });
        });
    }

    pub(super) fn start_skill_install_request(
        &mut self,
        client_id: u64,
        request_id: i64,
        payload: &Value,
    ) -> HostResult<()> {
        let operation_id = required_non_blank(payload, "operationId")?;
        let skill_name = required_non_blank(payload, "skill")?;
        let runner_name = required_non_blank(payload, "runner")?;
        let skill = SkillKind::parse(&skill_name)
            .ok_or_else(|| HostError::format("skill must be cli or orchestration."))?;
        let runner = SkillRunner::parse(&runner_name)
            .ok_or_else(|| HostError::format("runner must be auto, npx, or bunx."))?;
        self.broadcast_authenticated(event(
            "agentSkillInstallProgress",
            json!({
                "operationId": operation_id,
                "skill": skill_name,
                "phase": "installing",
                "message": "Installing Skill",
            }),
        ));
        let store = self.runtime_store.clone();
        let runtime_dir = self.runtime_dir.clone();
        let inbox = self.inbox.clone();
        let operation_for_task = operation_id.clone();
        let skill_for_task = skill_name.clone();
        tokio::spawn(async move {
            let install_result = install_skill(skill, runner).await;
            let mut value = serde_json::to_value(&install_result)
                .map_err(|error| HostError::state(error.to_string()));
            if install_result.succeeded && matches!(skill, SkillKind::Orchestration) {
                let settings = store
                    .agent_status_hook_settings()
                    .await
                    .map_err(|error| HostError::state(error.to_string()));
                if let Ok(settings) = settings {
                    let warnings = tokio::task::spawn_blocking(move || {
                        reconcile_agent_integrations(&runtime_dir, &settings)
                    })
                    .await
                    .unwrap_or_else(|error| vec![error.to_string()]);
                    if let Ok(Value::Object(object)) = &mut value {
                        object.insert("hookWarnings".to_string(), json!(warnings));
                    }
                }
            }
            let _ = inbox.send(ServerCommand::HostToolFinished {
                client_id,
                request_id,
                result: value,
                operation_id: Some(operation_for_task),
                skill: Some(skill_for_task),
            });
        });
        Ok(())
    }

    pub(super) fn handle_host_tool_finished(
        &mut self,
        client_id: u64,
        request_id: i64,
        result: HostResult<Value>,
        operation_id: Option<String>,
        skill: Option<String>,
    ) {
        match &result {
            Ok(value) => self.client_write(client_id, ok_response(request_id, value.clone())),
            Err(error) => self.client_write(client_id, error_response(request_id, error)),
        }
        if let (Some(operation_id), Some(skill)) = (operation_id, skill) {
            let succeeded = result
                .as_ref()
                .ok()
                .and_then(|value| value.get("succeeded"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            self.broadcast_authenticated(event(
                "agentSkillInstallProgress",
                json!({
                    "operationId": operation_id,
                    "skill": skill,
                    "phase": if succeeded { "completed" } else { "failed" },
                    "message": result
                        .as_ref()
                        .ok()
                        .and_then(|value| value.get("summary"))
                        .and_then(Value::as_str)
                        .unwrap_or("Skill Install Failed"),
                }),
            ));
        }
    }
}

fn validate_ai_assist_settings(settings: &RuntimeAiAssistSettings) -> HostResult<()> {
    alera_core::runtime::validate_ai_assist_settings(settings)
        .map_err(|error| HostError::format(error.to_string()))
}

fn validate_text_actions_settings(settings: &RuntimeTextActionsSettings) -> HostResult<()> {
    alera_core::runtime::validate_text_actions_settings(settings)
        .map_err(|error| HostError::format(error.to_string()))
}

pub(super) fn validate_agent_quota_settings(
    settings: &RuntimeAgentQuotaSettings,
) -> HostResult<()> {
    let mut aliases = std::collections::HashSet::new();
    let mut profiles = std::collections::HashSet::new();
    for profile in &settings.claude_profiles {
        let alias = profile.alias.trim();
        let profile_name = profile.profile.trim();
        if alias.is_empty() || profile_name.is_empty() {
            return Err(HostError::format(
                "Claude aliases and profiles are required.",
            ));
        }
        if !aliases.insert(alias) || !profiles.insert(profile_name) {
            return Err(HostError::format(
                "Claude aliases and profiles must be unique.",
            ));
        }
    }
    for name in [
        &settings.environment.kimi_api_key,
        &settings.environment.zai_api_key,
        &settings.environment.zai_base_url,
        &settings.environment.minimax_api_key,
        &settings.environment.minimax_api_host,
    ] {
        if !valid_environment_name(name) {
            return Err(HostError::format(format!(
                "Invalid environment variable name: {name}."
            )));
        }
    }
    Ok(())
}

fn valid_environment_name(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && chars.all(|char| char == '_' || char.is_ascii_alphanumeric())
}

fn runtime_value<T, E>(result: Result<T, E>) -> HostResult<Value>
where
    T: Serialize,
    E: std::fmt::Display,
{
    let value = result.map_err(|error| HostError::state(error.to_string()))?;
    serde_json::to_value(value).map_err(|error| HostError::state(error.to_string()))
}

pub(super) fn required_non_blank(payload: &Value, key: &str) -> HostResult<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| HostError::format(format!("{key} must be a non-empty string.")))
}

#[cfg(test)]
mod tests {
    use super::validate_text_actions_settings;
    use alera_core::runtime::{RuntimeTextAction, RuntimeTextActionsSettings};
    use std::collections::HashMap;
    use std::sync::Arc;

    #[tokio::test]
    async fn agent_status_hook_update_does_not_wait_for_reconcile_io_budget() {
        let directory = tempfile::tempdir().unwrap();
        let mut actor = crate::terminal_host::server::actor_test_harness::test_actor(
            &directory,
            HashMap::new(),
            HashMap::new(),
        )
        .await;
        actor.deferred_admission = Arc::new(
            crate::terminal_host::server::deferred_admission::DeferredAdmission::paused_with_limits(
                usize::MAX,
                usize::MAX,
                0,
            ),
        );
        let settings = alera_core::runtime::RuntimeAgentStatusHookSettings::default();

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            actor.apply_mobile_runtime_settings(&serde_json::json!({
                "agentStatusHooks": settings,
            })),
        )
        .await
        .expect("runtime settings persistence must not wait for hook filesystem admission")
        .expect("runtime settings update");

        assert_eq!(result["agentStatusHooks"], serde_json::json!(settings));
        assert_eq!(
            actor
                .runtime_store
                .agent_status_hook_settings()
                .await
                .unwrap(),
            settings
        );
    }

    #[test]
    fn text_action_names_are_case_insensitively_unique() {
        let action = |id: &str, name: &str, prompt: &str| RuntimeTextAction {
            id: id.to_string(),
            name: name.to_string(),
            prompt: prompt.to_string(),
            enabled: true,
            agent_override: None,
            model_override: None,
            reasoning_by_model: HashMap::new(),
        };
        let settings = RuntimeTextActionsSettings {
            actions: vec![
                action("one", "Polish", "Improve"),
                action("two", "polish", "Summarize"),
            ],
        };

        assert!(validate_text_actions_settings(&settings).is_err());
    }

    #[test]
    fn text_action_ids_are_unique() {
        let action = |name: &str| RuntimeTextAction {
            id: "same-id".to_string(),
            name: name.to_string(),
            prompt: "Improve".to_string(),
            enabled: true,
            agent_override: None,
            model_override: None,
            reasoning_by_model: HashMap::new(),
        };
        let settings = RuntimeTextActionsSettings {
            actions: vec![action("Polish"), action("Summarize")],
        };

        assert!(validate_text_actions_settings(&settings).is_err());
    }
}
