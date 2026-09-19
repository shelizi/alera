use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use ignore::WalkBuilder;
use uuid::Uuid;

use super::{
    is_protected_workspace_path, relative_string, workspace_root, WorkspaceFileError,
    WorkspaceFileErrorKind,
};

mod index;
mod ranking;

use self::index::{QuickOpenFile, QuickOpenIndex};

#[cfg(test)]
use self::index::character_counts;

#[derive(Debug, Clone)]
pub struct WorkspaceQuickOpenSession {
    pub id: String,
    pub indexed_file_count: u32,
}

#[derive(Debug, Clone)]
pub struct WorkspaceQuickOpenMatch {
    pub relative_path: String,
    pub score: i32,
}

#[derive(Debug)]
struct QuickOpenSessionEntry {
    index: Arc<QuickOpenIndex>,
    last_accessed: Instant,
}

const QUICK_OPEN_SESSION_IDLE_TTL: Duration = Duration::from_secs(15 * 60);
const MAX_QUICK_OPEN_SESSIONS: usize = 16;
const MAX_QUICK_OPEN_INDEXED_FILES: usize = 50_000;
const MAX_QUICK_OPEN_INDEXED_PATH_BYTES: usize = 2 * 1024 * 1024;
const MAX_QUICK_OPEN_IGNORED_INDEXED_FILES: usize = 25_000;
const MAX_QUICK_OPEN_IGNORED_INDEXED_PATH_BYTES: usize = 1024 * 1024;

const QUICK_OPEN_PROTECTED_DIRS: &[&str] = &[".git", ".hg", ".svn"];

pub const DEFAULT_QUICK_OPEN_EXCLUDED_DIRECTORIES: &[&str] = &[
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

static SESSIONS: OnceLock<Mutex<HashMap<String, QuickOpenSessionEntry>>> = OnceLock::new();
static INDEX_BUILD_GATE: OnceLock<Mutex<()>> = OnceLock::new();

pub fn start_workspace_quick_open_session(
    workspace_path: String,
    excluded_directories: Vec<String>,
) -> Result<WorkspaceQuickOpenSession, WorkspaceFileError> {
    start_workspace_quick_open_session_with_symlinks(workspace_path, true, excluded_directories)
}

pub fn start_workspace_quick_open_session_without_symlinks(
    workspace_path: String,
    excluded_directories: Vec<String>,
) -> Result<WorkspaceQuickOpenSession, WorkspaceFileError> {
    start_workspace_quick_open_session_with_symlinks(workspace_path, false, excluded_directories)
}

fn start_workspace_quick_open_session_with_symlinks(
    workspace_path: String,
    include_internal_symlinks: bool,
    excluded_directories: Vec<String>,
) -> Result<WorkspaceQuickOpenSession, WorkspaceFileError> {
    let root = workspace_root(&workspace_path)?;
    let exclusions = QuickOpenExclusions::new(excluded_directories);
    let mut files = with_quick_open_build_gate(|| {
        collect_quick_open_files(
            &root,
            MAX_QUICK_OPEN_INDEXED_FILES,
            MAX_QUICK_OPEN_INDEXED_PATH_BYTES,
            include_internal_symlinks,
            &exclusions,
        )
    })?;
    sort_quick_open_files(&mut files);
    let indexed_file_count = u32::try_from(files.len()).unwrap_or(u32::MAX);
    let id = Uuid::new_v4().to_string();
    let mut sessions = sessions().lock().map_err(|_| {
        WorkspaceFileError::new(
            WorkspaceFileErrorKind::Io,
            "quick open registry lock poisoned",
        )
    })?;
    prune_sessions(&mut sessions, Instant::now());
    sessions.insert(
        id.clone(),
        QuickOpenSessionEntry {
            index: Arc::new(QuickOpenIndex::new(files)),
            last_accessed: Instant::now(),
        },
    );
    enforce_session_limit(&mut sessions);
    Ok(WorkspaceQuickOpenSession {
        id,
        indexed_file_count,
    })
}

fn sort_quick_open_files(files: &mut [QuickOpenFile]) {
    files.sort_by(|left, right| {
        left.normalized_path
            .cmp(&right.normalized_path)
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });
}

fn with_quick_open_build_gate<T>(
    build: impl FnOnce() -> Result<T, WorkspaceFileError>,
) -> Result<T, WorkspaceFileError> {
    let _guard = INDEX_BUILD_GATE
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| {
            WorkspaceFileError::new(
                WorkspaceFileErrorKind::Io,
                "quick open indexing lock poisoned",
            )
        })?;
    build()
}

