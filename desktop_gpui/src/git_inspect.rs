//! Read-only, bounded Git inspection. No Git index/refs/worktree writes.
//! Background-only caller: Git status and patch calculation may touch many files.

use std::fs;
use std::path::{Component, Path, PathBuf};

const MAX_CHANGES: usize = 120;
const MAX_PATCH_BYTES: usize = 32 * 1024;

/// Coalesce repeated manual/file-system refresh requests without ignoring
/// clicks while a snapshot is in flight. A completion starts at most one
/// extra snapshot; no concurrent Git status readers are spawned by this gate.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GitRefreshGate {
    active: bool,
    queued: bool,
}

impl GitRefreshGate {
    /// Returns true only if a new background snapshot should start now.
    pub fn request(&mut self) -> bool {
        if self.active {
            self.queued = true;
            false
        } else {
            self.active = true;
            true
        }
    }

    /// Returns true if a follow-up snapshot should start immediately.
    /// When true, `active` stays set until that snapshot also completes.
    pub fn complete(&mut self) -> bool {
        if self.queued {
            self.queued = false;
            true
        } else {
            self.active = false;
            false
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn active(&self) -> bool {
        self.active
    }
}

#[derive(Debug, Clone)]
pub struct GitChange {
    pub relative: PathBuf,
    pub label: &'static str,
    pub status: git2::Status,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GitChangeArea {
    Conflicts,
    Staged,
    Changes,
    Untracked,
}

impl GitChangeArea {
    pub fn title(self) -> &'static str {
        match self {
            Self::Conflicts => "MERGE CONFLICTS",
            Self::Staged => "STAGED CHANGES",
            Self::Changes => "CHANGES",
            Self::Untracked => "UNTRACKED",
        }
    }
    pub fn can_stage(self) -> bool {
        matches!(self, Self::Changes | Self::Untracked)
    }
    pub fn can_unstage(self) -> bool {
        self == Self::Staged
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GitChangeRow {
    pub relative: PathBuf,
    pub label: &'static str,
    pub area: GitChangeArea,
}

#[derive(Debug, Clone)]
pub struct GitSnapshot {
    pub branch: String,
    pub changed_files: Vec<GitChange>,
    pub truncated: bool,
    pub repository_root: PathBuf,
}

impl GitSnapshot {
    pub fn load(workspace: &Path) -> Result<Self, String> {
        let repo = git2::Repository::discover(workspace).map_err(|e| e.to_string())?;
        let workdir = repo
            .workdir()
            .ok_or("A bare repository has no editable working tree")?;
        let root = dunce::canonicalize(workdir).map_err(|e| e.to_string())?;
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(true)
            .include_ignored(false);
        let statuses = repo.statuses(Some(&mut opts)).map_err(|e| e.to_string())?;
        let mut changed_files = Vec::new();
        let mut truncated = false;
        for item in statuses.iter() {
            let Ok(path) = item.path() else { continue };
            let relative = PathBuf::from(path);
            if !safe_relative(&relative) {
                continue;
            }
            changed_files.push(GitChange {
                relative,
                label: classify(item.status()),
                status: item.status(),
            });
            if changed_files.len() >= MAX_CHANGES {
                truncated = true;
                break;
            }
        }
        changed_files.sort_by(|a, b| a.relative.cmp(&b.relative));
        let branch = repo
            .head()
            .ok()
            .and_then(|head| head.shorthand().ok().map(str::to_string))
            .unwrap_or_else(|| "(detached or unborn HEAD)".to_string());
        Ok(Self {
            branch,
            changed_files,
            truncated,
            repository_root: root,
        })
    }

    /// A file staged and edited again appears in both areas, like Classic.
    pub fn visible_rows(&self, filter: &str) -> Vec<GitChangeRow> {
        use git2::Status;
        let filter = filter.trim().to_lowercase();
        let mut rows = Vec::new();
        for change in &self.changed_files {
            if !filter.is_empty()
                && !change
                    .relative
                    .to_string_lossy()
                    .to_lowercase()
                    .contains(&filter)
            {
                continue;
            }
            let status = change.status;
            let mut push = |area, label| {
                rows.push(GitChangeRow {
                    relative: change.relative.clone(),
                    label,
                    area,
                })
            };
            if status.contains(Status::CONFLICTED) {
                push(GitChangeArea::Conflicts, "!");
                continue;
            }
            if status.intersects(
                Status::INDEX_NEW
                    | Status::INDEX_MODIFIED
                    | Status::INDEX_DELETED
                    | Status::INDEX_RENAMED
                    | Status::INDEX_TYPECHANGE,
            ) {
                push(GitChangeArea::Staged, area_label(status, true));
            }
            if status.contains(Status::WT_NEW) {
                push(GitChangeArea::Untracked, "?");
            } else if status.intersects(
                Status::WT_MODIFIED
                    | Status::WT_DELETED
                    | Status::WT_RENAMED
                    | Status::WT_TYPECHANGE,
            ) {
                push(GitChangeArea::Changes, area_label(status, false));
            }
        }
        rows.sort_by(|a, b| {
            a.area
                .cmp(&b.area)
                .then_with(|| a.relative.cmp(&b.relative))
        });
        rows
    }
}

fn area_label(status: git2::Status, index: bool) -> &'static str {
    use git2::Status;
    if index {
        if status.contains(Status::INDEX_DELETED) {
            "D"
        } else if status.contains(Status::INDEX_NEW) {
            "A"
        } else if status.contains(Status::INDEX_RENAMED) {
            "R"
        } else if status.contains(Status::INDEX_TYPECHANGE) {
            "T"
        } else {
            "M"
        }
    } else if status.contains(Status::WT_DELETED) {
        "D"
    } else if status.contains(Status::WT_RENAMED) {
        "R"
    } else if status.contains(Status::WT_TYPECHANGE) {
        "T"
    } else {
        "M"
    }
}

fn safe_relative(path: &Path) -> bool {
    path.components().all(|c| matches!(c, Component::Normal(_))) && !path.as_os_str().is_empty()
}

fn classify(status: git2::Status) -> &'static str {
    use git2::Status;
    if status.contains(Status::CONFLICTED) {
        "CONFLICT"
    } else if status.intersects(Status::INDEX_DELETED | Status::WT_DELETED) {
        "D"
    } else if status.intersects(Status::INDEX_NEW | Status::WT_NEW) {
        "?"
    } else if status.intersects(Status::INDEX_RENAMED | Status::WT_RENAMED) {
        "R"
    } else if status.intersects(Status::INDEX_MODIFIED | Status::WT_MODIFIED) {
        "M"
    } else if status.intersects(Status::INDEX_TYPECHANGE | Status::WT_TYPECHANGE) {
        "T"
    } else {
        "·"
    }
}

pub fn read_file_diff(workspace: &Path, relative: &Path) -> Result<String, String> {
    if !safe_relative(relative) {
        return Err("Unsafe Git path".into());
    }
    let repo = git2::Repository::discover(workspace).map_err(|e| e.to_string())?;
    let repo_root = repo.workdir().ok_or("No Git working tree")?;
    let mut options = git2::DiffOptions::new();
    options.pathspec(relative).context_lines(3);
    let diff = repo
        .diff_index_to_workdir(None, Some(&mut options))
        .map_err(|e| e.to_string())?;
    let mut output = String::new();
    append_diff(&diff, &mut output)?;
    if output.is_empty() {
        let tree = repo.head().ok().and_then(|head| head.peel_to_tree().ok());
        let staged = repo
            .diff_tree_to_index(tree.as_ref(), None, Some(&mut options))
            .map_err(|e| e.to_string())?;
        append_diff(&staged, &mut output)?;
    }
    if output.is_empty() {
        // Untracked files don't participate in Git's index-to-workdir diff.
        let file = repo_root.join(relative);
        let canonical = dunce::canonicalize(&file).map_err(|e| e.to_string())?;
        let root = dunce::canonicalize(repo_root).map_err(|e| e.to_string())?;
        if !canonical.starts_with(root) || !canonical.is_file() {
            return Err("Git preview target is not a regular workspace file".into());
        }
        let metadata = fs::metadata(&canonical).map_err(|e| e.to_string())?;
        if metadata.len() > MAX_PATCH_BYTES as u64 {
            return Ok("Untracked file · too large for native diff preview".into());
        }
        let text = fs::read_to_string(&canonical)
            .map_err(|_| "Binary or non-UTF-8 file; preview unavailable".to_string())?;
        output = format!("Untracked (read-only): {}\n{}", relative.display(), text);
    }
    if output.len() > MAX_PATCH_BYTES {
        output.truncate(output.floor_char_boundary(MAX_PATCH_BYTES));
        output.push_str("\n… diff preview truncated");
    }
    Ok(output)
}

/// Area-aware, read-only diff. A staged+unstaged file is never collapsed:
/// Staged shows HEAD -> index, Changes shows index -> working tree.
pub fn read_file_diff_for_area(
    workspace: &Path,
    relative: &Path,
    area: GitChangeArea,
) -> Result<String, String> {
    if !safe_relative(relative) {
        return Err("Unsafe Git path".into());
    }
    let repo = git2::Repository::discover(workspace).map_err(|error| error.to_string())?;
    let root = repo.workdir().ok_or("No Git working tree")?;
    let status = repo
        .status_file(relative)
        .map_err(|error| error.to_string())?;
    let valid = match area {
        GitChangeArea::Staged => status.intersects(
            git2::Status::INDEX_NEW
                | git2::Status::INDEX_MODIFIED
                | git2::Status::INDEX_DELETED
                | git2::Status::INDEX_RENAMED
                | git2::Status::INDEX_TYPECHANGE,
        ),
        GitChangeArea::Changes => status.intersects(
            git2::Status::WT_MODIFIED
                | git2::Status::WT_DELETED
                | git2::Status::WT_RENAMED
                | git2::Status::WT_TYPECHANGE,
        ),
        GitChangeArea::Untracked => status.contains(git2::Status::WT_NEW),
        GitChangeArea::Conflicts => false,
    };
    if !valid || status.contains(git2::Status::CONFLICTED) {
        return Err(
            "This Git row changed or has unresolved conflicts; refresh Source Control".into(),
        );
    }
    if area == GitChangeArea::Untracked {
        let canonical =
            dunce::canonicalize(root.join(relative)).map_err(|error| error.to_string())?;
        let canonical_root = dunce::canonicalize(root).map_err(|error| error.to_string())?;
        if !canonical.starts_with(&canonical_root) || !canonical.is_file() {
            return Err("Untracked preview target is not a regular worktree file".into());
        }
        let metadata = fs::metadata(&canonical).map_err(|error| error.to_string())?;
        if metadata.len() > MAX_PATCH_BYTES as u64 {
            return Ok("Untracked file is too large for Native text preview".into());
        }
        let contents = fs::read_to_string(canonical)
            .map_err(|_| "Untracked file is binary or not UTF-8".to_string())?;
        return Ok(format!(
            "Untracked (read-only): {}\n{}",
            relative.display(),
            contents
        ));
    }
    let mut options = git2::DiffOptions::new();
    options.pathspec(relative).context_lines(3);
    let diff = match area {
        GitChangeArea::Staged => {
            let tree = repo.head().ok().and_then(|head| head.peel_to_tree().ok());
            repo.diff_tree_to_index(tree.as_ref(), None, Some(&mut options))
        }
        GitChangeArea::Changes => repo.diff_index_to_workdir(None, Some(&mut options)),
        GitChangeArea::Untracked | GitChangeArea::Conflicts => unreachable!(),
    }
    .map_err(|error| error.to_string())?;
    let mut output = format!("{}: {}\n", area.title(), relative.display());
    append_diff(&diff, &mut output)?;
    if output.len() > MAX_PATCH_BYTES {
        output.truncate(output.floor_char_boundary(MAX_PATCH_BYTES));
        output.push_str("\n… diff preview truncated");
    }
    Ok(output)
}

/// Read a *committed or indexed* original blob. Never read the working file,
/// and never fabricate an original for a new/untracked/conflicted file.
pub fn read_git_original_for_area(
    workspace: &Path,
    relative: &Path,
    area: GitChangeArea,
) -> Result<String, String> {
    if !safe_relative(relative) {
        return Err("Unsafe Git original path".into());
    }
    let repo = git2::Repository::discover(workspace).map_err(|error| error.to_string())?;
    repo.workdir().ok_or("No Git working tree")?;
    let status = repo
        .status_file(relative)
        .map_err(|error| error.to_string())?;
    if status.contains(git2::Status::CONFLICTED) {
        return Err("Use the merge conflict view to inspect original revisions".into());
    }
    let (blob, label) = match area {
        GitChangeArea::Staged => {
            if !status.intersects(
                git2::Status::INDEX_NEW
                    | git2::Status::INDEX_MODIFIED
                    | git2::Status::INDEX_DELETED
                    | git2::Status::INDEX_RENAMED
                    | git2::Status::INDEX_TYPECHANGE,
            ) {
                return Err("Staged Git row is stale; refresh Source Control".into());
            }
            let tree = match repo.head().and_then(|head| head.peel_to_tree()) {
                Ok(tree) => tree,
                Err(_) => return Ok("New file: no committed original at HEAD".into()),
            };
            let entry = match tree.get_path(relative) {
                Ok(entry) => entry,
                Err(error) if error.code() == git2::ErrorCode::NotFound => {
                    return Ok("New file: no committed original at HEAD".into())
                }
                Err(error) => return Err(error.to_string()),
            };
            let blob = repo
                .find_blob(entry.id())
                .map_err(|_| "HEAD original is not a regular Git blob".to_string())?;
            (blob, "HEAD (before staged changes)")
        }
        GitChangeArea::Changes => {
            if !status.intersects(
                git2::Status::WT_MODIFIED
                    | git2::Status::WT_DELETED
                    | git2::Status::WT_RENAMED
                    | git2::Status::WT_TYPECHANGE,
            ) {
                return Err("Unstaged Git row is stale; refresh Source Control".into());
            }
            let index = repo.index().map_err(|error| error.to_string())?;
            let entry = index
                .get_path(relative, 0)
                .ok_or("No indexed original for this worktree file")?;
            let blob = repo
                .find_blob(entry.id)
                .map_err(|_| "Indexed original is not a regular Git blob".to_string())?;
            (blob, "Index (before unstaged changes)")
        }
        GitChangeArea::Untracked => return Ok("Untracked file: no Git original available".into()),
        GitChangeArea::Conflicts => {
            return Err("Use the merge conflict view for conflicted files".into())
        }
    };
    let contents = blob.content();
    if contents.len() > MAX_PATCH_BYTES {
        return Ok(format!(
            "{label}: {}\nToo large for Native text preview",
            relative.display()
        ));
    }
    if contents.contains(&0) {
        return Ok(format!(
            "{label}: {}\nBinary file: text preview unavailable",
            relative.display()
        ));
    }
    let body = std::str::from_utf8(contents)
        .map_err(|_| "Original Git blob is not UTF-8 text".to_string())?;
    Ok(format!("{label}: {}\n\n{body}", relative.display()))
}

fn append_diff(diff: &git2::Diff<'_>, out: &mut String) -> Result<(), String> {
    diff.print(git2::DiffFormat::Patch, |_, _, line| {
        if out.len() < MAX_PATCH_BYTES {
            if matches!(line.origin(), '+' | '-' | ' ') {
                out.push(line.origin());
            }
            let available = MAX_PATCH_BYTES.saturating_sub(out.len());
            let bytes = line.content();
            let raw = String::from_utf8_lossy(&bytes[..bytes.len().min(available)]);
            out.push_str(&raw);
        }
        true
    })
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_gate_queues_manual_refresh_and_coalesces_many_requests() {
        let mut gate = GitRefreshGate::default();
        assert!(gate.request());
        assert!(!gate.request());
        assert!(!gate.request());
        assert!(gate.complete(), "one extra refresh must run");
        assert!(gate.active(), "work must stay marked active");
        assert!(!gate.complete());
        assert!(!gate.active());
        assert!(gate.request(), "refresh remains available after completion");
        gate.reset();
        assert!(!gate.active());
        assert!(gate.request(), "a new worktree starts a fresh refresh");
    }

    #[test]
    fn staged_and_working_changes_are_distinct_and_filterable() {
        let temp = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(temp.path()).unwrap();
        fs::write(temp.path().join("shared.txt"), "base\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("shared.txt")).unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let signature = git2::Signature::now("Test", "test@example.invalid").unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "base", &tree, &[])
            .unwrap();
        drop(tree);
        fs::write(temp.path().join("shared.txt"), "staged\n").unwrap();
        index.add_path(Path::new("shared.txt")).unwrap();
        index.write().unwrap();
        drop(index);
        fs::write(temp.path().join("shared.txt"), "working\n").unwrap();
        fs::write(temp.path().join("new.txt"), "untracked\n").unwrap();
        let snapshot = GitSnapshot::load(temp.path()).unwrap();
        let rows = snapshot.visible_rows("");
        assert_eq!(rows.len(), 3);
        assert!(rows
            .iter()
            .any(|r| r.area == GitChangeArea::Staged && r.relative == Path::new("shared.txt")));
        assert!(rows
            .iter()
            .any(|r| r.area == GitChangeArea::Changes && r.relative == Path::new("shared.txt")));
        assert!(rows
            .iter()
            .any(|r| r.area == GitChangeArea::Untracked && r.relative == Path::new("new.txt")));
        assert_eq!(snapshot.visible_rows("SHARED").len(), 2);
        assert!(snapshot.visible_rows("nonexistent").is_empty());
    }

    #[test]
    fn area_aware_diff_and_original_preserve_staged_vs_unstaged() {
        let temp = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(temp.path()).unwrap();
        repo.config()
            .unwrap()
            .set_bool("core.autocrlf", false)
            .unwrap();
        fs::write(temp.path().join("shared.txt"), "head-only\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("shared.txt")).unwrap();
        let signature = git2::Signature::now("Test", "test@example.invalid").unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
            .unwrap();
        drop(tree);
        fs::write(temp.path().join("shared.txt"), "index-only\n").unwrap();
        index.add_path(Path::new("shared.txt")).unwrap();
        index.write().unwrap();
        drop(index);
        fs::write(temp.path().join("shared.txt"), "worktree-only\n").unwrap();
        fs::write(temp.path().join("untracked.txt"), "do not stage\n").unwrap();
        let path = Path::new("shared.txt");
        let staged = read_file_diff_for_area(temp.path(), path, GitChangeArea::Staged).unwrap();
        assert!(staged.contains("+index-only"), "{staged}");
        assert!(!staged.contains("worktree-only"), "{staged}");
        let changes = read_file_diff_for_area(temp.path(), path, GitChangeArea::Changes).unwrap();
        assert!(changes.contains("+worktree-only"), "{changes}");
        assert!(!changes.contains("head-only"), "{changes}");
        let head_original =
            read_git_original_for_area(temp.path(), path, GitChangeArea::Staged).unwrap();
        let index_original =
            read_git_original_for_area(temp.path(), path, GitChangeArea::Changes).unwrap();
        assert!(head_original.contains("head-only"));
        assert!(!head_original.contains("index-only"));
        assert!(index_original.contains("index-only"));
        assert!(!index_original.contains("worktree-only"));
        let untracked = read_file_diff_for_area(
            temp.path(),
            Path::new("untracked.txt"),
            GitChangeArea::Untracked,
        )
        .unwrap();
        assert!(untracked.contains("do not stage"));
        assert!(read_git_original_for_area(
            temp.path(),
            Path::new("untracked.txt"),
            GitChangeArea::Untracked
        )
        .unwrap()
        .contains("no Git original"));
        let row_count = GitSnapshot::load(temp.path())
            .unwrap()
            .visible_rows("")
            .len();
        assert_eq!(row_count, 3);
        assert_eq!(
            fs::read_to_string(temp.path().join("shared.txt")).unwrap(),
            "worktree-only\n"
        );
    }

    #[test]
    fn area_aware_preview_rejects_stale_status_and_unsafe_paths() {
        let temp = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(temp.path()).unwrap();
        fs::write(temp.path().join("tracked.txt"), "base").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("tracked.txt")).unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("Test", "test@example.invalid").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "base", &tree, &[])
            .unwrap();
        index.write().unwrap();
        drop(tree);
        drop(index);
        assert!(read_file_diff_for_area(
            temp.path(),
            Path::new("tracked.txt"),
            GitChangeArea::Changes
        )
        .is_err());
        assert!(read_git_original_for_area(
            temp.path(),
            Path::new("tracked.txt"),
            GitChangeArea::Staged
        )
        .is_err());
        for path in ["../escape", "."] {
            assert!(
                read_file_diff_for_area(temp.path(), Path::new(path), GitChangeArea::Staged)
                    .is_err()
            );
            assert!(read_git_original_for_area(
                temp.path(),
                Path::new(path),
                GitChangeArea::Changes
            )
            .is_err());
        }
    }

    #[test]
    fn read_only_git_status_and_unified_diff_support_unicode_paths() {
        let temp = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(temp.path()).unwrap();
        fs::write(temp.path().join("existing.rs"), "before\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("existing.rs")).unwrap();
        let tree_oid = index.write_tree().unwrap();
        index.write().unwrap();
        drop(index);
        let tree = repo.find_tree(tree_oid).unwrap();
        let signature = git2::Signature::now("Alera Test", "test@local.invalid").unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "fixture", &tree, &[])
            .unwrap();
        drop(tree);
        fs::write(temp.path().join("existing.rs"), "after\n").unwrap();
        fs::write(temp.path().join("中文檔案.txt"), "測試文字").unwrap();
        let snapshot = GitSnapshot::load(temp.path()).unwrap();
        assert_eq!(snapshot.changed_files.len(), 2);
        assert!(snapshot
            .changed_files
            .iter()
            .any(|x| x.relative == Path::new("中文檔案.txt")));
        let patch = read_file_diff(temp.path(), Path::new("existing.rs")).unwrap();
        assert!(
            patch.contains("+after"),
            "expected Git unified addition, got: {patch}"
        );
        assert!(read_file_diff(temp.path(), Path::new("中文檔案.txt"))
            .unwrap()
            .contains("測試文字"));
        assert!(read_file_diff(temp.path(), Path::new("../escape")).is_err());
    }
}
