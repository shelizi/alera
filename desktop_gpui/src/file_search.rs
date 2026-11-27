//! Bounded, read-only project-wide file discovery for native Quick Open.
//! No project files are read or modified while indexing. Open-time canonical
//! containment still belongs to WorkbenchModel::open_path.
use crate::explorer_visibility::{ExplorerVisibility, ExplorerVisibilityOptions};
use alera_core::workspace_files::top_matches::TopMatches;
use std::cmp::Reverse;
use std::collections::HashSet;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const MAX_INDEXED_FILES: usize = 30_000;
const MAX_SCANNED_ENTRIES: usize = 75_000;
const MAX_DEPTH: usize = 20;

#[derive(Debug, Clone)]
pub struct WorkspaceFileIndex {
    root: PathBuf,
    files: Vec<IndexedSearchPath>,
    directories: Vec<IndexedSearchPath>,
    pub truncated: bool,
}

/// Cache query-independent text once per bounded index generation. Keep the
/// original path for ordering, display and open-time validation.
#[derive(Debug, Clone)]
struct IndexedSearchPath {
    relative: PathBuf,
    path: String,
    name: String,
    depth: usize,
    parent_names: Box<[String]>,
}

impl IndexedSearchPath {
    fn new(relative: PathBuf, is_directory: bool) -> Self {
        let path = relative.to_string_lossy().replace('\\', "/").to_lowercase();
        let name = relative
            .file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let depth = path.matches('/').count();
        // Preserve actual path components for exclusions, rather than
        // splitting the normalized search string (literal backslashes on Unix).
        // Directories only participate in Explorer search.
        let parent_names = if is_directory {
            Box::new([]) as Box<[String]>
        } else {
            relative
                .parent()
                .into_iter()
                .flat_map(Path::components)
                .filter_map(|component| match component {
                    std::path::Component::Normal(name) => {
                        Some(name.to_string_lossy().to_lowercase())
                    }
                    _ => None,
                })
                .collect()
        };
        Self {
            relative,
            path,
            name,
            depth,
            parent_names,
        }
    }

    fn score(&self, words: &[&str]) -> Option<i32> {
        let mut score = 0;
        for word in words {
            score += if self.name == *word {
                1000
            } else if self.name.starts_with(word) {
                650
            } else if self.name.contains(word) {
                400
            } else if self.path.contains(word) {
                200
            } else if is_subsequence(word, &self.path) {
                60
            } else {
                return None;
            };
        }
        Some(score)
    }