fn collect_quick_open_files(
    root: &Path,
    max_files: usize,
    max_path_bytes: usize,
    include_internal_symlinks: bool,
    exclusions: &QuickOpenExclusions,
) -> Result<Vec<QuickOpenFile>, WorkspaceFileError> {
    let filter_root = root.to_path_buf();
    let git_visible_paths = collect_git_visible_paths(root, exclusions)?;
    let entry_exclusions = exclusions.clone();
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .parents(true)
        .require_git(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .follow_links(false)
        .sort_by_file_path(|left, right| left.cmp(right))
        .filter_entry(move |entry| {
            entry.path() == filter_root || !entry_exclusions.excludes_entry(entry.path())
        })
        .build();
    let mut files = Vec::new();
    let mut indexed_path_bytes = 0_usize;
    let mut ignored_file_count = 0_usize;
    let mut ignored_path_bytes = 0_usize;
    for entry in walker {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) if error.depth().is_some_and(|depth| depth > 0) => continue,
            Err(error) => {
                return Err(WorkspaceFileError::new(
                    WorkspaceFileErrorKind::Io,
                    error.to_string(),
                ));
            }
        };
        if let Some(relative_path) =
            quick_open_relative_path(root, entry.path(), include_internal_symlinks, exclusions)?
        {
            let is_gitignored = !git_visible_paths.contains(entry.path());
            if is_gitignored {
                if ignored_file_count >= MAX_QUICK_OPEN_IGNORED_INDEXED_FILES {
                    continue;
                }
                let next_path_bytes = ignored_path_bytes.saturating_add(relative_path.len());
                if next_path_bytes > MAX_QUICK_OPEN_IGNORED_INDEXED_PATH_BYTES {
                    continue;
                }
                ignored_file_count += 1;
                ignored_path_bytes = next_path_bytes;
            } else {
                let indexed_file_count = files.len().saturating_sub(ignored_file_count);
                if indexed_file_count >= max_files {
                    continue;
                }
                let next_path_bytes = indexed_path_bytes.saturating_add(relative_path.len());
                if next_path_bytes > max_path_bytes {
                    continue;
                }
                indexed_path_bytes = next_path_bytes;
            }
            files.push(QuickOpenFile::with_gitignored(relative_path, is_gitignored));
        }
    }
    Ok(files)
}

fn collect_git_visible_paths(
    root: &Path,
    exclusions: &QuickOpenExclusions,
) -> Result<HashSet<PathBuf>, WorkspaceFileError> {
    let filter_root = root.to_path_buf();
    let entry_exclusions = exclusions.clone();
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .parents(true)
        .require_git(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .follow_links(false)
        .filter_entry(move |entry| {
            entry.path() == filter_root || !entry_exclusions.excludes_entry(entry.path())
        })
        .build();
    let mut visible = HashSet::new();
    for entry in walker {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) if error.depth().is_some_and(|depth| depth > 0) => continue,
            Err(error) => {
                return Err(WorkspaceFileError::new(
                    WorkspaceFileErrorKind::Io,
                    error.to_string(),
                ));
            }
        };
        if entry
            .file_type()
            .is_some_and(|file_type| !file_type.is_dir())
        {
            visible.insert(entry.into_path());
        }
    }
    Ok(visible)
}

pub fn search_workspace_quick_open_session(
    session: WorkspaceQuickOpenSession,
    query: String,
    limit: u32,
    include_gitignored: bool,
) -> Result<Vec<WorkspaceQuickOpenMatch>, WorkspaceFileError> {
    let index = {
        let mut sessions = sessions().lock().map_err(|_| {
            WorkspaceFileError::new(
                WorkspaceFileErrorKind::Io,
                "quick open registry lock poisoned",
            )
        })?;
        access_session(&mut sessions, &session.id, Instant::now())?
    };
    Ok(ranking::search(&index, &query, limit, include_gitignored))
}

