use alera_core::runtime::{RuntimeStore, WorkspaceTabRecord};

use crate::terminal_host::host_error::HostResult;
use crate::terminal_host::orchestration::agent_profile_launch_snapshot::AgentInitialDeliveryMechanismV1;

use super::terminal_startup_commands::{
    initial_delivery_mechanism, initial_prompt, replays_initial_prompt_on_restart, tab_agent_type,
};
use super::workspace_tab_requests::WorkspaceTabStoreHandler;
use super::ServerActor;

struct TerminalPromptRearmPersistence<'a> {
    tabs: WorkspaceTabStoreHandler<'a>,
}

impl<'a> TerminalPromptRearmPersistence<'a> {
    const fn new(runtime_store: &'a RuntimeStore) -> Self {
        Self {
            tabs: WorkspaceTabStoreHandler::new(runtime_store),
        }
    }

    async fn upsert_tab(&self, tab: WorkspaceTabRecord) -> HostResult<WorkspaceTabRecord> {
        self.tabs.upsert(tab).await
    }
}

impl ServerActor {
    pub(super) async fn rearm_terminal_after_ready_prompt(
        &mut self,
        tab: &WorkspaceTabRecord,
    ) -> HostResult<Option<WorkspaceTabRecord>> {
        if !replays_initial_prompt_on_restart(tab)?
            || initial_delivery_mechanism(tab)?
                != Some(AgentInitialDeliveryMechanismV1::TerminalAfterReady)
            || tab
                .payload
                .get("pendingAgentPrompt")
                .is_some_and(|value| !value.is_null())
        {
            return Ok(None);
        }
        let Some(prompt) = initial_prompt(tab) else {
            return Ok(None);
        };
        let Some(agent_type) = tab_agent_type(tab) else {
            return Ok(None);
        };

        let mut next = tab.clone();
        let Some(payload) = next.payload.as_object_mut() else {
            return Ok(None);
        };
        payload.insert(
            "pendingAgentPrompt".to_string(),
            serde_json::json!({"agent": agent_type, "prompt": prompt}),
        );
        next.updated_at = chrono::Utc::now();
        TerminalPromptRearmPersistence::new(&self.runtime_store)
            .upsert_tab(next)
            .await
            .map(Some)
    }
}

#[cfg(test)]
mod tests {
    use alera_core::runtime::{RuntimeStore, WorkspaceTabRecord};
    use chrono::Utc;
    use serde_json::json;

    use super::TerminalPromptRearmPersistence;

    #[tokio::test]
    async fn prompt_rearm_persistence_can_be_tested_without_server_actor() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        let now = Utc::now();
        let persistence = TerminalPromptRearmPersistence::new(&store);

        let saved = persistence
            .upsert_tab(WorkspaceTabRecord {
                id: "tab".into(),
                workspace_id: "workspace".into(),
                kind: "terminal".into(),
                title: "Agent".into(),
                created_at: now,
                updated_at: now,
                payload: json!({
                    "pendingAgentPrompt": {"agent": "codex", "prompt": "resume"},
                    "keep": "value",
                }),
            })
            .await
            .unwrap();

        assert_eq!(saved.payload["keep"], "value");
        assert_eq!(
            store
                .find_workspace_tab("tab")
                .await
                .unwrap()
                .unwrap()
                .payload,
            saved.payload
        );
    }
}
