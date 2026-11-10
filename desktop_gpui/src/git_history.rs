//! Bounded read-only history, commit preview and local branch comparison.
//! All repository I/O belongs on a background worker, never in GPUI render.
use crate::git_graph_search::GitGraphSearch;
use git2::{BranchType, Delta, DiffFindOptions, DiffFormat, DiffOptions, Oid, Repository, Sort};
use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

const MAX_HISTORY: usize = 96;
pub const MAX_HISTORY_PAGE: usize = 512;
pub const HISTORY_PAGE_STEP: usize = 96;
const MAX_BRANCHES: usize = 80;
const MAX_TAGS: usize = 80;
const MAX_PATCH: usize = 24 * 1024;
/// Path-limited graph reads are background jobs but may still traverse a
/// large repository. Never run an unbounded full-history file scan.
const MAX_FILTER_SCAN: usize = 2048;
/// An opt-in text search runs only on a background worker and has a hard
/// traversal budget even when a large repository contains no match.
pub const MAX_HISTORY_SEARCH_SCAN: usize = 50_000;
const MAX_RENAME_DIFF_DELTAS: usize = 512;

#[derive(Debug, Clone)]
pub struct GitBranchRow {
    pub name: String,
    pub current: bool,
    pub tip: String,
    pub remote: bool,
    pub favorite: bool,
}

#[derive(Debug, Clone)]
pub struct GitCommitRow {
    pub oid: String,
    pub short: String,
    pub subject: String,
    pub author: String,
    pub parents: usize,
    /// Exact parent commit hashes for Native graph edges.
    pub parent_ids: Vec<String>,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GitTagRow {
    pub name: String,
    pub tip: String,
    pub oid: String,
}

#[derive(Debug, Clone)]
pub struct GitHistorySnapshot {
    pub current: String,
    pub branches: Vec<GitBranchRow>,
    pub tags: Vec<GitTagRow>,
    pub commits: Vec<GitCommitRow>,
    pub truncated: bool,
    pub scanned_commits: usize,
    pub scan_limit_reached: bool,
}

/// Native-owned Git Graph selection. Favorites belong to independent Native
/// state, never to the Flutter edition or to the repository's Git config.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitHistoryOptions {
    pub all_branches: bool,
    pub favorites_only: bool,
    /// Zero uses the original 96-commit default; explicit requests are
    /// clamped to a bounded 512-commit maximum.
    pub max_commits: usize,
    pub selected_ref: Option<String>,
    pub favorites: Vec<String>,
    /// Host-validated Workspace-relative paths, checked independently again
    /// by Native. Paths never become shell arguments or Git revspecs.
    pub file_path: Option<String>,
    pub git_diff_root: Option<String>,
    /// Explicit background search across reachable history; ordinary typing
    /// in the Graph box continues to filter only loaded rows.
    pub search_query: Option<String>,
}

#[derive(Debug, Clone)]
struct HistoryPathFilter {
    relative_to_repo: PathBuf,
    scope_relative_to_repo: PathBuf,
    directory: bool,
    missing_at_worktree: bool,
}

impl HistoryPathFilter {
    fn in_scope(&self, path: &Path) -> bool {
        self.scope_relative_to_repo.as_os_str().is_empty()
            || path.starts_with(&self.scope_relative_to_repo)
    }

    fn touches(&self, diff: &git2::Diff<'_>) -> bool {
        diff.deltas().any(|delta| {
            [delta.old_file().path(), delta.new_file().path()]
                .into_iter()
                .flatten()
                .any(|path| {
                    if self.directory {
                        path.starts_with(&self.relative_to_repo)
                    } else {
                        path == self.relative_to_repo
                    }
                })
        })
    }

    /// Trace each selected file identity from this commit to its first
    /// parent. A new file stops at its addition, while a deleted file keeps
    /// its previous name. A detected rename follows the old pathname.
    fn trace_file(
        &self,
        diff: &mut git2::Diff<'_>,
        tracked_paths: &[PathBuf],
    ) -> Result<(bool, Vec<PathBuf>), String> {
        if tracked_paths.is_empty() {
            return Ok((false, Vec::new()));
        }
        // Similarity detection can be expensive for huge commits; beyond this
        // limit, do not invent a previous name or inspect arbitrary content.
        if diff.deltas().len() <= MAX_RENAME_DIFF_DELTAS {
            let has_add = diff.deltas().any(|delta| delta.status() == Delta::Added);
            let has_delete = diff.deltas().any(|delta| delta.status() == Delta::Deleted);
            if has_add && has_delete {
                let mut options = DiffFindOptions::new();
                options.renames(true).rename_limit(200);
                diff.find_similar(Some(&mut options))
                    .map_err(|error| error.to_string())?;
            }
        }
        let mut touched = false;
        let mut previous = Vec::<PathBuf>::new();
        for tracked in tracked_paths {
            let changed = diff.deltas().find(|delta| {
                delta.old_file().path() == Some(tracked.as_path())
                    || delta.new_file().path() == Some(tracked.as_path())
            });
            let parent_path = if let Some(delta) = changed {
                touched = true;
                match delta.status() {
                    Delta::Added if delta.new_file().path() == Some(tracked.as_path()) => None,
                    Delta::Renamed if delta.new_file().path() == Some(tracked.as_path()) => {
                        delta.old_file().path()
                    }
                    _ => Some(tracked.as_path()),
                }
            } else {
                Some(tracked.as_path())
            };
            if let Some(path) = parent_path {
                if self.in_scope(path) && !previous.iter().any(|candidate| candidate == path) {
                    previous.push(path.to_path_buf());
                }
            }
        }
        Ok((touched, previous))
    }
}

