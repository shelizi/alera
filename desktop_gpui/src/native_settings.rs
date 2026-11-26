//! Native-only settings owner. Never reads or mutates Flutter's preferences.
//! The Host permits only these scalar MCP paths; storage uses a separate
//! versioned JSON file under AleraNative (or an isolated smoke-test state).
use crate::explorer_visibility::ExplorerVisibilityOptions;
use crate::visual_tokens::{
    PROJECTS_SIDEBAR_WIDTH, RIGHT_SIDEBAR_MAX_WIDTH, RIGHT_SIDEBAR_WIDTH, SIDEBAR_MAX_WIDTH,
    SIDEBAR_MIN_WIDTH,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const VERSION: u32 = 1;
const MAX_BYTES: u64 = 16 * 1024;
pub const SETTINGS_FILENAME: &str = "native-settings-v1.json";

/// Distinguish an explicit Clear action from accidentally saving an empty
/// input field. The store still validates file type, absolute path and CAS.
pub fn agent_executable_form_value(clear: bool, raw: &str) -> Result<Option<String>, &'static str> {
    if clear {
        return Ok(None);
    }
    let value = raw.trim();
    if value.is_empty() {
        Err("Enter an executable path, or use Clear override")
    } else {
        Ok(Some(value.to_owned()))
    }
}

const fn default_explorer_show_hidden() -> bool {
    !cfg!(windows)
}

const fn default_terminal_scrollback_lines() -> u64 {
    10_000
}

pub fn valid_terminal_scrollback_lines(lines: u64) -> bool {
    (100..=200_000).contains(&lines)
}

/// GPUI-only shell layout. Classic owns its own preferences; never expose
/// these fields to Host MCP or DesktopSettings shared paths.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeSidebarLayout {
    pub projects_collapsed: bool,
    pub context_visible: bool,
    pub projects_width: f32,
    pub context_width: f32,
    pub context_is_git: bool,
    /// Optional since older GPUI versions only had Explorer and Git.
    #[serde(default)]
    pub context_is_search: bool,
    #[serde(default)]
    pub context_is_references: bool,
    #[serde(default)]
    pub context_is_pull_requests: bool,
    /// Optional context tab for older Native settings snapshots.
    #[serde(default)]
    pub context_is_agent_context: bool,
}

impl Default for NativeSidebarLayout {
    fn default() -> Self {
        Self {
            projects_collapsed: false,
            context_visible: true,
            projects_width: PROJECTS_SIDEBAR_WIDTH,
            context_width: RIGHT_SIDEBAR_WIDTH,
            context_is_git: false,
            context_is_search: false,
            context_is_references: false,
            context_is_pull_requests: false,
            context_is_agent_context: false,
        }
    }
}

impl NativeSidebarLayout {
    fn valid(&self) -> bool {
        self.projects_width.is_finite()
            && (SIDEBAR_MIN_WIDTH..=SIDEBAR_MAX_WIDTH).contains(&self.projects_width)
            && self.context_width.is_finite()
            && (SIDEBAR_MIN_WIDTH..=RIGHT_SIDEBAR_MAX_WIDTH).contains(&self.context_width)
            && [
                self.context_is_git,
                self.context_is_search,
                self.context_is_references,
                self.context_is_pull_requests,
                self.context_is_agent_context,
            ]
            .into_iter()
            .filter(|selected| *selected)
            .count()
                <= 1
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeSettings {
    version: u32,
    pub auto_git_fetch_enabled: bool,
    pub auto_git_fetch_interval_minutes: u64,
    pub terminal_font_size: f64,
    /// Native Alacritty viewport history limit. Classic's Flutter preference
    /// is not shared, and Runtime Host retains PTY output by *bytes*.
    #[serde(default = "default_terminal_scrollback_lines")]
    pub terminal_scrollback_lines: u64,
    pub editor_autosave_enabled: bool,
    pub editor_tab_size: u64,
    /// Legacy Native JSON fallback. When Host is available, Rust Core's
    /// editor.twoRowTabs is authoritative for both Classic and GPUI.
    #[serde(default)]
    pub editor_two_row_tabs: bool,
    /// Native Explorer-only preferences. Never exposed to Host MCP setting
    /// updates and never shared with the Classic Flutter preferences file.
    #[serde(default = "default_explorer_show_hidden")]
    pub explorer_show_hidden: bool,
    #[serde(default)]
    pub explorer_show_ignored: bool,
    /// Saved only under Native's versioned local JSON; older settings
    /// documents migrate to Classic-compatible shell defaults.
    #[serde(default)]
    pub sidebar_layout: NativeSidebarLayout,
    // These three safety controls cannot be changed by an Agent.
    pub confirm_project_removal: bool,
    pub confirm_workspace_removal: bool,
    pub auto_archive_workspaces_after_days: u64,
    /// A Native-only preference; deliberately excluded from Host MCP setting
    /// get/update and never copied from Classic's Agent Profile catalog.
    #[serde(default)]
    pub default_agent_profile_id: Option<String>,
    /// Native-local overrides for the same canonical Agent adapters as Classic.
    /// Never overwrite/read Classic's private Flutter preferences.
    #[serde(default)]
    pub agent_executable_paths: BTreeMap<String, String>,
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
            terminal_scrollback_lines: default_terminal_scrollback_lines(),
            editor_autosave_enabled: false,
            editor_tab_size: 4,
            editor_two_row_tabs: false,
            explorer_show_hidden: default_explorer_show_hidden(),
            explorer_show_ignored: false,
            sidebar_layout: NativeSidebarLayout::default(),
            confirm_project_removal: true,
            confirm_workspace_removal: true,
            auto_archive_workspaces_after_days: 0,
            default_agent_profile_id: None,
            agent_executable_paths: BTreeMap::new(),
        }
    }
}

