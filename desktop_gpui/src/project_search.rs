//! Bounded, read-only, case-sensitive literal content search, independent of GPUI.
//! Run on a background worker. Results describe disk contents, not unsaved edits.
//! Paths are checked before traversal/read; this is not an atomic filesystem
//! snapshot or a sandbox against concurrent hostile directory replacement.

use std::fs::{self, File, Metadata};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

const MAX_QUERY_BYTES: usize = 4096;
const MAX_SNIPPET_CHARS: usize = 240;

/// Callers may lower limits. Values above the defaults are clamped to them.
#[derive(Debug, Clone, Copy)]
pub struct SearchLimits {
    pub max_entries: usize,
    pub max_files: usize,
    pub max_results: usize,
    pub max_file_bytes: usize,
    pub max_total_bytes: usize,
    /// Root files are at depth zero.
    pub max_depth: usize,
}

impl Default for SearchLimits {
    fn default() -> Self {
        Self {
            max_entries: 75_000,
            max_files: 10_000,
            max_results: 1000,
            max_file_bytes: 256 * 1024,
            max_total_bytes: 32 * 1024 * 1024,
            max_depth: 20,
        }
    }
}

impl SearchLimits {
    fn bounded(self) -> Self {
        let cap = Self::default();
        Self {
            max_entries: self.max_entries.min(cap.max_entries),
            max_files: self.max_files.min(cap.max_files),
            max_results: self.max_results.min(cap.max_results),
            max_file_bytes: self.max_file_bytes.min(cap.max_file_bytes),
            max_total_bytes: self.max_total_bytes.min(cap.max_total_bytes),
            max_depth: self.max_depth.min(cap.max_depth),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentMatch {
    /// Workspace-relative path; revalidate with the workbench before opening.
    pub path: PathBuf,
    /// One-based line and Unicode scalar column, not byte or UTF-16 offsets.
    pub line: usize,
    pub column: usize,
    /// At most 240 Unicode scalars, starting up to 80 scalars before the match.
    pub snippet: String,
}

#[derive(Debug, Default)]
pub struct SearchResults {
    pub matches: Vec<ContentMatch>,
    /// A work limit was reached, or an entry could not be inspected.
    /// Reaching a limit is conservative even if no further matches exist.
    pub truncated: bool,
    pub entries_visited: usize,
    /// Regular-file candidates, including binary and oversized files.
    pub files_visited: usize,
    /// All bytes read, including rejected files and oversize detection bytes.
    pub bytes_read: usize,
}

#[derive(Debug, Clone)]
pub struct ProjectSearch {
    root: PathBuf,
}

impl ProjectSearch {
    pub fn new(root: &Path) -> io::Result<Self> {
        let search = Self {
            root: dunce::canonicalize(root)?,
        };
        if !search.checked_metadata(&search.root)?.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Not a workspace",
            ));
        }
        Ok(search)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn search(&self, query: &str) -> io::Result<SearchResults> {
        self.search_with_limits(query, SearchLimits::default())
    }

    /// Single-line literal matching; whitespace is significant, empty is a no-op.
    /// Occurrences do not overlap. Files must be UTF-8 without binary controls.
    /// No ignore-file parsing: only the explicit generated-directory list applies.
    /// Results are sorted, but a truncated subset depends on filesystem order.
    pub fn search_with_limits(
        &self,
        query: &str,
        limits: SearchLimits,
    ) -> io::Result<SearchResults> {
        if query.len() > MAX_QUERY_BYTES || query.contains(['\n', '\r']) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Query must be a single line of at most 4096 bytes",
            ));
        }
        let mut run = SearchRun {
            search: self,
            query,
            limits: limits.bounded(),
            result: SearchResults::default(),
        };
        if query.is_empty() {
            return Ok(run.result);
        }
        if !self.checked_metadata(&self.root)?.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Not a workspace",
            ));
        }
        run.visit_directory(&self.root, 0)?;
        run.result.matches.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then(a.line.cmp(&b.line))
                .then(a.column.cmp(&b.column))
        });
        Ok(run.result)
    }

    fn checked_metadata(&self, path: &Path) -> io::Result<Metadata> {
        let relative = path.strip_prefix(&self.root).map_err(|_| blocked_path())?;
        if relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(blocked_path());
        }
        // Check ancestors too: a directory may have changed since enumeration.
        for ancestor in path.ancestors() {
            if is_link(&fs::symlink_metadata(ancestor)?) {
                return Err(blocked_path());
            }
        }
        if dunce::canonicalize(path)? != path {
            return Err(blocked_path());
        }
        fs::symlink_metadata(path)
    }
}

