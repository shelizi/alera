//! Explicit, local-only exact history resume. A JSONL file proves its own
//! provider UUID and registered CWD but never proves the active CLI login.
//! Until account-selection launches can be verified, only the configured
//! *default* provider root may be resumed after a separate user confirmation.
//! No arbitrary transcript path, shell command or provider argv is accepted.
use alera_core::runtime::{RuntimeStore, WorkspaceStatus};
use serde_json::{json, Value};
use uuid::Uuid;

use super::ServerActor;
use crate::agent_history_sync::search_roots;
use crate::agent_session_history_scan::{
    discover_history, HistoryScanPolicy, HistoryScanReport, HistorySourceProof,
    RegisteredHistoryWorkspace,
};
use crate::terminal_host::host_error::{HostError, HostResult};
use super::terminal_startup_commands::provider_resume_id;

#[derive(Clone, Debug)]
pub(super) struct VerifiedHistoryResume {
    pub provider: String,
    pub native_id: String,
    pub codex_transcript_path: Option<String>,
}

fn bounded_identifier(payload: &Value, key: &str, max: usize) -> HostResult<String> {
    match payload.get(key).and_then(Value::as_str) {
        Some(value)
            if !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control) =>
        {
            Ok(value.to_owned())
        }
        _ => Err(HostError::format(format!("Invalid exact history {key}"))),
    }
}

#[derive(Debug)]
struct ExactResumeRequest {
    provider: String,
    account: String,
    project: String,
    workspace: String,
    profile: String,
    native_id: String,
    client_mutation_id: String,
}

/// No history title, partial UUID, or mirrored account can select an exact
/// session. The full identity must match the current bounded native scan.
fn exact_source<'a>(
    report: &'a HistoryScanReport,
    request: &ExactResumeRequest,
) -> Option<&'a HistorySourceProof> {
    report.source_proofs.iter().find(|proof| {
        proof.matches(
            &request.provider,
            &request.account,
            &request.project,
            &request.workspace,
            &request.native_id,
        ) && !proof.runtime_instance_id.is_empty()
    })
}

impl ExactResumeRequest {
    fn parse(payload: &Value) -> HostResult<Self> {
        if payload.get("confirmResume") != Some(&Value::Bool(true))
            || payload.get("confirmProviderAccount") != Some(&Value::Bool(true))
        {
            return Err(HostError::state(
                "Explicit resume and current CLI account confirmations are required.",
            ));
        }
        let provider = bounded_identifier(payload, "provider", 12)?;
        if !matches!(provider.as_str(), "codex" | "claude") {
            return Err(HostError::format("Unsupported historical resume provider"));
        }
        let account = bounded_identifier(payload, "accountId", 128)?;
        if account != "default" {
            return Err(HostError::state(
                "Non-default provider accounts remain preview-only until their launch environment can be verified.",
            ));
        }
        let native = bounded_identifier(payload, "providerSessionId", 64)?;
        let native_id = Uuid::parse_str(&native)
            .map_err(|_| HostError::format("Invalid provider-native session UUID"))?
            .to_string();
        if native != native_id {
            return Err(HostError::format(
                "Provider-native session UUID is not canonical",
            ));
        }
        let client_mutation_id = bounded_identifier(payload, "clientMutationId", 128)?;
        Uuid::parse_str(&client_mutation_id)
            .map_err(|_| HostError::format("Exact resume requires a fresh mutation UUID"))?;
        Ok(Self {
            provider,
            account,
            project: bounded_identifier(payload, "projectId", 256)?,
            workspace: bounded_identifier(payload, "workspaceId", 256)?,
            profile: bounded_identifier(payload, "profileId", 256)?,
            native_id,
            client_mutation_id,
        })
    }
}

async fn verify_native_source(
    store: &RuntimeStore,
    target: &ExactResumeRequest,
) -> HostResult<VerifiedHistoryResume> {
    let workspace = store
        .find_workspace(&target.workspace)
        .await
        .map_err(|error| HostError::state(error.to_string()))?
        .ok_or_else(|| HostError::state("Original registered Worktree is missing"))?;
    if workspace.status != WorkspaceStatus::Active
        || workspace.project_id != target.project
        || workspace.instance_id.is_empty()
    {
        return Err(HostError::state(
            "Original Project/Worktree is no longer active or no longer matches",
        ));
    }
    let settings = store
        .agent_quota_settings()
        .await
        .map_err(|error| HostError::state(error.to_string()))?;
    let roots = search_roots(&settings)
        .await
        .into_iter()
        .filter(|root| root.provider == target.provider && root.account_id == target.account)
        .collect::<Vec<_>>();
    let known = vec![RegisteredHistoryWorkspace {
        runtime_instance_id: workspace.instance_id,
        project_id: workspace.project_id,
        workspace_id: workspace.id,
        path: workspace.path.into(),
    }];
    let scan = tokio::task::spawn_blocking(move || {
        discover_history(
            &roots,
            &known,
            HistoryScanPolicy {
                index_user_text: false,
                max_files: 256,
            },
        )
    })
    .await
    .map_err(|_| HostError::state("History source verifier failed"))?;
    let proof = exact_source(&scan, target);
    let Some(proof) = proof else {
        return Err(HostError::state(
            "Exact source unavailable within bounded verification; no Agent was launched.",
        ));
    };
    Ok(VerifiedHistoryResume {
        provider: target.provider.clone(),
        native_id: target.native_id.clone(),
        codex_transcript_path: (target.provider == "codex")
            .then(|| proof.canonical_file.to_string_lossy().into_owned()),
    })
}

