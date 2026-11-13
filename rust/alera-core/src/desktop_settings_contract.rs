//! Reviewed, non-secret preferences used by both Classic Flutter and GPUI.
//! This contract is intentionally free of the runtime/sqlite feature so the
//! Native frontend, CLI and Host can all depend on one Rust Core vocabulary.
//! Adding a path requires a Rust owner/validator and Classic migration test.
pub const SHARED_DESKTOP_PATHS: [&str; 31] = [
    "general.language",
    "general.autoGitFetchEnabled",
    "general.autoGitFetchIntervalMinutes",
    "terminal.fontSize",
    "editor.autosaveEnabled",
    "editor.tabSize",
    "editor.twoRowTabs",
    "general.confirmProjectRemoval",
    "general.confirmWorkspaceRemoval",
    "general.autoArchiveWorkspacesAfterDays",
    "agents.defaultAgentProfileId",
    "explorer.showHiddenFiles",
    "explorer.showIgnoredFiles",
    "general.showTrayIcon",
    "general.showDockBadge",
    "general.showTrayBadge",
    "terminal.fontFamily",
    "terminal.lineHeight",
    "terminal.cursorShape",
    "terminal.cursorBlink",
    "terminal.themeName",
    "terminal.backgroundOpacity",
    "terminal.tuiScrollSensitivity",
    "terminal.clipboardOnSelect",
    "editor.themeName",
    "editor.autosaveDelaySeconds",
    "agents.agentStatusNotificationsEnabled",
    "agents.agentStatusFinishedNotificationsEnabled",
    "agents.showTabTitlesInSidebar",
    "editor.quickOpenExcludedDirectories",
    "agents.quotaUnpinnedKeys",
];

/// CLI/Agent-facing writes stay narrower than owner-backed Classic Settings.
/// An Agent cannot silently disable destructive confirmation switches.
pub const CLI_WRITABLE_PATHS: [&str; 28] = [
    "general.language",
    "general.autoGitFetchEnabled",
    "general.autoGitFetchIntervalMinutes",
    "terminal.fontSize",
    "editor.autosaveEnabled",
    "editor.tabSize",
    "editor.twoRowTabs",
    "agents.defaultAgentProfileId",
    "explorer.showHiddenFiles",
    "explorer.showIgnoredFiles",
    "general.showTrayIcon",
    "general.showDockBadge",
    "general.showTrayBadge",
    "terminal.fontFamily",
    "terminal.lineHeight",
    "terminal.cursorShape",
    "terminal.cursorBlink",
    "terminal.themeName",
    "terminal.backgroundOpacity",
    "terminal.tuiScrollSensitivity",
    "terminal.clipboardOnSelect",
    "editor.themeName",
    "editor.autosaveDelaySeconds",
    "agents.agentStatusNotificationsEnabled",
    "agents.agentStatusFinishedNotificationsEnabled",
    "agents.showTabTitlesInSidebar",
    "editor.quickOpenExcludedDirectories",
    "agents.quotaUnpinnedKeys",
];

/// Already-Classic-portable, reviewed non-secret UI fields.
/// Classic's quota pin key format. Account identifiers are opaque and are
/// never interpreted as executable paths or authentication tokens.
pub fn valid_quota_unpinned_keys(value: &serde_json::Value) -> bool {
    let Some(keys) = value.as_array() else {
        return false;
    };
    if keys.len() > 128 {
        return false;
    }
    let mut seen = std::collections::HashSet::new();
    keys.iter().all(|entry| {
        let Some(key) = entry.as_str() else {
            return false;
        };
        let supported = matches!(
            key,
            "codex" | "kimi" | "grok" | "cursor" | "agy" | "minimax" | "zai" | "devin"
        ) || ["claude:", "codex:", "opencode:"]
            .iter()
            .any(|prefix| key.strip_prefix(prefix).is_some_and(|id| !id.is_empty()));
        supported
            && key.len() <= 300
            && key == key.trim()
            && !key.chars().any(char::is_control)
            && seen.insert(key)
    })
}

