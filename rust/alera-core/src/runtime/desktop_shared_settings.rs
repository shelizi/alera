//! One small, reviewed desktop Settings projection shared by Classic and GPUI.
//! Local-only fields live in RuntimeStore metadata; portable fields remain in
//! the existing configuration document. The Host/CLI are the only UI writers.
//! Every edit is a single SQLite BEGIN IMMEDIATE compare-and-set transaction.
use super::RuntimeStore;
use crate::desktop_settings_contract::SHARED_DESKTOP_PATHS;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Map, Value};

const LOCAL_KEY: &str = "settings.desktop.local.v1";
const PORTABLE_KEY: &str = "configuration.portable.v1";
const RUNTIME_REVISION_KEY: &str = "settings.runtimeSettingsRevision";
const NATIVE_PROFILE_MIGRATION_KEY: &str = "settings.desktop.nativeProfileMigration.v1";
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopSettingChange {
    pub path: String,
    #[serde(rename = "expectedValue")]
    pub expected_value: Value,
    pub value: Value,
}

fn local_path(path: &str) -> bool {
    matches!(
        path,
        "general.language"
            | "general.autoGitFetchEnabled"
            | "general.autoGitFetchIntervalMinutes"
            | "explorer.showHiddenFiles"
            | "explorer.showIgnoredFiles"
    )
}

fn runtime_path(path: &str) -> Option<&'static str> {
    match path {
        "general.confirmProjectRemoval" => Some("settings.general.confirmProjectRemoval"),
        "general.confirmWorkspaceRemoval" => Some("settings.general.confirmWorkspaceRemoval"),
        "general.autoArchiveWorkspacesAfterDays" => {
            Some("settings.general.autoArchiveWorkspacesAfterDays")
        }
        "agents.defaultAgentProfileId" => Some("settings.agents.defaultAgentProfileId"),
        _ => None,
    }
}

fn default_value(path: &str) -> Result<Value> {
    Ok(match path {
        "general.language" => json!("system"),
        "general.autoGitFetchEnabled" => json!(true),
        "general.autoGitFetchIntervalMinutes" => json!(5),
        "terminal.fontSize" => json!(13.0),
        "editor.autosaveEnabled" => json!(false),
        "editor.tabSize" => json!(4),
        "general.confirmProjectRemoval" => json!(true),
        "general.confirmWorkspaceRemoval" => json!(true),
        "general.autoArchiveWorkspacesAfterDays" => json!(30),
        "agents.defaultAgentProfileId" => Value::Null,
        "explorer.showHiddenFiles" => json!(false),
        "explorer.showIgnoredFiles" => json!(false),
        _ => bail!("Unsupported shared desktop setting path"),
    })
}

fn validate(path: &str, value: &Value) -> Result<()> {
    let valid = match path {
        "general.language" => value.as_str().is_some_and(|s| {
            matches!(
                s,
                "system" | "english" | "traditionalChinese" | "simplifiedChinese" | "japanese"
            )
        }),
        "general.autoGitFetchEnabled"
        | "editor.autosaveEnabled"
        | "explorer.showHiddenFiles"
        | "explorer.showIgnoredFiles"
        | "general.confirmProjectRemoval"
        | "general.confirmWorkspaceRemoval" => value.is_boolean(),
        "general.autoArchiveWorkspacesAfterDays" => value.as_u64().is_some_and(|n| n <= 36_500),
        "agents.defaultAgentProfileId" => {
            value.is_null()
                || value.as_str().is_some_and(|id| {
                    !id.is_empty()
                        && id.len() <= 128
                        && id.bytes().all(|ch| {
                            ch.is_ascii_alphanumeric() || matches!(ch, b'-' | b'_' | b'.' | b':')
                        })
                })
        }
        "general.autoGitFetchIntervalMinutes" => {
            value.as_u64().is_some_and(|n| (1..=60).contains(&n))
        }
        "terminal.fontSize" => value
            .as_f64()
            .is_some_and(|n| n.is_finite() && (8.0..=48.0).contains(&n)),
        "editor.tabSize" => value.as_u64().is_some_and(|n| (1..=16).contains(&n)),
        _ => false,
    };
    if !valid {
        bail!("Unsupported or invalid shared desktop setting");
    }
    Ok(())
}

