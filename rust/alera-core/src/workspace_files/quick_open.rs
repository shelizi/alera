use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use ignore::WalkBuilder;
use uuid::Uuid;

use super::{
    is_protected_workspace_path, nested_git_worktree_paths, relative_string, workspace_root,
    WorkspaceFileError, WorkspaceFileErrorKind,
};

mod index;
mod ranking;

#[cfg(test)]
use self::index::QuickOpenFile;
use self::index::{QuickOpenIndex, QuickOpenPath};

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
    pub is_directory: bool,
}

#[derive(Debug)]
struct QuickOpenSessionEntry {
    index: Arc<QuickOpenIndex>,
    scope: QuickOpenScope,
    last_accessed: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct QuickOpenScope {
    root: PathBuf,
    include_internal_symlinks: bool,
    exclusions: QuickOpenExclusions,
}

const QUICK_OPEN_SESSION_IDLE_TTL: Duration = Duration::from_secs(15 * 60);
const MAX_QUICK_OPEN_SESSIONS: usize = 16;
const MAX_QUICK_OPEN_INDEXED_FILES: usize = 50_000;
const MAX_QUICK_OPEN_INDEXED_PATH_BYTES: usize = 2 * 1024 * 1024;
// Separate directory quotas ensure new Explorer searches cannot reduce the
// count of indexed files used by the existing Ctrl+P Quick Open.
const MAX_QUICK_OPEN_INDEXED_DIRECTORIES: usize = 50_000;
const MAX_QUICK_OPEN_INDEXED_DIRECTORY_PATH_BYTES: usize = 2 * 1024 * 1024;
const MAX_QUICK_OPEN_IGNORED_INDEXED_FILES: usize = 25_000;
const MAX_QUICK_OPEN_IGNORED_INDEXED_PATH_BYTES: usize = 1024 * 1024;
const MAX_QUICK_OPEN_IGNORED_INDEXED_DIRECTORIES: usize = 25_000;
const MAX_QUICK_OPEN_IGNORED_DIRECTORY_PATH_BYTES: usize = 1024 * 1024;

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
    let scope = QuickOpenScope {
        root,
        include_internal_symlinks,
        exclusions,
    };
    with_quick_open_build_gate(|| {
        let mut paths = collect_quick_open_files(
            &scope.root,
            MAX_QUICK_OPEN_INDEXED_FILES,
            MAX_QUICK_OPEN_INDEXED_PATH_BYTES,
            scope.include_internal_symlinks,
            &scope.exclusions,
        )?;
        paths.sort_by(|left, right| {
            left.normalized_path
                .cmp(&right.normalized_path)
                .then_with(|| left.relative_path.cmp(&right.relative_path))
        });
        let indexed_file_count =
            u32::try_from(paths.iter().filter(|file| !file.is_directory).count())
                .unwrap_or(u32::MAX);
        // Obtain only a shared snapshot under the registry lock. Directory
        // walking and ranking index construction never hold that lock, so
        // searches against other live sessions need not wait for rebuilds.
        let previous = {
            let mut registry = sessions().lock().map_err(|_| {
                WorkspaceFileError::new(
                    WorkspaceFileErrorKind::Io,
                    "quick open registry lock poisoned",
                )
            })?;
            prune_sessions(&mut registry, Instant::now());
            registry
                .values()
                .filter(|entry| entry.scope == scope)
                .max_by_key(|entry| entry.last_accessed)
                .map(|entry| Arc::clone(&entry.index))
        };
        let index = QuickOpenIndex::from_paths(paths, previous.as_ref());
        let id = Uuid::new_v4().to_string();
        let mut registry = sessions().lock().map_err(|_| {
            WorkspaceFileError::new(
                WorkspaceFileErrorKind::Io,
                "quick open registry lock poisoned",
            )
        })?;
        prune_sessions(&mut registry, Instant::now());
        registry.insert(
            id.clone(),
            QuickOpenSessionEntry {
                index,
                scope,
                last_accessed: Instant::now(),
            },
        );
        enforce_session_limit(&mut registry);
        Ok(WorkspaceQuickOpenSession {
            id,
            indexed_file_count,
        })
    })
}

#[cfg(test)]
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
) -> Result<Vec<QuickOpenPath>, WorkspaceFileError> {
    let workers = std::thread::available_parallelism().map_or(1, |n| n.get().min(4));
    collect_quick_open_files_with_workers(
        root,
        max_files,
        max_path_bytes,
        include_internal_symlinks,
        exclusions,
        workers,
    )
}

