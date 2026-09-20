use std::future::Future;

use serde_json::Value;

use crate::managed_workspace::ManagedWorkspaceCreateRequest;
use crate::terminal_host::host_error::HostResult;
use crate::terminal_host::protocol::{error_response, ok_response};

use super::automation_policy_requests::load_automation_policy_show;
use super::deferred_read_requests::DeferredReadRequestHandler;
use super::deferred_request_scheduler::DeferredRequestScheduler;
use super::project_registration_requests::ProjectRegistrationRequestHandler;
use super::request_payloads::parse_payload;
use super::request_route_policy::{request_route_policy, DeferredWriteRoute};
use super::requests::{require_string_key, validate_mobile_runtime_settings_payload};
use super::ServerActor;

impl ServerActor {
    pub(super) fn start_deferred_request<F>(
        &self,
        client_id: u64,
        request_id: i64,
        request_type: &str,
        task: F,
    ) -> HostResult<()>
    where
        F: Future<Output = HostResult<Value>> + Send + 'static,
    {
        self.deferred_request_scheduler()
            .schedule(client_id, request_id, request_type, task)
    }

    pub(super) fn deferred_request_scheduler(&self) -> DeferredRequestScheduler {
        DeferredRequestScheduler::new(self.deferred_admission.clone(), self.inbox.clone())
    }

    pub(super) fn finish_deferred_request(
        &self,
        client_id: u64,
        request_id: i64,
        result: HostResult<Value>,
    ) {
        if self.require_auth(client_id).is_err() {
            return;
        }
        match result {
            Ok(value) => self.client_write(client_id, ok_response(request_id, value)),
            Err(error) => self.client_write(client_id, error_response(request_id, &error)),
        }
    }

