use alera_core::runtime::{AgentProfile, RuntimeStore, RuntimeStoreError};
use serde_json::{json, Value};

use crate::terminal_host::host_error::{HostError, HostResult};

pub(super) struct AgentProfileCatalogRequestHandler<'a> {
    runtime_store: &'a RuntimeStore,
}

impl<'a> AgentProfileCatalogRequestHandler<'a> {
    pub(super) const fn new(runtime_store: &'a RuntimeStore) -> Self {
        Self { runtime_store }
    }

    pub(super) async fn list(&self) -> HostResult<Value> {
        let profiles = self
            .runtime_store
            .list_agent_profiles()
            .await
            .map_err(agent_profile_store_error)?;
        let items = serde_json::to_value(profiles).map_err(format_error)?;
        Ok(json!({ "kind": "agentProfiles", "items": items, "filters": {} }))
    }

    pub(super) async fn upsert(
        &self,
        profile: AgentProfile,
        expected_revision: Option<i64>,
    ) -> HostResult<Value> {
        let stored = self
            .runtime_store
            .upsert_agent_profile(profile, expected_revision)
            .await
            .map_err(agent_profile_store_error)?;
        serde_json::to_value(stored).map_err(format_error)
    }
}

pub(super) fn agent_profile_store_error(error: anyhow::Error) -> HostError {
    if let Some(RuntimeStoreError::AgentProfileRevisionConflict {
        profile_id,
        expected,
        current,
    }) = error.downcast_ref::<RuntimeStoreError>()
    {
        return HostError::conflict(
            "agent_profile_revision_conflict",
            error.to_string(),
            json!({
                "profileId": profile_id,
                "expectedRevision": expected,
                "currentRevision": current,
            }),
        );
    }
    HostError::state(error.to_string())
}

fn format_error(error: impl std::fmt::Display) -> HostError {
    HostError::format(error.to_string())
}

#[cfg(test)]
mod tests {
    use alera_core::runtime::{AgentProfile, AgentProfileLaunchMode, RuntimeStore};
    use chrono::Utc;

    use super::AgentProfileCatalogRequestHandler;

    fn profile(id: &str) -> AgentProfile {
        let now = Utc::now();
        AgentProfile {
            id: id.into(),
            name: "Codex".into(),
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
        }
    }

    #[tokio::test]
    async fn list_and_upsert_can_be_tested_without_server_actor() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        let handler = AgentProfileCatalogRequestHandler::new(&store);

        let saved = handler.upsert(profile("profile"), None).await.unwrap();
        let mut updated = profile("profile");
        updated.name = "Codex Updated".into();
        let updated = handler.upsert(updated, Some(0)).await.unwrap();
        let listed = handler.list().await.unwrap();

        assert_eq!(saved["id"], "profile");
        assert_eq!(saved["revision"], 0);
        assert_eq!(updated["name"], "Codex Updated");
        assert_eq!(updated["revision"], 1);
        assert_eq!(listed["kind"], "agentProfiles");
        assert_eq!(listed["items"].as_array().unwrap().len(), 1);
    }
}