fn validated_relative_path(path: &str) -> Result<&Path, String> {
    let relative = Path::new(path);
    if path.is_empty()
        || path.len() > 1024
        || path.contains(':')
        || path.contains('\0')
        || !relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err("Invalid Git Graph Workspace-relative path".into());
    }
    Ok(relative)
}

/// Resolve scope from an actual current Workspace directory. Disallow
/// symlink escapes even if the Host already validated its request.
/// A previously committed file may no longer exist. Resolve its nearest
/// existing ancestor and keep the missing suffix purely lexical. A symlinked
/// parent must resolve safely within the chosen Workspace and Git root.
fn resolve_current_or_deleted_file(
    root: &Path,
    relative: &Path,
) -> Result<(PathBuf, bool), String> {
    let requested = root.join(relative);
    match fs::symlink_metadata(&requested) {
        Ok(_) => {
            let canonical = dunce::canonicalize(&requested)
                .map_err(|_| "Git Graph file target cannot be resolved".to_string())?;
            if !canonical.is_file() {
                return Err("Git Graph file target is not a regular file".into());
            }
            return Ok((canonical, false));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("Git Graph file target cannot be accessed".into()),
    }
    let mut ancestor = requested.as_path();
    loop {
        ancestor = ancestor
            .parent()
            .ok_or("Git Graph file has no valid ancestor")?;
        match fs::symlink_metadata(ancestor) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err("Git Graph file ancestor is inaccessible".into()),
        }
    }
    let canonical = dunce::canonicalize(ancestor)
        .map_err(|_| "Git Graph file ancestor cannot be resolved".to_string())?;
    if !canonical.is_dir() {
        return Err("Git Graph missing file has an invalid parent".into());
    }
    let suffix = requested
        .strip_prefix(ancestor)
        .map_err(|_| "Git Graph file suffix is invalid".to_string())?;
    Ok((canonical.join(suffix), true))
}

fn history_repo_and_path_filter(
    workspace: &Path,
    options: &GitHistoryOptions,
) -> Result<(Repository, Option<HistoryPathFilter>), String> {
    let root = dunce::canonicalize(workspace).map_err(|error| error.to_string())?;
    if !root.is_dir() {
        return Err("Git Graph Workspace directory is unavailable".into());
    }
    let scope = if let Some(relative) = options.git_diff_root.as_deref() {
        let relative = validated_relative_path(relative)?;
        let scope = dunce::canonicalize(root.join(relative))
            .map_err(|_| "Git Graph source control scope does not exist".to_string())?;
        if !scope.starts_with(&root) || !scope.is_dir() {
            return Err("Git Graph scope is outside the Workspace".into());
        }
        scope
    } else {
        root.clone()
    };
    let repo = repo_in(&scope)?;
    let repo_root = dunce::canonicalize(
        repo.workdir()
            .ok_or("Git Graph worktree has no working directory")?,
    )
    .map_err(|error| error.to_string())?;
    let (selected, directory, missing_at_worktree) =
        if let Some(file) = options.file_path.as_deref() {
            let file = validated_relative_path(file)?;
            let (target, missing) = resolve_current_or_deleted_file(&root, file)?;
            if !target.starts_with(&scope) {
                return Err("Git Graph file must be inside the selected Workspace scope".into());
            }
            (target, false, missing)
        } else {
            (scope.clone(), true, false)
        };
    let relative = selected
        .strip_prefix(&repo_root)
        .map_err(|_| "Git Graph path is outside the repository".to_string())?;
    let scope_relative_to_repo = scope
        .strip_prefix(&repo_root)
        .map_err(|_| "Git Graph root is outside repository".to_string())?
        .to_path_buf();
    let filter = (!relative.as_os_str().is_empty()).then(|| HistoryPathFilter {
        relative_to_repo: relative.to_path_buf(),
        scope_relative_to_repo,
        directory,
        missing_at_worktree,
    });
    Ok((repo, filter))
}

fn repo_in(workspace: &Path) -> Result<Repository, String> {
    let repo = Repository::discover(workspace).map_err(|e| e.to_string())?;
    if repo.is_bare() {
        return Err("Bare repositories cannot be viewed in this GPUI workbench".into());
    }
    Ok(repo)
}

fn subject(line: &str, count: usize) -> String {
    line.chars().take(count).collect()
}

impl GitHistorySnapshot {
    pub fn load(workspace: &Path) -> Result<Self, String> {
        Self::load_filtered(workspace, &GitHistoryOptions::default())
    }

    pub fn load_filtered(workspace: &Path, options: &GitHistoryOptions) -> Result<Self, String> {
        Self::load_filtered_cancellable(workspace, options, || false)
    }