fn access_session(
    sessions: &mut HashMap<String, QuickOpenSessionEntry>,
    session_id: &str,
    now: Instant,
) -> Result<Arc<QuickOpenIndex>, WorkspaceFileError> {
    let index = {
        let entry = sessions.get_mut(session_id).ok_or_else(|| {
            WorkspaceFileError::new(
                WorkspaceFileErrorKind::NotFound,
                "quick open session not found",
            )
        })?;
        entry.last_accessed = now;
        Arc::clone(&entry.index)
    };
    prune_sessions(sessions, now);
    Ok(index)
}

pub fn stop_workspace_quick_open_session(session: WorkspaceQuickOpenSession) {
    if let Ok(mut sessions) = sessions().lock() {
        sessions.remove(&session.id);
    }
}

fn quick_open_relative_path(
    root: &std::path::Path,
    path: &std::path::Path,
    include_internal_symlinks: bool,
    exclusions: &QuickOpenExclusions,
) -> Result<Option<String>, WorkspaceFileError> {
    let relative_path = relative_string(root, path)?;
    if relative_path.is_empty() || exclusions.excludes_path(std::path::Path::new(&relative_path)) {
        return Ok(None);
    }
    let link_metadata = fs::symlink_metadata(path)
        .map_err(|error| WorkspaceFileError::from_io(error, path.display().to_string()))?;
    let is_symlink = link_metadata.file_type().is_symlink();
    if is_symlink {
        if !include_internal_symlinks {
            return Ok(None);
        }
        let canonical = match fs::canonicalize(path) {
            Ok(canonical) => canonical,
            Err(_) => return Ok(None),
        };
        if !canonical.starts_with(root) {
            return Ok(None);
        }
        let canonical_relative = relative_string(root, &canonical)?;
        if exclusions.excludes_path(std::path::Path::new(&canonical_relative)) {
            return Ok(None);
        }
    }
    let metadata = if is_symlink {
        match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(_) => return Ok(None),
        }
    } else {
        link_metadata
    };
    Ok(metadata.is_file().then_some(relative_path))
}

#[derive(Clone)]
struct QuickOpenExclusions {
    configured: HashSet<String>,
}

impl QuickOpenExclusions {
    fn new(excluded_directories: Vec<String>) -> Self {
        let configured = excluded_directories
            .into_iter()
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty() && !value.contains('/') && !value.contains('\\'))
            .collect();
        Self { configured }
    }

    fn excludes_entry(&self, path: &Path) -> bool {
        path.file_name().is_some_and(|name| {
            let normalized = name.to_string_lossy().to_ascii_lowercase();
            self.excludes_name(&normalized)
        })
    }

    fn excludes_path(&self, path: &Path) -> bool {
        is_protected_workspace_path(path)
            || path.components().any(|component| {
                matches!(component, std::path::Component::Normal(value) if value.to_str().is_some_and(|value| {
                    self.excludes_name(&value.to_ascii_lowercase())
                }))
            })
    }

    fn excludes_name(&self, normalized: &str) -> bool {
        QUICK_OPEN_PROTECTED_DIRS.contains(&normalized) || self.configured.contains(normalized)
    }
}

fn sessions() -> &'static Mutex<HashMap<String, QuickOpenSessionEntry>> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn prune_sessions(sessions: &mut HashMap<String, QuickOpenSessionEntry>, now: Instant) {
    sessions.retain(|_, entry| {
        now.saturating_duration_since(entry.last_accessed) < QUICK_OPEN_SESSION_IDLE_TTL
    });
}

fn enforce_session_limit(sessions: &mut HashMap<String, QuickOpenSessionEntry>) {
    while sessions.len() > MAX_QUICK_OPEN_SESSIONS {
        let Some(oldest) = sessions
            .iter()
            .min_by_key(|(_, entry)| entry.last_accessed)
            .map(|(id, _)| id.clone())
        else {
            break;
        };
        sessions.remove(&oldest);
    }
}

#[cfg(test)]
mod tests;
