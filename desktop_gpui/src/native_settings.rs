//! Native-only settings owner. Never reads or mutates Flutter's preferences.
//! The Host permits only these scalar MCP paths; storage uses a separate
//! versioned JSON file under AleraNative (or an isolated smoke-test state).
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const VERSION: u32 = 1;
const MAX_BYTES: u64 = 16 * 1024;
pub const SETTINGS_FILENAME: &str = "native-settings-v1.json";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeSettings {
    version: u32,
    pub auto_git_fetch_enabled: bool,
    pub auto_git_fetch_interval_minutes: u64,
    pub terminal_font_size: f64,
    pub editor_autosave_enabled: bool,
    pub editor_tab_size: u64,
    // These three safety controls cannot be changed by an Agent.
    pub confirm_project_removal: bool,
    pub confirm_workspace_removal: bool,
    pub auto_archive_workspaces_after_days: u64,
}

impl Default for NativeSettings {
    fn default() -> Self {
        Self {
            version: VERSION,
            // Match Alera Classic's first-run default while keeping storage
            // and scheduling completely independent between the editions.
            auto_git_fetch_enabled: true,
            auto_git_fetch_interval_minutes: 5,
            terminal_font_size: 13.0,
            editor_autosave_enabled: false,
            editor_tab_size: 4,
            confirm_project_removal: true,
            confirm_workspace_removal: true,
            auto_archive_workspaces_after_days: 0,
        }
    }
}

impl NativeSettings {
    pub fn get(&self, path: &str) -> Option<Value> {
        Some(match path {
            "general.autoGitFetchEnabled" => json!(self.auto_git_fetch_enabled),
            "general.autoGitFetchIntervalMinutes" => json!(self.auto_git_fetch_interval_minutes),
            "terminal.fontSize" => json!(self.terminal_font_size),
            "editor.autosaveEnabled" => json!(self.editor_autosave_enabled),
            "editor.tabSize" => json!(self.editor_tab_size),
            "general.confirmProjectRemoval" => json!(self.confirm_project_removal),
            "general.confirmWorkspaceRemoval" => json!(self.confirm_workspace_removal),
            "general.autoArchiveWorkspacesAfterDays" => {
                json!(self.auto_archive_workspaces_after_days)
            }
            _ => return None,
        })
    }

    pub fn writable(path: &str) -> bool {
        matches!(
            path,
            "general.autoGitFetchEnabled"
                | "general.autoGitFetchIntervalMinutes"
                | "terminal.fontSize"
                | "editor.autosaveEnabled"
                | "editor.tabSize"
        )
    }

    pub fn valid_value(path: &str, value: &Value) -> bool {
        match path {
            "general.autoGitFetchEnabled"
            | "editor.autosaveEnabled"
            | "general.confirmProjectRemoval"
            | "general.confirmWorkspaceRemoval" => value.is_boolean(),
            "general.autoGitFetchIntervalMinutes" => {
                value.as_u64().is_some_and(|v| (1..=60).contains(&v))
            }
            "editor.tabSize" => value.as_u64().is_some_and(|v| (1..=16).contains(&v)),
            "general.autoArchiveWorkspacesAfterDays" => value.as_u64().is_some_and(|v| v <= 36_500),
            "terminal.fontSize" => value
                .as_f64()
                .is_some_and(|v| v.is_finite() && (8.0..=48.0).contains(&v)),
            _ => false,
        }
    }

