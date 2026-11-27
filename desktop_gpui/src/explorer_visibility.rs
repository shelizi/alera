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
        self.includes_with_hidden(path, is_directory, |name| is_hidden(path, name))
    }

    /// Enumeration fast path, retaining the GPUI-specific visibility policy.
    pub fn includes_entry(&self, entry: &std::fs::DirEntry, is_directory: bool) -> bool {
        let path = entry.path();
        self.includes_with_hidden(&path, is_directory, |name| {
            #[cfg(windows)]
            if !is_directory {
                use alera_core::workspace_files::desktop_listing::reusable_desktop_enumeration_metadata;
                use std::os::windows::fs::MetadataExt;
                if let Some(metadata) = reusable_desktop_enumeration_metadata(&path, || entry.metadata().ok()) {
                    return metadata.file_attributes() & 0x2 != 0;
                }
            }
            is_hidden(&path, name)
        })
    }

    fn includes_with_hidden(
        &self,
        path: &Path,
        is_directory: bool,
        hidden: impl FnOnce(&str) -> bool,
    ) -> bool {
        if !path.starts_with(&self.root) {
            return false;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return false;
        };
        if matches!(name, ".git" | ".worktrees") {
            return false;
        }
        if !self.options.show_hidden && hidden(name) {
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

    fn listing_fixture(count: usize) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        Repository::init(root.path()).unwrap();
        fs::write(root.path().join(".gitignore"), "*.tmp\n").unwrap();
        for index in 0..count {
            let name = if index % 5 == 0 {
                format!("cache-{index:05}.tmp")
            } else if index % 2 == 0 {
                format!("Item-{index:05}.rs")
            } else {
                format!("item-{index:05}.txt")
            };
            fs::write(root.path().join(name), "fixture").unwrap();
        }
        for name in ["src", "empty", "node_modules", ".worktrees", ".dot-dir"] {
            fs::create_dir(root.path().join(name)).unwrap();
        }
        for name in [".dotfile", "space name.txt", "unicode-\u{00e9}.txt"] {
            fs::write(root.path().join(name), "fixture").unwrap();
        }
        fs::write(root.path().join("src/.gitignore"), "private.rs\n").unwrap();
        fs::write(root.path().join("src/private.rs"), "ignored").unwrap();
        fs::write(root.path().join("src/public.rs"), "visible").unwrap();
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .attributes(0x2)
                .open(root.path().join("hidden.txt"))
                .unwrap();
            assert!(is_hidden(&root.path().join("hidden.txt"), "hidden.txt"));
        }
        root
    }

    fn collect_listing(
        root: &Path,
        relative: &Path,
        options: ExplorerVisibilityOptions,
        optimized: bool,
    ) -> Vec<(String, bool, PathBuf)> {
        let visibility = ExplorerVisibility::new(root, options);
        let mut rows = Vec::new();
        for entry in fs::read_dir(root.join(relative)).unwrap() {
            let entry = entry.unwrap();
            let kind = entry.file_type().unwrap();
            if kind.is_symlink() || (!kind.is_dir() && !kind.is_file()) {
                continue;
            }
            let included = if optimized {
                visibility.includes_entry(&entry, kind.is_dir())
            } else {
                visibility.includes(&entry.path(), kind.is_dir())
            };
            if included {
                let name = entry.file_name();
                rows.push((
                    name.to_string_lossy().into_owned(),
                    kind.is_dir(),
                    relative.join(name),
                ));
            }
        }
        if optimized {
            alera_core::workspace_files::desktop_listing::sort_desktop_directory_entries(
                &mut rows,
                |row| (row.0.as_str(), row.1),
            );
        } else {
            rows.sort_by(|a, b| {
                b.1.cmp(&a.1)
                    .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
                    .then_with(|| a.0.cmp(&b.0))
            });
        }
        rows
    }

    #[test]
    fn enumerated_visibility_and_cached_order_match_previous_gpui_policy() {
        let root = listing_fixture(100);
        let path = dunce::canonicalize(root.path()).unwrap();
        for relative in ["", "src"] {
            for show_ignored in [false, true] {
                for show_hidden in [false, true] {
                    let options = ExplorerVisibilityOptions {
                        show_ignored,
                        show_hidden,
                    };
                    let old = collect_listing(&path, Path::new(relative), options, false);
                    let new = collect_listing(&path, Path::new(relative), options, true);
                    assert_eq!(old, new, "{relative}: {options:?}");
                    assert!(!new
                        .iter()
                        .any(|row| row.0 == ".git" || row.0 == ".worktrees"));
                    if !show_ignored {
                        assert!(!new.iter().any(|row| row.0.ends_with(".tmp")
                            || row.0 == "node_modules"
                            || row.0 == "private.rs"));
                    }
                    #[cfg(windows)]
                    if !show_hidden {
                        assert!(!new.iter().any(|row| row.0 == "hidden.txt"));
                    }
                }
            }
        }
        let options = ExplorerVisibilityOptions {
            show_ignored: true,
            show_hidden: false,
        };
        let before = collect_listing(&path, Path::new(""), options, true);
        assert!(before.iter().any(|row| row.0 == "Item-00002.rs"));
        fs::remove_file(path.join("Item-00002.rs")).unwrap();
        fs::write(path.join("fresh.txt"), "fresh").unwrap();
        let after = collect_listing(&path, Path::new(""), options, true);
        assert_eq!(after, collect_listing(&path, Path::new(""), options, false));
        assert!(after.iter().any(|row| row.0 == "fresh.txt"));
        assert!(!after.iter().any(|row| row.0 == "Item-00002.rs"));
    }

    #[test]
    #[ignore = "filesystem performance benchmark"]
    fn benchmark_gpui_shared_listing_primitives() {
        use std::time::{Duration, Instant};
        let root = listing_fixture(1600);
        let path = dunce::canonicalize(root.path()).unwrap();
        for show_ignored in [true, false] {
            let options = ExplorerVisibilityOptions {
                show_ignored,
                show_hidden: false,
            };
            let expected = collect_listing(&path, Path::new(""), options, false);
            assert_eq!(
                expected,
                collect_listing(&path, Path::new(""), options, true)
            );
            let mut old = Vec::new();
            let mut new = Vec::new();
            for index in 0..7 {
                for optimized in if index % 2 == 0 {
                    [false, true]
                } else {
                    [true, false]
                } {
                    let started = Instant::now();
                    let rows = collect_listing(&path, Path::new(""), options, optimized);
                    let elapsed = started.elapsed();
                    assert_eq!(rows, expected);
                    if optimized {
                        new.push(elapsed);
                    } else {
                        old.push(elapsed);
                    }
                }
            }
            fn median(values: &mut [Duration]) -> f64 {
                values.sort();
                values[values.len() / 2].as_secs_f64() * 1000.0
            }
            let old = median(&mut old);
            let new = median(&mut new);
            println!("GPUI listing show_ignored={show_ignored} rows={} old_ms={old:.3} new_ms={new:.3} speedup={:.2}x", expected.len(), old / new);
        }
    }

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
