use std::collections::HashMap;

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

    pub(super) async fn reorder(
        &self,
        profile_ids: &[String],
        expected_revisions: &HashMap<String, i64>,
    ) -> HostResult<Value> {
        let profiles = self
            .runtime_store
            .reorder_agent_profiles(profile_ids, expected_revisions)
            .await
            .map_err(agent_profile_store_error)?;
        let items = serde_json::to_value(profiles).map_err(format_error)?;
        Ok(json!({
            "kind": "agentProfiles",
            "items": items,
            "filters": {}
        }))
    }

    pub(super) async fn removal_impact(
        &self,
        id: &str,
        expected_revision: i64,
    ) -> HostResult<Value> {
        let impact = self
            .runtime_store
            .agent_profile_removal_impact(id, expected_revision)
            .await
            .map_err(agent_profile_store_error)?;
        let reference_count = impact.reference_count();
        let blocking_reference_count =
            impact.automation_ids.len() + impact.execution_policy_run_ids.len() + impact.tabs.len();
        let mut value = serde_json::to_value(impact).map_err(format_error)?;
        let object = value
            .as_object_mut()
            .ok_or_else(|| HostError::format("Agent profile removal impact must be an object."))?;
        object.insert("referenceCount".into(), json!(reference_count));
        object.insert(
            "blockingReferenceCount".into(),
            json!(blocking_reference_count),
        );
        Ok(value)
    }

    pub(super) async fn remove(&self, id: &str, expected_revision: i64) -> HostResult<bool> {
        self.runtime_store
            .remove_agent_profile(id, expected_revision)
            .await
            .map_err(agent_profile_store_error)
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
    use std::collections::HashMap;

    use alera_core::runtime::{AgentProfile, AgentProfileLaunchMode, RuntimeStore};
    use chrono::Utc;

    use super::AgentProfileCatalogRequestHandler;

    fn profile(id: &str) -> AgentProfile {
        let now = Utc::now();
        AgentProfile {
            id: id.into(),
            name: format!("Codex {id}"),
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

    #[tokio::test]
    async fn reorder_and_removal_can_be_tested_without_server_actor() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        let handler = AgentProfileCatalogRequestHandler::new(&store);
        handler.upsert(profile("a"), None).await.unwrap();
        handler.upsert(profile("b"), None).await.unwrap();
        handler.upsert(profile("remove"), None).await.unwrap();

        let reordered = handler
            .reorder(
                &["b".into(), "a".into(), "remove".into()],
                &HashMap::from([("a".into(), 0), ("b".into(), 0), ("remove".into(), 0)]),
            )
            .await
            .unwrap();
        let ids = reordered["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["id"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["b", "a", "remove"]);

        let remove_revision = reordered["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"] == "remove")
            .unwrap()["revision"]
            .as_i64()
            .unwrap();
        let impact = handler
            .removal_impact("remove", remove_revision)
            .await
            .unwrap();
        assert_eq!(impact["referenceCount"], 0);
        assert_eq!(impact["blockingReferenceCount"], 0);
        assert!(handler.remove("remove", remove_revision).await.unwrap());
        assert_eq!(
            handler.list().await.unwrap()["items"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }
}