    fn validate(&self) -> io::Result<()> {
        if self.version != VERSION
            || [
                (
                    "general.autoGitFetchEnabled",
                    json!(self.auto_git_fetch_enabled),
                ),
                (
                    "general.autoGitFetchIntervalMinutes",
                    json!(self.auto_git_fetch_interval_minutes),
                ),
                ("terminal.fontSize", json!(self.terminal_font_size)),
                (
                    "editor.autosaveEnabled",
                    json!(self.editor_autosave_enabled),
                ),
                ("editor.tabSize", json!(self.editor_tab_size)),
                (
                    "general.confirmProjectRemoval",
                    json!(self.confirm_project_removal),
                ),
                (
                    "general.confirmWorkspaceRemoval",
                    json!(self.confirm_workspace_removal),
                ),
                (
                    "general.autoArchiveWorkspacesAfterDays",
                    json!(self.auto_archive_workspaces_after_days),
                ),
            ]
            .iter()
            .any(|(path, value)| !Self::valid_value(path, value))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid Native settings document",
            ));
        }
        Ok(())
    }

    fn set(&mut self, path: &str, value: &Value) -> io::Result<()> {
        if !Self::writable(path) || !Self::valid_value(path, value) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Native setting is not writable or has invalid type/range",
            ));
        }
        match path {
            "general.autoGitFetchEnabled" => self.auto_git_fetch_enabled = value.as_bool().unwrap(),
            "general.autoGitFetchIntervalMinutes" => {
                self.auto_git_fetch_interval_minutes = value.as_u64().unwrap()
            }
            "terminal.fontSize" => self.terminal_font_size = value.as_f64().unwrap(),
            "editor.autosaveEnabled" => self.editor_autosave_enabled = value.as_bool().unwrap(),
            "editor.tabSize" => self.editor_tab_size = value.as_u64().unwrap(),
            _ => unreachable!("writability checked"),
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct NativeSettingsStore {
    path: PathBuf,
}

impl NativeSettingsStore {
    pub fn new(state_dir: &Path) -> Self {
        Self {
            path: state_dir.join(SETTINGS_FILENAME),
        }
    }

    /// An absent file means first run. A corrupt or future-version file
    /// fails closed: do not overwrite it with defaults.
    pub fn read(&self) -> io::Result<NativeSettings> {
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(NativeSettings::default())
            }
            Err(error) => return Err(error),
        };
        if file.metadata()?.len() > MAX_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Native settings file is too large",
            ));
        }
        let mut text = String::new();
        file.take(MAX_BYTES + 1).read_to_string(&mut text)?;
        if text.len() as u64 > MAX_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Native settings file is too large",
            ));
        }
        let settings: NativeSettings = serde_json::from_str(&text)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        settings.validate()?;
        Ok(settings)
    }

    pub fn get(&self, path: &str) -> io::Result<Value> {
        self.read()?
            .get(path)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Unknown Native setting"))
    }

    /// Cross-window CAS is serialized by an exclusive local lock. No update
    /// succeeds until the actual JSON file has been saved and read back.
    pub fn compare_and_set(
        &self,
        path: &str,
        expected: &Value,
        value: &Value,
    ) -> io::Result<NativeSettings> {
        if !NativeSettings::writable(path)
            || !NativeSettings::valid_value(path, expected)
            || !NativeSettings::valid_value(path, value)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Native setting is not writable or has invalid value",
            ));
        }
        let parent = self
            .path
            .parent()
            .ok_or_else(|| io::Error::other("Missing Native state directory"))?;
        fs::create_dir_all(parent)?;
        let lock_path = self.path.with_extension("json.lock");
        let lock_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)?;
        let _lock = SettingsFileLock {
            path: lock_path,
            file: Some(lock_file),
        };
        let mut settings = self.read()?;
        let current = settings
            .get(path)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Unknown Native setting"))?;
        // The Host allows 13 and 13.0 as valid terminal font sizes. Compare
        // them semantically, not by serde_json's integer/float representation.
        let equals = if path == "terminal.fontSize" {
            current.as_f64() == expected.as_f64()
        } else {
            current == *expected
        };
        if !equals {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Native setting changed since expectedValue",
            ));
        }
        settings.set(path, value)?;
        settings.validate()?;
        self.persist(&settings)?;
        let readback = self.read()?;
        if readback.get(path) != settings.get(path) {
            return Err(io::Error::other(
                "Native settings readback did not match write",
            ));
        }
        Ok(readback)
    }

    fn persist(&self, settings: &NativeSettings) -> io::Result<()> {
        settings.validate()?;
        let bytes = serde_json::to_vec(settings).map_err(io::Error::other)?;
        let tick = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temp = self
            .path
            .with_extension(format!("json.tmp-{}-{tick}", std::process::id()));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temp, &self.path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }
}

