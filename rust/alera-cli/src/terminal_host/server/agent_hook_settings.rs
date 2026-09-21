use alera_core::runtime::{RuntimeAgentStatusHookSettings, RuntimeStore};

use crate::terminal_host::host_error::{HostError, HostResult};

pub(super) struct AgentHookSettingsQuery<'a> {
    runtime_store: &'a RuntimeStore,
}

impl<'a> AgentHookSettingsQuery<'a> {
    pub(super) const fn new(runtime_store: &'a RuntimeStore) -> Self {
        Self { runtime_store }
    }

    pub(super) async fn load(&self) -> HostResult<RuntimeAgentStatusHookSettings> {
        self.runtime_store
            .agent_status_hook_settings()
            .await
            .map_err(state_error)
    }
}

fn state_error(error: impl std::fmt::Display) -> HostError {
    HostError::state(error.to_string())
}

#[cfg(test)]
mod tests {
    use alera_core::runtime::RuntimeStore;

    use super::AgentHookSettingsQuery;

    #[tokio::test]
    async fn hook_settings_lookup_preserves_default_and_malformed_fallbacks() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        let query = AgentHookSettingsQuery::new(&store);

        assert!(!query.load().await.unwrap().is_enabled("codex"));

        store
            .set_metadata("settings.agents.agentStatusHooks", "{\"codex\":true}")
            .await
            .unwrap();
        assert!(query.load().await.unwrap().is_enabled("codex"));

        store
            .set_metadata("settings.agents.agentStatusHooks", "{not-json")
            .await
            .unwrap();
        let fallback = query.load().await.unwrap();
        assert!(!fallback.is_enabled("codex"));
        assert!(fallback.enabled_agents().is_empty());
    }
}
