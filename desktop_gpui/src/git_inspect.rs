//! Read-only, bounded Git inspection. No Git index/refs/worktree writes.
//! Background-only caller: Git status and patch calculation may touch many files.

use std::fs;
use std::path::{Component, Path, PathBuf};

const MAX_CHANGES: usize = 120;
const MAX_PATCH_BYTES: usize = 32 * 1024;
const MAX_PAGED_DIFF_LINES: usize = 100_000;

/// One bounded, read-only slice of a file's current Git patch. Cursors count
/// Git patch records rather than byte offsets, so multi-byte text is never
/// split across normal page boundaries. Every page is fetched independently.
#[derive(Debug, Clone)]
pub struct GitPatchPage {
    pub text: String,
    pub start_line: usize,
    pub next_line: Option<usize>,
    pub clipped_long_line: bool,
}

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
    let mut truncated = false;
    append_diff(&diff, &mut output, &mut truncated)?;
    if output.is_empty() {
        let tree = repo.head().ok().and_then(|head| head.peel_to_tree().ok());
        let staged = repo
            .diff_tree_to_index(tree.as_ref(), None, Some(&mut options))
            .map_err(|e| e.to_string())?;
        append_diff(&staged, &mut output, &mut truncated)?;
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
    if truncated {
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
    let mut truncated = false;
    append_diff(&diff, &mut output, &mut truncated)?;
    if truncated {
        output.push_str("\n… diff preview truncated");
    }
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

/// An immutable Git blob snapshot suitable for a dedicated read-only tab.
/// Unlike a worktree editor buffer it has no save path, drafts or write API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitOriginalSnapshot {
    pub relative: PathBuf,
    pub area: GitChangeArea,
    pub blob_oid: String,
    pub content: String,
}

impl GitOriginalSnapshot {
    pub fn title(&self) -> String {
        let filename = self
            .relative
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        let revision = match self.area {
            GitChangeArea::Staged => "HEAD",
            GitChangeArea::Changes => "INDEX",
            GitChangeArea::Untracked | GitChangeArea::Conflicts => "invalid",
        };
        format!("{filename} · {revision} · {}", &self.blob_oid[..8])
    }
}

/// Resolve an original Git blob with its exact object identity. New files,
/// untracked paths, conflicted paths, binaries and too-large snapshots are
/// rejected instead of opening a writable surrogate or synthetic message.
pub fn read_git_original_snapshot(
    workspace: &Path,
    relative: &Path,
    area: GitChangeArea,
) -> Result<GitOriginalSnapshot, String> {
    if !safe_relative(relative) {
        return Err("Unsafe Git original path".into());
    }
    let repo = git2::Repository::discover(workspace).map_err(|e| e.to_string())?;
    let root = dunce::canonicalize(repo.workdir().ok_or("No Git working tree")?)
        .map_err(|e| e.to_string())?;
    let workspace = dunce::canonicalize(workspace).map_err(|e| e.to_string())?;
    if root != workspace {
        return Err("Original requires the currently selected worktree root".into());
    }
    let status = repo.status_file(relative).map_err(|e| e.to_string())?;
    if status.contains(git2::Status::CONFLICTED) {
        return Err("Unresolved conflicts require the merge resolution view".into());
    }
    let id = match area {
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
            let tree = repo
                .head()
                .and_then(|head| head.peel_to_tree())
                .map_err(|_| "No original HEAD revision exists".to_string())?;
            tree.get_path(relative)
                .map_err(|_| "New file: no committed HEAD original".to_string())?
                .id()
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
            let index = repo.index().map_err(|e| e.to_string())?;
            index
                .get_path(relative, 0)
                .ok_or("No indexed original for this changed file")?
                .id
        }
        GitChangeArea::Untracked | GitChangeArea::Conflicts => {
            return Err("Untracked or conflicted files have no single Git original".into());
        }
    };
    let blob = repo
        .find_blob(id)
        .map_err(|_| "Git original is not a regular blob".to_string())?;
    if blob.content().len() > MAX_PATCH_BYTES {
        return Err("Git original exceeds the bounded read-only text tab size".into());
    }
    if blob.is_binary() || blob.content().contains(&0) {
        return Err("Binary Git original cannot be displayed as text".into());
    }
    let content = std::str::from_utf8(blob.content())
        .map_err(|_| "Git original is not valid UTF-8".to_string())?
        .to_owned();
    Ok(GitOriginalSnapshot {
        relative: relative.to_path_buf(),
        area,
        blob_oid: id.to_string(),
        content,
    })
}

