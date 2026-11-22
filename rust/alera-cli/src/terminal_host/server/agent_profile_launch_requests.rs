use alera_core::agent_descriptor::agent_descriptor;
use alera_core::runtime::{AgentProfileLaunchReceiptOutcome, WorkspaceStatus, WorkspaceTabRecord};
use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};

use super::agent_history_resume::VerifiedHistoryResume;
use super::agent_prompt_composition::compose_agent_prompt;
use super::agent_title_state::AgentTitleState;
use super::client_delivery::LocalClientRole;
use super::host_service_requests::required_non_blank;
use super::orchestration_profile_spawn::launch_for_profile;
use super::tab_compatibility::redact_private_tab_payload;
use super::terminal_startup_commands::{configure_provider_resume, resume_bare_agent_command};
use super::workspace_tab_requests::WorkspaceTabStoreHandler;
use super::{ClientKind, ServerActor};
use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::orchestration::agent_profile_launch_snapshot::{
    AgentInitialDeliveryMechanismV1, AgentInitialDeliveryReplayV1, AgentProfileLaunchSnapshotV1,
    AGENT_PROFILE_LAUNCH_SNAPSHOT_KEY,
};

const CLIENT_MUTATION_ID_MAX_BYTES: usize = 128;

fn verify_agy_profile_account_request(payload: &Value) -> HostResult<Option<&str>> {
    match payload.get("agyAccountId") {
        None => Ok(None),
        Some(Value::String(id)) if id == "default" => Ok(None),
        Some(Value::String(id))
            if !id.is_empty()
                && id.len() <= 64
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')) =>
        {
            Ok(Some(id))
        }
        Some(Value::String(_)) => Err(HostError::format("Invalid AGY account ID")),
        Some(_) => Err(HostError::format("agyAccountId must be a string.")),
    }
}

impl ServerActor {
    pub(super) async fn launch_agent_profile(
        &mut self,
        client_id: Option<u64>,
        payload: &Value,
    ) -> HostResult<Value> {
        self.launch_agent_profile_inner(client_id, payload, None)
            .await
    }

    /// Called exclusively after the local Host verified the exact original
    /// native source. Remote/mobile profile launch cannot supply this proof.
    pub(super) async fn launch_agent_profile_with_verified_history(
        &mut self,
        client_id: u64,
        payload: &Value,
        verified: VerifiedHistoryResume,
    ) -> HostResult<Value> {
        self.launch_agent_profile_inner(Some(client_id), payload, Some(verified))
            .await
    }