struct SearchRun<'a> {
    search: &'a ProjectSearch,
    query: &'a str,
    limits: SearchLimits,
    result: SearchResults,
}

impl SearchRun<'_> {
    fn exhausted(&mut self) -> bool {
        let stop = self.result.entries_visited >= self.limits.max_entries
            || self.result.files_visited >= self.limits.max_files
            || self.result.matches.len() >= self.limits.max_results
            || self.result.bytes_read >= self.limits.max_total_bytes;
        self.result.truncated |= stop;
        stop
    }

    fn visit_directory(&mut self, directory: &Path, depth: usize) -> io::Result<()> {
        if self.exhausted() {
            return Ok(());
        }
        let entries = self
            .search
            .checked_metadata(directory)
            .and_then(|metadata| {
                if metadata.is_dir() {
                    fs::read_dir(directory)
                } else {
                    Err(blocked_path())
                }
            });
        let entries = match entries {
            Ok(entries) => entries,
            Err(error) if directory == self.search.root => return Err(error),
            Err(_) => {
                self.result.truncated = true;
                return Ok(());
            }
        };
        // Stream directory entries; never collect an unbounded directory in RAM.
        for entry in entries {
            if self.exhausted() {
                break;
            }
            self.result.entries_visited += 1;
            let inspected = entry.and_then(|entry| {
                let path = entry.path();
                let metadata = fs::symlink_metadata(&path)?;
                Ok((path, metadata))
            });
            let (path, metadata) = match inspected {
                Ok(entry) => entry,
                Err(_) => {
                    self.result.truncated = true;
                    continue;
                }
            };
            if is_link(&metadata) {
                continue;
            }
            if metadata.is_dir() {
                if generated_directory(&path) {
                    continue;
                }
                if depth >= self.limits.max_depth {
                    self.result.truncated = true;
                } else {
                    self.visit_directory(&path, depth + 1)?;
                }
            } else if metadata.is_file() {
                self.result.files_visited += 1;
                if self.search_file(&path).is_err() {
                    self.result.truncated = true;
                }
            }
        }
        Ok(())
    }

    fn search_file(&mut self, path: &Path) -> io::Result<()> {
        let metadata = self.search.checked_metadata(path)?;
        if is_link(&metadata)
            || !metadata.is_file()
            || metadata.len() > self.limits.max_file_bytes as u64
        {
            return Ok(());
        }
        let file = File::open(path)?;
        let opened = file.metadata()?;
        if !opened.is_file() || is_link(&opened) {
            return Err(blocked_path());
        }
        self.search.checked_metadata(path)?;
        let remaining = self.limits.max_total_bytes - self.result.bytes_read;
        let cap = remaining.min(self.limits.max_file_bytes + 1);
        let mut bytes = Vec::new();
        let read = file.take(cap as u64).read_to_end(&mut bytes);
        self.result.bytes_read += bytes.len();
        read?;
        if bytes.len() > self.limits.max_file_bytes {
            return Ok(());
        }
        if opened.len() > bytes.len() as u64
            || (bytes.len() == cap && cap < self.limits.max_file_bytes + 1)
        {
            self.result.truncated = true;
            return Ok(());
        }
        if bytes
            .iter()
            .any(|byte| matches!(byte, 0..=8 | 11..=12 | 14..=31 | 127))
        {
            return Ok(());
        }
        let Ok(text) = std::str::from_utf8(&bytes) else {
            return Ok(());
        };
        let relative = path
            .strip_prefix(&self.search.root)
            .map_err(|_| blocked_path())?;
        for (line_number, line) in text.lines().enumerate() {
            let mut previous_byte = 0;
            let mut column = 1;
            for (byte, _) in line.match_indices(self.query) {
                column += line[previous_byte..byte].chars().count();
                previous_byte = byte;
                let start = line[..byte]
                    .char_indices()
                    .rev()
                    .nth(79)
                    .map_or(0, |(byte, _)| byte);
                self.result.matches.push(ContentMatch {
                    path: relative.to_path_buf(),
                    line: line_number + 1,
                    column,
                    snippet: line[start..].chars().take(MAX_SNIPPET_CHARS).collect(),
                });
                if self.result.matches.len() >= self.limits.max_results {
                    self.result.truncated = true;
                    return Ok(());
                }
            }
        }
        Ok(())
    }
}

fn blocked_path() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "Path is outside the workspace or contains a link",
    )
}

fn is_link(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // Junctions and other reparse points must also be excluded on Windows.
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    metadata.file_type().is_symlink()
}

