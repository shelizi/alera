use alera_core::runtime::format_timestamp;
use chrono::Utc;
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::SqliteConnection;

use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::protocol::event;

use super::runtime_settings_validation::{parse_runtime_settings_update, RuntimeSettingsUpdate};
use super::ServerActor;

const RUNTIME_SETTINGS_REVISION_KEY: &str = "settings.runtimeSettingsRevision";

impl ServerActor {
    pub(crate) async fn runtime_settings_snapshot(&self) -> HostResult<Value> {
        let mut value = runtime_value(self.runtime_store.runtime_settings().await)?;
        let revision = self.runtime_settings_revision().await?;
        let object = value
            .as_object_mut()
            .ok_or_else(|| HostError::state("Runtime settings must be a JSON object."))?;
        object.insert("revision".to_string(), json!(revision));
        Ok(value)
    }

    async fn runtime_settings_revision(&self) -> HostResult<i64> {
        let raw = self
            .runtime_store
            .get_metadata(RUNTIME_SETTINGS_REVISION_KEY)
            .await
            .map_err(|error| HostError::state(error.to_string()))?;
        parse_stored_runtime_settings_revision(raw.as_deref())
    }

    pub(crate) async fn persist_runtime_settings_update(
        &mut self,
        payload: &Value,
    ) -> HostResult<Value> {
        let update = parse_runtime_settings_update(payload)?;
        let revision = self.commit_runtime_settings_update(&update).await?;

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

    async fn commit_runtime_settings_update(
        &self,
        update: &RuntimeSettingsUpdate,
    ) -> HostResult<i64> {
        let mut transaction = self
            .runtime_store
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|error| HostError::state(error.to_string()))?;
        let operation: HostResult<i64> = async {
            let raw_revision: Option<String> =
                sqlx::query_scalar("SELECT value FROM runtimeMetadata WHERE key = ?")
                    .bind(RUNTIME_SETTINGS_REVISION_KEY)
                    .fetch_optional(&mut *transaction)
                    .await
                    .map_err(|error| HostError::state(error.to_string()))?;
            let actual_revision = parse_stored_runtime_settings_revision(raw_revision.as_deref())?;
            if let Some(expected_revision) = update.expected_revision {
                if expected_revision != actual_revision {
                    return Err(HostError::conflict(
                        "runtime_settings_revision_conflict",
                        "Runtime settings revision conflict. Refresh and retry.",
                        json!({
                            "expectedRevision": expected_revision,
                            "actualRevision": actual_revision,
                        }),
                    ));
                }
            }

            let next_revision = if update.has_settings() {
                actual_revision.saturating_add(1)
            } else {
                actual_revision
            };
            let updated_at = format_timestamp(Utc::now());

            if let Some(value) = &update.workspace_directory {
                match value
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    Some(value) => {
                        write_runtime_metadata(
                            &mut *transaction,
                            "settings.general.workspaceDirectory",
                            value,
                            &updated_at,
                        )
                        .await?;
                    }
                    None => {
                        delete_runtime_metadata(
                            &mut *transaction,
                            "settings.general.workspaceDirectory",
                        )
                        .await?;
                    }
                }
            }
            if let Some(value) = update.confirm_project_removal {
                write_runtime_metadata(
                    &mut *transaction,
                    "settings.general.confirmProjectRemoval",
                    if value { "true" } else { "false" },
                    &updated_at,
                )
                .await?;
            }
            if let Some(value) = update.confirm_workspace_removal {
                write_runtime_metadata(
                    &mut *transaction,
                    "settings.general.confirmWorkspaceRemoval",
                    if value { "true" } else { "false" },
                    &updated_at,
                )
                .await?;
            }
            if let Some(value) = update.auto_archive_workspaces_after_days {
                write_runtime_metadata(
                    &mut *transaction,
                    "settings.general.autoArchiveWorkspacesAfterDays",
                    &value.to_string(),
                    &updated_at,
                )
                .await?;
            }
            if let Some(value) = &update.default_agent_profile_id {
                match value
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    Some(value) => {
                        write_runtime_metadata(
                            &mut *transaction,
                            "settings.agents.defaultAgentProfileId",
                            value,
                            &updated_at,
                        )
                        .await?;
                    }
                    None => {
                        delete_runtime_metadata(
                            &mut *transaction,
                            "settings.agents.defaultAgentProfileId",
                        )
                        .await?;
                    }
                }
            }
            if let Some(value) = &update.agent_status_hooks {
                let encoded = serde_json::to_string(value)
                    .map_err(|error| HostError::state(error.to_string()))?;
                write_runtime_metadata(
                    &mut *transaction,
                    "settings.agents.agentStatusHooks",
                    &encoded,
                    &updated_at,
                )
                .await?;
            }
            if let Some(value) = &update.agent_quotas {
                let encoded = serde_json::to_string(&value.clone().normalized())
                    .map_err(|error| HostError::state(error.to_string()))?;
                write_runtime_metadata(
                    &mut *transaction,
                    "settings.agents.quotas",
                    &encoded,
                    &updated_at,
                )
                .await?;
            }
            if let Some(value) = &update.mobile_push_notifications {
                let encoded = serde_json::to_string(value)
                    .map_err(|error| HostError::state(error.to_string()))?;
                write_runtime_metadata(
                    &mut *transaction,
                    "settings.mobile.pushNotifications",
                    &encoded,
                    &updated_at,
                )
                .await?;
            }
            if let Some(value) = &update.ai_assist {
                let encoded = serde_json::to_string(&value.clone().normalized())
                    .map_err(|error| HostError::state(error.to_string()))?;
                write_runtime_metadata(
                    &mut *transaction,
                    "settings.aiTextGeneration",
                    &encoded,
                    &updated_at,
                )
                .await?;
            }
            if let Some(value) = &update.text_actions {
                let encoded = serde_json::to_string(&value.clone().normalized())
                    .map_err(|error| HostError::state(error.to_string()))?;
                write_runtime_metadata(
                    &mut *transaction,
                    "settings.textActions",
                    &encoded,
                    &updated_at,
                )
                .await?;
            }
            if let Some(value) = &update.automation {
                let encoded = serde_json::to_string(value)
                    .map_err(|error| HostError::state(error.to_string()))?;
                write_runtime_metadata(
                    &mut *transaction,
                    "settings.automation",
                    &encoded,
                    &updated_at,
                )
                .await?;
            }
            if update.has_settings() {
                write_runtime_metadata(
                    &mut *transaction,
                    RUNTIME_SETTINGS_REVISION_KEY,
                    &next_revision.to_string(),
                    &updated_at,
                )
                .await?;
            }
            Ok(next_revision)
        }
        .await;
        match operation {
            Ok(revision) => {
                transaction
                    .commit()
                    .await
                    .map_err(|error| HostError::state(error.to_string()))?;
                Ok(revision)
            }
            Err(error) => {
                let _ = transaction.rollback().await;
                Err(error)
            }
        }
    }
}

