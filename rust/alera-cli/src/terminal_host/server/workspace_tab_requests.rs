use alera_core::runtime::{RuntimeStore, WorkspaceTabRecord};

use crate::terminal_host::host_error::{HostError, HostResult};

pub(super) struct WorkspaceTabQueryHandler<'a> {
    runtime_store: &'a RuntimeStore,
}

impl<'a> WorkspaceTabQueryHandler<'a> {
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
}

fn state_error(error: impl std::fmt::Display) -> HostError {
    HostError::state(error.to_string())
}

#[cfg(test)]
mod tests {
    use alera_core::runtime::RuntimeStore;

    use super::WorkspaceTabQueryHandler;

    #[tokio::test]
    async fn empty_tab_queries_can_be_tested_without_server_actor() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        let handler = WorkspaceTabQueryHandler::new(&store);

        assert!(handler.list("workspace").await.unwrap().is_empty());
        assert!(handler.find("tab").await.unwrap().is_none());
    }
}