impl ServerActor {
    pub(super) async fn resume_exact_agent_history(
        &mut self,
        client_id: u64,
        payload: &Value,
    ) -> HostResult<Value> {
        // Route policy already rejects mobile and unauthenticated callers.
        let target = ExactResumeRequest::parse(payload)?;
        let verified = verify_native_source(&self.runtime_store, &target).await?;
        // Prefer an existing Alera-owned tab over spawning a second CLI that
        // edits/locks the same native provider session concurrently. This
        // includes stopped tabs that Alera already knows how to restore.
        let existing = self
            .runtime_store
            .list_workspace_tabs(&target.workspace)
            .await
            .map_err(|err| HostError::state(err.to_string()))?
            .into_iter()
            .any(|tab| {
                provider_resume_id(&tab).is_some_and(|(provider, native)| {
                    provider == target.provider && native == target.native_id
                })
            });
        if existing {
            return Err(HostError::state(
                "This exact provider session already has an Alera Tab in its original Worktree. Open or restore that Tab instead of creating a duplicate.",
            ));
        }
        self.launch_agent_profile_with_verified_history(
            client_id,
            &json!({
                "workspaceId": target.workspace,
                "profileId": target.profile,
                // Display title only. Never forward a new prompt to a resumed conversation.
                "prompt": "Resume historical Agent conversation",
                "clientMutationId": target.client_mutation_id,
            }),
            verified,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_payload() -> Value {
        json!({
            "provider":"codex",
            "accountId":"default",
            "projectId":"project",
            "workspaceId":"workspace",
            "profileId":"codex-profile",
            "providerSessionId":"550e8400-e29b-41d4-a716-446655440001",
            "clientMutationId":"550e8400-e29b-41d4-a716-446655440002",
            "confirmResume":true,
            "confirmProviderAccount":true,
        })
    }

    #[test]
    fn exact_resume_requires_full_identity_and_two_confirmations() {
        let mut request = valid_payload();
        assert!(ExactResumeRequest::parse(&request).is_ok());
        request["confirmProviderAccount"] = json!(false);
        assert!(ExactResumeRequest::parse(&request).is_err());
        request["confirmProviderAccount"] = json!(true);
        request["accountId"] = json!("secondary");
        assert!(ExactResumeRequest::parse(&request).is_err());
        request["accountId"] = json!("default");
        request["providerSessionId"] = json!("--last");
        assert!(ExactResumeRequest::parse(&request).is_err());
        request["providerSessionId"] = json!("550e8400-e29b-41d4-a716-446655440001");
        request["clientMutationId"] = json!("not-a-uuid");
        assert!(ExactResumeRequest::parse(&request).is_err());
    }

    #[tokio::test]
    async fn forged_history_cannot_create_a_tab_or_index() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        let request = ExactResumeRequest::parse(&valid_payload()).unwrap();
        assert!(verify_native_source(&store, &request).await.is_err());
        assert!(!dir.path().join("agent-session-search").exists());
        assert!(store.list_all_workspaces().await.unwrap().is_empty());
    }

    #[test]
    fn exact_resume_rejects_other_accounts_worktrees_and_native_ids() {
        let root = tempfile::tempdir().unwrap();
        let worktree = tempfile::tempdir().unwrap();
        let native = "550e8400-e29b-41d4-a716-446655440001";
        let dir = root.path().join("sessions/2026/11/21");
        std::fs::create_dir_all(&dir).unwrap();
        let transcript = dir.join(format!("rollout-2026-11-21T10-00-00-{native}.jsonl"));
        std::fs::write(
            &transcript,
            format!(
                "{}\n",
                json!({"type":"session_meta","payload":{"id":native,"cwd":worktree.path()}})
            ),
        )
        .unwrap();
        let report = discover_history(
            &[crate::agent_session_history_scan::ProviderHistoryRoot {
                provider: "codex",
                account_id: "default".into(),
                path: root.path().join("sessions"),
            }],
            &[RegisteredHistoryWorkspace {
                runtime_instance_id: "runtime".into(),
                project_id: "project".into(),
                workspace_id: "workspace".into(),
                path: worktree.path().into(),
            }],
            HistoryScanPolicy {
                index_user_text: false,
                max_files: 8,
            },
        );
        let mut request = ExactResumeRequest::parse(&valid_payload()).unwrap();
        assert_eq!(
            exact_source(&report, &request).unwrap().canonical_file,
            transcript.canonicalize().unwrap()
        );
        request.account = "secondary".into();
        assert!(exact_source(&report, &request).is_none());
        request.account = "default".into();
        request.workspace = "another-worktree".into();
        assert!(exact_source(&report, &request).is_none());
        request.workspace = "workspace".into();
        request.native_id = Uuid::new_v4().to_string();
        assert!(exact_source(&report, &request).is_none());
    }

    #[test]
    fn exact_resume_route_is_local_only_and_conflicting() {
        use super::super::request_route_policy::{request_route_policy, RuntimeMutationPolicy};
        let route = request_route_policy("agentHistory.resumeExact");
        assert!(!route.mobile_allowed);
        assert_eq!(route.runtime_mutation, RuntimeMutationPolicy::Conflicts);
    }
}
