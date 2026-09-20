use alera_core::runtime::{RuntimeStore, WorkspaceTabRecord};

use crate::terminal_host::host_error::{HostError, HostResult};

pub(super) struct WorkspaceTabStoreHandler<'a> {
    runtime_store: &'a RuntimeStore,
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
}

fn state_error(error: impl std::fmt::Display) -> HostError {
    HostError::state(error.to_string())
}

#[cfg(test)]
mod tests {
    use alera_core::runtime::{RuntimeStore, WorkspaceTabRecord};
    use chrono::Utc;
    use serde_json::json;

    use super::WorkspaceTabStoreHandler;

    #[tokio::test]
    async fn empty_tab_queries_can_be_tested_without_server_actor() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        let handler = WorkspaceTabStoreHandler::new(&store);

        assert!(handler.list("workspace").await.unwrap().is_empty());
        assert!(handler.find("tab").await.unwrap().is_none());
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
    }
}