fn value_at(data: &Value, path: &str) -> Result<(Value, bool)> {
    let mut parts = path.split('.');
    let section = parts.next().unwrap_or_default();
    let field = parts.next().unwrap_or_default();
    if parts.next().is_some() {
        bail!("Invalid shared settings path");
    }
    let current = data.get(section).and_then(|s| s.get(field));
    let value = current.cloned().unwrap_or(default_value(path)?);
    validate(path, &value)?;
    Ok((value, current.is_some()))
}

fn put_at(data: &mut Value, path: &str, value: Value) -> Result<()> {
    validate(path, &value)?;
    let (section, field) = path
        .split_once('.')
        .context("Invalid shared settings path")?;
    if !data.get(section).is_some_and(Value::is_object) {
        data[section] = json!({});
    }
    data[section][field] = value;
    Ok(())
}

fn parse_document(raw: Option<String>, portable: bool) -> Result<Value> {
    let document = match raw {
        Some(raw) => serde_json::from_str::<Value>(&raw)
            .context("Stored desktop settings JSON is invalid")?,
        None if portable => {
            json!({"schemaVersion":1,"shared":{},"desktop":{"settings":{}},"mobile":{}})
        }
        None => json!({}),
    };
    if !document.is_object()
        || (portable
            && !document
                .pointer("/desktop/settings")
                .is_some_and(Value::is_object))
    {
        bail!("Stored desktop settings document is not an object");
    }
    Ok(document)
}

fn settings_data<'a>(document: &'a Value, portable: bool) -> &'a Value {
    if portable {
        &document["desktop"]["settings"]
    } else {
        document
    }
}

fn settings_data_mut<'a>(document: &'a mut Value, portable: bool) -> &'a mut Value {
    if portable {
        &mut document["desktop"]["settings"]
    } else {
        document
    }
}

fn equal_value(path: &str, a: &Value, b: &Value) -> bool {
    if path == "terminal.fontSize" {
        a.as_f64() == b.as_f64()
    } else {
        a == b
    }
}

async fn read_raw(tx: &mut sqlx::SqliteConnection, key: &str) -> Result<Option<String>> {
    Ok(
        sqlx::query_scalar("SELECT value FROM runtimeMetadata WHERE key = ?")
            .bind(key)
            .fetch_optional(&mut *tx)
            .await?,
    )
}

async fn write_raw(tx: &mut sqlx::SqliteConnection, key: &str, value: Option<&str>) -> Result<()> {
    match value {
        Some(value) => {
            sqlx::query("INSERT INTO runtimeMetadata (key,value,updatedAt) VALUES (?,?,strftime('%Y-%m-%dT%H:%M:%fZ','now')) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updatedAt=excluded.updatedAt")
                .bind(key).bind(value).execute(&mut *tx).await?;
        }
        None => {
            sqlx::query("DELETE FROM runtimeMetadata WHERE key = ?")
                .bind(key)
                .execute(&mut *tx)
                .await?;
        }
    }
    Ok(())
}

fn runtime_value(path: &str, raw: Option<String>) -> Result<(Value, bool)> {
    let exists = raw.is_some();
    let value = match raw {
        None => default_value(path)?,
        Some(value) if path == "agents.defaultAgentProfileId" => json!(value),
        Some(value) => {
            serde_json::from_str(&value).context("Invalid existing Rust runtime setting")?
        }
    };
    validate(path, &value)?;
    Ok((value, exists))
}

async fn read_setting_in(tx: &mut sqlx::SqliteConnection, path: &str) -> Result<(Value, bool)> {
    if let Some(key) = runtime_path(path) {
        return runtime_value(path, read_raw(tx, key).await?);
    }
    let portable = !local_path(path);
    let key = if portable { PORTABLE_KEY } else { LOCAL_KEY };
    let document = parse_document(read_raw(tx, key).await?, portable)?;
    value_at(settings_data(&document, portable), path)
}

async fn snapshot_in(tx: &mut sqlx::SqliteConnection) -> Result<Value> {
    let mut values = Map::new();
    let mut present = Map::new();
    for path in SHARED_DESKTOP_PATHS {
        let (value, exists) = read_setting_in(tx, path).await?;
        values.insert(path.to_owned(), value);
        present.insert(path.to_owned(), json!(exists));
    }
    Ok(json!({"values": values, "present": present}))
}