fn collect_quick_open_files_with_workers(
    root: &Path,
    max_files: usize,
    max_path_bytes: usize,
    include_internal_symlinks: bool,
    exclusions: &QuickOpenExclusions,
    workers: usize,
) -> Result<Vec<QuickOpenPath>, WorkspaceFileError> {
    let filter_root = root.to_path_buf();
    let nested_worktrees = nested_git_worktree_paths(root);
    let git_visible_paths = collect_git_visible_paths(root, exclusions, &nested_worktrees)?;
    let entry_exclusions = exclusions.clone();
    let excluded_worktrees = nested_worktrees.clone();
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
            entry.path() == filter_root
                || (!excluded_worktrees.contains(entry.path())
                    && !entry_exclusions.excludes_entry(entry.path()))
        })
        .build();
    let mut files = Vec::new();
    let mut budgets = [
        PathBudget::new(max_files, max_path_bytes),
        PathBudget::new(
            MAX_QUICK_OPEN_INDEXED_DIRECTORIES,
            MAX_QUICK_OPEN_INDEXED_DIRECTORY_PATH_BYTES,
        ),
        PathBudget::new(
            MAX_QUICK_OPEN_IGNORED_INDEXED_FILES,
            MAX_QUICK_OPEN_IGNORED_INDEXED_PATH_BYTES,
        ),
        PathBudget::new(
            MAX_QUICK_OPEN_IGNORED_INDEXED_DIRECTORIES,
            MAX_QUICK_OPEN_IGNORED_DIRECTORY_PATH_BYTES,
        ),
    ];
    let validate = |path: PathBuf| {
        let Some((relative_path, is_directory)) =
            quick_open_relative_path(root, &path, include_internal_symlinks, exclusions)?
        else {
            return Ok(None);
        };
        Ok(Some(QuickOpenPath::new(
            relative_path,
            !git_visible_paths.contains(&path),
            is_directory,
        )))
    };
    let mut admit = |path: QuickOpenPath| {
        let category = usize::from(path.is_directory) + 2 * usize::from(path.is_gitignored);
        if budgets[category].admit(path.relative_path.len()) {
            files.push(path);
        }
    };
    scan_ordered_paths(walker, workers, &validate, &mut admit)?;
    Ok(files)
}

struct PathBudget {
    remaining_files: usize,
    remaining_bytes: usize,
}

impl PathBudget {
    fn new(remaining_files: usize, remaining_bytes: usize) -> Self {
        Self {
            remaining_files,
            remaining_bytes,
        }
    }
    fn admit(&mut self, bytes: usize) -> bool {
        if self.remaining_files == 0 || bytes > self.remaining_bytes {
            return false;
        }
        self.remaining_files -= 1;
        self.remaining_bytes -= bytes;
        true
    }
}

fn walk_paths(walker: ignore::Walk) -> impl Iterator<Item = Result<PathBuf, WorkspaceFileError>> {
    walker.filter_map(|entry| match entry {
        Ok(entry) => Some(Ok(entry.into_path())),
        Err(error) if error.depth().is_some_and(|depth| depth > 0) => None,
        Err(error) => Some(Err(WorkspaceFileError::new(
            WorkspaceFileErrorKind::Io,
            error.to_string(),
        ))),
    })
}

type ValidatedPath = Result<Option<QuickOpenPath>, WorkspaceFileError>;
const QUICK_OPEN_READ_AHEAD: usize = 32;