impl NativeSettings {
    /// Apply the reviewed Classic/GPUI shared fields to the current UI
    /// snapshot. Explorer visibility and Native-only metadata remain intact.
    /// No local JSON is written; Runtime Host stays the authoritative owner.
    pub fn apply_shared_snapshot(&mut self, snapshot: &Value) -> io::Result<String> {
        let values = snapshot
            .get("values")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Shared desktop Settings values missing",
                )
            })?;
        let mut required = Vec::new();
        for path in alera_core::desktop_settings_contract::SHARED_DESKTOP_PATHS {
            let Some(value) = values.get(path) else {
                if matches!(
                    path,
                    "agents.quotaUnpinnedKeys"
                        | "editor.externalEditor"
                        | "agents.builtinMcpEnabled"
                ) {
                    // Existing Host binaries without the new optional quota
                    // preference must not break all other shared settings.
                    continue;
                }
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Incomplete shared desktop Settings snapshot",
                ));
            };
            if path == "general.language" {
                if !value.as_str().is_some_and(|v| {
                    matches!(
                        v,
                        "system"
                            | "english"
                            | "traditionalChinese"
                            | "simplifiedChinese"
                            | "japanese"
                    )
                }) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Invalid shared language",
                    ));
                }
            } else if path == "agents.defaultAgentProfileId" {
                if !value.is_null() && !value.as_str().is_some_and(Self::valid_agent_profile_id) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Invalid shared Host default Agent Profile ID",
                    ));
                }
            } else if path == "agents.builtinMcpEnabled" {
                if !value.is_boolean() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Invalid built-in MCP setting",
                    ));
                }
            } else if path == "agents.quotaUnpinnedKeys" {
                if !alera_core::desktop_settings_contract::valid_quota_unpinned_keys(value) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Invalid shared quota pins",
                    ));
                }
            } else if alera_core::desktop_settings_contract::extended_portable(path) {
                if !alera_core::desktop_settings_contract::valid_extended_portable(path, value) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Invalid shared portable setting",
                    ));
                }
            } else if !Self::valid_value(path, value) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid shared setting",
                ));
            }
            required.push((path, value.clone()));
        }
        let language = values["general.language"].as_str().unwrap().to_owned();
        for (path, value) in required {
            match path {
                "general.autoGitFetchEnabled" => {
                    self.auto_git_fetch_enabled = value.as_bool().unwrap()
                }
                "general.autoGitFetchIntervalMinutes" => {
                    self.auto_git_fetch_interval_minutes = value.as_u64().unwrap()
                }
                "terminal.fontSize" => self.terminal_font_size = value.as_f64().unwrap(),
                "editor.autosaveEnabled" => self.editor_autosave_enabled = value.as_bool().unwrap(),
                "editor.tabSize" => self.editor_tab_size = value.as_u64().unwrap(),
                "editor.twoRowTabs" => self.editor_two_row_tabs = value.as_bool().unwrap(),
                "general.confirmProjectRemoval" => {
                    self.confirm_project_removal = value.as_bool().unwrap()
                }
                "general.confirmWorkspaceRemoval" => {
                    self.confirm_workspace_removal = value.as_bool().unwrap()
                }
                "general.autoArchiveWorkspacesAfterDays" => {
                    self.auto_archive_workspaces_after_days = value.as_u64().unwrap()
                }
                "agents.defaultAgentProfileId" => {
                    self.default_agent_profile_id = value.as_str().map(str::to_owned)
                }
                "explorer.showHiddenFiles" => self.explorer_show_hidden = value.as_bool().unwrap(),
                "explorer.showIgnoredFiles" => {
                    self.explorer_show_ignored = value.as_bool().unwrap()
                }
                _ => {}
            }
        }
        Ok(language)
    }

    fn valid_agent_profile_id(id: &str) -> bool {
        !id.is_empty()
            && id.len() <= 128
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
    }

    pub fn get(&self, path: &str) -> Option<Value> {
        Some(match path {
            "general.autoGitFetchEnabled" => json!(self.auto_git_fetch_enabled),
            "general.autoGitFetchIntervalMinutes" => json!(self.auto_git_fetch_interval_minutes),
            "terminal.fontSize" => json!(self.terminal_font_size),
            "terminal.nativeScrollbackLines" => json!(self.terminal_scrollback_lines),
            "editor.autosaveEnabled" => json!(self.editor_autosave_enabled),
            "editor.tabSize" => json!(self.editor_tab_size),
            "editor.twoRowTabs" => json!(self.editor_two_row_tabs),
            "general.confirmProjectRemoval" => json!(self.confirm_project_removal),
            "general.confirmWorkspaceRemoval" => json!(self.confirm_workspace_removal),
            "general.autoArchiveWorkspacesAfterDays" => {
                json!(self.auto_archive_workspaces_after_days)
            }
            "agents.defaultAgentProfileId" => json!(self.default_agent_profile_id),
            "explorer.showHiddenFiles" => json!(self.explorer_show_hidden),
            "explorer.showIgnoredFiles" => json!(self.explorer_show_ignored),
            _ => return None,
        })
    }

    /// Ownership is independent of MCP write permission. Every Classic/GPUI
    /// shared setting must be read from the Rust Runtime Host snapshot, even
    /// when its user-facing control is read-only to MCP.
    pub fn shared_host_owned(path: &str) -> bool {
        alera_core::desktop_settings_contract::SHARED_DESKTOP_PATHS.contains(&path)
            || alera_core::desktop_settings_contract::extended_portable(path)
            || path == "agents.quotaUnpinnedKeys"
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
            | "editor.twoRowTabs"
            | "explorer.showHiddenFiles"
            | "explorer.showIgnoredFiles"
            | "general.confirmProjectRemoval"
            | "general.confirmWorkspaceRemoval" => value.is_boolean(),
            "general.autoGitFetchIntervalMinutes" => {
                value.as_u64().is_some_and(|v| (1..=60).contains(&v))
            }
            "editor.tabSize" => value.as_u64().is_some_and(|v| (1..=16).contains(&v)),
            "editor.externalEditor" => value
                .as_str()
                .is_some_and(|editor| matches!(editor, "zed" | "vscode")),
            "general.autoArchiveWorkspacesAfterDays" => value.as_u64().is_some_and(|v| v <= 36_500),
            "terminal.nativeScrollbackLines" => {
                value.as_u64().is_some_and(valid_terminal_scrollback_lines)
            }
            "terminal.fontSize" => value
                .as_f64()
                .is_some_and(|v| v.is_finite() && (8.0..=48.0).contains(&v)),
            _ => false,
        }
    }

    fn validate(&self) -> io::Result<()> {
        if self.version != VERSION
            || self
                .default_agent_profile_id
                .as_deref()
                .is_some_and(|id| !Self::valid_agent_profile_id(id))
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
                    "terminal.nativeScrollbackLines",
                    json!(self.terminal_scrollback_lines),
                ),
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
        if !self.sidebar_layout.valid() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid Native sidebar layout",
            ));
        }
        if self.agent_executable_paths.len() > 12
            || self.agent_executable_paths.iter().any(|(id, path)| {
                alera_core::agent_descriptor::agent_descriptor(id)
                    .is_none_or(|descriptor| descriptor.id != id)
                    || path.len() > 1024
                    || path.contains(['\0', '\n', '\r'])
                    || !Path::new(path).is_absolute()
            })
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid Native Agent executable override map",
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

    /// Only an actual legacy file can seed the Host; the first-run default
    /// must not accidentally win over a Classic preference.
    pub fn has_saved_state(&self) -> bool {
        self.path.is_file()
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
        let _lock = self.lock()?;
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

    /// A Native-only preference deliberately absent from the Host's MCP
    /// get/update allowlist and shared Classic DesktopSettings contract.
    /// CAS and disk readback prevent multiple Native windows from clobbering
    /// each other's chosen scrollback limits.
    pub fn compare_and_set_terminal_scrollback_lines(
        &self,
        expected: u64,
        next: u64,
    ) -> io::Result<NativeSettings> {
        if !valid_terminal_scrollback_lines(expected) || !valid_terminal_scrollback_lines(next) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Native scrollback lines out of range",
            ));
        }
        let _lock = self.lock()?;
        let mut settings = self.read()?;
        if settings.terminal_scrollback_lines != expected {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Native scrollback preference was changed in another window",
            ));
        }
        settings.terminal_scrollback_lines = next;
        settings.validate()?;
        self.persist(&settings)?;
        let readback = self.read()?;
        if readback.terminal_scrollback_lines != next {
            return Err(io::Error::other(
                "Native scrollback preference readback did not match write",
            ));
        }
        Ok(readback)
    }

    /// Native Explorer preferences use the same atomic file lock as MCP
    /// settings, but are deliberately not MCP-writable scalar paths.
    pub fn compare_and_set_explorer_visibility(
        &self,
        expected: ExplorerVisibilityOptions,
        next: ExplorerVisibilityOptions,
    ) -> io::Result<NativeSettings> {
        let _lock = self.lock()?;
        let mut settings = self.read()?;
        if settings.explorer_show_hidden != expected.show_hidden
            || settings.explorer_show_ignored != expected.show_ignored
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Native Explorer visibility was changed by another window",
            ));
        }
        settings.explorer_show_hidden = next.show_hidden;
        settings.explorer_show_ignored = next.show_ignored;
        self.persist(&settings)?;
        let readback = self.read()?;
        if readback.explorer_show_hidden != next.show_hidden
            || readback.explorer_show_ignored != next.show_ignored
        {
            return Err(io::Error::other(
                "Native Explorer visibility readback does not match write",
            ));
        }
        Ok(readback)
    }

    /// Atomic Native-only layout persistence with the existing cross-window
    /// lock. Preserve all unrelated settings and refuse stale writes.
    pub fn compare_and_set_sidebar_layout(
        &self,
        expected: &NativeSidebarLayout,
        next: &NativeSidebarLayout,
    ) -> io::Result<NativeSettings> {
        if !expected.valid() || !next.valid() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Native sidebar width out of range",
            ));
        }
        let _lock = self.lock()?;
        let mut settings = self.read()?;
        if settings.sidebar_layout != *expected {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Native sidebar layout changed in another window",
            ));
        }
        settings.sidebar_layout = next.clone();
        self.persist(&settings)?;
        let readback = self.read()?;
        if readback.sidebar_layout != *next {
            return Err(io::Error::other("Native sidebar layout readback mismatch"));
        }
        Ok(readback)
    }

    /// Native-local CAS for the default Agent Profile. This setting cannot
    /// be accessed through the Host's scalar MCP settings allowlist.
    pub fn compare_and_set_agent_executable_path(
        &self,
        agent_id: &str,
        expected: Option<&str>,
        next: Option<&str>,
    ) -> io::Result<NativeSettings> {
        if alera_core::agent_descriptor::agent_descriptor(agent_id)
            .is_none_or(|descriptor| descriptor.id != agent_id)
            || next.is_some_and(|path| {
                path.len() > 1024
                    || path.contains(['\0', '\n', '\r'])
                    || !Path::new(path).is_absolute()
                    || !crate::local_agent_detection::executable_is_runnable(Path::new(path))
            })
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid Agent executable path",
            ));
        }
        let _lock = self.lock()?;
        let mut settings = self.read()?;
        if settings
            .agent_executable_paths
            .get(agent_id)
            .map(String::as_str)
            != expected
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Agent executable path was changed by another window",
            ));
        }
        if let Some(next) = next.filter(|next| !next.is_empty()) {
            settings
                .agent_executable_paths
                .insert(agent_id.into(), next.into());
        } else {
            settings.agent_executable_paths.remove(agent_id);
        }
        settings.validate()?;
        self.persist(&settings)?;
        let readback = self.read()?;
        if readback.agent_executable_paths != settings.agent_executable_paths {
            return Err(io::Error::other("Agent executable path readback mismatch"));
        }
        Ok(readback)
    }

    pub fn compare_and_set_default_agent_profile(
        &self,
        expected: Option<&str>,
        next: Option<&str>,
    ) -> io::Result<NativeSettings> {
        if expected.is_some_and(|id| !NativeSettings::valid_agent_profile_id(id))
            || next.is_some_and(|id| !NativeSettings::valid_agent_profile_id(id))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid Native default Agent Profile id",
            ));
        }
        let _lock = self.lock()?;
        let mut settings = self.read()?;
        if settings.default_agent_profile_id.as_deref() != expected {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Native default Agent Profile changed since selection",
            ));
        }
        settings.default_agent_profile_id = next.map(str::to_owned);
        settings.validate()?;
        self.persist(&settings)?;
        let readback = self.read()?;
        if readback.default_agent_profile_id.as_deref() != next {
            return Err(io::Error::other(
                "Native Agent Profile readback did not match write",
            ));
        }
        Ok(readback)
    }

    /// UI-only local preference: never expose the Classic-specific toggle
    /// as an MCP-writable Host shared scalar without a Core contract.
    pub fn compare_and_set_two_row_tabs(
        &self,
        expected: bool,
        next: bool,
    ) -> io::Result<NativeSettings> {
        let _lock = self.lock()?;
        let mut settings = self.read()?;
        if settings.editor_two_row_tabs != expected {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Native two-row tabs preference changed in another window",
            ));
        }
        settings.editor_two_row_tabs = next;
        self.persist(&settings)?;
        let readback = self.read()?;
        if readback.editor_two_row_tabs != next {
            return Err(io::Error::other("Native two-row tabs readback mismatch"));
        }
        Ok(readback)
    }

    fn lock(&self) -> io::Result<SettingsFileLock> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| io::Error::other("Missing Native state directory"))?;
        fs::create_dir_all(parent)?;
        let path = self.path.with_extension("json.lock");
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        Ok(SettingsFileLock {
            path,
            file: Some(file),
        })
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
    fn s02_mcp_shared_reads_use_host_for_read_only_and_extended_settings() {
        for path in [
            "editor.twoRowTabs",
            "general.confirmProjectRemoval",
            "general.confirmWorkspaceRemoval",
            "general.autoArchiveWorkspacesAfterDays",
            "terminal.fontFamily",
            "editor.themeName",
            "agents.quotaUnpinnedKeys",
            "agents.defaultAgentProfileId",
            "editor.quickOpenExcludedDirectories",
        ] {
            assert!(
                NativeSettings::shared_host_owned(path),
                "MCP get_setting must use Host snapshot, never old Native JSON: {path}"
            );
        }
        assert!(!NativeSettings::writable("editor.twoRowTabs"));
        assert!(!NativeSettings::writable("general.confirmProjectRemoval"));
        assert!(NativeSettings::shared_host_owned("editor.tabSize"));
        assert!(NativeSettings::writable("editor.tabSize"));
        for unknown in [
            "native.agentExecutablePath",
            "ai.provider.apiKey",
            "unsupported",
        ] {
            assert!(
                !NativeSettings::shared_host_owned(unknown),
                "unowned MCP setting must fail closed: {unknown}"
            );
        }
    }
    #[test]
    fn legacy_two_row_tabs_persists_for_one_time_host_migration_only() {
        let dir = tempfile::tempdir().unwrap();
        let store = NativeSettingsStore::new(dir.path());
        assert!(!store.read().unwrap().editor_two_row_tabs);
        assert!(!store.has_saved_state());
        assert!(
            store
                .compare_and_set_two_row_tabs(false, true)
                .unwrap()
                .editor_two_row_tabs
        );
        assert!(store.read().unwrap().editor_two_row_tabs);
        assert!(store.has_saved_state());
        assert!(store.compare_and_set_two_row_tabs(false, true).is_err());
        assert!(
            !store
                .compare_and_set_two_row_tabs(true, false)
                .unwrap()
                .editor_two_row_tabs
        );
        assert!(!store.read().unwrap().editor_two_row_tabs);
        assert!(!NativeSettings::writable("editor.twoRowTabs"));
        assert_eq!(
            NativeSettings::default().get("editor.twoRowTabs"),
            Some(json!(false))
        );
    }

    #[test]
    fn shared_host_snapshot_validates_quota_pin_field_without_erasing_native_only_data() {
        let mut state = NativeSettings::default();
        state.explorer_show_ignored = true;
        state.default_agent_profile_id = Some("managed_codex".to_owned());
        let snapshot = json!({"values":{
            "general.language":"traditionalChinese",
            "general.autoGitFetchEnabled":false,
            "general.autoGitFetchIntervalMinutes":10,
            "terminal.fontSize":17.0,
            "editor.autosaveEnabled":true,
            "editor.tabSize":8,
            "editor.twoRowTabs":true,
            "editor.externalEditor":"vscode",
            "general.confirmProjectRemoval":false,
            "general.confirmWorkspaceRemoval":true,
            "general.autoArchiveWorkspacesAfterDays":60,
            "agents.defaultAgentProfileId":"host_codex",
            "agents.builtinMcpEnabled":true,
            "explorer.showHiddenFiles":true,
            "explorer.showIgnoredFiles":false,
            "general.showTrayIcon":true,
            "general.showDockBadge":true,
            "general.showTrayBadge":false,
            "terminal.fontFamily":"JetBrains Mono",
            "terminal.lineHeight":1.5,
            "terminal.cursorShape":"bar",
            "terminal.cursorBlink":true,
            "terminal.themeName":"Dracula",
            "terminal.backgroundOpacity":0.8,
            "terminal.tuiScrollSensitivity":2,
            "terminal.clipboardOnSelect":true,
            "editor.themeName":"Tokyo Night Dark",
            "editor.autosaveDelaySeconds":7,
            "agents.agentStatusNotificationsEnabled":true,
            "agents.agentStatusFinishedNotificationsEnabled":false,
            "agents.showTabTitlesInSidebar":true,
            "agents.showTaskSummariesInSidebar":false,
            "editor.quickOpenExcludedDirectories":["node_modules","target","Generated"],
            "agents.quotaUnpinnedKeys":["claude:default","agy"]
        }});
        for path in alera_core::desktop_settings_contract::SHARED_DESKTOP_PATHS {
            assert!(
                snapshot["values"].get(path).is_some(),
                "Native Settings test fixture missing Host contract path: {path}"
            );
        }
        assert_eq!(
            state.apply_shared_snapshot(&snapshot).unwrap(),
            "traditionalChinese"
        );
        assert_eq!(state.terminal_font_size, 17.0);
        assert_eq!(state.editor_tab_size, 8);
        assert!(state.editor_two_row_tabs);
        assert!(!state.auto_git_fetch_enabled);
        assert!(!state.confirm_project_removal);
        assert!(state.confirm_workspace_removal);
        assert_eq!(state.auto_archive_workspaces_after_days, 60);
        assert!(!state.explorer_show_ignored);
        assert!(state.explorer_show_hidden);
        assert_eq!(
            state.default_agent_profile_id.as_deref(),
            Some("host_codex")
        );
        let previous = state.clone();
        let mut invalid_pins = snapshot.clone();
        invalid_pins["values"]["agents.quotaUnpinnedKeys"] = json!(["agy", "agy"]);
        assert!(state.apply_shared_snapshot(&invalid_pins).is_err());
        let mut old_host = snapshot.clone();
        old_host["values"]
            .as_object_mut()
            .unwrap()
            .remove("agents.quotaUnpinnedKeys");
        assert_eq!(
            state.apply_shared_snapshot(&old_host).unwrap(),
            "traditionalChinese",
            "Older Host without quota-pins key should still provide other shared settings",
        );
        let mut invalid_extended = snapshot.clone();
        invalid_extended["values"]["terminal.cursorShape"] = json!("unexpected");
        assert!(state.apply_shared_snapshot(&invalid_extended).is_err());
        let mut invalid_exclusions = snapshot.clone();
        invalid_exclusions["values"]["editor.quickOpenExcludedDirectories"] =
            json!(["target", "TARGET"]);
        assert!(state.apply_shared_snapshot(&invalid_exclusions).is_err());
        let mut invalid_two_row = snapshot.clone();
        invalid_two_row["values"]["editor.twoRowTabs"] = json!("true");
        assert!(state.apply_shared_snapshot(&invalid_two_row).is_err());
        let mut invalid_external_editor = snapshot.clone();
        invalid_external_editor["values"]["editor.externalEditor"] = json!("arbitrary-executable");
        assert!(state
            .apply_shared_snapshot(&invalid_external_editor)
            .is_err());
        let mut selected_external_editor = snapshot.clone();
        selected_external_editor["values"]["editor.externalEditor"] = json!("vscode");
        assert_eq!(
            state
                .apply_shared_snapshot(&selected_external_editor)
                .unwrap(),
            "traditionalChinese",
            "A valid Host-selected external editor must not alter unrelated Native settings",
        );
        assert_eq!(
            state, previous,
            "Invalid extended settings must not partially install"
        );
        assert!(state
            .apply_shared_snapshot(&json!({"values":{
                "general.language":"japanese",
                "general.autoGitFetchEnabled":true
            }}))
            .is_err());
        assert_eq!(state, previous);
    }

    #[test]
    fn native_scrollback_is_local_persistent_bounded_and_not_mcp_writable() {
        let dir = tempfile::tempdir().unwrap();
        let store = NativeSettingsStore::new(dir.path());
        assert_eq!(store.read().unwrap().terminal_scrollback_lines, 10_000);
        assert_eq!(
            store.get("terminal.nativeScrollbackLines").unwrap(),
            json!(10_000)
        );
        assert!(!NativeSettings::writable("terminal.nativeScrollbackLines"));
        assert!(!NativeSettings::shared_host_owned(
            "terminal.nativeScrollbackLines"
        ));
        assert_eq!(
            store
                .compare_and_set_terminal_scrollback_lines(10_000, 20_000)
                .unwrap()
                .terminal_scrollback_lines,
            20_000
        );
        assert_eq!(
            NativeSettingsStore::new(dir.path())
                .read()
                .unwrap()
                .terminal_scrollback_lines,
            20_000
        );
        assert!(store
            .compare_and_set_terminal_scrollback_lines(10_000, 100)
            .is_err());
        for invalid in [0, 99, 200_001, u64::MAX] {
            assert!(store
                .compare_and_set_terminal_scrollback_lines(20_000, invalid)
                .is_err());
        }
        assert_eq!(store.read().unwrap().terminal_scrollback_lines, 20_000);
    }

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

    #[test]
    fn native_default_agent_profile_is_local_only_cas_and_survives_restart() {
        let temp = tempfile::tempdir().unwrap();
        let store = NativeSettingsStore::new(temp.path());
        assert_eq!(store.read().unwrap().default_agent_profile_id, None);
        let saved = store
            .compare_and_set_default_agent_profile(None, Some("managed_codex"))
            .unwrap();
        assert_eq!(
            saved.default_agent_profile_id.as_deref(),
            Some("managed_codex")
        );
        assert!(
            saved.get("agents.defaultProfileId").is_none(),
            "Host MCP must not see this local-only preference"
        );
        assert_eq!(
            NativeSettingsStore::new(temp.path())
                .read()
                .unwrap()
                .default_agent_profile_id
                .as_deref(),
            Some("managed_codex")
        );
        assert!(store
            .compare_and_set_default_agent_profile(None, Some("other"))
            .is_err());
        assert!(store
            .compare_and_set_default_agent_profile(Some("managed_codex"), Some("../bad"))
            .is_err());
        assert_eq!(
            store
                .compare_and_set_default_agent_profile(Some("managed_codex"), None)
                .unwrap()
                .default_agent_profile_id,
            None
        );
        assert!(store
            .compare_and_set_default_agent_profile(Some("managed_codex"), None)
            .is_err());
    }

    #[test]
    fn native_sidebar_layout_is_private_atomic_and_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let store = NativeSettingsStore::new(dir.path());
        let initial = NativeSidebarLayout::default();
        assert!(!NativeSettings::writable("sidebar.projectsWidth"));
        assert!(!NativeSettings::writable("sidebar.contextVisible"));
        let next = NativeSidebarLayout {
            projects_collapsed: true,
            context_visible: false,
            projects_width: 365.0,
            context_width: 670.0,
            context_is_git: true,
            context_is_search: false,
            context_is_references: false,
            context_is_pull_requests: false,
            context_is_agent_context: false,
        };
        let result = store
            .compare_and_set_sidebar_layout(&initial, &next)
            .unwrap();
        assert_eq!(result.sidebar_layout, next);
        assert_eq!(store.read().unwrap().sidebar_layout, next);
        assert!(store
            .compare_and_set_sidebar_layout(&initial, &next)
            .is_err());

        let mut invalid = next.clone();
        invalid.context_width = f32::NAN;
        assert!(store
            .compare_and_set_sidebar_layout(&next, &invalid)
            .is_err());
        invalid.context_width = 2000.0;
        assert!(store
            .compare_and_set_sidebar_layout(&next, &invalid)
            .is_err());
        assert_eq!(store.read().unwrap().sidebar_layout, next);

        // A persisted Search tab cannot also be Source Control.
        let mut invalid = next.clone();
        invalid.context_is_search = true;
        assert!(store
            .compare_and_set_sidebar_layout(&next, &invalid)
            .is_err());

        let search = NativeSidebarLayout {
            context_is_git: false,
            context_is_search: true,
            ..next.clone()
        };
        let saved = store
            .compare_and_set_sidebar_layout(&next, &search)
            .unwrap();
        assert_eq!(saved.sidebar_layout, search);
        assert_eq!(store.read().unwrap().sidebar_layout, search);
        let references = NativeSidebarLayout {
            context_is_search: false,
            context_is_references: true,
            ..search.clone()
        };
        assert_eq!(
            store
                .compare_and_set_sidebar_layout(&search, &references)
                .unwrap()
                .sidebar_layout,
            references
        );
        let mut invalid = references.clone();
        invalid.context_is_git = true;
        assert!(store
            .compare_and_set_sidebar_layout(&references, &invalid)
            .is_err());
        let review = NativeSidebarLayout {
            context_is_references: false,
            context_is_pull_requests: true,
            context_is_agent_context: false,
            ..references.clone()
        };
        assert_eq!(
            store
                .compare_and_set_sidebar_layout(&references, &review)
                .unwrap()
                .sidebar_layout,
            review
        );
        let mut invalid = review.clone();
        invalid.context_is_references = true;
        assert!(store
            .compare_and_set_sidebar_layout(&review, &invalid)
            .is_err());
    }

    #[test]
    fn old_sidebar_settings_without_search_field_migrate_to_explorer_git() {
        let dir = tempfile::tempdir().unwrap();
        let store = NativeSettingsStore::new(dir.path());
        let mut document = serde_json::to_value(NativeSettings::default()).unwrap();
        let layout = document
            .get_mut("sidebar_layout")
            .unwrap()
            .as_object_mut()
            .unwrap();
        layout.remove("context_is_search");
        layout.insert("context_is_git".into(), serde_json::json!(true));
        fs::write(
            dir.path().join(SETTINGS_FILENAME),
            serde_json::to_vec(&document).unwrap(),
        )
        .unwrap();
        let stored = store.read().unwrap().sidebar_layout;
        assert!(stored.context_is_git);
        assert!(!stored.context_is_search);
        assert!(!stored.context_is_references);
        assert!(!stored.context_is_pull_requests);
    }

    #[test]
    fn legacy_native_settings_without_sidebar_layout_still_load() {
        let dir = tempfile::tempdir().unwrap();
        let store = NativeSettingsStore::new(dir.path());
        let mut document = serde_json::to_value(NativeSettings::default()).unwrap();
        document.as_object_mut().unwrap().remove("sidebar_layout");
        fs::write(
            dir.path().join(SETTINGS_FILENAME),
            serde_json::to_vec(&document).unwrap(),
        )
        .unwrap();
        assert_eq!(
            store.read().unwrap().sidebar_layout,
            NativeSidebarLayout::default()
        );
    }

    #[test]
    fn native_explorer_visibility_roundtrips_without_exposing_mcp_write_paths() {
        let dir = tempfile::tempdir().unwrap();
        let store = NativeSettingsStore::new(dir.path());
        let initial = ExplorerVisibilityOptions::default();
        assert!(!NativeSettings::writable("explorer.showIgnored"));
        assert!(!NativeSettings::writable("explorer.showHidden"));
        let next = ExplorerVisibilityOptions {
            show_ignored: true,
            show_hidden: !initial.show_hidden,
        };
        let result = store
            .compare_and_set_explorer_visibility(initial, next)
            .unwrap();
        assert_eq!(result.explorer_show_hidden, next.show_hidden);
        assert!(result.explorer_show_ignored);
        assert!(store
            .compare_and_set_explorer_visibility(initial, initial)
            .is_err());
        let reloaded = store.read().unwrap();
        assert_eq!(reloaded.explorer_show_hidden, next.show_hidden);
        assert!(reloaded.explorer_show_ignored);
    }

    #[test]
    fn native_agent_executable_overrides_are_cas_local_only_and_legacy_compatible() {
        let dir = tempfile::tempdir().unwrap();
        let store = NativeSettingsStore::new(dir.path());
        assert!(store.read().unwrap().agent_executable_paths.is_empty());
        let executable = std::env::current_exe().unwrap();
        let path = executable.to_str().unwrap();
        let saved = store
            .compare_and_set_agent_executable_path("claude", None, Some(path))
            .unwrap();
        assert_eq!(
            saved
                .agent_executable_paths
                .get("claude")
                .map(String::as_str),
            Some(path)
        );
        assert!(store
            .compare_and_set_agent_executable_path("claude", None, Some(path))
            .is_err());
        assert!(store
            .compare_and_set_agent_executable_path("claude", Some(path), Some("relative.exe"))
            .is_err());
        assert!(store
            .compare_and_set_agent_executable_path("unknown", None, Some(path))
            .is_err());
        assert!(store
            .compare_and_set_agent_executable_path("claude", Some(path), Some("x\nabc"))
            .is_err());
        assert_eq!(store.read().unwrap().agent_executable_paths["claude"], path);
        assert!(store
            .compare_and_set_agent_executable_path("claude", Some(path), None)
            .unwrap()
            .agent_executable_paths
            .is_empty());
    }

    #[test]
    fn native_agent_override_rejects_an_existing_non_executable_file() {
        let dir = tempfile::tempdir().unwrap();
        let note = dir.path().join("agent-not-executable.txt");
        std::fs::write(&note, b"not an executable").unwrap();
        let store = NativeSettingsStore::new(dir.path());
        let error = store
            .compare_and_set_agent_executable_path("codex", None, note.to_str())
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(store.read().unwrap().agent_executable_paths.is_empty());
    }

    #[test]
    fn agent_override_blank_save_does_not_act_as_clear() {
        assert_eq!(
            agent_executable_form_value(false, "   ").unwrap_err(),
            "Enter an executable path, or use Clear override"
        );
        assert_eq!(agent_executable_form_value(true, "anything"), Ok(None));
        assert_eq!(
            agent_executable_form_value(false, "  C:\\Agents\\codex.exe  "),
            Ok(Some("C:\\Agents\\codex.exe".into()))
        );
    }

    #[test]
    fn older_native_settings_json_migrates_missing_profile_field_without_erasing_preferences() {
        let temp = tempfile::tempdir().unwrap();
        let store = NativeSettingsStore::new(temp.path());
        let previous = NativeSettings::default();
        let mut value = serde_json::to_value(previous).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("default_agent_profile_id");
        std::fs::write(&store.path, serde_json::to_vec(&value).unwrap()).unwrap();
        let restored = store.read().unwrap();
        assert!(restored.auto_git_fetch_enabled);
        assert_eq!(restored.default_agent_profile_id, None);
        store
            .compare_and_set_default_agent_profile(None, Some("test_profile"))
            .unwrap();
        assert_eq!(store.get("editor.tabSize").unwrap(), json!(4));
        assert_eq!(
            store.read().unwrap().default_agent_profile_id.as_deref(),
            Some("test_profile")
        );
    }
}