    async fn launch_agent_profile_inner(
        &mut self,
        client_id: Option<u64>,
        payload: &Value,
        verified_history: Option<VerifiedHistoryResume>,
    ) -> HostResult<Value> {
        let workspace_id = required_non_blank(payload, "workspaceId")?;
        let profile_id = required_non_blank(payload, "profileId")?;
        let prompt = required_non_blank(payload, "prompt")?;
        // Validate the requested account before idempotency lookup. Nondefault
        // accounts must pass live SSH verification before any new PTY is made.
        let selected_agy_account = verify_agy_profile_account_request(payload)?;
        let client_mutation_id = match payload.get("clientMutationId") {
            None => None,
            Some(Value::String(value)) if !value.trim().is_empty() => {
                Some(value.trim().to_string())
            }
            Some(_) => {
                return Err(HostError::format(
                    "clientMutationId must be a non-empty string.",
                ));
            }
        };
        if client_mutation_id
            .as_ref()
            .is_some_and(|value| value.len() > CLIENT_MUTATION_ID_MAX_BYTES)
        {
            return Err(HostError::format(format!(
                "clientMutationId must not exceed {CLIENT_MUTATION_ID_MAX_BYTES} bytes."
            )));
        }
        let caller_scope = client_mutation_id
            .as_ref()
            .map(|_| {
                client_id
                    .ok_or_else(|| {
                        HostError::unauthorized(
                            "clientMutationId requires an authenticated client identity.",
                        )
                    })
                    .and_then(|client_id| self.agent_profile_launch_caller_scope(client_id))
            })
            .transpose()?;
        let automation_run_id = payload
            .get("automationRunId")
            .and_then(Value::as_str)
            .map(str::to_string);
        let automation_owned = payload
            .get("automationOwned")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let payload_digest = client_mutation_id.as_ref().map(|_| {
            let mut digest = agent_profile_launch_payload_digest(
                &workspace_id,
                &profile_id,
                &prompt,
                automation_run_id.as_deref(),
                automation_owned,
            );
            if let Some(history) = verified_history.as_ref() {
                digest = hex::encode(Sha256::digest(
                    format!(
                        "{digest}\0exact_history:{}:{}:{}",
                        history.provider, history.account_id, history.native_id
                    )
                    .as_bytes(),
                ));
            }
            if let Some(account_id) = selected_agy_account {
                hex::encode(Sha256::digest(
                    format!("{digest}\0agy:{account_id}").as_bytes(),
                ))
            } else {
                digest
            }
        });
        if let (Some(mutation_id), Some(scope), Some(digest)) = (
            client_mutation_id.as_deref(),
            caller_scope.as_deref(),
            payload_digest.as_deref(),
        ) {
            match self
                .runtime_store
                .find_agent_profile_launch_receipt(scope, &workspace_id, mutation_id, digest)
                .await
                .map_err(|error| HostError::state(error.to_string()))?
            {
                Some(AgentProfileLaunchReceiptOutcome::Replay(result)) => {
                    return Ok(redact_agent_profile_launch_result(result));
                }
                Some(AgentProfileLaunchReceiptOutcome::Conflict) => {
                    return Err(HostError::state(
                        "clientMutationId was already used with a different agentProfile.launch payload.",
                    ));
                }
                Some(AgentProfileLaunchReceiptOutcome::Created) | None => {}
            }
        }
        if let Some(history) = verified_history.as_ref() {
            // Only new exact-Resume launches need the duplicate-owner guard.
            // A retry of the *same* confirmed request must replay the durable
            // receipt above instead of being mistaken for a second owner.
            super::agent_history_resume::ensure_history_session_unowned(
                &self.runtime_store,
                &workspace_id,
                history,
            )
            .await?;
        }
        let workspace = self
            .runtime_store
            .find_workspace(&workspace_id)
            .await
            .map_err(|error| HostError::state(error.to_string()))?
            .ok_or_else(|| HostError::state(format!("Workspace not found: {workspace_id}")))?;
        if workspace.status != WorkspaceStatus::Active {
            return Err(HostError::state(format!(
                "Workspace is not active: {workspace_id}"
            )));
        }
        let effective_config = crate::project_management::effective_project_config(
            &self.runtime_store,
            &workspace.project_id,
        )
        .await
        .map_err(|error| HostError::state(error.to_string()))?;
        if let Some(error) = effective_config.error {
            return Err(HostError::state(format!(
                "Could not load project configuration: {error}"
            )));
        }
        let profile = self
            .runtime_store
            .find_agent_profile(&profile_id)
            .await
            .map_err(|error| HostError::state(error.to_string()))?
            .ok_or_else(|| HostError::state(format!("Agent profile not found: {profile_id}")))?;
        if let Some(history) = verified_history.as_ref() {
            if profile.agent_type != history.provider
                || automation_owned
                || automation_run_id.is_some()
            {
                return Err(HostError::state(
                    "Exact history resume requires an ordinary matching Claude/Codex profile",
                ));
            }
        }
        if let Some(account_id) = selected_agy_account {
            if !matches!(profile.agent_type.as_str(), "agy" | "antigravity") {
                return Err(HostError::format(
                    "AGY account routing requires an Antigravity Agent profile",
                ));
            }
            let quota_settings = self
                .runtime_store
                .agent_quota_settings()
                .await
                .map_err(|error| HostError::state(error.to_string()))?;
            let registered = quota_settings
                .agy_profiles
                .iter()
                .find(|registered| registered.account_id == account_id)
                .ok_or_else(|| HostError::state("Google account has no registered SSH Host"))?;
            crate::agy_ssh_route::verify_ssh_route(&self.runtime_store, registered)
                .await
                .map_err(|error| HostError::state(format!("AGY SSH Host not verified: {error}")))?;
        }
        let title_prompt = prompt.clone();
        // Resuming an existing conversation must NOT send a new task,
        // profile custom prompt or project append instructions.
        let prompt = if verified_history.is_some() {
            String::new()
        } else {
            compose_agent_prompt(
                &prompt,
                &profile.custom_prompt,
                &effective_config.config.new_workspace.prompt_append,
            )
        };
        let descriptor = agent_descriptor(&profile.agent_type).ok_or_else(|| {
            HostError::format(format!("Unsupported agent type: {}", profile.agent_type))
        })?;
        let (command, managed_launch) = launch_for_profile(&profile).map_err(HostError::format)?;
        if let Some(history) = verified_history.as_ref() {
            let supported = match (command.as_deref(), managed_launch.as_ref()) {
                (None, Some(launch)) => {
                    // Profile launch arguments can choose a different auth/config
                    // home. For v1 permit only the standard CLI with no flags
                    // (or Codex's known --no-daemon flag).
                    let allowed = launch.arguments.is_empty()
                        || (history.provider == "codex" && launch.arguments == ["--no-daemon"]);
                    let mut candidate = launch.clone();
                    allowed
                        && configure_provider_resume(
                            &mut candidate,
                            &history.provider,
                            &history.native_id,
                        )
                }
                (Some(command), None) => resume_bare_agent_command(
                    command,
                    &history.provider,
                    &history.native_id,
                    "pwsh.exe",
                )
                .is_some(),
                _ => false,
            };
            if !supported {
                return Err(HostError::state(
                    "Resume requires a bare official Claude/Codex CLI profile without account overrides, wrappers, or existing resume flags",
                ));
            }
        }
        let launch_snapshot = AgentProfileLaunchSnapshotV1::new(
            &profile,
            descriptor,
            command,
            managed_launch,
            AgentInitialDeliveryReplayV1::Once,
        )
        .map_err(HostError::format)?;
        let prompt_after_ready = verified_history.is_none()
            && launch_snapshot.initial_delivery.mechanism
                == AgentInitialDeliveryMechanismV1::TerminalAfterReady;
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now();
        let mut tab_payload = json!({
            "terminalSessionId": id,
            // Most adapters take the prompt at launch. fx receives it
            // after its built-in lifecycle reports that the TUI is ready.
            "initialPrompt": (verified_history.is_none() && !prompt_after_ready).then(|| prompt.clone()),
            "pendingAgentPrompt": prompt_after_ready.then(|| json!({
                "agent": descriptor.id,
                "prompt": prompt,
            })),
            "spawnOnCreate": true,
            "automationRunId": automation_run_id,
            "automationOwned": automation_owned,
            "agyAccountId": selected_agy_account,
        });
        tab_payload[AGENT_PROFILE_LAUNCH_SNAPSHOT_KEY] = serde_json::to_value(launch_snapshot)
            .map_err(|error| {
                HostError::state(format!(
                    "could not encode agent profile launch snapshot: {error}"
                ))
            })?;
        let mut tab = WorkspaceTabRecord {
            id: id.clone(),
            workspace_id: workspace_id.clone(),
            kind: "terminal".to_string(),
            title: profile.name.clone(),
            created_at: now,
            updated_at: now,
            payload: tab_payload,
        };
        super::agent_title_state::initialize(&mut tab, &title_prompt);
        if let Some(history) = verified_history.as_ref() {
            configure_exact_history_checkpoint(&mut tab, history)?;
        }
        let mut projected_tab = tab.clone();
        redact_private_tab_payload(&mut projected_tab);
        let result = json!({
            "tab": projected_tab,
            "agentType": descriptor.id,
            "profileId": profile.id,
        });
        let Some(mutation_id) = client_mutation_id.as_deref() else {
            let mut saved = self.upsert_workspace_tab_and_spawn(tab).await?;
            self.observe_agent_title(&saved.id, descriptor.id, None, &title_prompt, true)
                .await;
            redact_private_tab_payload(&mut saved);
            return Ok(json!({
                "tab": saved,
                "agentType": descriptor.id,
                "profileId": profile.id,
            }));
        };
        let scope = caller_scope.expect("mutation caller scope was computed");
        let digest = payload_digest.expect("mutation payload digest was computed");
        match self
            .runtime_store
            .record_agent_profile_launch(&scope, &workspace_id, mutation_id, &digest, &result, &tab)
            .await
            .map_err(|error| HostError::state(error.to_string()))?
        {
            AgentProfileLaunchReceiptOutcome::Replay(result) => {
                return Ok(redact_agent_profile_launch_result(result));
            }
            AgentProfileLaunchReceiptOutcome::Conflict => {
                return Err(HostError::state(
                    "clientMutationId was already used with a different agentProfile.launch payload.",
                ));
            }
            AgentProfileLaunchReceiptOutcome::Created => {}
        }
        if let Err(error) = self.ensure_spawn_on_create_terminal(&tab).await {
            if let Err(cleanup_error) = WorkspaceTabStoreHandler::new(&self.runtime_store)
                .remove(&tab.id)
                .await
            {
                tracing::error!(
                    tab_id = %tab.id,
                    "failed to roll back agent profile launch receipt: {cleanup_error}"
                );
            }
            self.terminate_sessions_for_tab(&tab.id).await;
            return Err(error);
        }
        self.observe_agent_title(&tab.id, descriptor.id, None, &title_prompt, true)
            .await;
        self.broadcast_workspace_tabs_changed(Some(&workspace_id));
        Ok(result)
    }

