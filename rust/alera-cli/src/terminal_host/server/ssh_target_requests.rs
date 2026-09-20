use alera_core::runtime::{RuntimeStore, SshTarget};
use serde_json::Value;

use crate::terminal_host::host_error::{HostError, HostResult};

pub(super) struct SshTargetRequestHandler<'a> {
    runtime_store: &'a RuntimeStore,
}

impl<'a> SshTargetRequestHandler<'a> {
    pub(super) const fn new(runtime_store: &'a RuntimeStore) -> Self {
        Self { runtime_store }
    }

    pub(super) async fn list(&self) -> HostResult<Value> {
        let targets = self
            .runtime_store
            .list_ssh_targets()
            .await
            .map_err(state_error)?;
        serde_json::to_value(targets).map_err(format_error)
    }

    pub(super) async fn upsert(&self, payload: &Value) -> HostResult<Value> {
        let mut target: SshTarget =
            serde_json::from_value(payload.clone()).map_err(format_error)?;
        if payload.get("installDir").is_none() {
            if let Some(existing) = self
                .runtime_store
                .find_ssh_target(&target.id)
                .await
                .map_err(state_error)?
            {
                target.install_dir = existing.install_dir;
            }
        }
        let stored = self
            .runtime_store
            .upsert_ssh_target(target)
            .await
            .map_err(state_error)?;
        serde_json::to_value(stored).map_err(format_error)
    }

    pub(super) async fn remove(&self, id: &str) -> HostResult<()> {
        self.runtime_store
            .remove_ssh_target(id)
            .await
            .map_err(state_error)
    }
}

fn format_error(error: impl std::fmt::Display) -> HostError {
    HostError::format(error.to_string())
}

fn state_error(error: impl std::fmt::Display) -> HostError {
    HostError::state(error.to_string())
}

#[cfg(test)]
mod tests {
    use alera_core::runtime::{RuntimeStore, SshAuthKind, SshBootstrapStatus, SshTarget};
    use chrono::Utc;
    use serde_json::to_value;

    use super::SshTargetRequestHandler;

    fn target(id: &str) -> SshTarget {
        let now = Utc::now();
        SshTarget {
            id: id.into(),
            alias: "Remote".into(),
            host: "remote.example.test".into(),
            port: 22,
            username: "alera".into(),
            platform: None,
            arch: None,
            auth_kind: SshAuthKind::Agent,
            created_at: now,
            updated_at: now,
            last_status: None,
            install_dir: Some("/srv/alera".into()),
            runtime_version: None,
            runtime_platform: None,
            runtime_arch: None,
            bootstrap_status: SshBootstrapStatus::NotInstalled,
            last_bootstrap_at: None,
            last_checked_at: None,
            last_error: None,
        }
    }

    #[tokio::test]
    async fn target_crud_and_install_dir_compatibility_do_not_need_server_actor() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        let handler = SshTargetRequestHandler::new(&store);
        let initial = target("target");

        handler.upsert(&to_value(&initial).unwrap()).await.unwrap();
        let mut update = initial.clone();
        update.host = "renamed.example.test".into();
        let mut payload = to_value(update).unwrap();
        payload.as_object_mut().unwrap().remove("installDir");
        let updated = handler.upsert(&payload).await.unwrap();

        assert_eq!(updated["host"], "renamed.example.test");
        assert_eq!(updated["installDir"], "/srv/alera");
        assert_eq!(handler.list().await.unwrap().as_array().unwrap().len(), 1);

        handler.remove("target").await.unwrap();
        assert!(handler.list().await.unwrap().as_array().unwrap().is_empty());
    }
}
