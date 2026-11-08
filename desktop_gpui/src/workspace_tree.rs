//! Lazy, read-only workspace tree for the native GPUI Explorer.
//! The UI owns selection; this model owns expansion and safe visible rows.
use crate::explorer_visibility::{ExplorerVisibility, ExplorerVisibilityOptions};
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

pub const MAX_TREE_ROWS: usize = 4_000;
const MAX_TREE_DEPTH: usize = 24;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeRow {
    pub relative_path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub is_directory: bool,
    pub expanded: bool,
}

#[derive(Debug)]
pub struct WorkspaceTree {
    root: PathBuf,
    visibility: ExplorerVisibility,
    expanded: BTreeSet<PathBuf>,
    rows: Vec<TreeRow>,
    truncated: bool,
}

impl WorkspaceTree {
    pub fn new(root: &Path) -> io::Result<Self> {
        Self::new_with_options(root, ExplorerVisibilityOptions::default())
    }

    pub fn new_with_options(root: &Path, options: ExplorerVisibilityOptions) -> io::Result<Self> {
        let root = dunce::canonicalize(root)?;
        if !root.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Not a workspace",
            ));
        }
        let mut tree = Self {
            visibility: ExplorerVisibility::new(&root, options),
            root,
            expanded: BTreeSet::new(),
            rows: Vec::new(),
            truncated: false,
        };
        tree.refresh()?;
        Ok(tree)
    }

    pub fn set_visibility_options(&mut self, options: ExplorerVisibilityOptions) -> io::Result<()> {
        if self.visibility.options() == options {
            return Ok(());
        }
        let previous = std::mem::replace(
            &mut self.visibility,
            ExplorerVisibility::new(&self.root, options),
        );
        if let Err(error) = self.refresh() {
            self.visibility = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn rows(&self) -> &[TreeRow] {
        &self.rows
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// Mirrors Flutter Classic Explorer's Collapse All without replacing the
    /// workspace, clearing the selected file, or mutating the filesystem.
    pub fn collapse_all(&mut self) -> io::Result<()> {
        self.expanded.clear();
        self.refresh()
    }

    /// Unlike toggle, a context-menu Collapse operation may never expand a
    /// folder that was already closed.
    pub fn collapse(&mut self, relative: &Path) -> io::Result<bool> {
        if !self.checked_relative(relative)?.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Not a directory",
            ));
        }
        if !self.expanded.remove(relative) {
            return Ok(false);
        }
        self.refresh()?;
        Ok(true)
    }

    /// Filtered folder hits can be nested beneath collapsed ancestors.
    /// Reveal the folder in its original tree, then expand it once.
    pub fn reveal_directory(&mut self, relative: &Path) -> io::Result<()> {
        if !self.checked_relative(relative)?.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Not a directory",
            ));
        }
        let mut ancestor = Some(relative);
        while let Some(directory) = ancestor {
            if directory.as_os_str().is_empty() {
                break;
            }
            self.expanded.insert(directory.to_path_buf());
            ancestor = directory.parent();
        }
        self.refresh()
    }

    /// Never expand a symlink, a path outside the workspace, or a file.
    pub fn toggle(&mut self, relative: &Path) -> io::Result<()> {
        if !self.checked_relative(relative)?.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Not a directory",
            ));
        }
        if !self.expanded.insert(relative.to_path_buf()) {
            self.expanded.remove(relative);
        }
        self.refresh()
    }

    /// Reveal a file by expanding only its ancestors, without opening it.
    /// False means the entry is excluded or beyond the bounded visible rows.
    pub fn reveal(&mut self, relative: &Path) -> io::Result<bool> {
        if !self.checked_relative(relative)?.is_file() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Not a file"));
        }
        let mut ancestor = relative.parent();
        while let Some(directory) = ancestor {
            if directory.as_os_str().is_empty() {
                break;
            }
            self.expanded.insert(directory.to_path_buf());
            ancestor = directory.parent();
        }
        self.refresh()?;
        Ok(self.rows.iter().any(|row| row.relative_path == relative))
    }

    /// Preserve the last complete viewport if an I/O error interrupts refresh.
    pub fn refresh(&mut self) -> io::Result<()> {
        let mut rows = Vec::new();
        let mut truncated = false;
        self.load_children(Path::new(""), 0, &mut rows, &mut truncated)?;
        self.rows = rows;
        self.truncated = truncated;
        Ok(())
    }

    fn checked_relative(&self, relative: &Path) -> io::Result<fs::Metadata> {
        if relative.as_os_str().is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Empty path"));
        }
        let mut candidate = self.root.clone();
        for component in relative.components() {
            let Component::Normal(part) = component else {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Invalid path",
                ));
            };
            candidate.push(part);
            if fs::symlink_metadata(&candidate)?.file_type().is_symlink() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Symlink refused",
                ));
            }
        }
        if !dunce::canonicalize(&candidate)?.starts_with(&self.root) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Outside workspace",
            ));
        }
        fs::metadata(candidate)
    }

    fn load_children(
        &self,
        relative: &Path,
        depth: usize,
        rows: &mut Vec<TreeRow>,
        truncated: &mut bool,
    ) -> io::Result<()> {
        if depth >= MAX_TREE_DEPTH {
            *truncated = true;
            return Ok(());
        }
        let mut entries = Vec::new();
        let directory = match fs::read_dir(self.root.join(relative)) {
            Ok(directory) => directory,
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => return Ok(()),
            Err(error) => return Err(error),
        };
        for entry in directory {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let kind = entry.file_type()?;
            if kind.is_symlink() || (!kind.is_dir() && !kind.is_file()) {
                continue;
            }
            if !self.visibility.includes(&entry.path(), kind.is_dir()) {
                continue;
            }
            entries.push((name, kind.is_dir(), relative.join(entry.file_name())));
        }
        entries.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
                .then_with(|| a.0.cmp(&b.0))
        });
        for (name, is_directory, path) in entries {
            if rows.len() >= MAX_TREE_ROWS {
                *truncated = true;
                break;
            }
            let expanded = is_directory && self.expanded.contains(&path);
            rows.push(TreeRow {
                relative_path: path.clone(),
                name,
                depth,
                is_directory,
                expanded,
            });
            if expanded {
                self.load_children(&path, depth + 1, rows, truncated)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_gitignore_visibility_keeps_expanded_folders_and_safe_root() {
        let root = tempfile::tempdir().unwrap();
        git2::Repository::init(root.path()).unwrap();
        fs::write(root.path().join(".gitignore"), "ignored/\n").unwrap();
        fs::create_dir_all(root.path().join("src/inner")).unwrap();
        fs::create_dir(root.path().join("ignored")).unwrap();
        fs::write(root.path().join("ignored/data.txt"), "x").unwrap();
        fs::write(root.path().join("src/inner/code.rs"), "code").unwrap();
        let mut tree = WorkspaceTree::new(root.path()).unwrap();
        tree.reveal(Path::new("src/inner/code.rs")).unwrap();
        assert!(!tree.rows().iter().any(|row| row.name == "ignored"));
        tree.set_visibility_options(ExplorerVisibilityOptions {
            show_ignored: true,
            show_hidden: true,
        })
        .unwrap();
        assert!(tree.rows().iter().any(|row| row.name == "ignored"));
        assert!(tree.rows().iter().any(|row| row.name == "code.rs"));
        assert!(!tree.rows().iter().any(|row| row.name == ".git"));
        tree.set_visibility_options(ExplorerVisibilityOptions::default())
            .unwrap();
        assert!(!tree.rows().iter().any(|row| row.name == "ignored"));
        assert!(tree.rows().iter().any(|row| row.name == "code.rs"));
    }

    #[test]
    fn lazy_expansion_and_reveal_keep_paths_and_order_stable() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src/nested")).unwrap();
        fs::create_dir_all(dir.path().join("node_modules/skip")).unwrap();
        fs::write(dir.path().join("src/nested/中文.rs"), "hello").unwrap();
        fs::write(dir.path().join("z.txt"), "hello").unwrap();
        fs::write(dir.path().join("node_modules/skip/a.rs"), "x").unwrap();
        let mut tree = WorkspaceTree::new(dir.path()).unwrap();
        assert_eq!(tree.rows().len(), 2);
        assert_eq!(tree.rows()[0].name, "src");
        assert_eq!(tree.rows()[0].depth, 0);
        assert!(tree.reveal(Path::new("src/nested/中文.rs")).unwrap());
        assert_eq!(tree.rows().len(), 4);
        assert_eq!(tree.rows()[3].name, "z.txt");
        tree.toggle(Path::new("src")).unwrap();
        assert_eq!(tree.rows().len(), 2);
        tree.toggle(Path::new("src")).unwrap();
        assert_eq!(tree.rows().len(), 4);
        assert!(tree.rows()[0].expanded);
        assert!(!tree.truncated());
    }

    #[test]
    fn rejects_outside_and_non_directory_expansions_without_writes() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("file.txt"), "unchanged").unwrap();
        let mut tree = WorkspaceTree::new(dir.path()).unwrap();
        for path in ["../file.txt", "src/../../file.txt"] {
            assert!(tree.toggle(Path::new(path)).is_err());
        }
        assert!(tree.toggle(Path::new("file.txt")).is_err());
        assert_eq!(
            fs::read_to_string(dir.path().join("file.txt")).unwrap(),
            "unchanged"
        );
    }

    #[test]
    fn classic_collapse_all_and_filter_reveal_folder_preserve_file_data() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src/nested")).unwrap();
        fs::write(dir.path().join("src/nested/data.rs"), "safe").unwrap();
        let mut tree = WorkspaceTree::new(dir.path()).unwrap();
        tree.reveal_directory(Path::new("src/nested")).unwrap();
        assert!(tree.rows().iter().any(|row| row.name == "data.rs"));
        assert!(tree
            .rows()
            .iter()
            .any(|row| row.name == "nested" && row.expanded));
        assert!(tree.collapse(Path::new("src/nested")).unwrap());
        assert!(!tree.collapse(Path::new("src/nested")).unwrap());
        assert!(!tree.rows().iter().any(|row| row.name == "data.rs"));
        tree.reveal_directory(Path::new("src/nested")).unwrap();
        tree.collapse_all().unwrap();
        assert_eq!(tree.rows().len(), 1);
        assert!(!tree.rows()[0].expanded);
        assert!(tree
            .reveal_directory(Path::new("src/nested/data.rs"))
            .is_err());
        assert!(tree.reveal_directory(Path::new("../src")).is_err());
        assert_eq!(
            fs::read_to_string(dir.path().join("src/nested/data.rs")).unwrap(),
            "safe"
        );
    }
}