pub fn extended_portable(path: &str) -> bool {
    matches!(
        path,
        "general.showTrayIcon"
            | "general.showDockBadge"
            | "general.showTrayBadge"
            | "terminal.fontFamily"
            | "terminal.lineHeight"
            | "terminal.cursorShape"
            | "terminal.cursorBlink"
            | "terminal.themeName"
            | "terminal.backgroundOpacity"
            | "terminal.tuiScrollSensitivity"
            | "terminal.clipboardOnSelect"
            | "editor.themeName"
            | "editor.autosaveDelaySeconds"
            | "agents.agentStatusNotificationsEnabled"
            | "agents.agentStatusFinishedNotificationsEnabled"
            | "agents.showTabTitlesInSidebar"
            | "editor.quickOpenExcludedDirectories"
    )
}

/// Readback contract is available to Native even without the SQLite runtime.
pub fn valid_extended_portable(path: &str, value: &serde_json::Value) -> bool {
    match path {
        "editor.quickOpenExcludedDirectories" => value.as_array().is_some_and(|items| {
            items.len() <= 128 && {
                let mut seen = std::collections::HashSet::new();
                items.iter().all(|item| {
                    item.as_str().is_some_and(|name| {
                        valid_excluded_directory_name(name) && seen.insert(name.to_lowercase())
                    })
                })
            }
        }),
        "general.showTrayIcon"
        | "general.showDockBadge"
        | "general.showTrayBadge"
        | "terminal.cursorBlink"
        | "terminal.clipboardOnSelect"
        | "agents.agentStatusNotificationsEnabled"
        | "agents.agentStatusFinishedNotificationsEnabled"
        | "agents.showTabTitlesInSidebar" => value.is_boolean(),
        "terminal.lineHeight" => value
            .as_f64()
            .is_some_and(|v| v.is_finite() && (0.8..=2.4).contains(&v)),
        "terminal.backgroundOpacity" => value
            .as_f64()
            .is_some_and(|v| v.is_finite() && (0.0..=1.0).contains(&v)),
        "terminal.tuiScrollSensitivity" => value.as_u64().is_some_and(|v| (1..=10).contains(&v)),
        "editor.autosaveDelaySeconds" => value.as_u64().is_some_and(|v| (1..=60).contains(&v)),
        "terminal.cursorShape" => value
            .as_str()
            .is_some_and(|v| matches!(v, "block" | "bar" | "underline")),
        "terminal.fontFamily" | "terminal.themeName" | "editor.themeName" => {
            value.as_str().is_some_and(|v| {
                !v.trim().is_empty() && v.len() <= 128 && !v.chars().any(char::is_control)
            })
        }
        _ => false,
    }
}

/// A Quick Open exclusion is one directory *name*, never a filesystem path.
pub fn valid_excluded_directory_name(name: &str) -> bool {
    !name.is_empty()
        && name == name.trim()
        && name.len() <= 128
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !name.chars().any(char::is_control)
}

/// Match the Classic editor's first-run exclusion catalog.
pub const QUICK_OPEN_DEFAULT_EXCLUDED_DIRECTORIES: &[&str] = &[
    "node_modules",
    ".dart_tool",
    "vendor",
    "vendor-bin",
    ".phpunit.cache",
    "coverage",
    "Pods",
    ".gradle",
    ".venv",
    "venv",
    ".tox",
    "__pypackages__",
    "bower_components",
    "jspm_packages",
    ".pnpm-store",
    ".pub-cache",
    "target",
    "bin",
    "obj",
    ".vs",
    "packages",
    "TestResults",
    "BenchmarkDotNet.Artifacts",
    "artifacts",
];

pub fn quick_open_default_excluded_directories_json() -> serde_json::Value {
    serde_json::json!(QUICK_OPEN_DEFAULT_EXCLUDED_DIRECTORIES)
}

pub fn cli_writable(path: &str) -> bool {
    CLI_WRITABLE_PATHS.contains(&path)
}