/// Fetch a fresh segment without assembling the complete diff in memory.
/// Page navigation is intentionally limited to the selected changed file;
/// commit history and branch comparisons retain their own preview semantics.
///
/// A caller must replace the displayed page rather than concatenating pages:
/// worktree contents can change between independent requests.
pub fn read_file_diff_page(
    workspace: &Path,
    relative: &Path,
    start_line: usize,
) -> Result<GitPatchPage, String> {
    if !safe_relative(relative) {
        return Err("Unsafe Git path".into());
    }
    if start_line >= MAX_PAGED_DIFF_LINES {
        return Err("Git diff paging safety limit reached".into());
    }
    let repo = git2::Repository::discover(workspace).map_err(|e| e.to_string())?;
    let mut options = git2::DiffOptions::new();
    options.pathspec(relative).context_lines(3);
    let unstaged = repo
        .diff_index_to_workdir(None, Some(&mut options))
        .map_err(|e| e.to_string())?;
    if unstaged.deltas().len() != 0 {
        return render_git_diff_page(&unstaged, start_line);
    }

    let tree = repo.head().ok().and_then(|head| head.peel_to_tree().ok());
    let staged = repo
        .diff_tree_to_index(tree.as_ref(), None, Some(&mut options))
        .map_err(|e| e.to_string())?;
    if staged.deltas().len() != 0 {
        return render_git_diff_page(&staged, start_line);
    }

    // An untracked file is not represented in the Git diff engine; preserve
    // the existing safe absolute-path/symlink/size checks for its preview.
    if start_line != 0 {
        return Err("Git patch changed; reopen its first page".into());
    }
    let mut text = read_file_diff(workspace, relative)?;
    let mut clipped = false;
    if text.len() > MAX_PATCH_BYTES {
        // The normal untracked file-size check counts file bytes, whereas
        // its display adds a read-only header. The *complete preview* must
        // still respect the same page budget and explicitly warn on loss.
        text.truncate(text.floor_char_boundary(MAX_PATCH_BYTES));
        text.push_str("\n… diff preview truncated");
        clipped = true;
    }
    Ok(GitPatchPage {
        text,
        start_line,
        next_line: None,
        clipped_long_line: clipped,
    })
}

