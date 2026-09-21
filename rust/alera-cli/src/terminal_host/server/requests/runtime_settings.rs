use serde_json::{json, Value};

use crate::terminal_host::host_error::HostResult;
use crate::terminal_host::protocol::event;

use super::runtime_settings_store::RuntimeSettingsStoreHandler;
use super::runtime_settings_validation::parse_runtime_settings_update;
use super::ServerActor;

impl ServerActor {
    pub(crate) async fn runtime_settings_snapshot(&self) -> HostResult<Value> {
        RuntimeSettingsStoreHandler::new(&self.runtime_store)
            .snapshot()
            .await
    }

    pub(crate) async fn persist_runtime_settings_update(
        &mut self,
        payload: &Value,
    ) -> HostResult<Value> {
        let update = parse_runtime_settings_update(payload)?;
        let revision = RuntimeSettingsStoreHandler::new(&self.runtime_store)
            .commit(&update)
            .await?;

        if let Some(settings) = &update.agent_status_hooks {
            self.agent_presence
                .retain_enabled(&settings.enabled_agents());
            self.schedule_agent_integration_reconcile(settings.clone());
            self.broadcast_agent_presence_changed();
        }
        if update.agent_quotas.is_some() {
            self.agent_quota_cache = None;
        }
        if update.mobile_push_notifications.is_some() {
            self.account_push.push_enabled = false;
            self.account_push.active_subscriptions = 0;
        }
        if let Some(settings) = &update.ai_assist {
            let cancel_titles = self
                .agent_title_jobs
                .iter()
                .filter(|(_, job)| {
                    !settings.enabled || (job.automatic && !settings.auto_generate_agent_titles)
                })
                .map(|(tab, _)| tab.clone())
                .collect::<Vec<_>>();
            let titles_canceled = !cancel_titles.is_empty();
            for tab_id in cancel_titles {
                self.cancel_agent_title_job(&tab_id);
                if let Ok(Some(mut tab)) = self.runtime_store.find_workspace_tab(&tab_id).await {
                    tab.payload["agentTitleStatus"] = json!("idle");
                    let _ = self.runtime_store.upsert_workspace_tab(tab).await;
                }
            }
            if titles_canceled {
                self.broadcast_workspace_tabs_changed(None);
            }
        }

        let mut value = self.runtime_settings_snapshot().await?;
        value["revision"] = json!(revision);
        Ok(value)
    }

    pub(crate) async fn apply_mobile_runtime_settings(
        &mut self,
        payload: &Value,
    ) -> HostResult<Value> {
        let value = self.persist_runtime_settings_update(payload).await?;
        if payload.get("automation").is_some() {
            self.schedule_autostart_reconcile();
        }
        self.broadcast_authenticated(event("runtimeSettingsChanged", json!({})));
        Ok(value)
    }
}