impl RuntimeStore {
    pub async fn desktop_shared_setting(&self, path: &str) -> Result<Value> {
        validate(path, &default_value(path)?)?;
        let mut tx = self.pool().begin().await?;
        let value = read_setting_in(&mut tx, path).await?.0;
        tx.commit().await?;
        Ok(value)
    }

    /// A narrow, secret-free view. Never returns executable paths, tokens or
    /// unrelated portable/cloud configuration settings.
    pub async fn desktop_shared_settings_snapshot(&self) -> Result<Value> {
        let mut tx = self.pool().begin().await?;
        let snapshot = snapshot_in(&mut tx).await?;
        tx.commit().await?;
        Ok(snapshot)
    }

    /// Import only the three Classic local-only values on first contact.
    /// If GPUI already saved a value, late Classic migration never replaces it.
    pub async fn desktop_shared_seed_local(&self, fields: &Value) -> Result<Value> {
        let fields = fields.as_object().context("Local seed must be an object")?;
        if fields.len() > 5 {
            bail!("Too many local desktop settings");
        }
        for (path, value) in fields {
            if !local_path(path) {
                bail!("Local seed does not allow this field");
            }
            validate(path, value)?;
        }
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let raw: Option<String> =
            sqlx::query_scalar("SELECT value FROM runtimeMetadata WHERE key = ?")
                .bind(LOCAL_KEY)
                .fetch_optional(&mut *tx)
                .await?;
        let mut doc = parse_document(raw, false)?;
        let mut modified = false;
        for (path, value) in fields {
            if !value_at(&doc, path)?.1 {
                put_at(&mut doc, path, value.clone())?;
                modified = true;
            }
        }
        if modified {
            sqlx::query("INSERT INTO runtimeMetadata (key,value,updatedAt) VALUES (?,?,strftime('%Y-%m-%dT%H:%M:%fZ','now')) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updatedAt=excluded.updatedAt")
                .bind(LOCAL_KEY).bind(doc.to_string()).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        self.desktop_shared_settings_snapshot().await
    }

    /// A one-field convenience API. The batch implementation below is also
    /// used by Classic so a multi-field edit is all-or-nothing.
    pub async fn desktop_shared_compare_and_set(
        &self,
        path: &str,
        expected: &Value,
        next: &Value,
    ) -> Result<Value> {
        self.desktop_shared_compare_and_set_batch(&[DesktopSettingChange {
            path: path.to_owned(),
            expected_value: expected.clone(),
            value: next.clone(),
        }])
        .await
    }

    /// Mutate reviewed Classic/GPUI settings in one SQLite transaction.
    /// Validate and compare *all* values before applying any change, and
    /// preserve every unrelated local, portable, or Host runtime field.
    pub async fn desktop_shared_compare_and_set_batch(
        &self,
        changes: &[DesktopSettingChange],
    ) -> Result<Value> {
        if changes.is_empty() || changes.len() > SHARED_DESKTOP_PATHS.len() {
            bail!("Invalid shared settings batch size");
        }
        let mut paths = std::collections::HashSet::new();
        for change in changes {
            if !paths.insert(change.path.as_str()) {
                bail!("Duplicate shared desktop setting path");
            }
            validate(&change.path, &change.expected_value)?;
            validate(&change.path, &change.value)?;
        }
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let mut local = parse_document(read_raw(&mut tx, LOCAL_KEY).await?, false)?;
        let mut portable = parse_document(read_raw(&mut tx, PORTABLE_KEY).await?, true)?;
        let mut local_changed = false;
        let mut portable_changed = false;
        let mut runtime_changes: Vec<(&str, Option<String>)> = Vec::new();
        // First pass compares every field before a single mutation is made.
        for change in changes {
            let path = change.path.as_str();
            let current = if let Some(key) = runtime_path(path) {
                runtime_value(path, read_raw(&mut tx, key).await?)?.0
            } else if local_path(path) {
                value_at(&local, path)?.0
            } else {
                value_at(settings_data(&portable, true), path)?.0
            };
            if !equal_value(path, &current, &change.expected_value) {
                bail!("desktop_settings_revision_conflict");
            }
        }
        for change in changes {
            let path = change.path.as_str();
            if equal_value(path, &change.expected_value, &change.value) {
                continue;
            }
            if let Some(key) = runtime_path(path) {
                let encoded = if change.value.is_null() {
                    None
                } else if let Some(id) = change.value.as_str() {
                    Some(id.to_owned())
                } else {
                    Some(change.value.to_string())
                };
                runtime_changes.push((key, encoded));
            } else if local_path(path) {
                put_at(&mut local, path, change.value.clone())?;
                local_changed = true;
            } else {
                put_at(
                    settings_data_mut(&mut portable, true),
                    path,
                    change.value.clone(),
                )?;
                portable_changed = true;
            }
        }
        if portable_changed {
            super::configuration_validation::validate_settings(&portable["desktop"]["settings"])?;
            write_raw(&mut tx, PORTABLE_KEY, Some(&portable.to_string())).await?;
        }
        if local_changed {
            write_raw(&mut tx, LOCAL_KEY, Some(&local.to_string())).await?;
        }
        let update_runtime_revision = !runtime_changes.is_empty();
        for (key, encoded) in runtime_changes {
            write_raw(&mut tx, key, encoded.as_deref()).await?;
        }
        if update_runtime_revision {
            // Keep Host's normal runtimeSettings revision conflict guard
            // coherent with this alternate, field-scoped mutation path.
            let old = read_raw(&mut tx, RUNTIME_REVISION_KEY).await?;
            let revision = match old {
                Some(raw) => raw
                    .parse::<i64>()
                    .ok()
                    .filter(|v| *v >= 0)
                    .context("Invalid stored Runtime settings revision")?,
                None => 0,
            };
            write_raw(
                &mut tx,
                RUNTIME_REVISION_KEY,
                Some(&(revision.saturating_add(1)).to_string()),
            )
            .await?;
        }
        tx.commit().await?;
        self.desktop_shared_settings_snapshot().await
    }

    /// Import the pre-Rust GPUI default Agent Profile once, and only when it
    /// names an existing Host profile and the Host has no explicit selection.
    /// A tombstone prevents a later GPUI restart from reviving a cleared
    /// Classic default. The old Native JSON remains as rollback evidence.
    pub async fn desktop_shared_seed_native_default_profile(
        &self,
        profile_id: Option<&str>,
    ) -> Result<Value> {
        if let Some(id) = profile_id {
            validate("agents.defaultAgentProfileId", &json!(id))?;
        }
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let completed = read_raw(&mut tx, NATIVE_PROFILE_MIGRATION_KEY).await?;
        if completed.is_none() {
            let host_selected = read_raw(&mut tx, "settings.agents.defaultAgentProfileId").await?;
            if host_selected.is_none() {
                if let Some(id) = profile_id {
                    let exists: Option<i32> =
                        sqlx::query_scalar("SELECT 1 FROM agentProfiles WHERE id = ? LIMIT 1")
                            .bind(id)
                            .fetch_optional(&mut *tx)
                            .await?;
                    if exists.is_some() {
                        write_raw(&mut tx, "settings.agents.defaultAgentProfileId", Some(id))
                            .await?;
                    }
                }
            }
            write_raw(&mut tx, NATIVE_PROFILE_MIGRATION_KEY, Some("true")).await?;
        }
        tx.commit().await?;
        self.desktop_shared_settings_snapshot().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(path: &str, expected: Value, value: Value) -> DesktopSettingChange {
        DesktopSettingChange {
            path: path.into(),
            expected_value: expected,
            value,
        }
    }

    async fn create_profile(store: &RuntimeStore, id: &str) {
        let now = chrono::Utc::now();
        store
            .upsert_agent_profile(
                super::super::AgentProfile {
                    id: id.into(),
                    name: format!("Shared test {id}"),
                    sort_order: 0,
                    agent_type: "codex".into(),
                    command: "codex".into(),
                    launch_mode: super::super::AgentProfileLaunchMode::Command,
                    managed_config: None,
                    custom_prompt: String::new(),
                    description: String::new(),
                    quota_group: None,
                    revision: 0,
                    created_at: now,
                    updated_at: now,
                },
                None,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn independent_classic_and_gpui_stores_share_explorer_preferences_and_restart() {
        let dir = tempfile::tempdir().unwrap();
        let classic = RuntimeStore::open(dir.path()).await.unwrap();
        let native = RuntimeStore::open(dir.path()).await.unwrap();
        assert_eq!(
            classic
                .desktop_shared_setting("explorer.showHiddenFiles")
                .await
                .unwrap(),
            false
        );
        let classic_seed = classic
            .desktop_shared_seed_local(&json!({
                "explorer.showHiddenFiles": true,
                "explorer.showIgnoredFiles": false,
            }))
            .await
            .unwrap();
        assert_eq!(classic_seed["values"]["explorer.showHiddenFiles"], true);
        // Legacy GPUI startup arrives late; Classic's explicit choice wins.
        native
            .desktop_shared_seed_local(&json!({
                "explorer.showHiddenFiles": false,
                "explorer.showIgnoredFiles": true,
            }))
            .await
            .unwrap();
        assert_eq!(
            native
                .desktop_shared_setting("explorer.showHiddenFiles")
                .await
                .unwrap(),
            true
        );
        assert_eq!(
            native
                .desktop_shared_setting("explorer.showIgnoredFiles")
                .await
                .unwrap(),
            false
        );
        native
            .desktop_shared_compare_and_set(
                "explorer.showIgnoredFiles",
                &json!(false),
                &json!(true),
            )
            .await
            .unwrap();
        assert_eq!(
            classic
                .desktop_shared_setting("explorer.showIgnoredFiles")
                .await
                .unwrap(),
            true
        );
        assert!(classic
            .desktop_shared_compare_and_set(
                "explorer.showIgnoredFiles",
                &json!(false),
                &json!(false)
            )
            .await
            .is_err());
        let restart = RuntimeStore::open(dir.path()).await.unwrap();
        assert_eq!(
            restart
                .desktop_shared_setting("explorer.showHiddenFiles")
                .await
                .unwrap(),
            true
        );
        assert_eq!(
            restart
                .desktop_shared_setting("explorer.showIgnoredFiles")
                .await
                .unwrap(),
            true
        );
    }

    #[tokio::test]
    async fn existing_native_default_profile_is_migrated_once_and_cannot_reappear_after_clear() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        create_profile(&store, "native_one").await;
        create_profile(&store, "native_two").await;
        store
            .desktop_shared_seed_native_default_profile(Some("native_one"))
            .await
            .unwrap();
        assert_eq!(
            store
                .desktop_shared_setting("agents.defaultAgentProfileId")
                .await
                .unwrap(),
            "native_one"
        );
        store
            .desktop_shared_compare_and_set(
                "agents.defaultAgentProfileId",
                &json!("native_one"),
                &Value::Null,
            )
            .await
            .unwrap();
        store
            .desktop_shared_seed_native_default_profile(Some("native_two"))
            .await
            .unwrap();
        assert_eq!(
            store
                .desktop_shared_setting("agents.defaultAgentProfileId")
                .await
                .unwrap(),
            Value::Null
        );
        assert_eq!(
            RuntimeStore::open(dir.path())
                .await
                .unwrap()
                .desktop_shared_setting("agents.defaultAgentProfileId")
                .await
                .unwrap(),
            Value::Null
        );
    }

    #[tokio::test]
    async fn shared_runtime_safety_and_profile_are_authoritative_and_survive_restart() {
        let temp = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(temp.path()).await.unwrap();
        create_profile(&store, "prof_alpha").await;
        let initial = store.desktop_shared_settings_snapshot().await.unwrap();
        assert_eq!(initial["values"]["general.confirmProjectRemoval"], true);
        assert_eq!(initial["values"]["general.confirmWorkspaceRemoval"], true);
        assert_eq!(
            initial["values"]["general.autoArchiveWorkspacesAfterDays"],
            30
        );
        assert!(initial["values"]["agents.defaultAgentProfileId"].is_null());
        store
            .desktop_shared_compare_and_set_batch(&[
                change("general.confirmProjectRemoval", json!(true), json!(false)),
                change("general.confirmWorkspaceRemoval", json!(true), json!(false)),
                change(
                    "general.autoArchiveWorkspacesAfterDays",
                    json!(30),
                    json!(7),
                ),
                change(
                    "agents.defaultAgentProfileId",
                    Value::Null,
                    json!("prof_alpha"),
                ),
            ])
            .await
            .unwrap();
        let runtime = store.runtime_settings().await.unwrap();
        assert!(!runtime.confirm_project_removal);
        assert!(!runtime.confirm_workspace_removal);
        assert_eq!(runtime.auto_archive_workspaces_after_days, 7);
        assert_eq!(
            runtime.default_agent_profile_id.as_deref(),
            Some("prof_alpha")
        );
        assert_eq!(
            store.configuration_settings().await.unwrap()["general"]["confirmProjectRemoval"],
            false
        );
        assert_eq!(
            store
                .desktop_shared_setting("agents.defaultAgentProfileId")
                .await
                .unwrap(),
            "prof_alpha"
        );
        assert_eq!(
            store
                .get_metadata(RUNTIME_REVISION_KEY)
                .await
                .unwrap()
                .as_deref(),
            Some("1")
        );
        let reopened = RuntimeStore::open(temp.path()).await.unwrap();
        assert_eq!(
            reopened
                .desktop_shared_setting("general.autoArchiveWorkspacesAfterDays")
                .await
                .unwrap(),
            7
        );
        assert_eq!(
            reopened
                .desktop_shared_setting("agents.defaultAgentProfileId")
                .await
                .unwrap(),
            "prof_alpha"
        );
        reopened
            .desktop_shared_compare_and_set(
                "agents.defaultAgentProfileId",
                &json!("prof_alpha"),
                &Value::Null,
            )
            .await
            .unwrap();
        assert_eq!(
            reopened
                .runtime_settings()
                .await
                .unwrap()
                .default_agent_profile_id,
            None
        );
    }

    #[tokio::test]
    async fn mixed_local_portable_and_runtime_batch_is_atomic_on_stale_or_invalid_input() {
        let temp = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(temp.path()).await.unwrap();
        store
            .desktop_shared_compare_and_set_batch(&[
                change(
                    "general.language",
                    json!("system"),
                    json!("traditionalChinese"),
                ),
                change("editor.tabSize", json!(4), json!(8)),
                change("general.confirmWorkspaceRemoval", json!(true), json!(false)),
            ])
            .await
            .unwrap();
        let stale = store
            .desktop_shared_compare_and_set_batch(&[
                change(
                    "general.language",
                    json!("traditionalChinese"),
                    json!("english"),
                ),
                change("editor.tabSize", json!(4), json!(2)),
            ])
            .await;
        assert!(stale.is_err());
        assert_eq!(
            store
                .desktop_shared_setting("general.language")
                .await
                .unwrap(),
            "traditionalChinese"
        );
        assert_eq!(
            store
                .desktop_shared_setting("editor.tabSize")
                .await
                .unwrap(),
            8
        );
        assert_eq!(
            store
                .desktop_shared_setting("general.confirmWorkspaceRemoval")
                .await
                .unwrap(),
            false
        );
        assert!(store
            .desktop_shared_compare_and_set_batch(&[
                change("editor.tabSize", json!(8), json!(6)),
                change(
                    "agents.defaultAgentProfileId",
                    Value::Null,
                    json!("../unsafe")
                ),
            ])
            .await
            .is_err());
        assert_eq!(
            store
                .desktop_shared_setting("editor.tabSize")
                .await
                .unwrap(),
            8
        );
        assert!(store
            .desktop_shared_compare_and_set_batch(&[
                change("editor.tabSize", json!(8), json!(6)),
                change("editor.tabSize", json!(8), json!(7)),
            ])
            .await
            .is_err());
        assert!(store
            .desktop_shared_compare_and_set("agents.apiKey", &Value::Null, &json!("key"))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn native_profile_seed_is_one_time_and_does_not_resurrect_cleared_defaults() {
        let temp = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(temp.path()).await.unwrap();
        create_profile(&store, "good_profile").await;
        assert!(store
            .desktop_shared_seed_native_default_profile(Some("../escape"))
            .await
            .is_err());
        assert!(store
            .desktop_shared_seed_native_default_profile(Some("missing_profile"))
            .await
            .is_ok());
        assert!(store
            .desktop_shared_setting("agents.defaultAgentProfileId")
            .await
            .unwrap()
            .is_null());
        store
            .desktop_shared_compare_and_set(
                "agents.defaultAgentProfileId",
                &Value::Null,
                &json!("good_profile"),
            )
            .await
            .unwrap();
        store
            .desktop_shared_compare_and_set(
                "agents.defaultAgentProfileId",
                &json!("good_profile"),
                &Value::Null,
            )
            .await
            .unwrap();
        store
            .desktop_shared_seed_native_default_profile(Some("good_profile"))
            .await
            .unwrap();
        assert!(store
            .desktop_shared_setting("agents.defaultAgentProfileId")
            .await
            .unwrap()
            .is_null());
    }

    #[tokio::test]
    async fn shared_settings_seed_and_cas_preserve_classic_and_native_values() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        let start = store.desktop_shared_settings_snapshot().await.unwrap();
        assert_eq!(start["values"]["general.language"], "system");
        assert_eq!(start["present"]["general.language"], false);
        assert_eq!(start["values"]["terminal.fontSize"], json!(13.0));
        store
            .desktop_shared_seed_local(&json!({
                "general.language":"traditionalChinese",
                "general.autoGitFetchEnabled":false,
                "general.autoGitFetchIntervalMinutes":7,
            }))
            .await
            .unwrap();
        assert_eq!(
            store
                .desktop_shared_setting("general.language")
                .await
                .unwrap(),
            "traditionalChinese"
        );
        store
            .desktop_shared_compare_and_set(
                "general.language",
                &json!("traditionalChinese"),
                &json!("japanese"),
            )
            .await
            .unwrap();
        // A delayed Classic first-run import must never undo GPUI's edit.
        store
            .desktop_shared_seed_local(&json!({"general.language":"english"}))
            .await
            .unwrap();
        assert_eq!(
            store
                .desktop_shared_setting("general.language")
                .await
                .unwrap(),
            "japanese"
        );
        assert!(store
            .desktop_shared_compare_and_set("general.language", &json!("english"), &json!("system"))
            .await
            .is_err());
        assert_eq!(
            store
                .desktop_shared_setting("general.autoGitFetchIntervalMinutes")
                .await
                .unwrap(),
            7
        );
        let reopen = RuntimeStore::open(dir.path()).await.unwrap();
        assert_eq!(
            reopen
                .desktop_shared_setting("general.language")
                .await
                .unwrap(),
            "japanese"
        );
    }

    #[tokio::test]
    async fn portable_fields_update_existing_configuration_without_erasing_others() {
        let dir = tempfile::tempdir().unwrap();
        let store = RuntimeStore::open(dir.path()).await.unwrap();
        store.configuration_update_settings(json!({"terminal":{"fontFamily":"TestMono","fontSize":17.0},"editor":{"tabSize":8,"themeName":"Alera"}})).await.unwrap();
        store
            .desktop_shared_compare_and_set("terminal.fontSize", &json!(17), &json!(20.0))
            .await
            .unwrap();
        assert_eq!(
            store.configuration_settings().await.unwrap()["terminal"]["fontFamily"],
            "TestMono"
        );
        assert_eq!(
            store.configuration_settings().await.unwrap()["editor"]["themeName"],
            "Alera"
        );
        assert_eq!(
            store
                .desktop_shared_setting("editor.tabSize")
                .await
                .unwrap(),
            8
        );
        assert!(store
            .desktop_shared_compare_and_set("editor.tabSize", &json!(4), &json!(2))
            .await
            .is_err());
        assert!(store
            .desktop_shared_compare_and_set("terminal.fontSize", &json!(20), &json!(10000))
            .await
            .is_err());
        assert!(store
            .desktop_shared_seed_local(&json!({"ai.provider.apiKey":"xxx"}))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn independent_writers_do_not_lose_updates() {
        let dir = tempfile::tempdir().unwrap();
        let first = RuntimeStore::open(dir.path()).await.unwrap();
        let second = RuntimeStore::open(dir.path()).await.unwrap();
        first
            .desktop_shared_compare_and_set("editor.tabSize", &json!(4), &json!(5))
            .await
            .unwrap();
        second
            .desktop_shared_compare_and_set("editor.autosaveEnabled", &json!(false), &json!(true))
            .await
            .unwrap();
        assert_eq!(
            first
                .desktop_shared_setting("editor.tabSize")
                .await
                .unwrap(),
            5
        );
        assert_eq!(
            first
                .desktop_shared_setting("editor.autosaveEnabled")
                .await
                .unwrap(),
            true
        );
        assert!(second
            .desktop_shared_compare_and_set("editor.tabSize", &json!(4), &json!(6))
            .await
            .is_err());
    }
}