/// Render exactly the selected Source Control area, never choosing a
/// different staged/working patch for a file with both kinds of changes.
/// This supports the same central Hunk pager for Staged and Changes rows.
pub fn read_file_diff_page_for_area(
    workspace: &Path,
    relative: &Path,
    area: GitChangeArea,
    start_line: usize,
) -> Result<GitPatchPage, String> {
    use git2::Status;
    if !safe_relative(relative) {
        return Err("Unsafe Git path".into());
    }
    if start_line >= MAX_PAGED_DIFF_LINES {
        return Err("Git diff paging safety limit reached".into());
    }
    if area == GitChangeArea::Conflicts {
        return Err("Resolve the Git conflict before opening a file diff".into());
    }
    let repo = git2::Repository::discover(workspace).map_err(|error| error.to_string())?;
    let status = repo
        .status_file(relative)
        .map_err(|error| error.to_string())?;
    let valid = match area {
        GitChangeArea::Staged => status.intersects(
            Status::INDEX_NEW
                | Status::INDEX_MODIFIED
                | Status::INDEX_DELETED
                | Status::INDEX_RENAMED
                | Status::INDEX_TYPECHANGE,
        ),
        GitChangeArea::Changes => status.intersects(
            Status::WT_MODIFIED | Status::WT_DELETED | Status::WT_RENAMED | Status::WT_TYPECHANGE,
        ),
        GitChangeArea::Untracked => status.contains(Status::WT_NEW),
        GitChangeArea::Conflicts => false,
    };
    if !valid || status.contains(Status::CONFLICTED) {
        return Err("Git change area is stale; refresh Source Control".into());
    }
    if area == GitChangeArea::Untracked {
        if start_line != 0 {
            return Err("Untracked preview is one page; refresh the selected file".into());
        }
        let mut text = read_file_diff_for_area(workspace, relative, area)?;
        let mut clipped = false;
        if text.len() > MAX_PATCH_BYTES {
            text.truncate(text.floor_char_boundary(MAX_PATCH_BYTES));
            text.push_str("\n… diff preview truncated");
            clipped = true;
        }
        return Ok(GitPatchPage {
            text,
            start_line,
            next_line: None,
            clipped_long_line: clipped,
        });
    }
    let mut opts = git2::DiffOptions::new();
    opts.pathspec(relative).context_lines(3);
    let patch = match area {
        GitChangeArea::Staged => {
            let tree = repo.head().ok().and_then(|head| head.peel_to_tree().ok());
            repo.diff_tree_to_index(tree.as_ref(), None, Some(&mut opts))
        }
        GitChangeArea::Changes => repo.diff_index_to_workdir(None, Some(&mut opts)),
        GitChangeArea::Untracked | GitChangeArea::Conflicts => unreachable!(),
    }
    .map_err(|error| error.to_string())?;
    if patch.deltas().len() == 0 {
        return Err("Selected Git patch changed; refresh Source Control".into());
    }
    render_git_diff_page(&patch, start_line)
}
fn render_git_diff_page(diff: &git2::Diff<'_>, start_line: usize) -> Result<GitPatchPage, String> {
    let mut text = String::new();
    let mut seen = 0usize;
    let mut next_line = None;
    let mut clipped_long_line = false;
    let mut deliberately_stopped = false;
    let mut safety_limit_reached = false;
    let print = diff.print(git2::DiffFormat::Patch, |_, _, line| {
        seen += 1;
        if seen > MAX_PAGED_DIFF_LINES {
            safety_limit_reached = true;
            deliberately_stopped = true;
            return false;
        }
        if seen <= start_line {
            return true;
        }
        let prefix = matches!(line.origin(), '+' | '-' | ' ');
        let raw = line.content();
        let requested = raw.len().saturating_add(usize::from(prefix));
        let remaining = MAX_PATCH_BYTES.saturating_sub(text.len());
        if clipped_long_line || (requested > remaining && !text.is_empty()) {
            next_line = Some(seen - 1);
            deliberately_stopped = true;
            return false;
        }
        if prefix {
            text.push(line.origin());
        }
        let remaining = MAX_PATCH_BYTES.saturating_sub(text.len());
        let slice = &raw[..raw.len().min(remaining)];
        let lossy = String::from_utf8_lossy(slice);
        if lossy.len() <= remaining {
            text.push_str(&lossy);
        } else {
            text.push_str(&lossy[..lossy.floor_char_boundary(remaining)]);
            clipped_long_line = true;
        }
        if raw.len() > remaining {
            // Exception for a single oversize line: preserve a bounded
            // excerpt and explicitly disclose that the rest was skipped.
            clipped_long_line = true;
        }
        true
    });
    if safety_limit_reached {
        return Err("Git diff paging safety limit reached".into());
    }
    if let Err(error) = print {
        if !deliberately_stopped {
            return Err(error.to_string());
        }
    }
    if seen < start_line || (text.is_empty() && start_line != 0) {
        return Err("Git patch changed; reopen its first page".into());
    }
    if clipped_long_line {
        text.push_str("\n… Git diff line exceeds 32 KiB and was clipped");
    }
    Ok(GitPatchPage {
        text,
        start_line,
        next_line,
        clipped_long_line,
    })
}

