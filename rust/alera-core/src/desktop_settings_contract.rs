//! Reviewed, non-secret preferences used by both Classic Flutter and GPUI.
//! This contract is intentionally free of the runtime/sqlite feature so the
//! Native frontend, CLI and Host can all depend on one Rust Core vocabulary.
//! Adding a path requires a Rust owner/validator and Classic migration test.
pub const SHARED_DESKTOP_PATHS: [&str; 29] = [
    "general.language",
    "general.autoGitFetchEnabled",
    "general.autoGitFetchIntervalMinutes",
    "terminal.fontSize",
    "editor.autosaveEnabled",
    "editor.tabSize",
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
];

/// CLI/Agent-facing writes stay narrower than owner-backed Classic Settings.
/// An Agent cannot silently disable destructive confirmation switches.
pub const CLI_WRITABLE_PATHS: [&str; 26] = [
    "general.language",
    "general.autoGitFetchEnabled",
    "general.autoGitFetchIntervalMinutes",
    "terminal.fontSize",
    "editor.autosaveEnabled",
    "editor.tabSize",
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
];

/// Already-Classic-portable, reviewed non-secret UI fields.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn safe_cli_writes_are_only_a_subset_of_shared_rust_settings() {
        let unique: HashSet<_> = SHARED_DESKTOP_PATHS.into_iter().collect();
        assert_eq!(unique.len(), SHARED_DESKTOP_PATHS.len());
        assert!(CLI_WRITABLE_PATHS.iter().all(|path| unique.contains(path)));
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
}
