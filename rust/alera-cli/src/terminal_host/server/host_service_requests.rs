use serde_json::{json, Value};

use crate::agent_status::reconcile_agent_integrations;
use crate::host_tools::{
    cli_registration_status, install_cli_registration, install_skill, SkillKind, SkillRunner,
};
use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::protocol::{error_response, event, ok_response};

use super::deferred_admission::DeferredRequestClass;
use super::request_route_policy::CliRegistrationOperation;
use super::{ServerActor, ServerCommand};

impl ServerActor {
    /// Fire-and-forget login-item reconcile for no-id updates. Identified
    /// `runtimeSettings.update` calls wait on `AutostartReconcileFinished`.
    pub(super) fn schedule_autostart_reconcile(&self) {
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

    pub(super) async fn start_autostart_reconcile_update(
        &mut self,
        client_id: u64,
        request_id: i64,
        payload: &Value,
    ) -> HostResult<()> {
        let value = self.persist_runtime_settings_update(payload).await?;
        let store = self.runtime_store.clone();
        let runtime_dir = self.runtime_dir.clone();
        let inbox = self.inbox.clone();
        self.deferred_admission.schedule(
            super::deferred_admission::DeferredRequestClass::Maintenance,
            "automation.autostart.reconcile",
            Some(client_id),
            async move {
                let result = crate::automation_autostart::reconcile_runtime_autostart_result(
                    &store,
                    &runtime_dir,
                )
                .await
                .map_err(|error| HostError::state(error.to_string()));
                let _ = inbox.send(ServerCommand::AutostartReconcileFinished {
                    client_id,
                    request_id,
                    value,
                    result,
                });
            },
        )?;
        Ok(())
    }

    pub(super) fn handle_autostart_reconcile_finished(
        &mut self,
        client_id: u64,
        request_id: i64,
        value: Value,
        result: HostResult<()>,
    ) {
        if self.require_auth(client_id).is_err() {
            return;
        }
        match result {
            Ok(()) => {
                self.broadcast_authenticated(event("runtimeSettingsChanged", json!({})));
                self.client_write(client_id, ok_response(request_id, value));
            }
            Err(error) => self.client_write(client_id, error_response(request_id, &error)),
        }
    }

    pub(super) fn start_cli_registration_request(
        &mut self,
        client_id: u64,
        request_id: i64,
        operation: CliRegistrationOperation,
    ) -> HostResult<()> {
        let runtime_dir = self.runtime_dir.clone();
        let inbox = self.inbox.clone();
        self.deferred_admission.schedule(
            DeferredRequestClass::Bulk,
            operation.request_type(),
            Some(client_id),
            async move {
                let result = match operation {
                    CliRegistrationOperation::Install => install_cli_registration(&runtime_dir)
                        .await
                        .map_err(|error| HostError::state(error.to_string())),
                    CliRegistrationOperation::Status => {
                        Ok(cli_registration_status(&runtime_dir).await)
                    }
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
            },
        )
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
        self.deferred_admission.schedule(
            DeferredRequestClass::Bulk,
            "agentSkill.install",
            Some(client_id),
            async move {
                let install_result = install_skill(skill, runner).await;
                let mut value = serde_json::to_value(&install_result)
                    .map_err(|error| HostError::state(error.to_string()));
                if install_result.succeeded && matches!(skill, SkillKind::Orchestration) {
                    let settings = store
                        .agent_status_hook_settings()
                        .await
                        .map_err(|error| HostError::state(error.to_string()));
                    if let Ok(settings) = settings {
                        let _permit =
                            super::host_service_agent_integrations::agent_integration_semaphore()
                                .acquire()
                                .await;
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
            },
        )
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
    use crate::terminal_host::server::requests::validate_text_actions_settings;
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
