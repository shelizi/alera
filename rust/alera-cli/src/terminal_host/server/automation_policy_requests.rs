use alera_core::runtime::{
    AutomationActor, AutomationActorKind, AutomationAgentPolicy, AutomationDefinition,
    AutomationProjectPolicy, AutomationRun, AutomationTarget, Project, ProjectKind, RuntimeStore,
    Workspace,
};
use chrono::Utc;
use serde_json::{json, Map, Value};
use std::path::Path;

use crate::terminal_host::host_error::{HostError, HostResult};

use super::terminal_startup_commands::agent_profile_id;
use super::workspace_tab_requests::WorkspaceTabStoreHandler;
use super::ServerActor;

struct AutomationPolicyContextStoreHandler<'a> {
    runtime_store: &'a RuntimeStore,
}

impl<'a> AutomationPolicyContextStoreHandler<'a> {
    const fn new(runtime_store: &'a RuntimeStore) -> Self {
        Self { runtime_store }
    }

    async fn find_run(&self, run_id: &str) -> HostResult<Option<AutomationRun>> {
        self.runtime_store
            .find_automation_run(run_id)
            .await
            .map_err(state_error)
    }

    async fn find_workspace(&self, workspace_id: &str) -> HostResult<Option<Workspace>> {
        self.runtime_store
            .find_workspace(workspace_id)
            .await
            .map_err(state_error)
    }

    async fn find_project(&self, project_id: &str) -> HostResult<Option<Project>> {
        self.runtime_store
            .find_project(project_id)
            .await
            .map_err(state_error)
    }
}

struct AutomationAgentPolicyStoreHandler<'a> {
    runtime_store: &'a RuntimeStore,
}

impl<'a> AutomationAgentPolicyStoreHandler<'a> {
    const fn new(runtime_store: &'a RuntimeStore) -> Self {
        Self { runtime_store }
    }

    async fn get(&self, profile_id: &str) -> HostResult<AutomationAgentPolicy> {
        self.runtime_store
            .automation_agent_policy(profile_id)
            .await
            .map_err(state_error)
    }

    async fn set(&self, policy: AutomationAgentPolicy) -> HostResult<AutomationAgentPolicy> {
        self.runtime_store
            .set_automation_agent_policy(policy)
            .await
            .map_err(state_error)
    }
}

struct AutomationProjectPolicyStoreHandler<'a> {
    runtime_store: &'a RuntimeStore,
}

impl<'a> AutomationProjectPolicyStoreHandler<'a> {
    const fn new(runtime_store: &'a RuntimeStore) -> Self {
        Self { runtime_store }
    }

    async fn get(&self, project_id: &str) -> HostResult<AutomationProjectPolicy> {
        self.runtime_store
            .automation_project_policy(project_id)
            .await
            .map_err(state_error)
    }

    async fn set(&self, policy: AutomationProjectPolicy) -> HostResult<AutomationProjectPolicy> {
        self.runtime_store
            .set_automation_project_policy(policy)
            .await
            .map_err(state_error)
    }
}

