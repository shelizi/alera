use alera_core::runtime::{format_timestamp, RuntimeStore};
use chrono::Utc;
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::SqliteConnection;

use crate::terminal_host::host_error::{HostError, HostResult};

use super::runtime_settings_validation::RuntimeSettingsUpdate;

const RUNTIME_SETTINGS_REVISION_KEY: &str = "settings.runtimeSettingsRevision";

pub(super) struct RuntimeSettingsStoreHandler<'a> {
    runtime_store: &'a RuntimeStore,
}

impl<'a> RuntimeSettingsStoreHandler<'a> {
    pub(super) const fn new(runtime_store: &'a RuntimeStore) -> Self {
        Self { runtime_store }
    }

    pub(super) async fn snapshot(&self) -> HostResult<Value> {
        let mut value = runtime_value(self.runtime_store.runtime_settings().await)?;
        let revision = self.revision().await?;
        let object = value
            .as_object_mut()
            .ok_or_else(|| HostError::state("Runtime settings must be a JSON object."))?;
        object.insert("revision".to_string(), json!(revision));
        Ok(value)
    }

    pub(super) async fn commit(&self, update: &RuntimeSettingsUpdate) -> HostResult<i64> {
        let mut transaction = self
            .runtime_store
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(state_error)?;
        let operation: HostResult<i64> = async {
            let raw_revision: Option<String> =
                sqlx::query_scalar("SELECT value FROM runtimeMetadata WHERE key = ?")
                    .bind(RUNTIME_SETTINGS_REVISION_KEY)
                    .fetch_optional(&mut *transaction)
                    .await
                    .map_err(state_error)?;
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
                let encoded = serde_json::to_string(value).map_err(state_error)?;
                write_runtime_metadata(
                    &mut *transaction,
                    "settings.agents.agentStatusHooks",
                    &encoded,
                    &updated_at,
                )
                .await?;
            }
            if let Some(value) = &update.agent_quotas {
                let encoded =
                    serde_json::to_string(&value.clone().normalized()).map_err(state_error)?;
                write_runtime_metadata(
                    &mut *transaction,
                    "settings.agents.quotas",
                    &encoded,
                    &updated_at,
                )
                .await?;
            }
            if let Some(value) = &update.mobile_push_notifications {
                let encoded = serde_json::to_string(value).map_err(state_error)?;
                write_runtime_metadata(
                    &mut *transaction,
                    "settings.mobile.pushNotifications",
                    &encoded,
                    &updated_at,
                )
                .await?;
            }
            if let Some(value) = &update.ai_assist {
                let encoded =
                    serde_json::to_string(&value.clone().normalized()).map_err(state_error)?;
                write_runtime_metadata(
                    &mut *transaction,
                    "settings.aiTextGeneration",
                    &encoded,
                    &updated_at,
                )
                .await?;
            }
            if let Some(value) = &update.text_actions {
                let encoded =
                    serde_json::to_string(&value.clone().normalized()).map_err(state_error)?;
                write_runtime_metadata(
                    &mut *transaction,
                    "settings.textActions",
                    &encoded,
                    &updated_at,
                )
                .await?;
            }
            if let Some(value) = &update.automation {
                let encoded = serde_json::to_string(value).map_err(state_error)?;
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
                transaction.commit().await.map_err(state_error)?;
                Ok(revision)
            }
            Err(error) => {
                let _ = transaction.rollback().await;
                Err(error)
            }
        }
    }

    async fn revision(&self) -> HostResult<i64> {
        let raw = self
            .runtime_store
            .get_metadata(RUNTIME_SETTINGS_REVISION_KEY)
            .await
            .map_err(state_error)?;
        parse_stored_runtime_settings_revision(raw.as_deref())
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
    .map_err(state_error)?;
    Ok(())
}

async fn delete_runtime_metadata(connection: &mut SqliteConnection, key: &str) -> HostResult<()> {
    sqlx::query("DELETE FROM runtimeMetadata WHERE key = ?")
        .bind(key)
        .execute(&mut *connection)
        .await
        .map_err(state_error)?;
    Ok(())
}

fn runtime_value<T, E>(result: Result<T, E>) -> HostResult<Value>
where
    T: Serialize,
    E: std::fmt::Display,
{
    let value = result.map_err(state_error)?;
    serde_json::to_value(value).map_err(state_error)
}

fn state_error(error: impl std::fmt::Display) -> HostError {
    HostError::state(error.to_string())
}

#[cfg(test)]
mod tests {
    use alera_core::runtime::RuntimeStore;
    use serde_json::json;

    use super::super::runtime_settings_validation::parse_runtime_settings_update;
    use super::RuntimeSettingsStoreHandler;

    #[tokio::test]
    async fn snapshot_commit_conflict_and_lww_do_not_need_server_actor() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        let handler = RuntimeSettingsStoreHandler::new(&store);

        assert_eq!(handler.snapshot().await.unwrap()["revision"], 0);

        let first =
            parse_runtime_settings_update(&json!({"confirmProjectRemoval": false})).unwrap();
        assert_eq!(handler.commit(&first).await.unwrap(), 1);
        let first_snapshot = handler.snapshot().await.unwrap();
        assert_eq!(first_snapshot["revision"], 1);
        assert_eq!(first_snapshot["confirmProjectRemoval"], false);

        let stale = parse_runtime_settings_update(
            &json!({"confirmProjectRemoval": true, "expectedRevision": 0}),
        )
        .unwrap();
        let conflict = handler.commit(&stale).await.unwrap_err();
        assert_eq!(
            conflict.error_code(),
            Some("runtime_settings_revision_conflict")
        );
        assert!(!store.confirm_project_removal().await.unwrap());

        let current = parse_runtime_settings_update(
            &json!({"confirmProjectRemoval": true, "expectedRevision": 1}),
        )
        .unwrap();
        assert_eq!(handler.commit(&current).await.unwrap(), 2);

        let lww = parse_runtime_settings_update(&json!({"confirmProjectRemoval": false})).unwrap();
        assert_eq!(handler.commit(&lww).await.unwrap(), 3);
        let final_snapshot = handler.snapshot().await.unwrap();
        assert_eq!(final_snapshot["revision"], 3);
        assert_eq!(final_snapshot["confirmProjectRemoval"], false);
    }
}
