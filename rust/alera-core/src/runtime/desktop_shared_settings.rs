//! One small, reviewed desktop Settings projection shared by Classic and GPUI.
//! Local-only fields live in RuntimeStore metadata; portable fields remain in
//! the existing configuration document. The Host/CLI are the only UI writers.
//! Every edit is a single SQLite BEGIN IMMEDIATE compare-and-set transaction.
use super::RuntimeStore;
use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};

const LOCAL_KEY: &str = "settings.desktop.local.v1";
const PORTABLE_KEY: &str = "configuration.portable.v1";
pub const SHARED_DESKTOP_PATHS: [&str; 6] = [
    "general.language",
    "general.autoGitFetchEnabled",
    "general.autoGitFetchIntervalMinutes",
    "terminal.fontSize",
    "editor.autosaveEnabled",
    "editor.tabSize",
];

fn local_path(path: &str) -> bool {
    matches!(
        path,
        "general.language" | "general.autoGitFetchEnabled" | "general.autoGitFetchIntervalMinutes"
    )
}

fn default_value(path: &str) -> Result<Value> {
    Ok(match path {
        "general.language" => json!("system"),
        "general.autoGitFetchEnabled" => json!(true),
        "general.autoGitFetchIntervalMinutes" => json!(5),
        "terminal.fontSize" => json!(13.0),
        "editor.autosaveEnabled" => json!(false),
        "editor.tabSize" => json!(4),
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
        "general.autoGitFetchEnabled" | "editor.autosaveEnabled" => value.is_boolean(),
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

impl RuntimeStore {
    pub async fn desktop_shared_setting(&self, path: &str) -> Result<Value> {
        validate(path, &default_value(path)?)?;
        let portable = !local_path(path);
        let key = if portable { PORTABLE_KEY } else { LOCAL_KEY };
        let raw = self.get_metadata(key).await?;
        let doc = parse_document(raw, portable)?;
        Ok(value_at(settings_data(&doc, portable), path)?.0)
    }

    /// A narrow, secret-free view. Never returns executable paths, tokens or
    /// unrelated portable/cloud configuration settings.
    pub async fn desktop_shared_settings_snapshot(&self) -> Result<Value> {
        let mut values = Map::new();
        let mut present = Map::new();
        for path in SHARED_DESKTOP_PATHS {
            let portable = !local_path(path);
            let key = if portable { PORTABLE_KEY } else { LOCAL_KEY };
            let doc = parse_document(self.get_metadata(key).await?, portable)?;
            let (value, exists) = value_at(settings_data(&doc, portable), path)?;
            values.insert(path.to_owned(), value);
            present.insert(path.to_owned(), json!(exists));
        }
        Ok(json!({"values": values, "present": present}))
    }

    /// Import only the three Classic local-only values on first contact.
    /// If GPUI already saved a value, late Classic migration never replaces it.
    pub async fn desktop_shared_seed_local(&self, fields: &Value) -> Result<Value> {
        let fields = fields.as_object().context("Local seed must be an object")?;
        if fields.len() > 3 {
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

    /// Compare at the single field; preserve every other setting unchanged.
    /// A stale Classic/GPUI window cannot silently win by writing a whole doc.
    pub async fn desktop_shared_compare_and_set(
        &self,
        path: &str,
        expected: &Value,
        next: &Value,
    ) -> Result<Value> {
        validate(path, expected)?;
        validate(path, next)?;
        let portable = !local_path(path);
        let key = if portable { PORTABLE_KEY } else { LOCAL_KEY };
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let raw: Option<String> =
            sqlx::query_scalar("SELECT value FROM runtimeMetadata WHERE key = ?")
                .bind(key)
                .fetch_optional(&mut *tx)
                .await?;
        let mut doc = parse_document(raw, portable)?;
        let current = value_at(settings_data(&doc, portable), path)?.0;
        if !equal_value(path, &current, expected) {
            bail!("desktop_settings_revision_conflict");
        }
        if !equal_value(path, &current, next) {
            put_at(settings_data_mut(&mut doc, portable), path, next.clone())?;
            if portable {
                super::configuration_validation::validate_settings(&doc["desktop"]["settings"])?;
            }
            sqlx::query("INSERT INTO runtimeMetadata (key,value,updatedAt) VALUES (?,?,strftime('%Y-%m-%dT%H:%M:%fZ','now')) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updatedAt=excluded.updatedAt")
                .bind(key).bind(doc.to_string()).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        self.desktop_shared_settings_snapshot().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