/// CLI only accepts scalars except for the reviewed Classic quota pin array.
/// Never broaden arbitrary shared Settings or credential writes.
pub fn cli_set_value_allowed(path: &str, value: &serde_json::Value) -> bool {
    if !cli_writable(path) {
        return false;
    }
    if path == "agents.quotaUnpinnedKeys" {
        valid_quota_unpinned_keys(value)
    } else {
        !value.is_array() && !value.is_object()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn cli_set_accepts_only_validated_quota_pin_arrays_not_arbitrary_objects_or_lists() {
        let valid = serde_json::json!(["claude:default", "agy", "codex:work"]);
        assert!(cli_set_value_allowed("agents.quotaUnpinnedKeys", &valid));
        assert!(cli_set_value_allowed(
            "agents.quotaUnpinnedKeys",
            &serde_json::json!([])
        ));
        assert!(!cli_set_value_allowed(
            "agents.quotaUnpinnedKeys",
            &serde_json::json!(["agy", "agy"])
        ));
        assert!(!cli_set_value_allowed(
            "agents.quotaUnpinnedKeys",
            &serde_json::json!(true)
        ));
        assert!(!cli_set_value_allowed(
            "agents.quotaUnpinnedKeys",
            &serde_json::json!({"tokens":"secret"})
        ));
        assert!(!cli_set_value_allowed("general.language", &valid));
        assert!(!cli_set_value_allowed(
            "editor.twoRowTabs",
            &serde_json::json!({"enabled":true})
        ));
        assert!(cli_set_value_allowed(
            "general.language",
            &serde_json::json!("system")
        ));
        assert!(cli_set_value_allowed(
            "editor.twoRowTabs",
            &serde_json::json!(true)
        ));
        assert!(!cli_set_value_allowed(
            "agents.executablePath",
            &serde_json::json!("private")
        ));
    }

    #[test]
    fn safe_cli_writes_are_only_a_subset_of_shared_rust_settings() {
        let unique: HashSet<_> = SHARED_DESKTOP_PATHS.into_iter().collect();
        assert_eq!(unique.len(), SHARED_DESKTOP_PATHS.len());
        assert!(CLI_WRITABLE_PATHS.iter().all(|path| unique.contains(path)));
        assert!(cli_writable("editor.twoRowTabs"));
        assert!(cli_writable("agents.quotaUnpinnedKeys"));
        assert!(!extended_portable("editor.twoRowTabs"));
        assert!(!cli_writable("general.confirmProjectRemoval"));
        assert!(!cli_writable("general.confirmWorkspaceRemoval"));
        assert!(!cli_writable("general.autoArchiveWorkspacesAfterDays"));
        assert!(!cli_writable("agents.executablePath"));
        assert!(extended_portable("terminal.cursorBlink"));
        assert!(valid_extended_portable(
            "terminal.cursorShape",
            &serde_json::json!("underline")
        ));
        assert!(!valid_extended_portable(
            "terminal.cursorShape",
            &serde_json::json!("untrusted")
        ));
        assert!(!valid_extended_portable(
            "editor.autosaveDelaySeconds",
            &serde_json::json!(0)
        ));
        assert!(!valid_extended_portable(
            "terminal.fontFamily",
            &serde_json::json!("\n")
        ));
        assert!(valid_extended_portable(
            "editor.quickOpenExcludedDirectories",
            &quick_open_default_excluded_directories_json()
        ));
        assert!(!valid_extended_portable(
            "editor.quickOpenExcludedDirectories",
            &serde_json::json!(["target", "TARGET"])
        ));
        assert!(!valid_extended_portable(
            "editor.quickOpenExcludedDirectories",
            &serde_json::json!(["../escape"])
        ));
    }

    #[test]
    fn quota_pin_keys_accept_classic_account_ids_and_reject_duplicates_or_controls() {
        assert!(valid_quota_unpinned_keys(&serde_json::json!([
            "claude:default",
            "codex",
            "codex:secondary-profile",
            "agy",
            "opencode:go"
        ])));
        assert!(!valid_quota_unpinned_keys(&serde_json::json!([
            "claude:", "unknown"
        ])));
        assert!(!valid_quota_unpinned_keys(&serde_json::json!([
            "agy", "agy"
        ])));
        assert!(!valid_quota_unpinned_keys(&serde_json::json!([
            "codex:bad\nname"
        ])));
    }
}