fn parse_stored_runtime_settings_revision(raw: Option<&str>) -> HostResult<i64> {
    match raw {
        None => Ok(0),
        Some(value) => value
            .parse::<i64>()
            .ok()
            .filter(|revision| *revision >= 0)
            .ok_or_else(|| HostError::state("Stored runtime settings revision is invalid.")),
    }
}

async fn write_runtime_metadata(
    connection: &mut SqliteConnection,
    key: &str,
    value: &str,
    updated_at: &str,
) -> HostResult<()> {
    sqlx::query(
        "INSERT INTO runtimeMetadata (key, value, updatedAt) VALUES (?, ?, ?) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt",
    )
    .bind(key)
    .bind(value)
    .bind(updated_at)
    .execute(&mut *connection)
    .await
    .map_err(|error| HostError::state(error.to_string()))?;
    Ok(())
}

async fn delete_runtime_metadata(connection: &mut SqliteConnection, key: &str) -> HostResult<()> {
    sqlx::query("DELETE FROM runtimeMetadata WHERE key = ?")
        .bind(key)
        .execute(&mut *connection)
        .await
        .map_err(|error| HostError::state(error.to_string()))?;
    Ok(())
}

fn runtime_value<T, E>(result: Result<T, E>) -> HostResult<Value>
where
    T: Serialize,
    E: std::fmt::Display,
{
    let value = result.map_err(|error| HostError::state(error.to_string()))?;
    serde_json::to_value(value).map_err(|error| HostError::state(error.to_string()))
}