    pub(super) async fn try_start_deferred_request(
        &mut self,
        client_id: u64,
        request_id: i64,
        request_type: &str,
        payload: &Value,
    ) -> HostResult<bool> {
        if self.try_start_configuration_cloud(client_id, request_id, request_type, payload)? {
            return Ok(true);
        }
        if self.try_start_account_request(client_id, request_id, request_type, payload)? {
            return Ok(true);
        }
        if (request_type == "runtimeSettings.update"
            || request_type == "mobile.runtimeSettings.update")
            && payload.get("automation").is_some()
        {
            self.require_auth(client_id)?;
            self.require_request_allowed(client_id, request_type)?;
            if request_type == "mobile.runtimeSettings.update" {
                validate_mobile_runtime_settings_payload(payload)?;
            }
            self.start_autostart_reconcile_update(client_id, request_id, payload)
                .await?;
            return Ok(true);
        }
        if self.try_start_serialized_runtime_mutation(
            client_id,
            request_id,
            request_type,
            payload,
        )? {
            return Ok(true);
        }
        if let Some(route) = request_route_policy(request_type).deferred_read {
            self.require_auth(client_id)?;
            self.require_request_allowed(client_id, request_type)?;
            DeferredReadRequestHandler::new(
                self.runtime_store.clone(),
                self.deferred_request_scheduler(),
            )
            .start(route, client_id, request_id, request_type, payload)?;
            return Ok(true);
        }
        if let Some(route) = request_route_policy(request_type).deferred_write {
            self.require_auth(client_id)?;
            self.require_request_allowed(client_id, request_type)?;
            match route {
                DeferredWriteRoute::ProjectRegister => {
                    ProjectRegistrationRequestHandler::new(
                        self.runtime_store.clone(),
                        self.deferred_request_scheduler(),
                    )
                    .start(client_id, request_id, request_type, payload)?;
                }
            }
            return Ok(true);
        }
        match request_type {
            "automation.policy"
                if payload
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("show")
                    == "show"
                    && payload.get("run").and_then(Value::as_str).is_none() =>
            {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                let runtime_store = self.runtime_store.clone();
                self.start_deferred_request(
                    client_id,
                    request_id,
                    request_type,
                    load_automation_policy_show(runtime_store, payload.clone()),
                )?;
                Ok(true)
            }
            "workspaceSidebar.snapshot" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                self.start_workspace_sidebar_snapshot(client_id, request_id);
                Ok(true)
            }
            "mobile.status.get"
                if payload.get("includeNetworkStatus").and_then(Value::as_bool) != Some(false) =>
            {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                self.start_mobile_network_snapshot(client_id, request_id)
                    .await?;
                Ok(true)
            }
            "aiDictation.transcribe" => {
                self.require_authenticated_local_request(client_id, request_type)?;
                self.start_ai_dictation(client_id, request_id, payload)
                    .await?;
                Ok(true)
            }
            "mobile.aiDictation.transcribe" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                self.try_start_mobile_ai_dictation(client_id, request_id, payload)
                    .await
            }
            "aiText.agentTitle.generate" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                self.request_agent_title(client_id, request_id, payload)
                    .await?;
                Ok(true)
            }
            "aiText.workspaceIdentity.generate" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                self.start_ai_assist_workspace_identity(client_id, request_id, payload)?;
                Ok(true)
            }
            "aiText.speechMessage.generate" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                self.start_ai_assist_speech_message(client_id, request_id, payload)?;
                Ok(true)
            }
            "mobile.workspaceQuickOpen.start"
            | "mobile.workspaceQuickOpen.search"
            | "mobile.workspaceFile.read"
            | "mobile.promptAttachment.read" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                self.start_mobile_workspace_file_request(
                    client_id,
                    request_id,
                    request_type,
                    payload,
                )?;
                Ok(true)
            }
            "mobile.promptImage.start"
            | "mobile.promptImage.chunk"
            | "mobile.promptImage.complete"
            | "mobile.promptImage.cancel" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                self.start_mobile_prompt_image_request(
                    client_id,
                    request_id,
                    request_type,
                    payload,
                )?;
                Ok(true)
            }
            "mobile.promptFile.start"
            | "mobile.promptFile.chunk"
            | "mobile.promptFile.complete"
            | "mobile.promptFile.cancel" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                self.start_mobile_prompt_file_request(
                    client_id,
                    request_id,
                    request_type,
                    payload,
                )?;
                Ok(true)
            }
            "workspace.createManaged" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                let mut request: ManagedWorkspaceCreateRequest = parse_payload(payload)?;
                request.setup_script_directory = self.setup_script_directory();
                self.start_managed_workspace_create(client_id, request_id, request);
                Ok(true)
            }
            "workspace.runSetup" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                let workspace_id = require_string_key(payload, "id")?;
                let copies_only = payload
                    .get("copiesOnly")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                self.start_workspace_setup(client_id, request_id, workspace_id, copies_only);
                Ok(true)
            }
            "workspace.storageImpact" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                let workspace_id = require_string_key(payload, "id")?;
                let active_workspace_id = payload
                    .get("activeWorkspaceId")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .map(str::to_string);
                self.start_workspace_storage_measurement(
                    client_id,
                    request_id,
                    workspace_id,
                    active_workspace_id,
                    payload
                        .get("closeSessions")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                );
                Ok(true)
            }
            "write" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                self.queue_terminal_input(client_id, request_id, payload)
            }
            "terminal.pulse.configure" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                self.start_terminal_pulse_configuration(client_id, request_id, payload)
                    .await?;
                Ok(true)
            }
            "agentQuota.snapshot" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                self.start_agent_quota_request(client_id, request_id, payload)?;
                Ok(true)
            }
            "agentUsage.snapshot" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                self.start_agent_usage_request(client_id, request_id, payload)?;
                Ok(true)
            }
            "agentQuota.fetchClaudeTui" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                self.start_agent_quota_claude_tui_request(client_id, request_id, payload)?;
                Ok(true)
            }
            "agentQuota.consumeCodexResetCredit" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                self.start_agent_quota_codex_reset_request(client_id, request_id, payload)
                    .await?;
                Ok(true)
            }
            "cliRegistration.status" | "cliRegistration.install" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                self.start_cli_registration_request(
                    client_id,
                    request_id,
                    request_type.ends_with("install"),
                )?;
                Ok(true)
            }
            "agentSkill.install" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                self.start_skill_install_request(client_id, request_id, payload)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}