fn append_diff(
    diff: &git2::Diff<'_>,
    out: &mut String,
    truncated: &mut bool,
) -> Result<(), String> {
    diff.print(git2::DiffFormat::Patch, |_, _, line| {
        if *truncated {
            return true;
        }
        if matches!(line.origin(), '+' | '-' | ' ') {
            if out.len() == MAX_PATCH_BYTES {
                *truncated = true;
                return true;
            }
            out.push(line.origin());
        }
        let available = MAX_PATCH_BYTES.saturating_sub(out.len());
        let bytes = line.content();
        let raw = String::from_utf8_lossy(&bytes[..bytes.len().min(available)]);
        if raw.len() <= available {
            out.push_str(&raw);
        } else {
            // A sliced multibyte UTF-8 codepoint can expand to U+FFFD.
            // Preserve a valid UTF-8 boundary without exceeding the limit.
            out.push_str(&raw[..raw.floor_char_boundary(available)]);
            *truncated = true;
        }
        if bytes.len() > available {
            *truncated = true;
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
    fn immutable_original_snapshots_keep_exact_head_and_index_content() {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        repo.config()
            .unwrap()
            .set_bool("core.autocrlf", false)
            .unwrap();
        fs::write(dir.path().join("same.txt"), "committed\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("same.txt")).unwrap();
        let sig = git2::Signature::now("Test", "test@example.invalid").unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();
        drop(tree);
        fs::write(dir.path().join("same.txt"), "staged\n").unwrap();
        index.add_path(Path::new("same.txt")).unwrap();
        index.write().unwrap();
        drop(index);
        fs::write(dir.path().join("same.txt"), "working\n").unwrap();
        let head =
            read_git_original_snapshot(dir.path(), Path::new("same.txt"), GitChangeArea::Staged)
                .unwrap();
        let indexed =
            read_git_original_snapshot(dir.path(), Path::new("same.txt"), GitChangeArea::Changes)
                .unwrap();
        assert_eq!(head.content, "committed\n");
        assert_eq!(indexed.content, "staged\n");
        assert_ne!(head.blob_oid, indexed.blob_oid);
        assert!(head.title().contains("HEAD"));
        assert!(indexed.title().contains("INDEX"));
        assert_eq!(
            fs::read_to_string(dir.path().join("same.txt")).unwrap(),
            "working\n"
        );
        fs::write(dir.path().join("untracked.txt"), "untracked").unwrap();
        assert!(read_git_original_snapshot(
            dir.path(),
            Path::new("untracked.txt"),
            GitChangeArea::Untracked
        )
        .is_err());
        assert!(read_git_original_snapshot(
            dir.path(),
            Path::new("../escape"),
            GitChangeArea::Staged
        )
        .is_err());
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
    fn g02_exact_staged_and_worktree_pages_never_mix_for_one_file() {
        let temp = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(temp.path()).unwrap();
        repo.config()
            .unwrap()
            .set_bool("core.autocrlf", false)
            .unwrap();
        let relative = Path::new("dual.txt");
        let path = temp.path().join(relative);
        fs::write(&path, "head line\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(relative).unwrap();
        let sig = git2::Signature::now("Diff QA", "qa@example.invalid").unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "base", &tree, &[])
            .unwrap();
        drop(tree);
        fs::write(&path, "index line\n").unwrap();
        index.add_path(relative).unwrap();
        index.write().unwrap();
        drop(index);
        fs::write(&path, "worktree line\n").unwrap();
        let original_head = repo.head().unwrap().target();
        let index_blob = repo.index().unwrap().get_path(relative, 0).unwrap().id;
        let staged =
            read_file_diff_page_for_area(temp.path(), relative, GitChangeArea::Staged, 0).unwrap();
        let working =
            read_file_diff_page_for_area(temp.path(), relative, GitChangeArea::Changes, 0).unwrap();
        assert!(staged.text.contains("+index line"), "{staged:?}");
        assert!(!staged.text.contains("worktree line"), "{staged:?}");
        assert!(working.text.contains("+worktree line"), "{working:?}");
        assert!(!working.text.contains("+index line"), "{working:?}");
        assert_eq!(staged.next_line, None);
        assert_eq!(working.next_line, None);
        assert!(
            read_file_diff_page_for_area(temp.path(), relative, GitChangeArea::Untracked, 0)
                .is_err()
        );
        assert!(read_file_diff_page_for_area(
            temp.path(),
            Path::new("../dual.txt"),
            GitChangeArea::Staged,
            0
        )
        .is_err());
        assert_eq!(repo.head().unwrap().target(), original_head);
        assert_eq!(
            repo.index().unwrap().get_path(relative, 0).unwrap().id,
            index_blob
        );
        assert_eq!(fs::read_to_string(path).unwrap(), "worktree line\n");
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

    #[test]
    fn staged_patch_preview_does_not_change_head_index_or_worktree_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(temp.path()).unwrap();
        let relative = Path::new("example.rs");
        let file = temp.path().join(relative);
        fs::write(&file, "fn main() { println!(\"before\"); }\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(relative).unwrap();
        let base_tree_id = index.write_tree().unwrap();
        index.write().unwrap();
        let base_tree = repo.find_tree(base_tree_id).unwrap();
        let signature = git2::Signature::now("Alera QA", "test@local.invalid").unwrap();
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            "baseline",
            &base_tree,
            &[],
        )
        .unwrap();
        drop(base_tree);
        fs::write(&file, "fn main() { println!(\"staged\"); }\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(relative).unwrap();
        index.write().unwrap();
        let staged_id = index.get_path(relative, 0).unwrap().id;
        drop(index);

        let head_before = repo.head().unwrap().target().unwrap();
        let bytes_before = fs::read(&file).unwrap();
        let patch = read_file_diff(temp.path(), relative).unwrap();
        assert!(
            patch.contains("+fn main() { println!(\"staged\"); }"),
            "{patch}"
        );
        assert_eq!(repo.head().unwrap().target(), Some(head_before));
        assert_eq!(
            repo.index().unwrap().get_path(relative, 0).unwrap().id,
            staged_id
        );
        assert_eq!(fs::read(file).unwrap(), bytes_before);
    }

    #[test]
    fn untracked_large_patch_is_bounded_and_cannot_escape_workspace() {
        let temp = tempfile::tempdir().unwrap();
        git2::Repository::init(temp.path()).unwrap();
        let path = temp.path().join("large-untracked.txt");
        let text = "x".repeat(MAX_PATCH_BYTES + 2048);
        fs::write(&path, &text).unwrap();
        let message = read_file_diff(temp.path(), Path::new("large-untracked.txt")).unwrap();
        assert!(message.contains("too large"), "{message}");
        assert!(message.len() < MAX_PATCH_BYTES);
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
        assert!(read_file_diff(temp.path(), Path::new("../large-untracked.txt")).is_err());
        assert!(read_file_diff(temp.path(), &path).is_err());
    }

    #[test]
    fn untracked_file_at_exact_size_limit_does_not_overflow_paged_preview() {
        let temp = tempfile::tempdir().unwrap();
        git2::Repository::init(temp.path()).unwrap();
        let relative = Path::new("exactly-limit.txt");
        let path = temp.path().join(relative);
        let contents = "x".repeat(MAX_PATCH_BYTES);
        fs::write(&path, &contents).unwrap();
        let page = read_file_diff_page(temp.path(), relative, 0).unwrap();
        assert!(page.text.len() <= MAX_PATCH_BYTES + 64);
        assert!(
            page.text.contains("preview truncated"),
            "Untracked preview header must not silently exceed page limit"
        );
        assert!(page.next_line.is_none());
        assert_eq!(fs::read_to_string(path).unwrap(), contents);
    }

    #[test]
    fn oversized_tracked_ascii_patch_explicitly_reports_truncation_without_writes() {
        let temp = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(temp.path()).unwrap();
        let relative = Path::new("large-ascii.txt");
        let path = temp.path().join(relative);
        fs::write(&path, "baseline\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(relative).unwrap();
        let tree_oid = index.write_tree().unwrap();
        index.write().unwrap();
        drop(index);
        let tree = repo.find_tree(tree_oid).unwrap();
        let signature = git2::Signature::now("Alera QA", "test@local.invalid").unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "baseline", &tree, &[])
            .unwrap();
        drop(tree);
        let content = "plain ASCII content in a tracked change\n".repeat(3_000);
        fs::write(&path, &content).unwrap();
        let head_before = repo.head().unwrap().target();
        let index_before = repo.index().unwrap().get_path(relative, 0).unwrap().id;

        let preview = read_file_diff(temp.path(), relative).unwrap();
        assert!(preview.starts_with("diff --git "), "{preview:?}");
        assert!(
            preview.ends_with("\n… diff preview truncated"),
            "A partial tracked patch must carry a visible truncation warning"
        );
        assert!(preview.len() <= MAX_PATCH_BYTES + 64);
        assert!(preview.is_char_boundary(preview.len()));
        assert_eq!(fs::read_to_string(path).unwrap(), content);
        assert_eq!(repo.head().unwrap().target(), head_before);
        assert_eq!(
            repo.index().unwrap().get_path(relative, 0).unwrap().id,
            index_before
        );
    }

    #[test]
    fn truncated_tracked_unicode_patch_stays_valid_utf8_and_bounded() {
        let temp = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(temp.path()).unwrap();
        let relative = Path::new("unicode.txt");
        let file = temp.path().join(relative);
        fs::write(&file, "original\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(relative).unwrap();
        let tree_id = index.write_tree().unwrap();
        index.write().unwrap();
        drop(index);
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = git2::Signature::now("Alera QA", "test@local.invalid").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();
        drop(tree);
        let content = "中文修改🙂\n".repeat(6_000);
        fs::write(&file, &content).unwrap();
        let patch = read_file_diff(temp.path(), relative).unwrap();
        assert!(patch.ends_with("\n… diff preview truncated"));
        assert!(patch.len() <= MAX_PATCH_BYTES + 64);
        assert!(patch.is_char_boundary(patch.len()));
        assert_eq!(fs::read_to_string(file).unwrap(), content);
    }

    #[test]
    fn large_tracked_patch_can_be_read_across_bounded_pages_without_git_writes() {
        let temp = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(temp.path()).unwrap();
        let relative = Path::new("pages.txt");
        let file = temp.path().join(relative);
        fs::write(&file, "original\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(relative).unwrap();
        let tree_id = index.write_tree().unwrap();
        index.write().unwrap();
        drop(index);
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = git2::Signature::now("Alera Test", "test@local.invalid").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();
        drop(tree);
        let modified = (0..2_500)
            .map(|index| format!("line-{index:04} 中文🙂 checked and safe\n"))
            .collect::<String>();
        fs::write(&file, &modified).unwrap();
        let head_before = repo.head().unwrap().target();
        let index_before = repo.index().unwrap().get_path(relative, 0).unwrap().id;

        let mut cursor = 0;
        let mut pages = 0;
        let mut seen_text = String::new();
        loop {
            let page = read_file_diff_page(temp.path(), relative, cursor).unwrap();
            assert_eq!(page.start_line, cursor);
            assert!(!page.clipped_long_line);
            assert!(!page.text.is_empty());
            assert!(page.text.len() <= MAX_PATCH_BYTES);
            seen_text.push_str(&page.text);
            pages += 1;
            assert!(pages < 16, "Pagination must always advance");
            match page.next_line {
                Some(next) => {
                    assert!(next > cursor, "The next cursor must make progress");
                    cursor = next;
                }
                None => break,
            }
        }
        assert!(pages >= 3, "Expected more than two bounded pages");
        assert!(seen_text.contains("line-0000 中文🙂"));
        assert!(seen_text.contains("line-2499 中文🙂"));
        let first_again = read_file_diff_page(temp.path(), relative, 0).unwrap();
        assert_eq!(first_again.start_line, 0);
        assert_eq!(first_again.text, seen_text[..first_again.text.len()]);
        assert_eq!(fs::read_to_string(file).unwrap(), modified);
        assert_eq!(repo.head().unwrap().target(), head_before);
        assert_eq!(
            repo.index().unwrap().get_path(relative, 0).unwrap().id,
            index_before
        );
        assert!(read_file_diff_page(temp.path(), Path::new("../escape"), 0).is_err());
        assert!(read_file_diff_page(temp.path(), relative, MAX_PAGED_DIFF_LINES).is_err());
        assert!(read_file_diff_page(temp.path(), relative, cursor + 99_000).is_err());
    }

    #[test]
    fn staged_only_patch_pages_without_changing_head_index_or_disk() {
        let temp = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(temp.path()).unwrap();
        let relative = Path::new("staged.rs");
        let path = temp.path().join(relative);
        fs::write(&path, "before\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(relative).unwrap();
        let tree_id = index.write_tree().unwrap();
        index.write().unwrap();
        drop(index);
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = git2::Signature::now("Alera Test", "test@local.invalid").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();
        drop(tree);
        let staged_text = (0..2_000)
            .map(|i| format!("let staged_line_{i:04} = true;\n"))
            .collect::<String>();
        fs::write(&path, &staged_text).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(relative).unwrap();
        index.write().unwrap();
        drop(index);
        let head_before = repo.head().unwrap().target();
        let staged_oid = repo.index().unwrap().get_path(relative, 0).unwrap().id;
        let mut cursor = 0;
        let mut seen = String::new();
        let mut pages = 0;
        loop {
            let page = read_file_diff_page(temp.path(), relative, cursor).unwrap();
            seen.push_str(&page.text);
            pages += 1;
            assert!(pages < 12);
            if let Some(next) = page.next_line {
                assert!(next > cursor);
                cursor = next;
            } else {
                break;
            }
        }
        assert!(pages > 1);
        assert!(seen.contains("+let staged_line_0000 = true;"));
        assert!(seen.contains("+let staged_line_1999 = true;"));
        assert_eq!(repo.head().unwrap().target(), head_before);
        assert_eq!(
            repo.index().unwrap().get_path(relative, 0).unwrap().id,
            staged_oid
        );
        assert_eq!(fs::read_to_string(path).unwrap(), staged_text);
    }

    #[test]
    fn paged_preview_clips_oversized_single_line_with_explicit_warning() {
        let temp = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(temp.path()).unwrap();
        let relative = Path::new("long.txt");
        let file = temp.path().join(relative);
        fs::write(&file, "old\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(relative).unwrap();
        let tree_id = index.write_tree().unwrap();
        index.write().unwrap();
        drop(index);
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = git2::Signature::now("Alera Test", "test@local.invalid").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();
        drop(tree);
        let changed = format!("{}\nEND\n", "很長🙂".repeat(MAX_PATCH_BYTES));
        fs::write(&file, &changed).unwrap();
        let mut cursor = 0;
        let mut clipped = false;
        let mut found_end = false;
        for _ in 0..8 {
            let page = read_file_diff_page(temp.path(), relative, cursor).unwrap();
            clipped |= page.clipped_long_line && page.text.contains("was clipped");
            found_end |= page.text.contains("+END");
            if let Some(next) = page.next_line {
                assert!(next > cursor);
                cursor = next;
            } else {
                break;
            }
        }
        assert!(
            clipped,
            "A single oversized line must not be shown as complete"
        );
        assert!(found_end, "Later complete lines must remain accessible");
        assert_eq!(fs::read_to_string(file).unwrap(), changed);
    }
}