    /// Cooperative cancellation for obsolete background reads. A cancelled
    /// scan returns no partial snapshot and never updates repository state.
    /// UI callers cancel the previous token when a newer request starts.
    pub fn load_filtered_cancellable(
        workspace: &Path,
        options: &GitHistoryOptions,
        cancelled: impl Fn() -> bool,
    ) -> Result<Self, String> {
        fn check_cancelled(cancelled: &impl Fn() -> bool) -> Result<(), String> {
            if cancelled() {
                Err("Git Graph history scan cancelled".into())
            } else {
                Ok(())
            }
        }
        check_cancelled(&cancelled)?;
        let search = options.search_query.as_deref().map(GitGraphSearch::parse);
        if search
            .as_ref()
            .is_some_and(|query| !query.can_search_commits())
        {
            return Err(
                "Full-history search needs a nonempty commit, author, SHA or message query".into(),
            );
        }
        let search = search.filter(GitGraphSearch::can_search_commits);
        let (repo, filter) = history_repo_and_path_filter(workspace, options)?;
        check_cancelled(&cancelled)?;
        let current = repo
            .head()
            .ok()
            .and_then(|head| head.shorthand().ok().map(str::to_owned))
            .unwrap_or_else(|| "(detached/unborn)".into());
        let mut branches = Vec::new();
        let mut tips = Vec::new();
        let mut truncated = false;
        for branch in repo
            .branches(if options.all_branches || options.favorites_only {
                None
            } else {
                Some(BranchType::Local)
            })
            .map_err(|e| e.to_string())?
        {
            check_cancelled(&cancelled)?;
            let (branch, kind) = branch.map_err(|e| e.to_string())?;
            let Some(name) = branch.name().map_err(|e| e.to_string())?.map(str::to_owned) else {
                continue;
            };
            if kind == BranchType::Remote && name.ends_with("/HEAD") {
                continue;
            }
            let remote = kind == BranchType::Remote;
            let favorite_key = if remote {
                format!("refs/remotes/{name}")
            } else {
                name.clone()
            };
            let favorite = options.favorites.contains(&favorite_key);
            if options.favorites_only && !favorite {
                continue;
            }
            let Ok(commit) = branch.get().peel_to_commit() else {
                continue;
            };
            if branches.len() == MAX_BRANCHES {
                truncated = true;
                continue;
            }
            let tip = commit.id();
            tips.push((tip, name.clone(), remote));
            branches.push(GitBranchRow {
                current: branch.is_head(),
                name,
                tip: tip.to_string()[..8].to_owned(),
                remote,
                favorite,
            });
        }
        // Like Classic: keep HEAD first, then favorite branches ahead of
        // ordinary refs. Equal-priority refs remain alphabetical.
        branches.sort_by(|a, b| {
            b.current
                .cmp(&a.current)
                .then(b.favorite.cmp(&a.favorite))
                .then_with(|| a.name.cmp(&b.name))
        });
        // Lightweight and annotated tags both peel to a real commit. Keep
        // UI refs distinct from local branches and reject unsupported tags.
        let mut tags = Vec::new();
        for item in repo
            .references_glob("refs/tags/*")
            .map_err(|e| e.to_string())?
        {
            check_cancelled(&cancelled)?;
            let reference = item.map_err(|e| e.to_string())?;
            let Some(name) = reference
                .name()
                .ok()
                .and_then(|name| name.strip_prefix("refs/tags/"))
            else {
                continue;
            };
            let Ok(commit) = reference.peel_to_commit() else {
                continue;
            };
            if tags.len() == MAX_TAGS {
                truncated = true;
                continue;
            }
            tags.push(GitTagRow {
                name: name.to_owned(),
                tip: commit.id().to_string()[..8].to_owned(),
                oid: commit.id().to_string(),
            });
        }
        tags.sort_by(|a, b| a.name.cmp(&b.name));
        if let Some(requested) = options.selected_ref.as_deref() {
            // Host references are accepted only after resolving an exact,
            // visible branch. Do not interpret arbitrary revspec operators.
            let tag_selected = requested
                .strip_prefix("refs/tags/")
                .and_then(|name| tags.iter().find(|tag| tag.name == name))
                .map(|tag| tag.oid.clone());
            let expected_remote = if requested.starts_with("refs/remotes/") {
                Some(true)
            } else if requested.starts_with("refs/heads/") {
                Some(false)
            } else {
                None
            };
            let requested = requested
                .strip_prefix("refs/heads/")
                .or_else(|| requested.strip_prefix("refs/remotes/"))
                .unwrap_or(requested);
            let selected = if requested == "HEAD" {
                current.as_str()
            } else {
                requested
            };
            let matching = tips
                .iter()
                .filter(|(_, name, remote)| {
                    name == selected && expected_remote.is_none_or(|expected| expected == *remote)
                })
                .collect::<Vec<_>>();
            if let Some(tag_oid) = tag_selected {
                let oid = Oid::from_str(&tag_oid).map_err(|error| error.to_string())?;
                tips.clear();
                tips.push((oid, requested.to_owned(), false));
            } else {
                if matching.len() != 1 {
                    return Err("Requested Git Graph reference is missing or ambiguous".into());
                }
                let tip = (*matching[0]).clone();
                tips.clear();
                tips.push(tip);
            }
        }
        let mut walk = repo.revwalk().map_err(|e| e.to_string())?;
        walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)
            .map_err(|e| e.to_string())?;
        for (oid, _, _) in &tips {
            walk.push(*oid).map_err(|e| e.to_string())?;
        }
        let mut file_history_paths: HashMap<Oid, Vec<PathBuf>> = HashMap::new();
        if let Some(filter) = &filter {
            if !filter.directory {
                for (oid, _, _) in &tips {
                    file_history_paths
                        .entry(*oid)
                        .or_default()
                        .push(filter.relative_to_repo.clone());
                }
            }
        }
        if !options.favorites_only && options.selected_ref.is_none() {
            if let Ok(head) = repo.head().and_then(|head| head.peel_to_commit()) {
                walk.push(head.id()).map_err(|e| e.to_string())?;
                if filter.as_ref().is_some_and(|filter| !filter.directory) {
                    file_history_paths
                        .entry(head.id())
                        .or_default()
                        .push(filter.as_ref().unwrap().relative_to_repo.clone());
                }
            }
        }
        let mut commits = Vec::new();
        let mut scanned = 0usize;
        let mut scan_limit_reached = false;
        let scan_limit = if filter.is_some() {
            MAX_FILTER_SCAN
        } else if search.is_some() {
            MAX_HISTORY_SEARCH_SCAN
        } else {
            usize::MAX
        };
        for oid in walk {
            check_cancelled(&cancelled)?;
            if scanned >= scan_limit {
                truncated = true;
                scan_limit_reached = true;
                break;
            }
            scanned += 1;
            let oid = oid.map_err(|e| e.to_string())?;
            let commit = repo.find_commit(oid).map_err(|e| e.to_string())?;
            if let Some(filter) = &filter {
                check_cancelled(&cancelled)?;
                let first_parent = if commit.parent_count() > 0 {
                    Some(commit.parent(0).map_err(|error| error.to_string())?)
                } else {
                    None
                };
                let parent_tree = first_parent
                    .as_ref()
                    .map(|parent| parent.tree().map_err(|error| error.to_string()))
                    .transpose()?;
                let tree = commit.tree().map_err(|error| error.to_string())?;
                let mut diff = repo
                    .diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), None)
                    .map_err(|error| error.to_string())?;
                let touched = if filter.directory {
                    filter.touches(&diff)
                } else {
                    let aliases = file_history_paths.remove(&oid).unwrap_or_default();
                    let (touched, previous) = filter.trace_file(&mut diff, &aliases)?;
                    if let Some(parent) = first_parent {
                        let next = file_history_paths.entry(parent.id()).or_default();
                        for alias in previous {
                            if !next.contains(&alias) && next.len() < MAX_BRANCHES {
                                next.push(alias);
                            }
                        }
                    }
                    touched
                };
                if !touched {
                    continue;
                }
            }
            let labels = tips
                .iter()
                .filter(|(tip, _, _)| *tip == oid)
                .take(5)
                .map(|(_, branch, remote)| {
                    if *remote {
                        format!("refs/remotes/{branch}")
                    } else {
                        branch.clone()
                    }
                })
                .collect::<Vec<_>>();
            if let Some(query) = &search {
                let label_refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
                if !query.commit_fields(
                    &oid.to_string(),
                    commit.message().unwrap_or(""),
                    commit.author().name().unwrap_or("Unknown"),
                    &label_refs,
                ) {
                    continue;
                }
            }
            let requested = if options.max_commits == 0 {
                MAX_HISTORY
            } else {
                options.max_commits.clamp(MAX_HISTORY, MAX_HISTORY_PAGE)
            };
            if commits.len() >= requested {
                truncated = true;
                break;
            }
            commits.push(GitCommitRow {
                oid: oid.to_string(),
                short: oid.to_string()[..8].to_owned(),
                subject: subject(
                    commit.summary().ok().flatten().unwrap_or("(no subject)"),
                    110,
                ),
                author: subject(commit.author().name().unwrap_or("Unknown"), 48),
                parents: commit.parent_count(),
                parent_ids: commit.parent_ids().map(|id| id.to_string()).collect(),
                labels,
            });
        }
        check_cancelled(&cancelled)?;
        if search.is_none()
            && filter
                .as_ref()
                .is_some_and(|filter| filter.missing_at_worktree && commits.is_empty())
        {
            return Err(
                "Git Graph deleted file has no matching history within the scan limit".into(),
            );
        }
        Ok(Self {
            current,
            branches,
            tags,
            commits,
            truncated,
            scanned_commits: scanned,
            scan_limit_reached,
        })
    }
}

