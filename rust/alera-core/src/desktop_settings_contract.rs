//! Reviewed, non-secret preferences used by both Classic Flutter and GPUI.
//! This contract is intentionally free of the runtime/sqlite feature so the
//! Native frontend, CLI and Host can all depend on one Rust Core vocabulary.
//! Adding a path requires a Rust owner/validator and Classic migration test.
pub const SHARED_DESKTOP_PATHS: [&str; 12] = [
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
];

/// CLI/Agent-facing writes stay narrower than owner-backed Classic Settings.
/// An Agent cannot silently disable destructive confirmation switches.
pub const CLI_WRITABLE_PATHS: [&str; 9] = [
    "general.language",
    "general.autoGitFetchEnabled",
    "general.autoGitFetchIntervalMinutes",
    "terminal.fontSize",
    "editor.autosaveEnabled",
    "editor.tabSize",
    "agents.defaultAgentProfileId",
    "explorer.showHiddenFiles",
    "explorer.showIgnoredFiles",
];

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
    }
}