impl ServerActor {
    pub(super) async fn automation_policy_request(
        &self,
        client_id: u64,
        payload: &Value,
        actor: &AutomationActor,
    ) -> HostResult<Value> {
        let actor = self
            .resolve_policy_actor(client_id, payload, actor.clone())
            .await?;
        let kind = payload
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("show");
        let policy = payload.get("policy");
        match kind {
            "agent" => {
                require_policy_admin(&actor)?;
                let profile_id = payload
                    .get("profileId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| HostError::format("agent policy requires profileId"))?;
                if let Some(value) = policy {
                    let policy = decode_agent_policy(value, profile_id)?;
                    let saved = AutomationAgentPolicyStoreHandler::new(&self.runtime_store)
                        .set(policy)
                        .await?;
                    return serde_json::to_value(saved)
                        .map_err(|error| HostError::state(error.to_string()));
                }
                let policy = AutomationAgentPolicyStoreHandler::new(&self.runtime_store)
                    .get(profile_id)
                    .await?;
                serde_json::to_value(policy).map_err(|error| HostError::state(error.to_string()))
            }
            "project" => {
                require_policy_admin(&actor)?;
                let project_id = payload
                    .get("projectId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| HostError::format("project policy requires projectId"))?;
                if let Some(value) = policy {
                    let mut policy = decode_project_policy(value, project_id)?;
                    policy.repo_declared = self.repository_declared_for_project(project_id).await?;
                    let saved = AutomationProjectPolicyStoreHandler::new(&self.runtime_store)
                        .set(policy)
                        .await?;
                    return serde_json::to_value(saved)
                        .map_err(|error| HostError::state(error.to_string()));
                }
                let policy = self.effective_project_policy(project_id).await?;
                serde_json::to_value(policy).map_err(|error| HostError::state(error.to_string()))
            }
            "show" => {
                load_automation_policy_show(self.runtime_store.clone(), payload.clone()).await
            }
            _ => Err(HostError::format(
                "automation policy kind must be show, agent, or project",
            )),
        }
    }

    pub(super) async fn resolve_policy_actor(
        &self,
        client_id: u64,
        payload: &Value,
        actor: AutomationActor,
    ) -> HostResult<AutomationActor> {
        let Some(run_id) = payload.get("run").and_then(Value::as_str) else {
            return Ok(actor);
        };
        let identity = super::automation_run_target_requests::requested_target_identity(payload)?;
        self.verify_live_target_identity(client_id, &identity)
            .await?;
        let run = AutomationPolicyContextStoreHandler::new(&self.runtime_store)
            .find_run(run_id)
            .await?
            .ok_or_else(|| HostError::state(format!("automation run not found: {run_id}")))?;
        if run
            .target_identity
            .as_ref()
            .is_none_or(|bound| !bound.matches(&identity))
        {
            return Err(HostError::state(
                "automation run target identity does not match the live run",
            ));
        }
        if actor.kind == AutomationActorKind::LocalCli
            && run.actor_kind == Some(AutomationActorKind::ManagedAgent)
        {
            return Ok(AutomationActor {
                kind: AutomationActorKind::ManagedAgent,
                id: run.actor_id,
                label: Some("managed automation agent".to_string()),
            });
        }
        Ok(actor)
    }

    pub(super) async fn ensure_agent_policy(
        &self,
        definition: &AutomationDefinition,
        actor: &AutomationActor,
        execute: bool,
    ) -> HostResult<()> {
        if execute {
            let Some(profile_id) = self
                .target_profile_id(definition)
                .await?
                .as_deref()
                .map(str::to_string)
            else {
                return Err(HostError::state(
                    "automation target must resolve to an agent profile",
                ));
            };
            let policy = AutomationAgentPolicyStoreHandler::new(&self.runtime_store)
                .get(&profile_id)
                .await?;
            if !policy.may_execute {
                return Err(HostError::state(format!(
                    "agent profile {profile_id} is not opted in to automation execution"
                )));
            }
        } else if actor.kind == AutomationActorKind::ManagedAgent {
            let Some(profile_id) = actor.id.as_deref() else {
                return Err(HostError::state(
                    "managed agent identity has no editing profile",
                ));
            };
            let policy = AutomationAgentPolicyStoreHandler::new(&self.runtime_store)
                .get(profile_id)
                .await?;
            let allowed = policy.may_activate_or_edit_active;
            if !allowed {
                return Err(HostError::state(format!(
                    "agent policy for profile {profile_id} does not allow managed agents to activate or edit active automations"
                )));
            }
        }

        let source_workspace_id = match &definition.target {
            AutomationTarget::ExistingTab { workspace_id, .. }
            | AutomationTarget::FreshTab { workspace_id, .. } => workspace_id,
            AutomationTarget::ManagedWorkspace {
                source_workspace_id,
                ..
            } => source_workspace_id,
        };
        let context = AutomationPolicyContextStoreHandler::new(&self.runtime_store);
        let Some(workspace) = context.find_workspace(source_workspace_id).await? else {
            return Err(HostError::state("automation target workspace is missing"));
        };
        let Some(project) = context.find_project(&workspace.project_id).await? else {
            return Err(HostError::state("automation target project is missing"));
        };
        if matches!(definition.target, AutomationTarget::ManagedWorkspace { .. })
            && project.kind == ProjectKind::Folder
        {
            return Err(HostError::state(
                "managed workspace automations require a git repository project",
            ));
        }
        let project_policy = AutomationProjectPolicyStoreHandler::new(&self.runtime_store)
            .get(&workspace.project_id)
            .await?;
        if !repository_declares_automation(&workspace.path, &project.repo_path).await {
            return Err(HostError::state(format!(
                "repository {} has no automation declaration in alera.toml",
                workspace.project_id
            )));
        }
        if project_policy.restrictive && !project_policy.local_approved {
            return Err(HostError::state(format!(
                "project policy for {} requires local approval",
                workspace.project_id
            )));
        }
        Ok(())
    }

    pub(super) async fn target_profile_id(
        &self,
        definition: &AutomationDefinition,
    ) -> HostResult<Option<String>> {
        match &definition.target {
            AutomationTarget::ExistingTab { tab_id, .. } => {
                let tab = WorkspaceTabStoreHandler::new(&self.runtime_store)
                    .find(tab_id)
                    .await?
                    .ok_or_else(|| HostError::state("automation existing tab is missing"))?;
                Ok(agent_profile_id(&tab).map(str::to_string))
            }
            AutomationTarget::FreshTab {
                agent_profile_id, ..
            }
            | AutomationTarget::ManagedWorkspace {
                agent_profile_id, ..
            } => Ok(Some(agent_profile_id.clone())),
        }
    }

    pub(super) async fn effective_project_policy(
        &self,
        project_id: &str,
    ) -> HostResult<AutomationProjectPolicy> {
        load_effective_project_policy(&self.runtime_store, project_id).await
    }

    async fn repository_declared_for_project(&self, project_id: &str) -> HostResult<bool> {
        repository_declared_for_project(&self.runtime_store, project_id).await
    }
}

pub(super) async fn load_automation_policy_show(
    runtime_store: RuntimeStore,
    payload: Value,
) -> HostResult<Value> {
    let profile_id = payload
        .get("profileId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let project_id = payload
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::to_string);
    if profile_id.is_none() && project_id.is_none() {
        return Err(HostError::format(
            "policy show requires profileId or projectId",
        ));
    }

    let agent_policy = if let Some(profile_id) = profile_id.as_deref() {
        Some(
            AutomationAgentPolicyStoreHandler::new(&runtime_store)
                .get(profile_id)
                .await?,
        )
    } else {
        None
    };
    let project_policy = if let Some(project_id) = project_id.as_deref() {
        Some(load_effective_project_policy(&runtime_store, project_id).await?)
    } else {
        None
    };

    let mut result = Map::new();
    if let Some(policy) = agent_policy.as_ref() {
        result.insert(
            "agent".to_string(),
            serde_json::to_value(policy).map_err(|error| HostError::state(error.to_string()))?,
        );
    }
    if let Some(policy) = project_policy.as_ref() {
        result.insert(
            "project".to_string(),
            serde_json::to_value(policy).map_err(|error| HostError::state(error.to_string()))?,
        );
    }
    if let Some(policy) = agent_policy {
        result.insert(
            "effective".to_string(),
            json!({"targetProfile": policy, "project": project_policy}),
        );
    }
    Ok(Value::Object(result))
}

async fn load_effective_project_policy(
    runtime_store: &RuntimeStore,
    project_id: &str,
) -> HostResult<AutomationProjectPolicy> {
    let mut policy = AutomationProjectPolicyStoreHandler::new(runtime_store)
        .get(project_id)
        .await?;
    policy.repo_declared = repository_declared_for_project(runtime_store, project_id).await?;
    Ok(policy)
}

async fn repository_declared_for_project(
    runtime_store: &RuntimeStore,
    project_id: &str,
) -> HostResult<bool> {
    let Some(project) = AutomationPolicyContextStoreHandler::new(runtime_store)
        .find_project(project_id)
        .await?
    else {
        return Ok(false);
    };
    Ok(repository_declares_automation("", &project.repo_path).await)
}

fn require_policy_admin(actor: &AutomationActor) -> HostResult<()> {
    if actor.kind == AutomationActorKind::ManagedAgent {
        return Err(HostError::state(
            "managed agents cannot administer automation policies",
        ));
    }
    Ok(())
}

pub(super) fn require_human_automation_actor(actor: &AutomationActor) -> HostResult<()> {
    if actor.kind == AutomationActorKind::ManagedAgent {
        return Err(HostError::state(
            "managed agents cannot approve automation revisions",
        ));
    }
    Ok(())
}

async fn repository_declares_automation(workspace_path: &str, project_repo_path: &str) -> bool {
    let candidates = [
        Path::new(workspace_path).join("alera.toml"),
        Path::new(project_repo_path).join("alera.toml"),
    ];
    for path in candidates {
        let Ok(contents) = tokio::fs::read_to_string(path).await else {
            continue;
        };
        if contents_declare_automation(&contents) {
            return true;
        }
    }
    false
}

fn contents_declare_automation(contents: &str) -> bool {
    // toml 1.x FromStr for Value parses a single value, not a document.
    // A file like `automation_declared = true` must go through from_str.
    let Ok(value) = toml::from_str::<toml::Value>(contents) else {
        return false;
    };
    let Some(root) = value.as_table() else {
        return false;
    };
    root.get("automation_declared")
        .and_then(toml::Value::as_bool)
        .unwrap_or(false)
        || root
            .get("automation")
            .and_then(toml::Value::as_table)
            .is_some_and(|table| {
                table
                    .get("declared")
                    .or_else(|| table.get("enabled"))
                    .and_then(toml::Value::as_bool)
                    .unwrap_or(false)
            })
}

fn decode_agent_policy(value: &Value, profile_id: &str) -> HostResult<AutomationAgentPolicy> {
    let object = policy_object(value, "agent")?;
    let mut object = object;
    object.insert(
        "profileId".to_string(),
        Value::String(profile_id.to_string()),
    );
    object
        .entry("updatedAt".to_string())
        .or_insert_with(|| json!(Utc::now()));
    serde_json::from_value(Value::Object(object))
        .map_err(|error| HostError::format(format!("invalid agent policy: {error}")))
}

fn decode_project_policy(value: &Value, project_id: &str) -> HostResult<AutomationProjectPolicy> {
    let object = policy_object(value, "project")?;
    let mut object = object;
    object.insert(
        "projectId".to_string(),
        Value::String(project_id.to_string()),
    );
    object
        .entry("updatedAt".to_string())
        .or_insert_with(|| json!(Utc::now()));
    serde_json::from_value(Value::Object(object))
        .map_err(|error| HostError::format(format!("invalid project policy: {error}")))
}

fn policy_object(value: &Value, kind: &str) -> HostResult<Map<String, Value>> {
    value
        .as_object()
        .cloned()
        .ok_or_else(|| HostError::format(format!("{kind} policy must be a JSON object")))
}

fn state_error(error: impl std::fmt::Display) -> HostError {
    HostError::state(error.to_string())
}

#[cfg(test)]
mod tests {
    use alera_core::runtime::{
        AgentProfile, AgentProfileLaunchMode, AutomationActor, AutomationActorKind,
        AutomationAgentPolicy, AutomationDefinition, AutomationProjectPolicy, AutomationRun,
        AutomationTarget, AutomationTargetIdentity, Project, ProjectKind, RuntimeStore, Workspace,
        WorkspaceTabRecord,
    };
    use chrono::Utc;
    use serde_json::json;

    use super::{
        repository_declares_automation, AutomationAgentPolicyStoreHandler,
        AutomationPolicyContextStoreHandler, AutomationProjectPolicyStoreHandler,
    };
    use crate::terminal_host::client::ClientHandle;
    use crate::terminal_host::server::actor_test_harness::{local_client, test_actor};
    use crate::terminal_host::session::Session;
    use std::collections::HashMap;
    use std::fs;
    use std::path::Path;

    fn declare(dir: &Path, contents: &str) {
        fs::write(dir.join("alera.toml"), contents).unwrap();
    }

    fn automation_definition(target: AutomationTarget) -> AutomationDefinition {
        let now = Utc::now();
        let actor = json!({"kind": "humanDesktop"});
        let mut definition: AutomationDefinition = serde_json::from_value(json!({
            "id": "automation",
            "slug": "automation",
            "name": "Automation",
            "promptTemplate": "Run",
            "schedule": {"recurring": {"cron": "0 0 * * *", "timezone": "UTC"}},
            "target": {"freshTab": {"workspace_id": "placeholder", "agent_profile_id": "profile"}},
            "state": "draft",
            "revision": 1,
            "createdBy": actor,
            "modifiedBy": actor,
            "createdAt": now,
            "updatedAt": now,
        }))
        .unwrap();
        definition.target = target;
        definition
    }

    fn automation_run(id: &str, target_identity: AutomationTargetIdentity) -> AutomationRun {
        let now = Utc::now();
        serde_json::from_value(json!({
            "id": id,
            "automationId": "automation",
            "number": 1,
            "occurrenceKey": format!("manual|{id}"),
            "scheduledAt": now,
            "trigger": "manual",
            "actorKind": "managedAgent",
            "actorId": "profile",
            "targetIdentity": target_identity,
            "status": "pending",
            "attemptCount": 0,
            "createdAt": now,
            "updatedAt": now,
        }))
        .unwrap()
    }

    fn workspace(id: &str, project_id: &str, path: &Path) -> Workspace {
        let now = Utc::now();
        serde_json::from_value(json!({
            "id": id,
            "instanceId": format!("instance-{id}"),
            "hostId": "local",
            "projectId": project_id,
            "name": id,
            "path": path.to_string_lossy(),
            "createdAt": now,
            "updatedAt": now,
            "kind": "main",
            "status": "active",
            "reusesExistingBranch": false,
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn root_automation_declared_flag_is_recognized() {
        let dir = tempfile::tempdir().unwrap();
        declare(dir.path(), "automation_declared = true\n");
        assert!(repository_declares_automation(dir.path().to_str().unwrap(), "/missing").await);
    }

    #[tokio::test]
    async fn nested_automation_declared_flag_is_recognized() {
        let dir = tempfile::tempdir().unwrap();
        declare(dir.path(), "[automation]\ndeclared = true\n");
        assert!(repository_declares_automation("", dir.path().to_str().unwrap()).await);
    }

    #[tokio::test]
    async fn missing_or_false_declaration_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            !repository_declares_automation(
                dir.path().to_str().unwrap(),
                dir.path().to_str().unwrap()
            )
            .await
        );
        declare(dir.path(), "automation_declared = false\n");
        assert!(
            !repository_declares_automation(
                dir.path().to_str().unwrap(),
                dir.path().to_str().unwrap()
            )
            .await
        );
    }

    #[tokio::test]
    async fn agent_policy_store_handler_can_be_tested_without_server_actor() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        let handler = AutomationAgentPolicyStoreHandler::new(&store);
        let now = Utc::now();
        store
            .upsert_agent_profile(
                AgentProfile {
                    id: "profile".into(),
                    name: "Profile".into(),
                    sort_order: 0,
                    agent_type: "codex".into(),
                    command: "codex".into(),
                    launch_mode: AgentProfileLaunchMode::Command,
                    managed_config: None,
                    custom_prompt: String::new(),
                    description: String::new(),
                    quota_group: None,
                    revision: 0,
                    created_at: now,
                    updated_at: now,
                },
                None,
            )
            .await
            .unwrap();

        let default = handler.get("profile").await.unwrap();
        assert!(!default.may_activate_or_edit_active);
        assert!(!default.may_execute);

        let saved = handler
            .set(AutomationAgentPolicy {
                profile_id: "profile".into(),
                may_activate_or_edit_active: true,
                may_execute: true,
                updated_at: Utc::now(),
            })
            .await
            .unwrap();
        assert!(saved.may_activate_or_edit_active);
        assert!(saved.may_execute);

        let loaded = handler.get("profile").await.unwrap();
        assert_eq!(loaded, saved);
    }

    #[tokio::test]
    async fn project_policy_store_handler_can_be_tested_without_server_actor() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        let handler = AutomationProjectPolicyStoreHandler::new(&store);

        let default = handler.get("project").await.unwrap();
        assert_eq!(default.project_id, "project");
        assert!(!default.repo_declared);
        assert!(!default.local_approved);
        assert!(!default.restrictive);

        let saved = handler
            .set(alera_core::runtime::AutomationProjectPolicy {
                project_id: "project".into(),
                repo_declared: true,
                local_approved: true,
                restrictive: true,
                updated_at: Utc::now(),
            })
            .await
            .unwrap();
        assert!(saved.repo_declared);
        assert!(saved.local_approved);
        assert!(saved.restrictive);

        let loaded = handler.get("project").await.unwrap();
        assert_eq!(loaded, saved);
    }

    #[tokio::test]
    async fn automation_policy_context_reads_can_be_tested_without_server_actor() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        let now = Utc::now();
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
            .upsert_workspace(workspace("workspace", "project", &repo))
            .await
            .unwrap();
        let identity = AutomationTargetIdentity {
            workspace_id: Some("workspace".into()),
            tab_id: None,
            session_id: Some("session".into()),
            profile_id: None,
            conversation_id: None,
            terminal_handle: None,
        };
        store
            .insert_automation_run(&automation_run("run", identity))
            .await
            .unwrap();
        let handler = AutomationPolicyContextStoreHandler::new(&store);

        assert_eq!(handler.find_run("run").await.unwrap().unwrap().id, "run");
        assert_eq!(
            handler
                .find_workspace("workspace")
                .await
                .unwrap()
                .unwrap()
                .id,
            "workspace"
        );
        assert_eq!(
            handler.find_project("project").await.unwrap().unwrap().id,
            "project"
        );
        assert!(handler.find_run("missing").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn automation_policy_context_failures_keep_existing_messages_and_order() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        let actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
        let human = AutomationActor {
            kind: AutomationActorKind::HumanDesktop,
            id: None,
            label: None,
        };
        let fresh = automation_definition(AutomationTarget::FreshTab {
            workspace_id: "workspace".into(),
            agent_profile_id: "profile".into(),
        });

        assert_eq!(
            actor
                .ensure_agent_policy(&fresh, &human, false)
                .await
                .unwrap_err()
                .to_string(),
            "automation target workspace is missing"
        );

        actor
            .runtime_store
            .upsert_workspace(workspace("workspace", "project", &repo))
            .await
            .unwrap();
        assert_eq!(
            actor
                .ensure_agent_policy(&fresh, &human, false)
                .await
                .unwrap_err()
                .to_string(),
            "automation target project is missing"
        );

        let now = Utc::now();
        actor
            .runtime_store
            .upsert_project(Project {
                id: "project".into(),
                name: "Project".into(),
                repo_path: repo.to_string_lossy().into_owned(),
                created_at: now,
                updated_at: now,
                kind: ProjectKind::Folder,
            })
            .await
            .unwrap();
        let managed = automation_definition(AutomationTarget::ManagedWorkspace {
            source_workspace_id: "workspace".into(),
            source_branch: "main".into(),
            name_template: "automation/{slug}".into(),
            agent_profile_id: "profile".into(),
        });
        assert_eq!(
            actor
                .ensure_agent_policy(&managed, &human, false)
                .await
                .unwrap_err()
                .to_string(),
            "managed workspace automations require a git repository project"
        );

        actor
            .runtime_store
            .upsert_project(Project {
                id: "project".into(),
                name: "Project".into(),
                repo_path: repo.to_string_lossy().into_owned(),
                created_at: now,
                updated_at: Utc::now(),
                kind: ProjectKind::GitRepository,
            })
            .await
            .unwrap();
        assert_eq!(
            actor
                .ensure_agent_policy(&managed, &human, false)
                .await
                .unwrap_err()
                .to_string(),
            "repository project has no automation declaration in alera.toml"
        );

        declare(&repo, "automation_declared = true\n");
        actor
            .runtime_store
            .set_automation_project_policy(AutomationProjectPolicy {
                project_id: "project".into(),
                repo_declared: true,
                local_approved: false,
                restrictive: true,
                updated_at: Utc::now(),
            })
            .await
            .unwrap();
        assert_eq!(
            actor
                .ensure_agent_policy(&managed, &human, false)
                .await
                .unwrap_err()
                .to_string(),
            "project policy for project requires local approval"
        );
    }

    #[tokio::test]
    async fn existing_tab_target_profile_keeps_workspace_tab_semantics() {
        let dir = tempfile::tempdir().unwrap();
        let actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
        let now = Utc::now();
        actor
            .runtime_store
            .upsert_agent_profile(
                AgentProfile {
                    id: "profile".into(),
                    name: "Profile".into(),
                    sort_order: 0,
                    agent_type: "codex".into(),
                    command: "codex".into(),
                    launch_mode: AgentProfileLaunchMode::Command,
                    managed_config: None,
                    custom_prompt: String::new(),
                    description: String::new(),
                    quota_group: None,
                    revision: 0,
                    created_at: now,
                    updated_at: now,
                },
                None,
            )
            .await
            .unwrap();
        actor
            .runtime_store
            .upsert_workspace_tab(WorkspaceTabRecord {
                id: "tab".into(),
                workspace_id: "workspace".into(),
                kind: "terminal".into(),
                title: "Terminal".into(),
                created_at: now,
                updated_at: now,
                payload: json!({"agentProfileId": "profile"}),
            })
            .await
            .unwrap();
        let definition = automation_definition(AutomationTarget::ExistingTab {
            workspace_id: "workspace".into(),
            tab_id: "tab".into(),
            conversation_id: None,
        });

        assert_eq!(
            actor
                .target_profile_id(&definition)
                .await
                .unwrap()
                .as_deref(),
            Some("profile")
        );
    }

    #[tokio::test]
    async fn policy_actor_resolution_keeps_run_identity_and_managed_actor_semantics() {
        let dir = tempfile::tempdir().unwrap();
        let (handle, _responses) = ClientHandle::test_channels();
        let mut session = Session::driver_test_stub("session", 80, 24);
        session.workspace_id = "workspace".into();
        session.tab_id = "tab".into();
        session.attach(1);
        let actor = test_actor(
            &dir,
            HashMap::from([(1, local_client(handle))]),
            HashMap::from([("session".into(), session)]),
        )
        .await;
        let identity = AutomationTargetIdentity {
            workspace_id: Some("workspace".into()),
            tab_id: None,
            session_id: Some("session".into()),
            profile_id: None,
            conversation_id: None,
            terminal_handle: None,
        };
        actor
            .runtime_store
            .insert_automation_run(&automation_run("run", identity.clone()))
            .await
            .unwrap();
        let local_cli = AutomationActor {
            kind: AutomationActorKind::LocalCli,
            id: None,
            label: None,
        };
        let payload = json!({
            "run": "run",
            "targetIdentity": {"workspaceId": "workspace", "sessionId": "session"}
        });

        let resolved = actor
            .resolve_policy_actor(1, &payload, local_cli.clone())
            .await
            .unwrap();
        assert_eq!(resolved.kind, AutomationActorKind::ManagedAgent);
        assert_eq!(resolved.id.as_deref(), Some("profile"));

        let mut mismatch = identity;
        mismatch.workspace_id = Some("other".into());
        actor
            .runtime_store
            .insert_automation_run(&automation_run("run-mismatch", mismatch))
            .await
            .unwrap();
        let error = actor
            .resolve_policy_actor(
                1,
                &json!({
                    "run": "run-mismatch",
                    "targetIdentity": {"workspaceId": "workspace", "sessionId": "session"}
                }),
                local_cli,
            )
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "automation run target identity does not match the live run"
        );
    }
}