fn bounded_patch(diff: &git2::Diff<'_>) -> Result<String, String> {
    let mut patch = String::new();
    diff.print(DiffFormat::Patch, |_, _, line| {
        if patch.len() < MAX_PATCH {
            if matches!(line.origin(), '+' | '-' | ' ') {
                patch.push(line.origin());
            }
            let raw = String::from_utf8_lossy(line.content());
            for ch in raw.chars() {
                if patch.len() + ch.len_utf8() > MAX_PATCH {
                    break;
                }
                patch.push(ch);
            }
        }
        true
    })
    .map_err(|e| e.to_string())?;
    if patch.len() == MAX_PATCH {
        patch.push_str("\n... patch preview limited");
    }
    Ok(patch)
}

/// Preview a commit's first-parent patch (merge commits show first-parent).
pub fn preview_commit(workspace: &Path, sha: &str) -> Result<String, String> {
    if sha.len() != 40 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Invalid full commit ID".into());
    }
    let repo = repo_in(workspace)?;
    let oid = Oid::from_str(sha).map_err(|e| e.to_string())?;
    let commit = repo.find_commit(oid).map_err(|e| e.to_string())?;
    let current = commit.tree().map_err(|e| e.to_string())?;
    let parent = if commit.parent_count() > 0 {
        Some(
            commit
                .parent(0)
                .map_err(|e| e.to_string())?
                .tree()
                .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };
    let mut opts = git2::DiffOptions::new();
    opts.context_lines(3);
    let diff = repo
        .diff_tree_to_tree(parent.as_ref(), Some(&current), Some(&mut opts))
        .map_err(|e| e.to_string())?;
    let patch = bounded_patch(&diff)?;
    Ok(format!(
        "Commit {sha}\n{}\nAuthor: {}\nParents: {}\n\n{}",
        commit.summary().ok().flatten().unwrap_or("(no subject)"),
        commit.author().name().unwrap_or("Unknown"),
        commit.parent_count(),
        if patch.is_empty() {
            "(no first-parent text changes)"
        } else {
            &patch
        }
    ))
}