    fn excluded(&self, excluded: &HashSet<String>) -> bool {
        self.parent_names
            .iter()
            .any(|name| excluded.contains(name) || matches!(name.as_str(), ".git" | ".hg" | ".svn"))
    }
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
                    || !visibility.includes_entry(&entry, kind.is_dir())
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
            files: files
                .into_iter()
                .map(|path| IndexedSearchPath::new(path, false))
                .collect(),
            directories: directories
                .into_iter()
                .map(|path| IndexedSearchPath::new(path, true))
                .collect(),
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
        let default =
            alera_core::desktop_settings_contract::QUICK_OPEN_DEFAULT_EXCLUDED_DIRECTORIES
                .iter()
                .map(|name| (*name).to_owned())
                .collect::<Vec<_>>();
        self.search_with_exclusions(query, limit, &default)
    }

    /// Ctrl+P exclusions must not hide items from the Explorer's independent
    /// fuzzy filter, which intentionally uses the very same bounded index.
    pub fn search_with_exclusions(
        &self,
        query: &str,
        limit: usize,
        excluded_directory_names: &[String],
    ) -> Vec<PathBuf> {
        let limit = limit.min(60);
        if limit == 0 {
            return Vec::new();
        }
        let query = query.trim().to_lowercase().replace('\\', "/");
        let words: Vec<_> = query.split_whitespace().collect();
        let excluded: HashSet<_> = excluded_directory_names
            .iter()
            .map(|name| name.to_lowercase())
            .collect();
        let mut matches = TopMatches::new(limit);
        for entry in &self.files {
            if entry.excluded(&excluded) {
                continue;
            }
            if let Some(score) = entry.score(&words) {
                matches.push((Reverse(score), entry.depth, &entry.relative));
            }
        }
        matches
            .into_sorted_vec()
            .into_iter()
            .map(|(_, _, relative)| self.root.join(relative))
            .collect()
    }
    /// The original Flutter Explorer filter searches files and directories
    /// across the project even when their ancestors are collapsed. Unlike
    /// Ctrl+P, this includes folders and caps the list at 200 results.
    pub fn search_explorer(&self, query: &str, limit: usize) -> Vec<ExplorerSearchHit> {
        let limit = limit.min(200);
        let query = query.trim().to_lowercase().replace('\\', "/");
        if limit == 0 || query.is_empty() {
            return Vec::new();
        }
        let words: Vec<_> = query.split_whitespace().collect();
        let mut matches = TopMatches::new(limit);
        for (entry, is_directory) in self
            .directories
            .iter()
            .map(|entry| (entry, true))
            .chain(self.files.iter().map(|entry| (entry, false)))
        {
            if let Some(score) = entry.score(&words) {
                // Preserve directory-before-file ordering for exact key ties.
                matches.push((Reverse(score), entry.depth, &entry.relative, !is_directory));
            }
        }
        matches
            .into_sorted_vec()
            .into_iter()
            .map(|(_, _, relative, is_file)| ExplorerSearchHit {
                path: self.root.join(relative),
                is_directory: !is_file,
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

    // Test oracle: the pre-optimization search methods on the original
    // raw PathBuf index. Keep scores, sort and exclusion rules independent.
    struct LegacyIndex {
        root: PathBuf,
        files: Vec<PathBuf>,
        directories: Vec<PathBuf>,
    }

    impl LegacyIndex {
        fn search_with_exclusions(
            &self,
            query: &str,
            limit: usize,
            excluded_directory_names: &[String],
        ) -> Vec<PathBuf> {
            let query = query.trim().to_lowercase().replace('\\', "/");
            let words: Vec<_> = query.split_whitespace().filter(|s| !s.is_empty()).collect();
            let excluded = excluded_directory_names
                .iter()
                .map(|name| name.to_lowercase())
                .collect::<std::collections::HashSet<_>>();
            let mut matches: Vec<_> = self
                .files
                .iter()
                .filter(|relative| {
                    !relative.parent().is_some_and(|parent| {
                        parent.components().any(|component| {
                            if let std::path::Component::Normal(name) = component {
                                let lowercase = name.to_string_lossy().to_lowercase();
                                excluded.contains(&lowercase)
                                    || matches!(lowercase.as_str(), ".git" | ".hg" | ".svn")
                            } else {
                                false
                            }
                        })
                    })
                })
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
        fn search_explorer(&self, query: &str, limit: usize) -> Vec<ExplorerSearchHit> {
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

    fn synthetic_pair(count: usize) -> (LegacyIndex, WorkspaceFileIndex) {
        let root = PathBuf::from("fixture");
        let mut files = Vec::with_capacity(count);
        let mut directories = Vec::new();
        for index in 0..count {
            let group = match index % 11 {
                0 => ".svn",
                1 => ".hg",
                2 => ".git",
                3 => "Generated",
                _ => "Packages",
            };
            let dir = PathBuf::from(group)
                .join(format!("Feature-{:03}", index % 137))
                .join("src");
            let name = match index % 7 {
                0 => format!("TestSearch-{index:05}.rs"),
                1 => format!("testsearch-{index:05}.rs"),
                2 => format!("中文-{index:05}.ts"),
                3 => format!("\u{0130}tem-{index:05}.txt"),
                4 => format!("emoji-\u{1f680}-{index:05}.md"),
                5 => format!("Read Me-{index:05}.md"),
                _ => format!("main-{index:05}.rs"),
            };
            files.push(dir.join(name));
            directories.push(dir);
        }
        files.sort();
        directories.sort();
        directories.dedup();
        let index = WorkspaceFileIndex {
            root: root.clone(),
            files: files
                .iter()
                .cloned()
                .map(|path| IndexedSearchPath::new(path, false))
                .collect(),
            directories: directories
                .iter()
                .cloned()
                .map(|path| IndexedSearchPath::new(path, true))
                .collect(),
            truncated: false,
        };
        let legacy = LegacyIndex {
            root,
            files,
            directories,
        };
        (legacy, index)
    }

    #[test]
    fn cached_search_matches_legacy_full_sort_across_queries_and_limits() {
        let (legacy, index) = synthetic_pair(2400);
        let queries = [
            "",
            " ",
            "src",
            "TESTSEARCH",
            "tssr",
            "中文",
            "i\u{0307}tem",
            "\u{1f680}",
            "Packages\\Feature-012",
            "test rs",
            "read me",
            "zzzzmissing",
        ];
        for query in queries {
            for limit in [0, 1, 17, 60, 200, 999] {
                for exclusions in [
                    vec![],
                    vec!["PACKAGES".to_owned()],
                    vec!["generated".to_owned(), "feature-012".to_owned()],
                ] {
                    assert_eq!(
                        legacy.search_with_exclusions(query, limit, &exclusions),
                        index.search_with_exclusions(query, limit, &exclusions),
                        "quick open query={query:?} limit={limit} exclusions={exclusions:?}",
                    );
                }
                assert_eq!(
                    legacy.search_explorer(query, limit),
                    index.search_explorer(query, limit),
                    "explorer query={query:?} limit={limit}",
                );
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn cached_exclusions_do_not_split_literal_backslashes_in_unix_names() {
        let root = PathBuf::from("fixture");
        let files = vec![
            PathBuf::from("literal\\generated/find.rs"),
            PathBuf::from("generated/find.rs"),
        ];
        let index = WorkspaceFileIndex {
            root: root.clone(),
            files: files
                .iter()
                .cloned()
                .map(|path| IndexedSearchPath::new(path, false))
                .collect(),
            directories: Vec::new(),
            truncated: false,
        };
        let legacy = LegacyIndex {
            root,
            files,
            directories: Vec::new(),
        };
        let excluded = vec!["generated".to_owned()];
        assert_eq!(
            legacy.search_with_exclusions("find", 20, &excluded),
            index.search_with_exclusions("find", 20, &excluded)
        );
        assert_eq!(index.search_with_exclusions("find", 20, &excluded).len(), 1);
    }

    #[test]
    #[ignore = "synthetic in-memory search performance benchmark"]
    fn benchmark_cached_gpui_search_top_matches() {
        use std::time::{Duration, Instant};
        let (legacy, index) = synthetic_pair(30_000);
        let queries = [
            "",
            "src",
            "tssr",
            "中文",
            "Packages\\Feature-012",
            "test rs",
            "zzzzmissing",
        ];
        let excluded = vec!["generated".to_owned(), "feature-012".to_owned()];
        let expected_quick: Vec<_> = queries
            .iter()
            .map(|query| legacy.search_with_exclusions(query, 60, &excluded))
            .collect();
        let expected_explorer: Vec<_> = queries
            .iter()
            .map(|query| legacy.search_explorer(query, 200))
            .collect();
        for (i, query) in queries.iter().enumerate() {
            assert_eq!(
                expected_quick[i],
                index.search_with_exclusions(query, 60, &excluded)
            );
            assert_eq!(expected_explorer[i], index.search_explorer(query, 200));
        }
        fn median(values: &mut [Duration]) -> f64 {
            values.sort();
            values[values.len() / 2].as_secs_f64() * 1000.0
        }
        for explorer in [false, true] {
            let mut old = Vec::new();
            let mut new = Vec::new();
            for sample in 0..7 {
                for optimized in if sample % 2 == 0 {
                    [false, true]
                } else {
                    [true, false]
                } {
                    let started = Instant::now();
                    if explorer {
                        let actual: Vec<_> = queries
                            .iter()
                            .map(|query| {
                                if optimized {
                                    index.search_explorer(query, 200)
                                } else {
                                    legacy.search_explorer(query, 200)
                                }
                            })
                            .collect();
                        let elapsed = started.elapsed();
                        assert_eq!(actual, expected_explorer);
                        if optimized {
                            new.push(elapsed);
                        } else {
                            old.push(elapsed);
                        }
                    } else {
                        let actual: Vec<_> = queries
                            .iter()
                            .map(|query| {
                                if optimized {
                                    index.search_with_exclusions(query, 60, &excluded)
                                } else {
                                    legacy.search_with_exclusions(query, 60, &excluded)
                                }
                            })
                            .collect();
                        let elapsed = started.elapsed();
                        assert_eq!(actual, expected_quick);
                        if optimized {
                            new.push(elapsed);
                        } else {
                            old.push(elapsed);
                        }
                    }
                }
            }
            let old = median(&mut old);
            let new = median(&mut new);
            println!("GPUI search explorer={explorer} files={} dirs={} queries={} old_ms={old:.3} new_ms={new:.3} speedup={:.2}x", index.files.len(), index.directories.len(), queries.len(), old/new);
        }
        let mut construction = Vec::new();
        for _ in 0..7 {
            let started = Instant::now();
            let files: Vec<_> = legacy
                .files
                .iter()
                .cloned()
                .map(|path| IndexedSearchPath::new(path, false))
                .collect();
            let dirs: Vec<_> = legacy
                .directories
                .iter()
                .cloned()
                .map(|path| IndexedSearchPath::new(path, true))
                .collect();
            construction.push(started.elapsed());
            assert_eq!(files.len(), index.files.len());
            assert_eq!(dirs.len(), index.directories.len());
        }
        let payload_bytes: usize = index
            .files
            .iter()
            .chain(&index.directories)
            .map(|entry| {
                std::mem::size_of::<IndexedSearchPath>() - std::mem::size_of::<PathBuf>()
                    + entry.path.capacity()
                    + entry.name.capacity()
                    + entry.parent_names.len() * std::mem::size_of::<String>()
                    + entry
                        .parent_names
                        .iter()
                        .map(String::capacity)
                        .sum::<usize>()
            })
            .sum();
        println!(
            "GPUI cached index construction_ms={:.3} estimated_extra_payload_bytes={payload_bytes}",
            median(&mut construction)
        );
    }

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
        // When hidden files are explicitly enabled, .gitignore is a valid
        // searchable file. Only .git control metadata must stay excluded.
        let git_hits = visible.search_explorer(".git", 200);
        assert!(git_hits
            .iter()
            .any(|entry| entry.path.ends_with(".gitignore")));
        assert!(git_hits.iter().all(|entry| {
            entry
                .path
                .strip_prefix(visible.root())
                .unwrap()
                .components()
                .all(|component| component.as_os_str() != std::ffi::OsStr::new(".git"))
        }));
    }

    #[test]
    fn ctrl_p_excludes_host_names_but_explorer_search_still_finds_visible_files() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("packages/MyNested")).unwrap();
        fs::create_dir_all(root.path().join("src")).unwrap();
        fs::create_dir_all(root.path().join("generated")).unwrap();
        fs::create_dir_all(root.path().join(".svn/private")).unwrap();
        fs::write(root.path().join("packages/MyNested/find.rs"), "code").unwrap();
        fs::write(root.path().join("src/find.rs"), "code").unwrap();
        fs::write(root.path().join("generated/find.rs"), "code").unwrap();
        fs::write(root.path().join(".svn/private/find.rs"), "code").unwrap();
        let index = WorkspaceFileIndex::scan_with_options(
            root.path(),
            ExplorerVisibilityOptions {
                show_hidden: true,
                show_ignored: true,
            },
        )
        .unwrap();
        let excluded = vec!["PACKAGES".to_owned(), "generated".to_owned()];
        let matches = index.search_with_exclusions("find", 18, &excluded);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].ends_with("src/find.rs"));
        // Explorer's independent search is not filtered by Ctrl+P exclusions.
        let explorer = index.search_explorer("find", 40);
        assert!(explorer
            .iter()
            .any(|hit| hit.path.ends_with("packages/MyNested/find.rs")));
        assert!(explorer
            .iter()
            .any(|hit| hit.path.ends_with("generated/find.rs")));
        let cleared = index.search_with_exclusions("find", 18, &[]);
        assert_eq!(cleared.len(), 3);
        assert!(cleared
            .iter()
            .all(|path| !path.to_string_lossy().contains(".svn")));
        // Config changes take effect without rescanning Explorer's index.
        assert!(index
            .search_with_exclusions("find", 18, &["src".to_owned()])
            .iter()
            .any(|path| path.ends_with("generated/find.rs")));
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
