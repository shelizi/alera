//! Shared Explorer visibility rules for the tree and filtered file index.
//! This never follows symlinks and never exposes Git metadata directories.
use git2::Repository;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExplorerVisibilityOptions {
    pub show_ignored: bool,
    pub show_hidden: bool,
}

impl Default for ExplorerVisibilityOptions {
    fn default() -> Self {
        Self {
            show_ignored: false,
            // Flutter Classic displays this toggle only on Windows.
            show_hidden: !cfg!(windows),
        }
    }
}

pub struct ExplorerVisibility {
    root: PathBuf,
    options: ExplorerVisibilityOptions,
    repository: Option<Repository>,
    git_workdir: Option<PathBuf>,
}

impl std::fmt::Debug for ExplorerVisibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExplorerVisibility")
            .field("root", &self.root)
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

impl ExplorerVisibility {
    pub fn new(root: &Path, options: ExplorerVisibilityOptions) -> Self {
        let repository = Repository::discover(root).ok();
        let git_workdir = repository
            .as_ref()
            .and_then(|repository| repository.workdir())
            .map(|path| dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()));
        Self {
            root: root.to_path_buf(),
            options,
            repository,
            git_workdir,
        }
    }

    pub fn options(&self) -> ExplorerVisibilityOptions {
        self.options
    }

    pub fn includes(&self, path: &Path, is_directory: bool) -> bool {
        if !path.starts_with(&self.root) {
            return false;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return false;
        };
        if matches!(name, ".git" | ".worktrees") {
            return false;
        }
        if !self.options.show_hidden && is_hidden(path, name) {
            return false;
        }
        if !self.options.show_ignored {
            if is_generated(name) {
                return false;
            }
            if let (Some(repository), Some(workdir)) = (&self.repository, &self.git_workdir) {
                if let Ok(relative) = path.strip_prefix(workdir) {
                    if repository.status_should_ignore(relative).unwrap_or(false) {
                        return false;
                    }
                }
            }
        }
        // Explicitly record directory-ness at call sites; visibility itself
        // must never traverse into a directory or modify any entry.
        let _ = is_directory;
        true
    }
}

fn is_generated(name: &str) -> bool {
    matches!(
        name,
        "node_modules"
            | "target"
            | "dist"
            | "build"
            | ".next"
            | ".dart_tool"
            | ".venv"
            | "vendor"
            | ".idea"
    )
}

fn is_hidden(path: &Path, name: &str) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // Windows FILE_ATTRIBUTE_HIDDEN; a .gitignore file is not hidden
        // merely because its name begins with a dot.
        let _ = name;
        std::fs::metadata(path).is_ok_and(|metadata| metadata.file_attributes() & 0x2 != 0)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        name.starts_with('.')
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn gitignore_toggle_shows_and_hides_nested_ignored_entries() {
        let root = tempfile::tempdir().unwrap();
        Repository::init(root.path()).unwrap();
        fs::write(root.path().join(".gitignore"), "ignored/\n*.tmp\n").unwrap();
        fs::create_dir(root.path().join("ignored")).unwrap();
        fs::write(root.path().join("note.tmp"), "ignored").unwrap();
        fs::write(root.path().join("keep.txt"), "visible").unwrap();
        let hidden = ExplorerVisibility::new(root.path(), ExplorerVisibilityOptions::default());
        assert!(!hidden.includes(&root.path().join("ignored"), true));
        assert!(!hidden.includes(&root.path().join("note.tmp"), false));
        assert!(hidden.includes(&root.path().join("keep.txt"), false));
        let show_all = ExplorerVisibility::new(
            root.path(),
            ExplorerVisibilityOptions {
                show_ignored: true,
                show_hidden: true,
            },
        );
        assert!(show_all.includes(&root.path().join("ignored"), true));
        assert!(show_all.includes(&root.path().join("note.tmp"), false));
        assert!(!show_all.includes(&root.path().join(".git"), true));
    }
}