/// Compare the current local branch with another local branch. No files,
/// refs, Git index or worktree state are changed by this operation.
pub fn compare_local_branch(workspace: &Path, branch: &str) -> Result<String, String> {
    if branch.is_empty() || branch.len() > 255 || branch.contains(['\n', '\r', '\0']) {
        return Err("Enter a valid local branch to compare".into());
    }
    let repo = repo_in(workspace)?;
    let current = repo
        .head()
        .map_err(|e| e.to_string())?
        .peel_to_commit()
        .map_err(|e| e.to_string())?;
    let current_name = repo
        .head()
        .ok()
        .and_then(|h| h.shorthand().ok().map(str::to_owned))
        .unwrap_or_else(|| "HEAD".into());
    let other = repo
        .find_branch(branch, BranchType::Local)
        .map_err(|e| e.to_string())?
        .get()
        .peel_to_commit()
        .map_err(|e| e.to_string())?;
    let (ahead, behind) = repo
        .graph_ahead_behind(other.id(), current.id())
        .map_err(|e| e.to_string())?;
    let current_tree = current.tree().map_err(|e| e.to_string())?;
    let other_tree = other.tree().map_err(|e| e.to_string())?;
    let diff = repo
        .diff_tree_to_tree(Some(&current_tree), Some(&other_tree), None)
        .map_err(|e| e.to_string())?;
    let patch = bounded_patch(&diff)?;
    Ok(format!(
        "Compare {branch} vs {current_name}\n{branch}: {ahead} ahead / {behind} behind\n\n{}",
        if patch.is_empty() {
            "(trees are identical)"
        } else {
            &patch
        }
    ))
}

