//! Explicit, local-only exact history resume. A JSONL file proves its own
//! provider UUID and registered CWD but never proves the active CLI login.
//! The Host selects the configured source account's native config directory
//! for every launch (including after restart); no credentials are mirrored.
//! No arbitrary transcript path, shell command or provider argv is accepted.
use alera_core::runtime::{RuntimeStore, WorkspaceStatus, WorkspaceTabRecord};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use super::terminal_startup_commands::provider_resume_id;
use super::ServerActor;
use crate::agent_history_sync::search_roots;
use crate::agent_session_history_scan::{
    discover_exact_native_history, HistoryScanPolicy, HistoryScanReport, HistorySourceProof,
    RegisteredHistoryWorkspace,
};
use crate::terminal_host::host_error::{HostError, HostResult};

#[derive(Clone, Debug)]
pub(super) struct VerifiedHistoryResume {
    pub provider: String,
    pub account_id: String,
    pub native_id: String,
    pub codex_transcript_path: Option<String>,
    /// Host-private canonical native file; never serialized to desktop clients.
    pub source_file: PathBuf,
    /// Derived from a configured, canonical provider root; never caller input.
    pub config_directory: String,
}

pub(super) const RESUME_ORIGIN_KEY: &str = "agentHistoryResumeOriginV1";

/// A source account label is not a unique identity: two labels may point to
/// one native provider config directory. Keep the canonical directory itself
/// Host-private and persist only its stable comparison fingerprint.
pub(super) fn source_directory_fingerprint(canonical_dir: &str) -> String {
    let identity = if cfg!(windows) {
        canonical_dir.to_lowercase()
    } else {
        canonical_dir.to_owned()
    };
    hex::encode(Sha256::digest(identity.as_bytes()))
}

