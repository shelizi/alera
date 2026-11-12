use alera_core::runtime::{RuntimeStore, WorkspaceTabRecord};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::terminal_host::host_error::{HostError, HostResult};

pub(super) struct WorkspaceTabStoreHandler<'a> {
    runtime_store: &'a RuntimeStore,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct TabPinCasRequest {
    pub workspace_id: String,
    pub id: String,
    pub expected_updated_at: DateTime<Utc>,
    pub expected_pinned: bool,
    pub pinned: bool,
    pub expected_terminal_session_id: Option<String>,
}

impl<'a> WorkspaceTabStoreHandler<'a> {
    pub(super) const fn new(runtime_store: &'a RuntimeStore) -> Self {
        Self { runtime_store }
    }

    pub(super) async fn list(&self, workspace_id: &str) -> HostResult<Vec<WorkspaceTabRecord>> {
        self.runtime_store
            .list_workspace_tabs(workspace_id)
            .await
            .map_err(state_error)
    }

    pub(super) async fn find(&self, id: &str) -> HostResult<Option<WorkspaceTabRecord>> {
        self.runtime_store
            .find_workspace_tab(id)
            .await
            .map_err(state_error)
    }

    pub(super) async fn rename(&self, id: &str, title: &str) -> HostResult<WorkspaceTabRecord> {
        self.runtime_store
            .rename_workspace_tab(id, title)
            .await
            .map_err(state_error)
    }

    pub(super) async fn upsert(&self, tab: WorkspaceTabRecord) -> HostResult<WorkspaceTabRecord> {
        self.runtime_store
            .upsert_workspace_tab(tab)
            .await
            .map_err(state_error)
    }

    pub(super) async fn set_pin(
        &self,
        request: TabPinCasRequest,
    ) -> HostResult<WorkspaceTabRecord> {
        self.runtime_store
            .compare_and_set_workspace_tab_pin(
                &request.workspace_id,
                &request.id,
                request.expected_updated_at,
                request.expected_pinned,
                request.pinned,
                request.expected_terminal_session_id.as_deref(),
            )
            .await
            .map_err(|error| {
                let text = error.to_string();
                if text.contains("workspace_tab_pin_revision_conflict")
                    || text.contains("workspace_tab_pin_identity_conflict")
                {
                    HostError::conflict(
                        "workspace_tab_pin_conflict",
                        "Tab changed or belongs to another session. Refresh before pinning.",
                        serde_json::json!({}),
                    )
                } else {
                    HostError::state(text)
                }
            })
    }

    /// Secret-free Native pin projection. Never return Agent commands,
    /// credentials, terminal receipts, or other Host-owned payload fields.
    pub(super) async fn list_pins(&self, workspace_id: &str) -> HostResult<Value> {
        let tabs = self.list(workspace_id).await?;
        if tabs.len() > 256 {
            return Err(HostError::state("Too many tabs for Host pin snapshot"));
        }
        Ok(Value::Array(tabs.iter().map(tab_pin_summary).collect()))
    }

    pub(super) async fn remove(&self, id: &str) -> HostResult<()> {
        self.runtime_store
            .remove_workspace_tab(id)
            .await
            .map_err(state_error)
    }
}

pub(super) fn tab_pin_summary(tab: &WorkspaceTabRecord) -> Value {
    serde_json::json!({
        "workspaceId": tab.workspace_id,
        "id": tab.id,
        "updatedAt": tab.updated_at,
        "pinned": tab.payload.get("pinned") == Some(&serde_json::json!(true)),
        "terminalSessionId": tab.payload.get("terminalSessionId").and_then(Value::as_str),
    })
}

fn state_error(error: impl std::fmt::Display) -> HostError {
    HostError::state(error.to_string())
}

#[cfg(test)]
mod tests {
    use alera_core::runtime::{RuntimeStore, WorkspaceTabRecord};
    use chrono::Utc;
    use serde_json::json;

    use super::{TabPinCasRequest, WorkspaceTabStoreHandler};

    #[tokio::test]
    async fn empty_tab_queries_can_be_tested_without_server_actor() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        let handler = WorkspaceTabStoreHandler::new(&store);

        assert!(handler.list("workspace").await.unwrap().is_empty());
        assert!(handler.find("tab").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn native_pin_projection_is_secret_free_and_cas_preserves_agent_payload() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        let now = Utc::now();
        store
            .upsert_workspace_tab(WorkspaceTabRecord {
                id: "tab-pin".into(),
                workspace_id: "workspace".into(),
                kind: "terminal".into(),
                title: "Agent".into(),
                created_at: now,
                updated_at: now,
                payload: json!({
                    "terminalSessionId":"pty-1",
                    "initialCommand":"sensitive command",
                    "agentTitleRevision":"private-revision",
                }),
            })
            .await
            .unwrap();
        let handler = WorkspaceTabStoreHandler::new(&store);
        let pins = handler.list_pins("workspace").await.unwrap();
        assert_eq!(pins.as_array().unwrap().len(), 1);
        assert_eq!(pins[0]["pinned"], false);
        assert_eq!(pins[0]["terminalSessionId"], "pty-1");
        assert!(!pins.to_string().contains("sensitive"));
        assert!(!pins.to_string().contains("private-revision"));

        let stored = handler.find("tab-pin").await.unwrap().unwrap();
        let saved = handler
            .set_pin(TabPinCasRequest {
                workspace_id: "workspace".into(),
                id: "tab-pin".into(),
                expected_updated_at: stored.updated_at,
                expected_pinned: false,
                pinned: true,
                expected_terminal_session_id: Some("pty-1".into()),
            })
            .await
            .unwrap();
        assert_eq!(saved.payload["pinned"], true);
        assert_eq!(saved.payload["initialCommand"], "sensitive command");
        assert_eq!(
            handler.list_pins("workspace").await.unwrap()[0]["pinned"],
            true
        );
    }

    #[tokio::test]
    async fn tab_rename_can_be_tested_without_server_actor() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        let now = Utc::now();
        store
            .upsert_workspace_tab(WorkspaceTabRecord {
                id: "tab".into(),
                workspace_id: "workspace".into(),
                kind: "terminal".into(),
                title: "Before".into(),
                created_at: now,
                updated_at: now,
                payload: json!({}),
            })
            .await
            .unwrap();
        let handler = WorkspaceTabStoreHandler::new(&store);

        let renamed = handler.rename("tab", "After").await.unwrap();

        assert_eq!(renamed.title, "After");
        assert_eq!(handler.find("tab").await.unwrap().unwrap().title, "After");
    }

    #[tokio::test]
    async fn tab_upsert_can_be_tested_without_server_actor() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        let now = Utc::now();
        let handler = WorkspaceTabStoreHandler::new(&store);

        let saved = handler
            .upsert(WorkspaceTabRecord {
                id: "tab".into(),
                workspace_id: "workspace".into(),
                kind: "terminal".into(),
                title: "Stored".into(),
                created_at: now,
                updated_at: now,
                payload: json!({"state": "ready"}),
            })
            .await
            .unwrap();

        assert_eq!(saved.title, "Stored");
        assert_eq!(
            handler.find("tab").await.unwrap().unwrap().payload["state"],
            "ready"
        );

        handler.remove("tab").await.unwrap();
        assert!(handler.find("tab").await.unwrap().is_none());
    }
}
