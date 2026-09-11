use std::future::Future;

use serde_json::Value;

use crate::managed_workspace::{
    ManagedWorkspaceCreateRequest, ManagedWorkspaceRemoveRequest,
    ManagedWorkspaceSwitchBranchRequest,
};
use crate::project_management::list_host_directory;
use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::protocol::{error_response, ok_response};

use super::automation_policy_requests::load_automation_policy_show;
use super::project_requests::{load_effective_project_config, load_project_branches};
use super::request_payloads::parse_payload;
use super::requests::require_string_key;
use super::runtime_mutations::RuntimeMutationRequest;
use super::workspace_sidebar_requests::load_workspace_repository_web_url;
use super::{ServerActor, ServerCommand};

impl ServerActor {
    pub(super) fn start_deferred_request<F>(&self, client_id: u64, request_id: i64, task: F)
    where
        F: Future<Output = HostResult<Value>> + Send + 'static,
    {
        let inbox = self.inbox.clone();
        let slots = self.deferred_request_slots.clone();
        tokio::spawn(async move {
            let _permit = slots
                .acquire_owned()
                .await
                .expect("deferred request semaphore must remain open");
            let result = task.await;
            let _ = inbox.send(ServerCommand::DeferredRequestFinished {
                client_id,
                request_id,
                result,
            });
        });
    }

    pub(super) fn start_deferred_blocking_request<F>(
        &self,
        client_id: u64,
        request_id: i64,
        task: F,
    ) where
        F: FnOnce() -> HostResult<Value> + Send + 'static,
    {
        self.start_deferred_request(client_id, request_id, async move {
            tokio::task::spawn_blocking(task)
                .await
                .unwrap_or_else(|error| {
                    Err(HostError::state(format!(
                        "Deferred request failed: {error}"
                    )))
                })
        });
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
                    load_automation_policy_show(runtime_store, payload.clone()),
                );
                Ok(true)
            }
            "projectConfig.effective" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                let project_id = require_string_key(payload, "projectId")?.to_string();
                let runtime_store = self.runtime_store.clone();
                self.start_deferred_request(
                    client_id,
                    request_id,
                    load_effective_project_config(runtime_store, project_id),
                );
                Ok(true)
            }
            "project.branches.list" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                let project_id = require_string_key(payload, "projectId")?.to_string();
                let runtime_store = self.runtime_store.clone();
                self.start_deferred_request(
                    client_id,
                    request_id,
                    load_project_branches(runtime_store, project_id),
                );
                Ok(true)
            }
            "workspace.repositoryWebUrl" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                let workspace_id = require_string_key(payload, "workspaceId")?.to_string();
                let runtime_store = self.runtime_store.clone();
                self.start_deferred_request(
                    client_id,
                    request_id,
                    load_workspace_repository_web_url(runtime_store, workspace_id),
                );
                Ok(true)
            }
            "hostDirectory.list" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                let path = require_string_key(payload, "path")?.to_string();
                self.start_deferred_blocking_request(client_id, request_id, move || {
                    let entries = list_host_directory(&path)
                        .map_err(|error| HostError::state(error.to_string()))?;
                    serde_json::to_value(entries)
                        .map_err(|error| HostError::state(error.to_string()))
                });
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
            "mobile.promptFile.start"
            | "mobile.promptFile.chunk"
            | "mobile.promptFile.complete"
            | "mobile.promptFile.cancel" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                self.start_mobile_prompt_file_request(client_id, request_id, request_type, payload);
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
            "workspace.switchBranch" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                let request: ManagedWorkspaceSwitchBranchRequest = parse_payload(payload)?;
                self.start_runtime_mutation(
                    client_id,
                    request_id,
                    RuntimeMutationRequest::SwitchWorkspaceBranch { request },
                );
                Ok(true)
            }
            "workspace.removeManaged" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                let request: ManagedWorkspaceRemoveRequest = parse_payload(payload)?;
                if !request.close_sessions
                    && request.active_workspace_id.as_deref() == Some(request.id.as_str())
                {
                    return Err(HostError::state("Workspace is active in the workbench"));
                }
                if !request.close_sessions
                    && self
                        .sessions
                        .values()
                        .any(|session| session.workspace_id == request.id && session.running())
                {
                    return Err(HostError::state(
                        "Workspace has a live terminal session or process",
                    ));
                }
                let has_active_automation =
                    crate::managed_workspace::workspace_has_active_automation_owner(
                        &self.runtime_store,
                        &request.id,
                    )
                    .await
                    .map_err(|error| HostError::state(error.to_string()))?;
                if has_active_automation {
                    return Err(HostError::state(
                        "Workspace is owned by an active automation",
                    ));
                }
                crate::managed_workspace::validate_managed_workspace_removal(
                    &self.runtime_store,
                    &request,
                )
                .await
                .map_err(|error| HostError::state(error.to_string()))?;
                crate::managed_workspace::validate_workspace_storage_path(
                    &self.runtime_store,
                    &request.id,
                )
                .await
                .map_err(|error| HostError::state(error.to_string()))?;
                self.start_runtime_mutation(
                    client_id,
                    request_id,
                    RuntimeMutationRequest::RemoveManagedWorkspace { request },
                );
                Ok(true)
            }
            "project.remove" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                let project_id = require_string_key(payload, "id")?;
                self.start_runtime_mutation(
                    client_id,
                    request_id,
                    RuntimeMutationRequest::RemoveProject { project_id },
                );
                Ok(true)
            }
            "workspace.remove" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                let workspace_id = require_string_key(payload, "id")?;
                let cascade_tabs = payload
                    .get("cascadeTabs")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                self.start_runtime_mutation(
                    client_id,
                    request_id,
                    RuntimeMutationRequest::RemoveWorkspace {
                        workspace_id,
                        cascade_tabs,
                    },
                );
                Ok(true)
            }
            "workspace.removeForProject" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                let project_id = require_string_key(payload, "projectId")?;
                self.start_runtime_mutation(
                    client_id,
                    request_id,
                    RuntimeMutationRequest::RemoveProjectWorkspaces { project_id },
                );
                Ok(true)
            }
            "workspace.sleep" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                let workspace_id = require_string_key(payload, "workspaceId")?;
                self.start_runtime_mutation(
                    client_id,
                    request_id,
                    RuntimeMutationRequest::SleepWorkspace { workspace_id },
                );
                Ok(true)
            }
            "tab.remove" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                let tab_id = require_string_key(payload, "id")?;
                self.cancel_agent_title_job(&tab_id);
                self.start_runtime_mutation(
                    client_id,
                    request_id,
                    RuntimeMutationRequest::RemoveTab { tab_id },
                );
                Ok(true)
            }
            "tab.removeForWorkspace" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                let workspace_id = require_string_key(payload, "workspaceId")?;
                self.start_runtime_mutation(
                    client_id,
                    request_id,
                    RuntimeMutationRequest::RemoveWorkspaceTabs { workspace_id },
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
                self.start_agent_quota_codex_reset_request(client_id, request_id, payload);
                Ok(true)
            }
            "cliRegistration.status" | "cliRegistration.install" => {
                self.require_auth(client_id)?;
                self.require_request_allowed(client_id, request_type)?;
                self.start_cli_registration_request(
                    client_id,
                    request_id,
                    request_type.ends_with("install"),
                );
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