/// Read-only Host-authorized two-ref Git range comparison. Match Classic's
/// merge-base..head semantics; never touch worktree files or the index.
pub fn compare_git_refs(
    workspace: &Path,
    base_ref: &str,
    head_ref: &str,
    scope: Option<&str>,
) -> Result<String, String> {
    fn valid_ref(name: &str) -> bool {
        !name.is_empty()
            && name.len() <= 128
            && !name.starts_with('-')
            && !name.contains("..")
            && !name.contains("@{")
            && !name.ends_with(['.', '/'])
            && name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"/._-".contains(&c))
    }
    if !valid_ref(base_ref) || !valid_ref(head_ref) {
        return Err("Invalid Git comparison ref".into());
    }
    let repo = repo_in(workspace)?;
    let base = repo
        .revparse_single(base_ref)
        .and_then(|value| value.peel_to_commit())
        .map_err(|_| format!("Git base ref unavailable: {base_ref}"))?;
    let head = repo
        .revparse_single(head_ref)
        .and_then(|value| value.peel_to_commit())
        .map_err(|_| format!("Git head ref unavailable: {head_ref}"))?;
    let ancestor = repo
        .merge_base(base.id(), head.id())
        .map_err(|_| "Refs have no common merge base".to_string())?;
    let merge_tree = repo
        .find_commit(ancestor)
        .and_then(|commit| commit.tree())
        .map_err(|e| e.to_string())?;
    let head_tree = head.tree().map_err(|e| e.to_string())?;
    let mut options = DiffOptions::new();
    options.context_lines(3);
    if let Some(scope) = scope {
        use std::path::Component;
        let relative = Path::new(scope);
        if scope.is_empty()
            || scope.len() > 1024
            || !relative
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
        {
            return Err("Invalid Git comparison subdirectory".into());
        }
        let repo_root = repo.workdir().ok_or("Git worktree path unavailable")?;
        let target = dunce::canonicalize(workspace.join(relative))
            .map_err(|_| "Git comparison scope unavailable".to_string())?;
        if !target.is_dir() || !target.starts_with(repo_root) {
            return Err("Git comparison scope outside worktree".into());
        }
        let in_repo = target
            .strip_prefix(repo_root)
            .map_err(|_| "Invalid Git scope".to_string())?;
        options.pathspec(in_repo);
    }
    let diff = repo
        .diff_tree_to_tree(Some(&merge_tree), Some(&head_tree), Some(&mut options))
        .map_err(|e| e.to_string())?;
    let patch = bounded_patch(&diff)?;
    let (ahead, behind) = repo
        .graph_ahead_behind(head.id(), base.id())
        .map_err(|e| e.to_string())?;
    Ok(format!("Compare {base_ref}..{head_ref}\nBase: {}\nHead: {}\nMerge base: {ancestor}\nHead: {ahead} ahead / {behind} behind\n\n{}",
        base.id(), head.id(), if patch.is_empty() { "(no changed files in this scope)" } else { &patch }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::fs;

    #[test]
    fn full_history_search_can_cancel_during_walk_without_changing_git_state() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let signature = git2::Signature::now("Cancel Test", "cancel@example.invalid").unwrap();
        let mut parent: Option<Oid> = None;
        for n in 0..135 {
            fs::write(dir.path().join("history.txt"), format!("{n}")).unwrap();
            let mut index = repo.index().unwrap();
            index.add_path(Path::new("history.txt")).unwrap();
            let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
            let parents = parent
                .map(|id| vec![repo.find_commit(id).unwrap()])
                .unwrap_or_default();
            parent = Some(
                repo.commit(
                    Some("HEAD"),
                    &signature,
                    &signature,
                    &format!("revision {n}"),
                    &tree,
                    &parents.iter().collect::<Vec<_>>(),
                )
                .unwrap(),
            );
        }
        let original_head = repo.head().unwrap().target();
        let original_file = fs::read(dir.path().join("history.txt")).unwrap();
        let calls = Cell::new(0);
        let options = GitHistoryOptions {
            search_query: Some("message:this-will-not-match".into()),
            ..Default::default()
        };
        let error = GitHistorySnapshot::load_filtered_cancellable(dir.path(), &options, || {
            calls.set(calls.get() + 1);
            calls.get() > 40
        })
        .unwrap_err();
        assert_eq!(error, "Git Graph history scan cancelled");
        assert!(calls.get() > 40);
        assert_eq!(repo.head().unwrap().target(), original_head);
        assert_eq!(
            fs::read(dir.path().join("history.txt")).unwrap(),
            original_file
        );
        assert!(repo.statuses(None).unwrap().is_empty());
    }

    #[test]
    fn pre_cancelled_history_scan_never_opens_repository() {
        let options = GitHistoryOptions::default();
        let error = GitHistorySnapshot::load_filtered_cancellable(
            Path::new("this-repository-does-not-exist"),
            &options,
            || true,
        )
        .unwrap_err();
        assert_eq!(error, "Git Graph history scan cancelled");
    }

    #[test]
    fn bounded_history_load_more_includes_older_commits_without_moving_head() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let sig = git2::Signature::now("History", "history@example.invalid").unwrap();
        let mut index = repo.index().unwrap();
        let mut parent: Option<git2::Oid> = None;
        for n in 0..110 {
            fs::write(dir.path().join("history.txt"), format!("revision {n}\n")).unwrap();
            index.add_path(Path::new("history.txt")).unwrap();
            let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
            let parents = parent
                .map(|oid| vec![repo.find_commit(oid).unwrap()])
                .unwrap_or_default();
            parent = Some(
                repo.commit(
                    Some("HEAD"),
                    &sig,
                    &sig,
                    &format!("revision {n}"),
                    &tree,
                    &parents.iter().collect::<Vec<_>>(),
                )
                .unwrap(),
            );
        }
        let before = repo.head().unwrap().target();
        let normal = GitHistorySnapshot::load(dir.path()).unwrap();
        assert_eq!(normal.commits.len(), MAX_HISTORY);
        assert!(normal.truncated);
        let expanded = GitHistorySnapshot::load_filtered(
            dir.path(),
            &GitHistoryOptions {
                max_commits: 192,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(expanded.commits.len(), 110);
        assert!(!expanded.truncated);
        assert_eq!(normal.commits[0].oid, expanded.commits[0].oid);
        assert_eq!(repo.head().unwrap().target(), before);
    }

    #[test]
    fn explicit_full_history_search_finds_old_subject_and_message_body_beyond_page_limit() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let sig = git2::Signature::now("Search Author", "search@example.invalid").unwrap();
        let mut parent: Option<Oid> = None;
        let mut expected = Vec::new();
        for n in 0..530 {
            fs::write(dir.path().join("history.txt"), format!("{n}\n")).unwrap();
            let mut index = repo.index().unwrap();
            index.add_path(Path::new("history.txt")).unwrap();
            let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
            let parents = parent
                .map(|oid| vec![repo.find_commit(oid).unwrap()])
                .unwrap_or_default();
            let message = match n {
                0 => "ancient-needle in first subject",
                1 => "second commit\n\nancient-needle hidden in body",
                _ => "ordinary revision",
            };
            let oid = repo
                .commit(
                    Some("HEAD"),
                    &sig,
                    &sig,
                    message,
                    &tree,
                    &parents.iter().collect::<Vec<_>>(),
                )
                .unwrap();
            if n < 2 {
                expected.push(oid.to_string());
            }
            parent = Some(oid);
        }
        let head_before = repo.head().unwrap().target();
        let default = GitHistorySnapshot::load(dir.path()).unwrap();
        assert_eq!(default.commits.len(), MAX_HISTORY);
        let found = GitHistorySnapshot::load_filtered(
            dir.path(),
            &GitHistoryOptions {
                search_query: Some("message:ancient-needle".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(found.commits.len(), 2);
        assert!(expected
            .iter()
            .all(|oid| found.commits.iter().any(|row| &row.oid == oid)));
        assert_eq!(found.scanned_commits, 530);
        assert!(!found.scan_limit_reached);
        assert!(!found.truncated);
        assert_eq!(repo.head().unwrap().target(), head_before);
        assert!(GitHistorySnapshot::load_filtered(
            dir.path(),
            &GitHistoryOptions {
                search_query: Some("branch:".into()),
                ..Default::default()
            },
        )
        .is_err());
    }

    #[test]
    fn history_compare_and_commit_preview_are_read_only() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        fs::write(dir.path().join("中文.txt"), "first\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("中文.txt")).unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("Fixture", "fixture@example.invalid").unwrap();
        let first = repo
            .commit(Some("HEAD"), &sig, &sig, "first", &tree, &[])
            .unwrap();
        repo.branch("feature", &repo.find_commit(first).unwrap(), false)
            .unwrap();
        repo.tag_lightweight(
            "v1.0.0",
            repo.find_commit(first).unwrap().as_object(),
            false,
        )
        .unwrap();
        drop(tree);
        fs::write(dir.path().join("中文.txt"), "second\n").unwrap();
        index.add_path(Path::new("中文.txt")).unwrap();
        let next = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let second = repo
            .commit(
                Some("HEAD"),
                &sig,
                &sig,
                "second",
                &next,
                &[&repo.find_commit(first).unwrap()],
            )
            .unwrap();
        repo.reference(
            "refs/remotes/origin/feature",
            second,
            false,
            "disposable fixture remote branch",
        )
        .unwrap();
        drop(next);
        drop(index);
        let snapshot = GitHistorySnapshot::load(dir.path()).unwrap();
        assert_eq!(snapshot.commits.len(), 2);
        assert_eq!(snapshot.branches.len(), 2);
        assert_eq!(snapshot.tags.len(), 1);
        assert_eq!(snapshot.tags[0].name, "v1.0.0");
        assert_eq!(snapshot.tags[0].oid, first.to_string());
        let tagged = GitHistorySnapshot::load_filtered(
            dir.path(),
            &GitHistoryOptions {
                selected_ref: Some("refs/tags/v1.0.0".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(tagged.commits.len(), 1);
        assert_eq!(tagged.commits[0].oid, first.to_string());
        assert!(GitHistorySnapshot::load_filtered(
            dir.path(),
            &GitHistoryOptions {
                selected_ref: Some("refs/tags/no-such-tag".into()),
                ..Default::default()
            },
        )
        .is_err());
        assert!(snapshot
            .commits
            .iter()
            .any(|row| row.oid == second.to_string() && row.parent_ids == vec![first.to_string()]));
        let all = GitHistorySnapshot::load_filtered(
            dir.path(),
            &GitHistoryOptions {
                all_branches: true,
                favorites: vec!["feature".into()],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(all.branches.len(), 3);
        assert!(all
            .branches
            .iter()
            .any(|row| row.remote && row.name == "origin/feature"));
        assert!(all
            .branches
            .iter()
            .any(|row| row.favorite && row.name == "feature"));
        assert!(all.branches[0].current, "current ref must lead Git Graph");
        assert_eq!(
            all.branches[1].name, "feature",
            "starred branch follows HEAD before other refs"
        );
        let focused = GitHistorySnapshot::load_filtered(
            dir.path(),
            &GitHistoryOptions {
                favorites_only: true,
                selected_ref: Some("feature".into()),
                favorites: vec!["feature".into()],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(focused.branches.len(), 1);
        assert_eq!(
            focused.commits.len(),
            1,
            "filter must not include unfavorited HEAD"
        );
        assert_eq!(focused.commits[0].oid, first.to_string());
        let remote = GitHistorySnapshot::load_filtered(
            dir.path(),
            &GitHistoryOptions {
                all_branches: true,
                selected_ref: Some("refs/remotes/origin/feature".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(remote.commits[0].oid, second.to_string());
        let favorite_remote = GitHistorySnapshot::load_filtered(
            dir.path(),
            &GitHistoryOptions {
                favorites_only: true,
                favorites: vec!["refs/remotes/origin/feature".into()],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(favorite_remote.branches.len(), 1);
        assert!(favorite_remote.branches[0].remote);
        assert!(
            GitHistorySnapshot::load_filtered(
                dir.path(),
                &GitHistoryOptions {
                    selected_ref: Some("origin/feature".into()),
                    ..Default::default()
                },
            )
            .is_err(),
            "remote references require allBranches"
        );
        assert!(
            GitHistorySnapshot::load_filtered(
                dir.path(),
                &GitHistoryOptions {
                    selected_ref: Some("feature~1".into()),
                    ..Default::default()
                },
            )
            .is_err(),
            "MCP cannot smuggle a revspec into branch selection"
        );
        assert!(preview_commit(dir.path(), &second.to_string())
            .unwrap()
            .contains("+second"));
        assert!(compare_local_branch(dir.path(), "feature")
            .unwrap()
            .contains("0 ahead / 1 behind"));
        let ref_comparison = compare_git_refs(dir.path(), "feature", "HEAD", None).unwrap();
        assert!(ref_comparison.contains("Compare feature..HEAD"));
        assert!(ref_comparison.contains("+second"));
        assert!(compare_git_refs(dir.path(), "../unsafe", "HEAD", None).is_err());
        assert!(compare_git_refs(dir.path(), "feature", "missing", None).is_err());
        assert!(compare_git_refs(dir.path(), "feature", "HEAD", Some("../outside")).is_err());
        assert!(preview_commit(dir.path(), "../invalid").is_err());
        assert!(compare_local_branch(dir.path(), "unknown").is_err());
        assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), second);
        assert_eq!(
            fs::read_to_string(dir.path().join("中文.txt")).unwrap(),
            "second\n"
        );
    }

    #[test]
    fn file_scoped_git_graph_only_lists_matching_commits_and_refuses_path_escape() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::create_dir_all(dir.path().join("docs")).unwrap();
        let signature = git2::Signature::now("Native Scope Test", "scope@example.invalid").unwrap();
        for (path, text, message) in [
            ("src/entry.rs", "first\n", "initial source"),
            ("docs/notes.md", "doc\n", "documentation change"),
            ("src/entry.rs", "second\n", "source change"),
        ] {
            fs::write(dir.path().join(path), text).unwrap();
            let mut index = repo.index().unwrap();
            index.add_path(Path::new(path)).unwrap();
            let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
            let parents = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
            if let Some(parent) = parents.as_ref() {
                repo.commit(
                    Some("HEAD"),
                    &signature,
                    &signature,
                    message,
                    &tree,
                    &[parent],
                )
                .unwrap();
            } else {
                repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &[])
                    .unwrap();
            }
        }
        let before_head = repo.head().unwrap().target();
        let source = GitHistorySnapshot::load_filtered(
            dir.path(),
            &GitHistoryOptions {
                file_path: Some("src/entry.rs".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(source.commits.len(), 2);
        assert!(source
            .commits
            .iter()
            .all(|commit| commit.subject.contains("source")));
        let subtree = GitHistorySnapshot::load_filtered(
            dir.path(),
            &GitHistoryOptions {
                git_diff_root: Some("src".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(subtree.commits.len(), 2);
        let selected = GitHistorySnapshot::load_filtered(
            dir.path(),
            &GitHistoryOptions {
                git_diff_root: Some("src".into()),
                file_path: Some("src/entry.rs".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(selected.commits.len(), 2);
        for options in [
            GitHistoryOptions {
                git_diff_root: Some("../outside".into()),
                ..Default::default()
            },
            GitHistoryOptions {
                file_path: Some("../outside.txt".into()),
                ..Default::default()
            },
            GitHistoryOptions {
                file_path: Some("docs/not-found.md".into()),
                ..Default::default()
            },
            GitHistoryOptions {
                git_diff_root: Some("src".into()),
                file_path: Some("docs/notes.md".into()),
                ..Default::default()
            },
        ] {
            assert!(GitHistorySnapshot::load_filtered(dir.path(), &options).is_err());
        }
        assert_eq!(repo.head().unwrap().target(), before_head);
        assert_eq!(
            fs::read_to_string(dir.path().join("src/entry.rs")).unwrap(),
            "second\n"
        );
    }

    /// A file's Git identity can outlive both its pathname and the on-disk
    /// file. No real user repository is touched by this test.
    #[test]
    fn file_history_follows_first_parent_renames_and_deleted_paths() {
        fn commit_file(repo: &Repository, add: &[&str], remove: &[&str], message: &str) -> Oid {
            let mut index = repo.index().unwrap();
            for path in remove {
                index.remove_path(Path::new(path)).unwrap();
            }
            for path in add {
                index.add_path(Path::new(path)).unwrap();
            }
            let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
            let sig = git2::Signature::now("Native Rename Test", "rename@example.invalid").unwrap();
            let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
            if let Some(parent) = parent.as_ref() {
                repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[parent])
                    .unwrap()
            } else {
                repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[])
                    .unwrap()
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::create_dir_all(dir.path().join("docs")).unwrap();
        fs::write(dir.path().join("src/old.txt"), "hello\n").unwrap();
        commit_file(&repo, &["src/old.txt"], &[], "created old path");
        fs::write(dir.path().join("src/old.txt"), "hello\nworld\n").unwrap();
        commit_file(&repo, &["src/old.txt"], &[], "modified old path");
        fs::rename(
            dir.path().join("src/old.txt"),
            dir.path().join("src/new.txt"),
        )
        .unwrap();
        commit_file(
            &repo,
            &["src/new.txt"],
            &["src/old.txt"],
            "renamed old to new",
        );
        fs::write(dir.path().join("docs/readme.md"), "unrelated\n").unwrap();
        commit_file(&repo, &["docs/readme.md"], &[], "unrelated documentation");
        fs::write(dir.path().join("src/new.txt"), "hello\nworld\nthird\n").unwrap();
        commit_file(&repo, &["src/new.txt"], &[], "modified new path");

        let renamed = GitHistorySnapshot::load_filtered(
            dir.path(),
            &GitHistoryOptions {
                file_path: Some("src/new.txt".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            renamed
                .commits
                .iter()
                .map(|row| row.subject.as_str())
                .collect::<Vec<_>>(),
            vec![
                "modified new path",
                "renamed old to new",
                "modified old path",
                "created old path",
            ]
        );

        fs::remove_file(dir.path().join("src/new.txt")).unwrap();
        commit_file(&repo, &[], &["src/new.txt"], "deleted renamed file");
        fs::write(dir.path().join("docs/readme.md"), "unrelated newer\n").unwrap();
        commit_file(&repo, &["docs/readme.md"], &[], "newest documentation");

        let deleted = GitHistorySnapshot::load_filtered(
            dir.path(),
            &GitHistoryOptions {
                file_path: Some("src/new.txt".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            deleted
                .commits
                .iter()
                .map(|row| row.subject.as_str())
                .collect::<Vec<_>>(),
            vec![
                "deleted renamed file",
                "modified new path",
                "renamed old to new",
                "modified old path",
                "created old path",
            ]
        );
        let old = GitHistorySnapshot::load_filtered(
            dir.path(),
            &GitHistoryOptions {
                file_path: Some("src/old.txt".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(old
            .commits
            .iter()
            .any(|row| row.subject == "modified old path"));
        assert!(!old
            .commits
            .iter()
            .any(|row| row.subject == "modified new path"));
        assert!(
            GitHistorySnapshot::load_filtered(
                dir.path(),
                &GitHistoryOptions {
                    file_path: Some("src/never-existed.txt".into()),
                    ..Default::default()
                },
            )
            .is_err(),
            "An unknown deleted path must fail rather than display empty success"
        );
        assert!(
            GitHistorySnapshot::load_filtered(
                dir.path(),
                &GitHistoryOptions {
                    git_diff_root: Some("docs".into()),
                    file_path: Some("src/old.txt".into()),
                    ..Default::default()
                },
            )
            .is_err(),
            "Historical file paths cannot escape the requested Git root"
        );
        assert_eq!(
            repo.head()
                .unwrap()
                .peel_to_commit()
                .unwrap()
                .summary()
                .unwrap(),
            Some("newest documentation")
        );
    }
}