fn scan_ordered_paths(
    walker: ignore::Walk,
    workers: usize,
    validate: &(impl Fn(PathBuf) -> ValidatedPath + Sync),
    admit: &mut impl FnMut(QuickOpenPath),
) -> Result<(), WorkspaceFileError> {
    let mut paths = walk_paths(walker);
    if workers < 2 {
        for path in paths {
            if let Some(path) = validate(path?)? {
                admit(path);
            }
        }
        return Ok(());
    }
    // The build gate serializes callers. Bound both worker count and queued
    // path descriptors; consume in the original sorted traversal order so
    // independent visible/ignored file/directory quotas remain identical.
    use std::collections::VecDeque;
    use std::sync::mpsc;
    let (job_tx, job_rx) = mpsc::channel::<(usize, PathBuf)>();
    let jobs = Mutex::new(job_rx);
    let (result_tx, result_rx) = mpsc::channel::<(usize, ValidatedPath)>();
    let stop = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|scope| {
        let mut spawned = 0;
        for _ in 0..workers.min(4) {
            let jobs = &jobs;
            let results = result_tx.clone();
            let stop = &stop;
            if std::thread::Builder::new()
                .name("quick-open-metadata".into())
                .spawn_scoped(scope, move || loop {
                    let job = jobs
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .recv();
                    let Ok((slot, path)) = job else {
                        break;
                    };
                    if stop.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }
                    if results.send((slot, validate(path))).is_err() {
                        break;
                    }
                })
                .is_ok()
            {
                spawned += 1;
            }
        }
        drop(result_tx);
        if spawned == 0 {
            drop(job_tx);
            for path in paths {
                if let Some(path) = validate(path?)? {
                    admit(path);
                }
            }
            return Ok(());
        }
        let result = (|| {
            let mut slots: VecDeque<Option<ValidatedPath>> =
                VecDeque::with_capacity(QUICK_OPEN_READ_AHEAD);
            let mut head = 0_usize;
            let mut exhausted = false;
            loop {
                while !exhausted && slots.len() < QUICK_OPEN_READ_AHEAD {
                    let Some(path) = paths.next() else {
                        exhausted = true;
                        break;
                    };
                    let slot = head + slots.len();
                    slots.push_back(None);
                    job_tx.send((slot, path?)).map_err(|_| {
                        WorkspaceFileError::new(
                            WorkspaceFileErrorKind::Io,
                            "quick open metadata workers stopped",
                        )
                    })?;
                }
                if slots.is_empty() {
                    break;
                }
                while slots.front().is_some_and(Option::is_none) {
                    let (slot, result) = result_rx.recv().map_err(|_| {
                        WorkspaceFileError::new(
                            WorkspaceFileErrorKind::Io,
                            "quick open metadata result missing",
                        )
                    })?;
                    *slots.get_mut(slot - head).expect("live quick open slot") = Some(result);
                }
                if let Some(path) = slots
                    .pop_front()
                    .expect("nonempty window")
                    .expect("ready slot")?
                {
                    admit(path);
                }
                head += 1;
            }
            Ok(())
        })();
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        drop(job_tx);
        result
    })
}

fn collect_git_visible_paths(
    root: &Path,
    exclusions: &QuickOpenExclusions,
    nested_worktrees: &HashSet<PathBuf>,
) -> Result<HashSet<PathBuf>, WorkspaceFileError> {
    let filter_root = root.to_path_buf();
    let entry_exclusions = exclusions.clone();
    let excluded_worktrees = nested_worktrees.clone();
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .parents(true)
        .require_git(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .follow_links(false)
        .filter_entry(move |entry| {
            entry.path() == filter_root
                || (!excluded_worktrees.contains(entry.path())
                    && !entry_exclusions.excludes_entry(entry.path()))
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
        if entry.file_type().is_some() {
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
    search_workspace_index_session(session, query, limit, include_gitignored, false)
}

/// Uses the same index as Ctrl+P, but includes directories for Explorer.
pub fn search_workspace_explorer_filter_session(
    session: WorkspaceQuickOpenSession,
    query: String,
    limit: u32,
    include_gitignored: bool,
) -> Result<Vec<WorkspaceQuickOpenMatch>, WorkspaceFileError> {
    search_workspace_index_session(session, query, limit, include_gitignored, true)
}

fn search_workspace_index_session(
    session: WorkspaceQuickOpenSession,
    query: String,
    limit: u32,
    include_gitignored: bool,
    include_directories: bool,
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
    Ok(ranking::search(
        &index,
        &query,
        limit,
        include_gitignored,
        include_directories,
    ))
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
) -> Result<Option<(String, bool)>, WorkspaceFileError> {
    let relative_path = relative_string(root, path)?;
    if relative_path.is_empty() || exclusions.excludes_path(std::path::Path::new(&relative_path)) {
        return Ok(None);
    }
    // Entries can disappear, become inaccessible, or be invalid to Windows
    // filesystem APIs (for example `nul` in a checkout made on another OS).
    // Skip just the affected entry; the workspace root is validated separately.
    let link_metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return Ok(None),
    };
    let is_symlink = link_metadata.file_type().is_symlink();
    if is_symlink {
        if !include_internal_symlinks {
            return Ok(None);
        }
        let canonical = match dunce::canonicalize(path) {
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
    Ok((metadata.is_file() || metadata.is_dir()).then_some((relative_path, metadata.is_dir())))
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
