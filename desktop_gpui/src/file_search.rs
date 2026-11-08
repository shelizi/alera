//! Bounded, read-only project-wide file discovery for native Quick Open.
//! No project files are read or modified while indexing. Open-time canonical
//! containment still belongs to WorkbenchModel::open_path.
use crate::explorer_visibility::{ExplorerVisibility, ExplorerVisibilityOptions};

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const MAX_INDEXED_FILES: usize = 30_000;
const MAX_SCANNED_ENTRIES: usize = 75_000;
const MAX_DEPTH: usize = 20;

#[derive(Debug, Clone)]
pub struct WorkspaceFileIndex {
    root: PathBuf,
    files: Vec<PathBuf>,
    directories: Vec<PathBuf>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorerSearchHit {
    pub path: PathBuf,
    pub is_directory: bool,
}

impl WorkspaceFileIndex {
    pub fn scan(root: &Path) -> io::Result<Self> {
        Self::scan_with_options(root, ExplorerVisibilityOptions::default())
    }

    pub fn scan_with_options(root: &Path, options: ExplorerVisibilityOptions) -> io::Result<Self> {
        let root = dunce::canonicalize(root)?;
        if !root.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Not a workspace",
            ));
        }
        let visibility = ExplorerVisibility::new(&root, options);
        let mut stack = vec![(root.clone(), 0)];
        let mut files = Vec::new();
        let mut directories = Vec::new();
        let mut visited = 0;
        let mut truncated = false;
        while let Some((directory, depth)) = stack.pop() {
            let entries = match fs::read_dir(&directory) {
                Ok(entries) => entries,
                Err(error) if error.kind() == io::ErrorKind::PermissionDenied => continue,
                Err(error) => return Err(error),
            };
            for entry in entries {
                let entry = entry?;
                visited += 1;
                if visited > MAX_SCANNED_ENTRIES {
                    truncated = true;
                    break;
                }
                let kind = entry.file_type()?;
                // Never follow directory or file symlinks out of the workspace.
                if kind.is_symlink() {
                    continue;
                }
                if (!kind.is_dir() && !kind.is_file())
                    || !visibility.includes(&entry.path(), kind.is_dir())
                {
                    continue;
                }
                if kind.is_dir() {
                    directories.push(entry.path().strip_prefix(&root).unwrap().to_path_buf());
                }
                if kind.is_dir() && depth < MAX_DEPTH {
                    stack.push((entry.path(), depth + 1));
                } else if kind.is_file() {
                    files.push(entry.path().strip_prefix(&root).unwrap().to_path_buf());
                    if files.len() >= MAX_INDEXED_FILES {
                        truncated = true;
                        break;
                    }
                }
            }
            if truncated {
                break;
            }
        }
        files.sort();
        directories.sort();
        Ok(Self {
            root,
            files,
            directories,
            truncated,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn total(&self) -> usize {
        self.files.len()
    }

    /// A case-insensitive Unicode filename/path fuzzy matcher. Prefer exact
    /// filename, prefix, word-boundary, then subsequence matches.
    pub fn search(&self, query: &str, limit: usize) -> Vec<PathBuf> {
        let query = query.trim().to_lowercase().replace('\\', "/");
        let words: Vec<_> = query.split_whitespace().filter(|s| !s.is_empty()).collect();
        let mut matches: Vec<_> = self
            .files
            .iter()
            .filter_map(|relative| {
                let path = relative.to_string_lossy().replace('\\', "/").to_lowercase();
                let name = relative.file_name()?.to_string_lossy().to_lowercase();
                let mut score = 0;
                for word in &words {
                    if name == *word {
                        score += 1000;
                    } else if name.starts_with(word) {
                        score += 650;
                    } else if name.contains(word) {
                        score += 400;
                    } else if path.contains(word) {
                        score += 200;
                    } else if is_subsequence(word, &path) {
                        score += 60;
                    } else {
                        return None;
                    }
                }
                // Prefer shallow paths when scores are otherwise equal.
                Some((score, path.matches('/').count(), relative))
            })
            .collect();
        matches.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(b.2)));
        matches
            .into_iter()
            .take(limit.min(60))
            .map(|(_, _, relative)| self.root.join(relative))
            .collect()
    }
    /// The original Flutter Explorer filter searches files and directories
    /// across the project even when their ancestors are collapsed. Unlike
    /// Ctrl+P, this includes folders and caps the list at 200 results.
    pub fn search_explorer(&self, query: &str, limit: usize) -> Vec<ExplorerSearchHit> {
        let query = query.trim().to_lowercase().replace('\\', "/");
        if query.is_empty() {
            return Vec::new();
        }
        let words: Vec<_> = query.split_whitespace().filter(|s| !s.is_empty()).collect();
        let mut matches = self
            .directories
            .iter()
            .map(|path| (path, true))
            .chain(self.files.iter().map(|path| (path, false)))
            .filter_map(|(relative, is_directory)| {
                let path = relative.to_string_lossy().replace('\\', "/").to_lowercase();
                let name = relative.file_name()?.to_string_lossy().to_lowercase();
                let mut score = 0;
                for word in &words {
                    if name == *word {
                        score += 1000;
                    } else if name.starts_with(word) {
                        score += 650;
                    } else if name.contains(word) {
                        score += 400;
                    } else if path.contains(word) {
                        score += 200;
                    } else if is_subsequence(word, &path) {
                        score += 60;
                    } else {
                        return None;
                    }
                }
                Some((score, path.matches('/').count(), relative, is_directory))
            })
            .collect::<Vec<_>>();
        matches.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(b.2)));
        matches
            .into_iter()
            .take(limit.min(200))
            .map(|(_, _, relative, is_directory)| ExplorerSearchHit {
                path: self.root.join(relative),
                is_directory,
            })
            .collect()
    }
}

fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut haystack = haystack.chars();
    needle.chars().all(|ch| haystack.by_ref().any(|c| c == ch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filtered_results_and_quick_open_follow_ignored_visibility_mode() {
        let root = tempfile::tempdir().unwrap();
        git2::Repository::init(root.path()).unwrap();
        fs::write(root.path().join(".gitignore"), "ignored/\n*.tmp\n").unwrap();
        fs::create_dir(root.path().join("ignored")).unwrap();
        fs::write(root.path().join("ignored/file.rs"), "ignore").unwrap();
        fs::write(root.path().join("something.tmp"), "ignore").unwrap();
        fs::write(root.path().join("kept.rs"), "keep").unwrap();
        let hidden = WorkspaceFileIndex::scan(root.path()).unwrap();
        assert_eq!(hidden.search("kept", 20).len(), 1);
        assert!(hidden.search("file", 20).is_empty());
        assert!(hidden.search_explorer("ignored", 20).is_empty());
        let visible = WorkspaceFileIndex::scan_with_options(
            root.path(),
            ExplorerVisibilityOptions {
                show_hidden: true,
                show_ignored: true,
            },
        )
        .unwrap();
        assert_eq!(visible.search("file", 20).len(), 1);
        assert_eq!(visible.search_explorer("ignored", 20).len(), 2);
        assert!(visible.search_explorer(".git", 200).is_empty());
    }

    #[test]
    fn finds_nested_unicode_and_fuzzy_names_without_indexing_generated_dirs() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("src/nested")).unwrap();
        fs::create_dir_all(root.path().join("node_modules/hidden")).unwrap();
        fs::write(root.path().join("src/nested/女主角.ts"), b"test").unwrap();
        fs::write(root.path().join("src/nested/TestSearch.rs"), b"test").unwrap();
        fs::write(root.path().join("node_modules/hidden/nope.ts"), b"test").unwrap();
        let index = WorkspaceFileIndex::scan(root.path()).unwrap();
        assert_eq!(index.total(), 2);
        assert_eq!(index.search("女主", 10).len(), 1);
        assert_eq!(index.search("tssr", 10).len(), 1);
        assert!(index.search("nope", 10).is_empty());
        assert_eq!(index.search("missing", 10).len(), 0);
        let filter = index.search_explorer("src", 200);
        assert!(filter
            .iter()
            .any(|entry| entry.is_directory && entry.path.ends_with("src")));
        assert!(filter
            .iter()
            .any(|entry| entry.is_directory && entry.path.ends_with("src/nested")));
        assert_eq!(index.search_explorer("女主", 200).len(), 1);
        assert!(index.search_explorer("node_modules", 200).is_empty());
        assert!(index.search_explorer(" ", 200).is_empty());
        assert_eq!(index.search_explorer("src", 1).len(), 1);
    }
}