fn generated_directory(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(
            ".git"
                | ".worktrees"
                | "node_modules"
                | "target"
                | "build"
                | "dist"
                | ".next"
                | ".dart_tool"
                | ".venv"
                | "vendor"
                | ".idea"
                | ".tools"
                | ".prebuilt"
                | "__pycache__"
                | ".cache"
                | "coverage"
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn finds_content_with_unicode_columns_and_bounded_snippets() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("src")).unwrap();
        fs::write(
            root.path().join("src/中文.rs"),
            "nothing\r\n甲乙搜尋 搜尋\r\n",
        )
        .unwrap();
        let search = ProjectSearch::new(root.path()).unwrap();
        let result = search.search("搜尋").unwrap();
        assert!(!result.truncated);
        assert_eq!(result.matches.len(), 2);
        assert_eq!(result.matches[0].path, Path::new("src/中文.rs"));
        assert_eq!((result.matches[0].line, result.matches[0].column), (2, 3));
        assert_eq!(result.matches[1].column, 6);
        assert_eq!(result.matches[0].snippet, "甲乙搜尋 搜尋");
        fs::write(
            root.path().join("long.txt"),
            format!("{}搜尋", "甲".repeat(400)),
        )
        .unwrap();
        let result = search.search("搜尋").unwrap();
        let hit = result
            .matches
            .iter()
            .find(|hit| hit.path == Path::new("long.txt"))
            .unwrap();
        assert_eq!(hit.column, 401);
        assert!(hit.snippet.contains("搜尋"));
        assert!(hit.snippet.chars().count() <= MAX_SNIPPET_CHARS);
        assert!(search.search("").unwrap().matches.is_empty());
        assert!(search.search("\n").is_err());
    }

    #[test]
    fn skips_generated_binary_and_oversized_files() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("target")).unwrap();
        fs::write(root.path().join("target/hidden"), "needle").unwrap();
        fs::write(root.path().join("binary"), b"needle\0").unwrap();
        fs::write(root.path().join("invalid"), b"needle\xff").unwrap();
        fs::write(root.path().join("large"), "needle".repeat(10)).unwrap();
        fs::write(root.path().join("ok"), "needle").unwrap();
        let search = ProjectSearch::new(root.path()).unwrap();
        let result = search
            .search_with_limits(
                "needle",
                SearchLimits {
                    max_file_bytes: 16,
                    ..SearchLimits::default()
                },
            )
            .unwrap();
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.matches[0].path, Path::new("ok"));
        assert!(result.bytes_read <= result.files_visited * 17);
        assert!(ProjectSearch::new(&root.path().join("ok")).is_err());
    }

    #[test]
    fn bounds_work_and_reports_partial_results() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("a"), "needle needle needle").unwrap();
        fs::write(root.path().join("b"), "needle").unwrap();
        let search = ProjectSearch::new(root.path()).unwrap();
        for limits in [
            SearchLimits {
                max_entries: 1,
                ..SearchLimits::default()
            },
            SearchLimits {
                max_files: 1,
                ..SearchLimits::default()
            },
            SearchLimits {
                max_results: 1,
                ..SearchLimits::default()
            },
            SearchLimits {
                max_total_bytes: 2,
                ..SearchLimits::default()
            },
            SearchLimits {
                max_entries: 0,
                ..SearchLimits::default()
            },
        ] {
            let result = search.search_with_limits("needle", limits).unwrap();
            assert!(result.truncated);
            assert!(result.entries_visited <= limits.max_entries);
            assert!(result.files_visited <= limits.max_files);
            assert!(result.matches.len() <= limits.max_results);
            assert!(result.bytes_read <= limits.max_total_bytes);
        }
        fs::create_dir(root.path().join("nested")).unwrap();
        fs::write(root.path().join("nested/c"), "needle").unwrap();
        let result = search
            .search_with_limits(
                "needle",
                SearchLimits {
                    max_depth: 0,
                    ..SearchLimits::default()
                },
            )
            .unwrap();
        assert!(result.truncated);
        assert!(result
            .matches
            .iter()
            .all(|hit| hit.path.components().count() == 1));
    }

    #[cfg(unix)]
    #[test]
    fn excludes_file_directory_and_ancestor_symlinks() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret"), "needle").unwrap();
        symlink(outside.path(), root.path().join("escape")).unwrap();
        symlink(outside.path().join("secret"), root.path().join("file")).unwrap();
        symlink(root.path(), root.path().join("loop")).unwrap();
        let search = ProjectSearch::new(root.path()).unwrap();
        assert!(search.search("needle").unwrap().matches.is_empty());
        fs::create_dir(root.path().join("src")).unwrap();
        let nested = ProjectSearch::new(&root.path().join("src")).unwrap();
        fs::remove_dir(root.path().join("src")).unwrap();
        symlink(outside.path(), root.path().join("src")).unwrap();
        assert!(nested.search("needle").is_err());
    }
}