    pub(in crate::terminal_host::server) fn agent_profile_launch_caller_scope(
        &self,
        client_id: u64,
    ) -> HostResult<String> {
        let client = self
            .clients
            .get(&client_id)
            .ok_or_else(|| HostError::unauthorized("Terminal host client is not authenticated."))?;
        match client.kind {
            ClientKind::Mobile => client
                .mobile_device_id
                .as_ref()
                .map(|device_id| format!("mobile:{device_id}"))
                .ok_or_else(|| {
                    HostError::unauthorized("Authenticated mobile device identity is missing.")
                }),
            ClientKind::Local => Ok(match client.local_role {
                LocalClientRole::App => "local:app",
                LocalClientRole::Cli => "local:cli",
            }
            .to_string()),
        }
    }

    /// Types a prompt into an agent that reported it is idle.
    ///
    /// fx has no interactive initial-prompt argument. Its lifecycle receiver
    /// reports the first idle state after the TUI is ready, which makes this
    /// PTY paste deterministic. Older tabs may also still carry the payload.
    pub(super) async fn deliver_pending_agent_prompt(&mut self, session_id: &str) {
        if !self.agent_presence.is_injection_ready(session_id) {
            return;
        }
        let Some(session) = self.sessions.get(session_id) else {
            return;
        };
        if session.initial_agent_prompt_delivered {
            return;
        }
        let tab_id = session.tab_id.clone();
        let instance_id = session.instance_id();
        let Ok(Some(mut tab)) = WorkspaceTabStoreHandler::new(&self.runtime_store)
            .find(&tab_id)
            .await
        else {
            return;
        };
        let Some(pending) = tab.payload.get("pendingAgentPrompt").cloned() else {
            return;
        };
        let Some(prompt) = pending.get("prompt").and_then(Value::as_str) else {
            return;
        };
        let matches_agent = pending
            .get("agent")
            .and_then(Value::as_str)
            .is_some_and(|agent| self.agent_presence.agent_type(session_id) == Some(agent));
        if !matches_agent {
            return;
        }
        let bytes =
            crate::terminal_host::orchestration::agent_prompt_injection::build_agent_prompt_paste_bytes(
                prompt,
            );
        let Some(session) = self.sessions.get_mut(session_id) else {
            return;
        };
        if session
            .queue_write_deferred(
                crate::terminal_host::session::PtyWriteCompletion::OrchestrationPaste {
                    session_instance_id: instance_id,
                    message_ids: Vec::new(),
                    force_submit: true,
                },
                &bytes,
                std::time::Duration::from_millis(
                    crate::terminal_host::orchestration::message_delivery::DEFERRED_ENTER_DELAY_MS,
                ),
                crate::terminal_host::orchestration::agent_prompt_injection::AGENT_PROMPT_SUBMIT,
            )
            .is_err()
        {
            return;
        }
        session.initial_agent_prompt_delivered = true;
        tab.payload["pendingAgentPrompt"] = Value::Null;
        tab.updated_at = chrono::Utc::now();
        let workspace_id = tab.workspace_id.clone();
        if let Err(error) = WorkspaceTabStoreHandler::new(&self.runtime_store)
            .upsert(tab)
            .await
        {
            tracing::error!(tab_id = %tab_id, "failed to clear pending agent prompt: {error}");
        }
        self.broadcast_workspace_tabs_changed(Some(&workspace_id));
    }
}