/// Native Claude uses its normal user-home config path when no override is
/// selected. Pinning its default history directory as CLAUDE_CONFIG_DIR
/// invents a second first-run config home and blocks interactive Resume.
/// Custom CCS/account directories still require the explicit override.
fn verified_history_launch_variable(
    provider: &str,
    config_directory: &str,
    native_default_claude: Option<&Path>,
) -> HostResult<&'static str> {
    match provider {
        "codex" => Ok("ALERA_ACCOUNT_CODEX_HOME"),
        "claude"
            if native_default_claude.is_some_and(|default| {
                source_directory_fingerprint(&default.to_string_lossy())
                    == source_directory_fingerprint(config_directory)
            }) =>
        {
            Ok("ALERA_ACCOUNT_CLAUDE_NATIVE_DEFAULT")
        }
        "claude" => Ok("ALERA_ACCOUNT_CLAUDE_CONFIG_DIR"),
        _ => Err(HostError::state("Unsupported history provider environment")),
    }
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
    /// Read-only lookup needs only the native source; it must not be able to
    /// select an executable Profile or create a new mutation.
    fn parse_source(payload: &Value) -> HostResult<Self> {
        let provider = bounded_identifier(payload, "provider", 12)?;
        if !matches!(provider.as_str(), "codex" | "claude") {
            return Err(HostError::format("Unsupported historical resume provider"));
        }
        let account = bounded_identifier(payload, "accountId", 256)?;
        let native = bounded_identifier(payload, "providerSessionId", 64)?;
        let native_id = Uuid::parse_str(&native)
            .map_err(|_| HostError::format("Invalid provider-native session UUID"))?
            .to_string();
        if native != native_id {
            return Err(HostError::format(
                "Provider-native session UUID is not canonical",
            ));
        }
        Ok(Self {
            provider,
            account,
            project: bounded_identifier(payload, "projectId", 256)?,
            workspace: bounded_identifier(payload, "workspaceId", 256)?,
            profile: String::new(),
            native_id,
            client_mutation_id: String::new(),
        })
    }

    fn parse(payload: &Value) -> HostResult<Self> {
        if payload.get("confirmResume") != Some(&Value::Bool(true))
            || payload.get("confirmProviderAccount") != Some(&Value::Bool(true))
        {
            return Err(HostError::state(
                "Explicit resume and current CLI account confirmations are required.",
            ));
        }
        let mut source = Self::parse_source(payload)?;
        source.profile = bounded_identifier(payload, "profileId", 256)?;
        source.client_mutation_id = bounded_identifier(payload, "clientMutationId", 128)?;
        Uuid::parse_str(&source.client_mutation_id)
            .map_err(|_| HostError::format("Exact resume requires a fresh mutation UUID"))?;
        Ok(source)
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
    // An account alias may be configured more than once. Do not guess which
    // auth store should own this resume.
    if roots.len() != 1 {
        return Err(HostError::state(
            "History account must resolve to exactly one configured provider directory.",
        ));
    }
    let config_directory = roots[0]
        .path
        .parent()
        .and_then(|directory| std::fs::canonicalize(directory).ok())
        .filter(|directory| directory.is_dir())
        .ok_or_else(|| HostError::state("Provider account config directory is missing"))?
        .to_string_lossy()
        .into_owned();
    let known = vec![RegisteredHistoryWorkspace {
        runtime_instance_id: workspace.instance_id,
        project_id: workspace.project_id,
        workspace_id: workspace.id,
        path: workspace.path.into(),
    }];
    let exact_native_id = target.native_id.clone();
    let scan = tokio::task::spawn_blocking(move || {
        discover_exact_native_history(
            &roots,
            &known,
            HistoryScanPolicy {
                index_user_text: false,
                max_files: 256,
            },
            &exact_native_id,
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
        account_id: target.account.clone(),
        native_id: target.native_id.clone(),
        codex_transcript_path: (target.provider == "codex")
            .then(|| proof.canonical_file.to_string_lossy().into_owned()),
        source_file: proof.canonical_file.clone(),
        config_directory,
    })
}

/// Called for every fresh PTY, including application restart. Never trust a
/// cached account path or a stale tab title: rebuild the native source proof
/// from current Host configuration and the authoritative tab UUID.
pub(super) async fn verified_history_launch_environment(
    store: &RuntimeStore,
    tab: &WorkspaceTabRecord,
) -> HostResult<Option<(&'static str, String)>> {
    let Some(origin) = tab.payload.get(RESUME_ORIGIN_KEY) else {
        return Ok(None);
    };
    let provider = bounded_identifier(origin, "provider", 12)?;
    let account = bounded_identifier(origin, "accountId", 256)?;
    let Some((saved_provider, native_id)) = provider_resume_id(tab) else {
        return Err(HostError::state(
            "Saved historical Agent tab has no exact native session identity",
        ));
    };
    if saved_provider != provider || tab.payload["automationOwned"] == true {
        return Err(HostError::state(
            "Historical Agent provider identity no longer matches its tab",
        ));
    }
    let workspace = store
        .find_workspace(&tab.workspace_id)
        .await
        .map_err(|error| HostError::state(error.to_string()))?
        .ok_or_else(|| HostError::state("Saved historical Agent Worktree no longer exists"))?;
    let target = ExactResumeRequest {
        provider: provider.clone(),
        account,
        project: workspace.project_id,
        workspace: tab.workspace_id.clone(),
        profile: String::new(),
        native_id,
        client_mutation_id: String::new(),
    };
    let proof = verify_native_source(store, &target).await?;
    if let Some(pinned) = origin.get("sourceDirectoryFingerprint") {
        let fingerprint = bounded_identifier(origin, "sourceDirectoryFingerprint", 64)?;
        if !pinned.is_string()
            || fingerprint != source_directory_fingerprint(&proof.config_directory)
        {
            return Err(HostError::state(
                "Historical source account now resolves to a different provider directory. No Agent was started.",
            ));
        }
    }
    let native_default_claude =
        dirs::home_dir().and_then(|home| std::fs::canonicalize(home.join(".claude")).ok());
    let variable = verified_history_launch_variable(
        &proof.provider,
        &proof.config_directory,
        native_default_claude.as_deref(),
    )?;
    Ok(Some((variable, proof.config_directory)))
}

fn existing_tab_owns_source(
    tab: &WorkspaceTabRecord,
    target: &ExactResumeRequest,
    verified: &VerifiedHistoryResume,
    registered_fingerprints: &HashMap<(String, String), String>,
) -> bool {
    if !provider_resume_id(tab)
        .is_some_and(|(provider, native)| provider == target.provider && native == target.native_id)
    {
        return false;
    }
    let origin = &tab.payload[RESUME_ORIGIN_KEY];
    let Some(account) = origin.get("accountId").and_then(Value::as_str) else {
        // Old tabs have no authoritative account identity. Do not risk two
        // owner processes for what might be the very same transcript.
        return true;
    };
    if account == target.account {
        return true;
    }
    let pinned_source = origin
        .get("sourceDirectoryFingerprint")
        .and_then(Value::as_str);
    let registered_source = registered_fingerprints
        .get(&(target.provider.clone(), account.to_owned()))
        .map(String::as_str);
    let requested_source = source_directory_fingerprint(&verified.config_directory);
    // A moved alias may now point to the requested directory even if its
    // historical checkpoint was pinned elsewhere. Compare both identities.
    // Unknown paths are not evidence of independent accounts.
    (pinned_source.is_none() && registered_source.is_none())
        || pinned_source == Some(requested_source.as_str())
        || registered_source == Some(requested_source.as_str())
}

/// Unknown/legacy owners must block duplicate Resume, but are not proof that
/// the caller may reveal or navigate to that Tab. This read-only lookup
/// requires the Host-private source-directory fingerprint saved at launch.
fn tab_is_verified_exact_history_owner(
    tab: &WorkspaceTabRecord,
    target: &ExactResumeRequest,
    verified: &VerifiedHistoryResume,
) -> bool {
    if tab.kind != "terminal"
        || tab.workspace_id != target.workspace
        || tab.payload["automationOwned"] == true
        || !provider_resume_id(tab)
            .is_some_and(|(provider, id)| provider == target.provider && id == target.native_id)
    {
        return false;
    }
    let origin = &tab.payload[RESUME_ORIGIN_KEY];
    origin["provider"].as_str() == Some(target.provider.as_str())
        && origin["accountId"]
            .as_str()
            .is_some_and(|account| !account.is_empty() && account.len() <= 256)
        && origin["sourceDirectoryFingerprint"].as_str()
            == Some(source_directory_fingerprint(&verified.config_directory).as_str())
}

/// Checked only AFTER an authenticated matching mutation receipt has had a
/// chance to replay its original Tab. A fresh mutation still cannot launch a
/// second CLI owner for the exact native conversation.
pub(super) async fn ensure_history_session_unowned(
    store: &RuntimeStore,
    workspace_id: &str,
    verified: &VerifiedHistoryResume,
) -> HostResult<()> {
    let target = ExactResumeRequest {
        provider: verified.provider.clone(),
        account: verified.account_id.clone(),
        project: String::new(),
        workspace: workspace_id.to_owned(),
        profile: String::new(),
        native_id: verified.native_id.clone(),
        client_mutation_id: String::new(),
    };
    let settings = store
        .agent_quota_settings()
        .await
        .map_err(|error| HostError::state(error.to_string()))?;
    let registered_fingerprints: HashMap<(String, String), String> = search_roots(&settings)
        .await
        .into_iter()
        .filter_map(|root| {
            let directory = root.path.parent()?.canonicalize().ok()?;
            Some((
                (root.provider.to_owned(), root.account_id),
                source_directory_fingerprint(&directory.to_string_lossy()),
            ))
        })
        .collect();
    let existing = store
        .list_workspace_tabs(workspace_id)
        .await
        .map_err(|err| HostError::state(err.to_string()))?
        .into_iter()
        .any(|tab| existing_tab_owns_source(&tab, &target, verified, &registered_fingerprints));
    if existing {
        return Err(HostError::state(
            "This exact provider session already has an Alera Tab in its original Worktree. Open or restore that Tab instead of creating a duplicate.",
        ));
    }
    Ok(())
}

/// A local desktop user must deliberately request the transcript of an
/// existing resumed Host Tab. No file path, provider root or account is
/// accepted from the caller.
fn parse_messages_request(payload: &Value) -> HostResult<(String, String, Option<u64>, usize)> {
    if payload.get("consentToReadMessages") != Some(&Value::Bool(true)) {
        return Err(HostError::state(
            "Open Historical Messages explicitly before reading",
        ));
    }
    let workspace = bounded_identifier(payload, "workspaceId", 256)?;
    let tab_id = bounded_identifier(payload, "tabId", 256)?;
    let before = match payload.get("before") {
        None | Some(Value::Null) => None,
        Some(Value::Number(value)) => Some(
            value
                .as_u64()
                .filter(|value| *value > 0)
                .ok_or_else(|| HostError::format("Invalid history page cursor"))?,
        ),
        _ => return Err(HostError::format("Invalid history page cursor")),
    };
    let limit = match payload.get("maxMessages") {
        None | Some(Value::Null) => 30,
        Some(Value::Number(value)) => value
            .as_u64()
            .filter(|count| {
                (1..=super::agent_history_transcript::MAX_MESSAGES as u64).contains(count)
            })
            .ok_or_else(|| HostError::format("History page size must be 1 to 60"))?
            as usize,
        _ => return Err(HostError::format("Invalid history page size")),
    };
    Ok((workspace, tab_id, before, limit))
}

impl ServerActor {
    /// Read-only recovery after a client restart or a lost Resume ACK. It
    /// never starts a PTY and never guesses an owner's account from its UUID.
    pub(super) async fn locate_exact_agent_history(
        &mut self,
        payload: &Value,
    ) -> HostResult<Value> {
        let target = ExactResumeRequest::parse_source(payload)?;
        let verified = verify_native_source(&self.runtime_store, &target).await?;
        let tabs = self
            .runtime_store
            .list_workspace_tabs(&target.workspace)
            .await
            .map_err(|error| HostError::state(error.to_string()))?;
        let mut matches = tabs
            .iter()
            .filter(|tab| tab_is_verified_exact_history_owner(tab, &target, &verified));
        let existing = matches.next();
        if matches.next().is_some() {
            return Err(HostError::state(
                "Multiple saved Tabs claim the same native history; refusing to guess.",
            ));
        }
        Ok(match existing {
            Some(tab) => json!({
                "found": true,
                "tabId": tab.id,
                "workspaceId": tab.workspace_id,
                "noAgentStarted": true,
            }),
            None => json!({"found": false, "noAgentStarted": true}),
        })
    }

    /// Read only the conversational text of a Host-pinned, provider-native
    /// resumed session. This does not attach, resize, start or write to a PTY.
    /// The source proof is checked afresh on every explicitly requested page.
    pub(super) async fn read_exact_agent_history_messages(
        &mut self,
        payload: &Value,
    ) -> HostResult<Value> {
        let (workspace_id, tab_id, before, limit) = parse_messages_request(payload)?;
        let workspace = self
            .runtime_store
            .find_workspace(&workspace_id)
            .await
            .map_err(|error| HostError::state(error.to_string()))?
            .ok_or_else(|| HostError::state("Original historical Workspace no longer exists"))?;
        if workspace.status != WorkspaceStatus::Active || workspace.project_id.is_empty() {
            return Err(HostError::state(
                "Original historical Workspace is not active",
            ));
        }
        let tabs = self
            .runtime_store
            .list_workspace_tabs(&workspace_id)
            .await
            .map_err(|error| HostError::state(error.to_string()))?;
        let tab = tabs
            .iter()
            .find(|tab| {
                tab.id == tab_id
                    && tab.workspace_id == workspace_id
                    && tab.kind == "terminal"
                    && tab.payload["automationOwned"] != true
            })
            .ok_or_else(|| HostError::state("Original resumed Agent Tab not found"))?;
        let origin = tab.payload.get(RESUME_ORIGIN_KEY).ok_or_else(|| {
            HostError::state("Only original resumed Agent Tabs have native history")
        })?;
        let provider = bounded_identifier(origin, "provider", 12)?;
        let account = bounded_identifier(origin, "accountId", 256)?;
        let (saved_provider, native_id) = provider_resume_id(tab).ok_or_else(|| {
            HostError::state("Historical Tab has no provider-native Session UUID")
        })?;
        if provider != saved_provider || !matches!(provider.as_str(), "claude" | "codex") {
            return Err(HostError::state(
                "Historical Tab provider identity mismatch",
            ));
        }
        let target = ExactResumeRequest {
            provider: provider.clone(),
            account,
            project: workspace.project_id,
            workspace: workspace_id,
            profile: String::new(),
            native_id: native_id.clone(),
            client_mutation_id: String::new(),
        };
        let verified = verify_native_source(&self.runtime_store, &target).await?;
        if !tab_is_verified_exact_history_owner(tab, &target, &verified) {
            return Err(HostError::state(
                "Historical Tab no longer matches its pinned account source",
            ));
        }
        let path = verified.source_file;
        let page = tokio::task::spawn_blocking(move || {
            super::agent_history_transcript::read_page(&path, &provider, &native_id, before, limit)
        })
        .await
        .map_err(|_| HostError::state("Historical transcript reader failed"))?
        .map_err(|_| HostError::state("Historical transcript could not be read"))?;
        serde_json::to_value(page)
            .map_err(|_| HostError::state("Historical transcript response could not be encoded"))
    }

    pub(super) async fn resume_exact_agent_history(
        &mut self,
        client_id: u64,
        payload: &Value,
    ) -> HostResult<Value> {
        // Route policy already rejects mobile and unauthenticated callers.
        let target = ExactResumeRequest::parse(payload)?;
        let verified = verify_native_source(&self.runtime_store, &target).await?;
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
    use crate::agent_session_history_scan::discover_history;
    use alera_core::runtime::{
        Project, ProjectKind, RuntimeAgentQuotaSettings, RuntimeClaudeQuotaProfile,
        RuntimeCodexQuotaProfile, Workspace, WorkspaceKind, WorkspaceStatus, WorkspaceTabRecord,
    };

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
    fn history_message_pages_require_explicit_consent_and_original_tab() {
        let base = json!({"workspaceId":"worktree","tabId":"original"});
        assert!(parse_messages_request(&base).is_err());
        let mut authorized = base;
        authorized["consentToReadMessages"] = json!(true);
        assert_eq!(
            parse_messages_request(&authorized).unwrap(),
            ("worktree".into(), "original".into(), None, 30)
        );
        authorized["maxMessages"] = json!(60);
        authorized["before"] = json!(1000);
        assert_eq!(parse_messages_request(&authorized).unwrap().2, Some(1000));
        for invalid in [json!(0), json!(-1), json!("3"), json!(true)] {
            authorized["before"] = invalid;
            assert!(parse_messages_request(&authorized).is_err());
        }
        authorized["before"] = Value::Null;
        authorized["maxMessages"] = json!(61);
        assert!(parse_messages_request(&authorized).is_err());
        authorized["maxMessages"] = json!(30);
        authorized["tabId"] = json!("../other-provider");
        // Path-like IDs have no special meaning, but can never become file paths:
        // the Host resolves only a registered Tab ID and its pinned source.
        let (workspace, tab, _, _) = parse_messages_request(&authorized).unwrap();
        assert_eq!(workspace, "worktree");
        assert_eq!(tab, "../other-provider");
        let policy =
            super::super::request_route_policy::request_route_policy("agentHistory.messages");
        assert!(!policy.mobile_allowed);
        assert_eq!(
            policy.runtime_mutation,
            super::super::request_route_policy::RuntimeMutationPolicy::Available
        );
    }

    #[test]
    fn native_claude_default_history_must_not_route_into_nested_fresh_config() {
        let root = tempfile::tempdir().unwrap();
        let native = root.path().join(".claude");
        let secondary = root.path().join("ccs-instance");
        std::fs::create_dir_all(&native).unwrap();
        std::fs::create_dir_all(&secondary).unwrap();
        let native = native.canonicalize().unwrap();
        let secondary = secondary.canonicalize().unwrap();
        assert_eq!(
            verified_history_launch_variable(
                "claude",
                &native.to_string_lossy(),
                Some(native.as_path()),
            )
            .unwrap(),
            "ALERA_ACCOUNT_CLAUDE_NATIVE_DEFAULT"
        );
        assert_eq!(
            verified_history_launch_variable(
                "claude",
                &secondary.to_string_lossy(),
                Some(native.as_path()),
            )
            .unwrap(),
            "ALERA_ACCOUNT_CLAUDE_CONFIG_DIR"
        );
        assert_eq!(
            verified_history_launch_variable("codex", &native.to_string_lossy(), None).unwrap(),
            "ALERA_ACCOUNT_CODEX_HOME"
        );
    }

    #[test]
    fn exact_resume_requires_full_identity_and_two_confirmations() {
        let mut request = valid_payload();
        assert!(ExactResumeRequest::parse(&request).is_ok());
        request["confirmProviderAccount"] = json!(false);
        assert!(ExactResumeRequest::parse(&request).is_err());
        request["confirmProviderAccount"] = json!(true);
        request["accountId"] = json!("secondary");
        assert_eq!(
            ExactResumeRequest::parse(&request).unwrap().account,
            "secondary"
        );
        request["accountId"] = json!("");
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

    #[tokio::test]
    async fn registered_secondary_codex_home_is_used_for_resume_and_revalidated_after_restart() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let account = temp.path().join("secondary-codex");
        let session_dir = account.join("sessions/2026/11/21");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&session_dir).unwrap();
        let native = "550e8400-e29b-41d4-a716-446655440001";
        let file = session_dir.join(format!("rollout-2026-11-21T10-00-00-{native}.jsonl"));
        std::fs::write(
            &file,
            format!(
                "{}\n",
                json!({
                    "type":"session_meta",
                    "payload":{"id":native,"cwd":repo}
                })
            ),
        )
        .unwrap();
        let store = RuntimeStore::open(temp.path()).await.unwrap();
        let now = chrono::Utc::now();
        store
            .upsert_project(Project {
                id: "project".into(),
                name: "Project".into(),
                repo_path: repo.to_string_lossy().into_owned(),
                created_at: now,
                updated_at: now,
                kind: ProjectKind::GitRepository,
            })
            .await
            .unwrap();
        store
            .upsert_workspace(Workspace {
                id: "workspace".into(),
                instance_id: "instance".into(),
                host_id: "local".into(),
                project_id: "project".into(),
                name: "Main".into(),
                branch: None,
                path: repo.to_string_lossy().into_owned(),
                created_at: now,
                updated_at: now,
                kind: WorkspaceKind::Main,
                status: WorkspaceStatus::Active,
                source_branch: None,
                reuses_existing_branch: false,
                is_pinned: false,
                tag_ids: vec![],
                tag_names: vec![],
                section_id: None,
                parent_workspace_id: None,
                child_count: 0,
                archived_at: None,
            })
            .await
            .unwrap();
        let account_id = account.to_string_lossy().into_owned();
        store
            .set_agent_quota_settings(RuntimeAgentQuotaSettings {
                codex_profiles: vec![RuntimeCodexQuotaProfile {
                    alias: "Secondary".into(),
                    profile: account_id.clone(),
                }],
                ..RuntimeAgentQuotaSettings::default()
            })
            .await
            .unwrap();
        let mut request = ExactResumeRequest::parse(&valid_payload()).unwrap();
        request.account = account_id.clone();
        let proof = verify_native_source(&store, &request).await.unwrap();
        assert_eq!(proof.account_id, account_id);
        assert_eq!(
            proof.config_directory,
            account.canonicalize().unwrap().to_string_lossy()
        );
        let mut tab = WorkspaceTabRecord {
            id: "tab".into(),
            workspace_id: "workspace".into(),
            kind: "terminal".into(),
            title: "Resumed".into(),
            created_at: now,
            updated_at: now,
            payload: json!({"agentType":"codex", "spawnOnCreate":true}),
        };
        super::super::agent_title_state::initialize(&mut tab, "");
        let mut title_state = super::super::agent_title_state::AgentTitleState::read(&tab).unwrap();
        title_state.observe("codex", Some(native), "", 0);
        title_state.write(&mut tab);
        tab.payload[RESUME_ORIGIN_KEY] = json!({
            "provider":"codex",
            "accountId":account_id,
            "sourceDirectoryFingerprint":source_directory_fingerprint(&proof.config_directory),
        });
        let env = verified_history_launch_environment(&store, &tab)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(env.0, "ALERA_ACCOUNT_CODEX_HOME");
        assert_eq!(env.1, proof.config_directory);
        // Changing an account registration cannot silently repoint an
        // existing resumed Tab to another provider session directory.
        tab.payload[RESUME_ORIGIN_KEY]["sourceDirectoryFingerprint"] =
            json!(source_directory_fingerprint("another-config-dir"));
        assert!(verified_history_launch_environment(&store, &tab)
            .await
            .is_err());
        tab.payload[RESUME_ORIGIN_KEY]["sourceDirectoryFingerprint"] =
            json!(source_directory_fingerprint(&proof.config_directory));
        std::fs::remove_file(file).unwrap();
        assert!(verified_history_launch_environment(&store, &tab)
            .await
            .is_err());

        // Claude uses a different native history layout and config variable,
        // but must bind to the same verified registered Worktree.
        let claude_account = temp.path().join("secondary-claude");
        let claude_projects = claude_account.join("projects/-repo");
        std::fs::create_dir_all(&claude_projects).unwrap();
        let claude_native = "550e8400-e29b-41d4-a716-446655440003";
        let claude_file = claude_projects.join(format!("{claude_native}.jsonl"));
        std::fs::write(
            &claude_file,
            format!(
                "{}\n",
                json!({
                    "type":"user",
                    "sessionId":claude_native,
                    "cwd":repo,
                    "message":{"role":"user","content":"Restore source session"}
                })
            ),
        )
        .unwrap();
        let claude_id = claude_account.to_string_lossy().into_owned();
        store
            .set_agent_quota_settings(RuntimeAgentQuotaSettings {
                claude_profiles: vec![RuntimeClaudeQuotaProfile {
                    alias: "Secondary Claude".into(),
                    profile: claude_id.clone(),
                    show_in_usage: true,
                    usage_display_name: None,
                }],
                ..RuntimeAgentQuotaSettings::default()
            })
            .await
            .unwrap();
        request.provider = "claude".into();
        request.account = claude_id.clone();
        request.native_id = claude_native.into();
        let claude_proof = verify_native_source(&store, &request).await.unwrap();
        assert_eq!(
            claude_proof.config_directory,
            claude_account.canonicalize().unwrap().to_string_lossy()
        );
        let mut claude_tab = tab.clone();
        claude_tab.payload["agentType"] = json!("claude");
        let mut title =
            super::super::agent_title_state::AgentTitleState::read(&claude_tab).unwrap();
        title.observe("claude", Some(claude_native), "", 0);
        title.write(&mut claude_tab);
        claude_tab.payload[RESUME_ORIGIN_KEY] = json!({
            "provider":"claude",
            "accountId":claude_id,
            "sourceDirectoryFingerprint":source_directory_fingerprint(&claude_proof.config_directory),
        });
        let env = verified_history_launch_environment(&store, &claude_tab)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(env.0, "ALERA_ACCOUNT_CLAUDE_CONFIG_DIR");
        assert_eq!(env.1, claude_proof.config_directory);
        std::fs::remove_file(claude_file).unwrap();
        assert!(verified_history_launch_environment(&store, &claude_tab)
            .await
            .is_err());

        assert!(verified_history_launch_environment(
            &store,
            &WorkspaceTabRecord {
                payload: json!({"agentType":"codex"}),
                ..tab
            }
        )
        .await
        .unwrap()
        .is_none());
    }

    #[test]
    fn equivalent_account_aliases_cannot_duplicate_the_same_native_session() {
        let now = chrono::Utc::now();
        let native = "550e8400-e29b-41d4-a716-446655440001";
        let mut tab = WorkspaceTabRecord {
            id: "existing".into(),
            workspace_id: "workspace".into(),
            kind: "terminal".into(),
            title: "Prior".into(),
            created_at: now,
            updated_at: now,
            payload: json!({"agentType":"codex","spawnOnCreate":true}),
        };
        super::super::agent_title_state::initialize(&mut tab, "");
        let mut state = super::super::agent_title_state::AgentTitleState::read(&tab).unwrap();
        state.observe("codex", Some(native), "", 0);
        state.write(&mut tab);
        let mut request = ExactResumeRequest::parse(&valid_payload()).unwrap();
        request.account = "alias-two".into();
        let verified = VerifiedHistoryResume {
            provider: "codex".into(),
            account_id: request.account.clone(),
            native_id: native.into(),
            codex_transcript_path: None,
            source_file: PathBuf::from("fixture-only"),
            config_directory: "C:/account-home".into(),
        };
        let matching = source_directory_fingerprint(&verified.config_directory);
        let other = source_directory_fingerprint("C:/another-home");
        let mut configured = HashMap::new();
        tab.payload[RESUME_ORIGIN_KEY] = json!({
            "provider":"codex",
            "accountId":"alias-one",
            "sourceDirectoryFingerprint":matching,
        });
        assert!(existing_tab_owns_source(
            &tab,
            &request,
            &verified,
            &configured
        ));
        tab.payload[RESUME_ORIGIN_KEY]["sourceDirectoryFingerprint"] = json!(other);
        assert!(!existing_tab_owns_source(
            &tab,
            &request,
            &verified,
            &configured
        ));
        configured.insert(("codex".into(), "alias-one".into()), matching.clone());
        assert!(existing_tab_owns_source(
            &tab,
            &request,
            &verified,
            &configured
        ));
        tab.payload[RESUME_ORIGIN_KEY]
            .as_object_mut()
            .unwrap()
            .remove("sourceDirectoryFingerprint");
        assert!(existing_tab_owns_source(
            &tab,
            &request,
            &verified,
            &configured
        ));
        configured.insert(("codex".into(), "alias-one".into()), other);
        assert!(!existing_tab_owns_source(
            &tab,
            &request,
            &verified,
            &configured
        ));
        configured.clear();
        assert!(existing_tab_owns_source(
            &tab,
            &request,
            &verified,
            &configured
        ));
        tab.payload
            .as_object_mut()
            .unwrap()
            .remove(RESUME_ORIGIN_KEY);
        assert!(existing_tab_owns_source(
            &tab,
            &request,
            &verified,
            &configured
        ));
        let mut state = super::super::agent_title_state::AgentTitleState::read(&tab).unwrap();
        state.native_id = Some("550e8400-e29b-41d4-a716-446655440004".into());
        state.write(&mut tab);
        assert!(!existing_tab_owns_source(
            &tab,
            &request,
            &verified,
            &configured
        ));
    }

    #[test]
    fn locate_existing_tab_requires_host_pinned_source_not_just_native_uuid() {
        let target = ExactResumeRequest::parse_source(&json!({
            "provider":"codex",
            "accountId":"alias-two",
            "projectId":"project",
            "workspaceId":"workspace",
            "providerSessionId":"550e8400-e29b-41d4-a716-446655440001",
        }))
        .unwrap();
        let verified = VerifiedHistoryResume {
            provider: target.provider.clone(),
            account_id: target.account.clone(),
            native_id: target.native_id.clone(),
            codex_transcript_path: None,
            source_file: PathBuf::from("fixture-only"),
            config_directory: "C:/same-source".into(),
        };
        let now = chrono::Utc::now();
        let mut tab = WorkspaceTabRecord {
            id: "tab".into(),
            workspace_id: "workspace".into(),
            kind: "terminal".into(),
            title: "Existing".into(),
            created_at: now,
            updated_at: now,
            payload: json!({"agentType":"codex","spawnOnCreate":true}),
        };
        super::super::agent_title_state::initialize(&mut tab, "");
        let mut state = super::super::agent_title_state::AgentTitleState::read(&tab).unwrap();
        state.observe("codex", Some(&target.native_id), "", 0);
        state.write(&mut tab);
        assert!(!tab_is_verified_exact_history_owner(
            &tab, &target, &verified
        ));
        // A different alias can locate only if it pins the *same* source root.
        tab.payload[RESUME_ORIGIN_KEY] = json!({
            "provider":"codex",
            "accountId":"alias-one",
            "sourceDirectoryFingerprint":source_directory_fingerprint(&verified.config_directory)
        });
        assert!(tab_is_verified_exact_history_owner(
            &tab, &target, &verified
        ));
        tab.payload[RESUME_ORIGIN_KEY]["accountId"] = Value::Null;
        assert!(!tab_is_verified_exact_history_owner(
            &tab, &target, &verified
        ));
        tab.payload[RESUME_ORIGIN_KEY]["accountId"] = json!("alias-one");
        tab.payload[RESUME_ORIGIN_KEY]["sourceDirectoryFingerprint"] =
            json!(source_directory_fingerprint("C:/different-account"));
        assert!(!tab_is_verified_exact_history_owner(
            &tab, &target, &verified
        ));
        tab.payload[RESUME_ORIGIN_KEY]["sourceDirectoryFingerprint"] =
            json!(source_directory_fingerprint(&verified.config_directory));
        tab.payload["automationOwned"] = json!(true);
        assert!(!tab_is_verified_exact_history_owner(
            &tab, &target, &verified
        ));
        tab.payload["automationOwned"] = json!(false);
        tab.workspace_id = "other-worktree".into();
        assert!(!tab_is_verified_exact_history_owner(
            &tab, &target, &verified
        ));
        tab.workspace_id = "workspace".into();
        let mut state = super::super::agent_title_state::AgentTitleState::read(&tab).unwrap();
        state.native_id = Some(Uuid::new_v4().to_string());
        state.write(&mut tab);
        assert!(!tab_is_verified_exact_history_owner(
            &tab, &target, &verified
        ));
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
        let locate = request_route_policy("agentHistory.locateExact");
        assert!(!locate.mobile_allowed);
        assert_eq!(locate.runtime_mutation, RuntimeMutationPolicy::Available);
        assert!(locate.deferred_read.is_none());
    }
}