struct SettingsFileLock {
    path: PathBuf,
    file: Option<File>,
}
impl Drop for SettingsFileLock {
    fn drop(&mut self) {
        // Windows requires closing the handle before removing the lock file.
        drop(self.file.take());
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolated_store_cas_survives_restart_and_refuses_stale_or_unsafe_mutations() {
        let temp = tempfile::tempdir().unwrap();
        let store = NativeSettingsStore::new(temp.path());
        assert_eq!(store.get("editor.tabSize").unwrap(), json!(4));
        assert!(!store.path.exists());
        store
            .compare_and_set("editor.tabSize", &json!(4), &json!(6))
            .unwrap();
        let restart = NativeSettingsStore::new(temp.path());
        assert_eq!(restart.get("editor.tabSize").unwrap(), json!(6));
        assert!(store
            .compare_and_set("editor.tabSize", &json!(4), &json!(8))
            .is_err());
        assert!(store
            .compare_and_set(
                "general.confirmWorkspaceRemoval",
                &json!(true),
                &json!(false)
            )
            .is_err());
        assert!(store
            .compare_and_set("editor.tabSize", &json!(6), &json!(100))
            .is_err());
        assert!(store
            .compare_and_set("agents.secret", &json!(false), &json!(true))
            .is_err());
        assert_eq!(restart.get("editor.tabSize").unwrap(), json!(6));
        assert!(!temp.path().join("Alera").exists());
    }

    #[test]
    fn corrupt_file_is_preserved_and_numeric_cas_is_semantic() {
        let temp = tempfile::tempdir().unwrap();
        let store = NativeSettingsStore::new(temp.path());
        store
            .compare_and_set("terminal.fontSize", &json!(13), &json!(14.5))
            .unwrap();
        assert_eq!(store.get("terminal.fontSize").unwrap(), json!(14.5));
        let before = fs::read(&store.path).unwrap();
        assert!(store
            .compare_and_set("terminal.fontSize", &json!(13), &json!(15))
            .is_err());
        assert_eq!(fs::read(&store.path).unwrap(), before);
        fs::write(&store.path, b"{bad json").unwrap();
        assert!(store.get("terminal.fontSize").is_err());
        assert!(store
            .compare_and_set("terminal.fontSize", &json!(13), &json!(15))
            .is_err());
        assert_eq!(fs::read(&store.path).unwrap(), b"{bad json");
    }

    #[test]
    fn complete_host_settings_allowlist_and_parallel_writer_guard() {
        let settings = NativeSettings::default();
        assert!(
            settings.auto_git_fetch_enabled,
            "Native must match Classic's default-on Git fetch"
        );
        let known = [
            ("general.autoGitFetchEnabled", false),
            ("general.autoGitFetchIntervalMinutes", true),
            ("terminal.fontSize", true),
            ("editor.autosaveEnabled", false),
            ("editor.tabSize", true),
            ("general.confirmProjectRemoval", false),
            ("general.confirmWorkspaceRemoval", false),
            ("general.autoArchiveWorkspacesAfterDays", true),
        ];
        for (path, numeric) in known {
            let value = settings.get(path).expect("Host-allowlisted Native setting");
            assert!(NativeSettings::valid_value(path, &value));
            assert_eq!(value.is_number(), numeric, "{path}");
        }
        assert!(settings.get("ai.provider.apiKey").is_none());
        assert!(!NativeSettings::writable("general.confirmProjectRemoval"));
        assert!(!NativeSettings::writable("general.confirmWorkspaceRemoval"));
        assert!(!NativeSettings::writable(
            "general.autoArchiveWorkspacesAfterDays"
        ));

        let fixture = tempfile::tempdir().unwrap();
        let store = NativeSettingsStore::new(fixture.path());
        let lock_path = store.path.with_extension("json.lock");
        let _hold = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&lock_path)
            .unwrap();
        assert!(
            store
                .compare_and_set("editor.tabSize", &json!(4), &json!(5))
                .is_err(),
            "A second Native window must not race the first writer"
        );
        assert_eq!(store.get("editor.tabSize").unwrap(), json!(4));
    }
}