fn configure_exact_history_checkpoint(
    tab: &mut WorkspaceTabRecord,
    history: &VerifiedHistoryResume,
) -> HostResult<()> {
    let mut state = AgentTitleState::read(tab).ok_or_else(|| {
        HostError::state("Could not initialize exact historical resume checkpoint")
    })?;
    if !matches!(history.provider.as_str(), "claude" | "codex")
        || uuid::Uuid::parse_str(&history.native_id).is_err()
    {
        return Err(HostError::state(
            "Exact historical resume identity is invalid",
        ));
    }
    state.agent = Some(history.provider.clone());
    state.native_id = Some(history.native_id.clone());
    state.codex_transcript_path = history.codex_transcript_path.clone();
    state.eligible = false;
    state.attempted = true;
    state.closed = false;
    state.initial_prompt.clear();
    tab.payload[super::agent_history_resume::RESUME_ORIGIN_KEY] = json!({
        "provider": history.provider,
        "accountId": history.account_id,
        "sourceDirectoryFingerprint": super::agent_history_resume::source_directory_fingerprint(
            &history.config_directory,
        ),
    });
    // Future prompt-path changes must not inject a new task into an existing
    // provider conversation or silently reinterpret Resume as a fresh chat.
    tab.payload["initialPrompt"] = Value::Null;
    tab.payload["pendingAgentPrompt"] = Value::Null;
    state.write(tab);
    Ok(())
}

