use alera_core::runtime::RuntimeStore;
use serde_json::Value;

use crate::ssh_bootstrap::{build_ssh_bootstrap_plan, SshTargetBootstrapRequest};
use crate::terminal_host::host_error::HostResult;

use super::request_payloads::json_result;

pub(super) struct SshBootstrapPlanRequestHandler<'a> {
    runtime_store: &'a RuntimeStore,
}

impl<'a> SshBootstrapPlanRequestHandler<'a> {
    pub(super) const fn new(runtime_store: &'a RuntimeStore) -> Self {
        Self { runtime_store }
    }

    pub(super) async fn build(&self, request: &SshTargetBootstrapRequest) -> HostResult<Value> {
        json_result(build_ssh_bootstrap_plan(self.runtime_store, request).await)
    }
}

#[cfg(test)]
mod tests {
    use alera_core::runtime::{RuntimeStore, SshAuthKind, SshBootstrapStatus, SshTarget};
    use chrono::Utc;

    use crate::ssh_bootstrap::SshTargetBootstrapRequest;

    use super::SshBootstrapPlanRequestHandler;

    #[tokio::test]
    async fn bootstrap_plan_can_be_tested_without_server_actor() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        let now = Utc::now();
        store
            .upsert_ssh_target(SshTarget {
                id: "target".into(),
                alias: "Remote".into(),
                host: "remote.example.test".into(),
                port: 22,
                username: "alera".into(),
                platform: Some("linux".into()),
                arch: Some("x64".into()),
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
            })
            .await
            .unwrap();
        let handler = SshBootstrapPlanRequestHandler::new(&store);

        let plan = handler
            .build(&SshTargetBootstrapRequest {
                target_id: "target".into(),
                channel: None,
                version: None,
                install_dir: None,
                platform: None,
                arch: None,
                archive_url: None,
                archive_path: None,
                artifact_path: None,
                manifest_public_key: None,
            })
            .await
            .unwrap();

        assert_eq!(plan["targetId"], "target");
        assert_eq!(plan["alias"], "Remote");
        assert_eq!(plan["installDir"], "/srv/alera");
        assert_eq!(plan["status"], "planned");
    }
}