fn redact_agent_profile_launch_result(mut result: Value) -> Value {
    let Some(tab_value) = result.get_mut("tab") else {
        return result;
    };
    let Ok(mut tab) = serde_json::from_value::<WorkspaceTabRecord>(tab_value.clone()) else {
        return result;
    };
    redact_private_tab_payload(&mut tab);
    *tab_value = json!(tab);
    result
}

fn agent_profile_launch_payload_digest(
    workspace_id: &str,
    profile_id: &str,
    prompt: &str,
    automation_run_id: Option<&str>,
    automation_owned: bool,
) -> String {
    let canonical = json!({
        "automationOwned": automation_owned,
        "automationRunId": automation_run_id,
        "profileId": profile_id,
        "prompt": prompt,
        "workspaceId": workspace_id,
    });
    let bytes = serde_json::to_vec(&canonical).expect("canonical launch payload is serializable");
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::terminal_host::client::ClientHandle;
    use crate::terminal_host::host_error::{HostError, OutcomeClass};
    use crate::terminal_host::server::actor_test_harness::{mobile_client, test_actor};

    #[test]
    fn verified_history_checkpoint_retains_exact_uuid_without_replaying_a_prompt() {
        let now = chrono::Utc::now();
        let mut tab = WorkspaceTabRecord {
            id: "tab".into(),
            workspace_id: "workspace".into(),
            kind: "terminal".into(),
            title: "Original".into(),
            created_at: now,
            updated_at: now,
            payload: json!({
                "agentType":"codex",
                "spawnOnCreate":true,
                "initialPrompt":"DO NOT REPLAY",
                "pendingAgentPrompt":{"agent":"codex","prompt":"DO NOT REPLAY"},
            }),
        };
        super::super::agent_title_state::initialize(&mut tab, "DO NOT REPLAY");
        let native = "550e8400-e29b-41d4-a716-446655440001";
        configure_exact_history_checkpoint(
            &mut tab,
            &VerifiedHistoryResume {
                provider: "codex".into(),
                account_id: "other".into(),
                native_id: native.into(),
                codex_transcript_path: Some("verified/native/transcript.jsonl".into()),
                source_file: std::path::PathBuf::from("verified/native/transcript.jsonl"),
                config_directory: "provider-config-dir".into(),
            },
        )
        .unwrap();
        let state = AgentTitleState::read(&tab).unwrap();
        assert_eq!(state.native_id.as_deref(), Some(native));
        assert_eq!(state.agent.as_deref(), Some("codex"));
        assert!(!state.eligible);
        assert!(!state.closed);
        assert!(state.initial_prompt.is_empty());
        assert!(tab.payload["initialPrompt"].is_null());
        assert!(tab.payload["pendingAgentPrompt"].is_null());
        assert_eq!(
            tab.payload[super::super::agent_history_resume::RESUME_ORIGIN_KEY]["accountId"],
            "other"
        );
        assert_eq!(
            tab.payload[super::super::agent_history_resume::RESUME_ORIGIN_KEY]
                ["sourceDirectoryFingerprint"],
            super::super::agent_history_resume::source_directory_fingerprint("provider-config-dir")
        );
        assert_eq!(
            super::super::terminal_startup_commands::provider_resume_id(&tab),
            Some(("codex", native.to_owned()))
        );
    }

    #[test]
    fn agent_profile_launch_validates_google_account_scope_before_host_mutation() {
        for request in [
            json!({"agyAccountId": ""}),
            json!({"agyAccountId": null}),
            json!({"agyAccountId": 123}),
            json!({"agyAccountId": "../escape"}),
        ] {
            assert!(verify_agy_profile_account_request(&request).is_err());
        }
        assert_eq!(
            verify_agy_profile_account_request(&json!({"agyAccountId": "research"})).unwrap(),
            Some("research")
        );
        assert!(verify_agy_profile_account_request(&json!({})).is_ok());
        assert!(verify_agy_profile_account_request(&json!({"agyAccountId": "default"})).is_ok());
    }

    #[test]
    fn payload_digest_uses_the_effective_launch_payload() {
        let first = agent_profile_launch_payload_digest(
            "workspace-1",
            "profile-1",
            "Build it",
            None,
            false,
        );
        assert_eq!(
            first,
            agent_profile_launch_payload_digest(
                "workspace-1",
                "profile-1",
                "Build it",
                None,
                false,
            )
        );
        assert_ne!(
            first,
            agent_profile_launch_payload_digest(
                "workspace-1",
                "profile-1",
                "Build something else",
                None,
                false,
            )
        );
    }

    fn assert_unauthorized(error: HostError, expected_message: &str) {
        assert_eq!(error.outcome_class(), OutcomeClass::Unauthorized);
        assert_eq!(error.wire_message(), expected_message);
    }

    #[tokio::test]
    async fn authentication_errors_are_unauthorized_with_legacy_messages() {
        let dir = tempfile::tempdir().unwrap();
        let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;

        assert_unauthorized(
            actor.agent_profile_launch_caller_scope(1).unwrap_err(),
            "Terminal host client is not authenticated.",
        );
        assert_unauthorized(
            actor
                .launch_agent_profile(
                    None,
                    &json!({
                        "workspaceId": "workspace-1",
                        "profileId": "profile-1",
                        "prompt": "Build it",
                        "clientMutationId": "mutation-1",
                    }),
                )
                .await
                .unwrap_err(),
            "clientMutationId requires an authenticated client identity.",
        );

        let dir = tempfile::tempdir().unwrap();
        let (handle, _receiver) = ClientHandle::test_channels();
        let mut client = mobile_client(handle, "phone");
        client.mobile_device_id = None;
        let actor = test_actor(&dir, HashMap::from([(1, client)]), HashMap::new()).await;
        assert_unauthorized(
            actor.agent_profile_launch_caller_scope(1).unwrap_err(),
            "Authenticated mobile device identity is missing.",
        );
    }
}
